mod auth;
mod auth_worker;
mod command;
mod config;
mod instance;
mod keyboard;
mod resources;
mod screenshot;
mod system;
#[cfg(feature = "mock-auth")]
mod test_control;
mod ui;

use std::{fs::File, io::Write, os::unix::process::CommandExt};

fn daemonize() -> Result<Option<File>, String> {
    use std::os::fd::FromRawFd;
    let mut fds = [0; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    match unsafe { libc::fork() } {
        -1 => {
            let error = std::io::Error::last_os_error().to_string();
            unsafe {
                libc::close(fds[0]);
                libc::close(fds[1]);
            }
            Err(error)
        }
        0 => {
            unsafe {
                libc::close(fds[0]);
                libc::setsid();
            }
            Ok(Some(unsafe { File::from_raw_fd(fds[1]) }))
        }
        _ => {
            unsafe {
                libc::close(fds[1]);
            }
            let mut descriptor = libc::pollfd {
                fd: fds[0],
                events: libc::POLLIN,
                revents: 0,
            };
            let mut byte = [1u8];
            let ready = unsafe {
                libc::poll(&mut descriptor, 1, 30000) > 0
                    && libc::read(fds[0], byte.as_mut_ptr().cast(), 1) == 1
                    && byte[0] == 0
            };
            std::process::exit(if ready { 0 } else { 1 });
        }
    }
}
fn panic_recovery() {
    let already_restarted = std::env::var_os("RHINE_REEXEC").is_some();
    std::panic::set_hook(Box::new(move |info| {
        // Do not format the panic payload: a third-party panic could contain a secret.
        eprintln!(
            "Fatal backend panic at {:?}; {}",
            info.location(),
            if already_restarted {
                "restart already used; compositor stays locked"
            } else {
                "restarting with the compositor kept locked"
            }
        );
        if !already_restarted && let Ok(exe) = std::env::current_exe() {
            let e = std::process::Command::new(exe)
                .args(std::env::args_os().skip(1))
                .env("RHINE_REEXEC", "1")
                .exec();
            eprintln!("Restart failed: {e}");
        }
        std::process::abort();
    }));
}
fn run() -> Result<i32, String> {
    auth::protect_process()?;
    let args = config::Args::parse()?;
    if args.auth_worker {
        return auth_worker::run();
    }
    // Must precede singleton, daemonization, GTK and all compositor access.
    if args.check_pam {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let (status, output) = command::completed_output(
            exe.to_str().ok_or("Invalid executable path")?,
            &["--check-pam-worker"],
            std::time::Duration::from_secs(15),
            65536,
        )
        .ok_or("PAM preflight did not finish within 15 seconds, or its worker failed")?;
        std::io::stdout()
            .write_all(&output)
            .map_err(|e| e.to_string())?;
        return Ok(if status.success() { 0 } else { 1 });
    }
    if args.check_pam_worker {
        let (user, _) = auth::identity()?;
        let config = config::Config::load();
        return auth::check_pam(&config.lock.pam_service, &user)
            .map(|()| 0)
            .map_err(|failure| format!("PAM {}: {}", failure.stage.name(), failure.message));
    }
    let guard = if args.preview {
        None
    } else {
        match instance::Instance::acquire()? {
            Some(f) => Some(f),
            None => return Ok(0),
        }
    };
    let ready = if args.daemonize && std::env::var_os("RHINE_REEXEC").is_none() {
        daemonize()?
    } else {
        None
    };
    panic_recovery();
    if let Some(guard) = &guard {
        guard.serve()?;
    }
    gtk::init().map_err(|e| e.to_string())?;
    ui::run(args, ready, guard.as_ref().map(|g| g.confirmed.clone()))
}
fn main() {
    let status = match run() {
        Ok(n) => n,
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "rhine-lock: {e}");
            1
        }
    };
    std::process::exit(status);
}
