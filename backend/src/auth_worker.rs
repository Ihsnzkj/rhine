//! A disposable credential process: bounded IPC, lifetime, and no GUI state.
use crate::auth::{self, Event, Failure, Stage};
use std::{
    fs::File,
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::process::CommandExt,
    },
    process::{Command, Stdio},
    sync::mpsc::Sender,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

const MAX_IPC: usize = 65536;
const AUTH_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(serde::Serialize)]
struct Request<'a> {
    service: &'a str,
    password: &'a str,
    #[cfg(feature = "mock-auth")]
    expected: Option<&'a str>,
}

pub fn verify(
    service: &str,
    password: Zeroizing<String>,
    tx: &Sender<Event>,
    #[cfg(feature = "mock-auth")] expected: Option<Zeroizing<String>>,
) -> Result<(), Failure> {
    let invalid = || {
        Failure::new(
            Stage::Authenticate,
            "Password exceeds 4096 UTF-8 bytes or contains NUL",
        )
    };
    if password.len() > auth::MAX_PASSWORD_BYTES || password.contains('\0') {
        return Err(invalid());
    }
    if !crate::config::valid_service(service) {
        return Err(Failure::new(Stage::Start, "Invalid PAM service"));
    }
    #[cfg(feature = "mock-auth")]
    if expected
        .as_ref()
        .is_some_and(|s| s.len() > auth::MAX_PASSWORD_BYTES)
    {
        return Err(invalid());
    }
    let mut input = Zeroizing::new(Vec::with_capacity(MAX_IPC));
    serde_json::to_writer(
        &mut *input,
        &Request {
            service,
            password: &password,
            #[cfg(feature = "mock-auth")]
            expected: expected.as_deref().map(|s| s.as_str()),
        },
    )
    .map_err(|_| Failure::new(Stage::Start, "Cannot encode authentication request"))?;
    drop(password);
    let mut command = Command::new(
        std::env::current_exe()
            .map_err(|_| Failure::new(Stage::Start, "Cannot locate credential worker"))?,
    );
    command
        .arg("--auth-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .env("RHINE_AUTH_PARENT", std::process::id().to_string());
    let timeout = AUTH_TIMEOUT;
    #[cfg(feature = "mock-auth")]
    let timeout = {
        use std::sync::atomic::{AtomicBool, Ordering};
        static FAULT_USED: AtomicBool = AtomicBool::new(false);
        command.env_remove("RHINE_WORKER_FAULT");
        let fault = std::env::var("RHINE_TEST_AUTH_FAULT_ONCE").ok();
        if expected.is_some()
            && std::env::var("RHINE_TEST_CONTROL").as_deref() == Ok("1")
            && fault.is_some()
            && !FAULT_USED.swap(true, Ordering::SeqCst)
        {
            command.env("RHINE_WORKER_FAULT", fault.unwrap_or_default());
            Duration::from_millis(1500)
        } else {
            timeout
        }
    };
    let mut child = command
        .spawn()
        .map_err(|_| Failure::new(Stage::Start, "Cannot start credential worker"))?;
    let result = (|| {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| Failure::new(Stage::Start, "No credential pipe"))?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| Failure::new(Stage::Start, "No credential response pipe"))?;
        for fd in [stdin.as_raw_fd(), stdout.as_raw_fd()] {
            if unsafe { libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK) } < 0 {
                return Err(Failure::new(
                    Stage::Start,
                    "Cannot configure credential pipes",
                ));
            }
        }
        let start = Instant::now();
        let mut written = 0;
        // Writer closes after the bounded request, making EOF the framing.
        while written < input.len() {
            if start.elapsed() >= timeout {
                return Err(Failure::new(
                    Stage::Authenticate,
                    "Authentication timed out; try again",
                ));
            }
            match stdin.write(&input[written..]) {
                Ok(0) => return Err(Failure::new(Stage::Start, "Credential pipe closed")),
                Ok(n) => written += n,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(_) => return Err(Failure::new(Stage::Start, "Credential worker stopped")),
            }
        }
        drop(stdin);
        drop(input);
        let mut output = Vec::with_capacity(MAX_IPC);
        let mut parsed = 0;
        let mut done = None;
        let mut events = 0;
        let mut eof = false;
        while start.elapsed() < timeout {
            let mut buffer = [0; 4096];
            match stdout.read(&mut buffer) {
                Ok(0) => eof = true,
                Ok(n) if output.len() + n <= MAX_IPC => output.extend_from_slice(&buffer[..n]),
                Ok(_) => {
                    return Err(Failure::new(
                        Stage::Authenticate,
                        "Invalid credential response size",
                    ));
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => {
                    return Err(Failure::new(
                        Stage::Authenticate,
                        "Credential response failed",
                    ));
                }
            }
            while let Some(length) = output[parsed..].iter().position(|b| *b == b'\n') {
                events += 1;
                if events > 33 {
                    return Err(Failure::new(
                        Stage::Authenticate,
                        "Too many credential messages",
                    ));
                }
                let event: Event = serde_json::from_slice(&output[parsed..parsed + length])
                    .map_err(|_| {
                        Failure::new(Stage::Authenticate, "Invalid credential response")
                    })?;
                parsed += length + 1;
                match event {
                    Event::Done(result) => {
                        if done.is_some() {
                            return Err(Failure::new(
                                Stage::Authenticate,
                                "Duplicate credential result",
                            ));
                        }
                        done = Some(result);
                    }
                    event @ Event::Pam { .. } => {
                        let _ = tx.send(event);
                    }
                }
            }
            if crate::command::exited_without_reaping(child.id()).map_err(|_| {
                Failure::new(Stage::Authenticate, "Cannot inspect credential worker")
            })? && eof
            {
                // Drain and validate the entire response, then kill remaining
                // helpers before reaping the reserved process-group leader.
                if parsed != output.len() {
                    return Err(Failure::new(
                        Stage::Authenticate,
                        "Incomplete credential response",
                    ));
                }
                return done.take().unwrap_or_else(|| {
                    Err(Failure::new(
                        Stage::Authenticate,
                        "Missing credential result; try again",
                    ))
                });
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Err(Failure::new(
            Stage::Authenticate,
            "Authentication timed out; try again",
        ))
    })();
    let status = crate::command::kill_and_reap(&mut child);
    result.and_then(|()| {
        if status.is_ok_and(|s| s.success()) {
            Ok(())
        } else {
            Err(Failure::new(
                Stage::Authenticate,
                "Authentication worker stopped; try again",
            ))
        }
    })
}

pub fn run() -> Result<i32, String> {
    let parent = std::env::var("RHINE_AUTH_PARENT")
        .ok()
        .and_then(|p| p.parse::<libc::pid_t>().ok())
        .ok_or("Credential worker requires a parent")?;
    if unsafe { libc::getppid() } != parent
        || unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) } != 0
        || unsafe { libc::getppid() } != parent
    {
        return Err("Credential parent disappeared".into());
    }
    let mut bytes = Zeroizing::new(Vec::with_capacity(MAX_IPC + 1));
    std::io::stdin()
        .take((MAX_IPC + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read credential request")?;
    if bytes.len() > MAX_IPC {
        return Err("Credential request too large".into());
    }
    let mut request = auth::SecretMessage(
        serde_json::from_slice(&bytes).map_err(|_| "Invalid credential request")?,
    );
    drop(bytes);
    let object = request.as_object_mut().ok_or("Invalid credential object")?;
    let service = object
        .get("service")
        .and_then(|v| v.as_str())
        .filter(|s| crate::config::valid_service(s))
        .ok_or("Invalid PAM service")?
        .to_owned();
    let password = match object.remove("password") {
        Some(serde_json::Value::String(p))
            if p.len() <= auth::MAX_PASSWORD_BYTES && !p.contains('\0') =>
        {
            Zeroizing::new(p)
        }
        Some(serde_json::Value::String(p)) => {
            drop(Zeroizing::new(p));
            return Err("Invalid password length/content".into());
        }
        _ => return Err("Missing password".into()),
    };
    #[cfg(feature = "mock-auth")]
    let expected = match object.remove("expected") {
        Some(serde_json::Value::String(p)) => Some(Zeroizing::new(p)),
        _ => None,
    };
    drop(request);

    // PAM modules may print to stdout. Give the event writer a private dup and
    // send ordinary stdout to /dev/null so module output cannot spoof IPC.
    let fd = unsafe { libc::fcntl(libc::STDOUT_FILENO, libc::F_DUPFD_CLOEXEC, 3) };
    if fd < 0 {
        return Err("Cannot create credential response channel".into());
    }
    let mut response = unsafe { File::from_raw_fd(fd) };
    let null = File::options()
        .write(true)
        .open("/dev/null")
        .map_err(|_| "Cannot redirect credential output")?;
    if unsafe { libc::dup2(null.as_raw_fd(), libc::STDOUT_FILENO) } < 0 {
        return Err("Cannot redirect credential output".into());
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("pam".into())
        .spawn(move || {
            #[cfg(feature = "mock-auth")]
            if expected.is_some() {
                match std::env::var("RHINE_WORKER_FAULT").as_deref() {
                    Ok("hang") => std::thread::sleep(Duration::from_secs(120)),
                    Ok("exit") => std::process::exit(7),
                    Ok("empty") => std::process::exit(0),
                    _ => {}
                }
            }
            #[cfg(feature = "mock-auth")]
            let result = if let Some(expected) = expected {
                auth::mock(&password, &expected).and_then(|()| {
                    if std::env::var("RHINE_WORKER_FAULT").as_deref() == Ok("account") {
                        Err(Failure::new(Stage::Account, "Fixture account policy"))
                    } else {
                        Ok(())
                    }
                })
            } else {
                real_pam(&service, password, &tx)
            };
            #[cfg(not(feature = "mock-auth"))]
            let result = real_pam(&service, password, &tx);
            let _ = tx.send(Event::Done(result));
        })
        .map_err(|_| "Cannot start PAM thread")?;
    while let Ok(event) = rx.recv() {
        serde_json::to_writer(&mut response, &event)
            .map_err(|_| "Cannot write credential result")?;
        response
            .write_all(b"\n")
            .map_err(|_| "Cannot write credential result")?;
        if matches!(event, Event::Done(_)) {
            return Ok(0);
        }
    }
    Err("Credential worker ended without a result".into())
}

fn real_pam(service: &str, password: Zeroizing<String>, tx: &Sender<Event>) -> Result<(), Failure> {
    auth::identity()
        .map_err(|e| Failure::new(Stage::Start, e))
        .and_then(|(user, _)| auth::pam(service, &user, password, tx))
}
