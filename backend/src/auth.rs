use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    sync::mpsc::Sender,
    time::{Duration, Instant},
};
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Start,
    Authenticate,
    Account,
    Credentials,
}
impl Stage {
    pub fn name(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Authenticate => "authenticate",
            Self::Account => "account",
            Self::Credentials => "credentials",
        }
    }
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Failure {
    pub stage: Stage,
    pub message: String,
}
impl Failure {
    pub fn new(stage: Stage, message: impl Into<String>) -> Self {
        Self {
            stage,
            message: message.into(),
        }
    }
}

pub fn identity() -> Result<(String, String), String> {
    // Use the account belonging to our UID, never an attacker-controlled USER variable.
    unsafe {
        let ptr = libc::getpwuid(libc::getuid());
        if ptr.is_null() || (*ptr).pw_name.is_null() {
            return Err("Cannot resolve session user".into());
        }
        let name = CStr::from_ptr((*ptr).pw_name)
            .to_string_lossy()
            .into_owned();
        let real = if (*ptr).pw_gecos.is_null() {
            name.clone()
        } else {
            CStr::from_ptr((*ptr).pw_gecos)
                .to_string_lossy()
                .split(',')
                .next()
                .unwrap_or(&name)
                .to_string()
        };
        Ok((name, real))
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum Event {
    Pam { kind: String, text: String },
    Done(Result<(), Failure>),
}
#[repr(C)]
struct PamMessage {
    style: c_int,
    msg: *const c_char,
}
#[repr(C)]
struct PamResponse {
    resp: *mut c_char,
    retcode: c_int,
}
#[repr(C)]
struct PamConv {
    conv: unsafe extern "C" fn(
        c_int,
        *mut *const PamMessage,
        *mut *mut PamResponse,
        *mut c_void,
    ) -> c_int,
    data: *mut c_void,
}
#[link(name = "pam")]
unsafe extern "C" {
    fn pam_start(
        service: *const c_char,
        user: *const c_char,
        conv: *const PamConv,
        handle: *mut *mut c_void,
    ) -> c_int;
    fn pam_authenticate(handle: *mut c_void, flags: c_int) -> c_int;
    fn pam_acct_mgmt(handle: *mut c_void, flags: c_int) -> c_int;
    fn pam_setcred(handle: *mut c_void, flags: c_int) -> c_int;
    fn pam_end(handle: *mut c_void, status: c_int) -> c_int;
    fn pam_strerror(handle: *mut c_void, status: c_int) -> *const c_char;
}
struct Conversation<'a> {
    user: &'a CStr,
    password: Option<SecretBytes>,
    tx: &'a Sender<Event>,
    messages: std::cell::Cell<usize>,
}

pub const MAX_PASSWORD_BYTES: usize = 4096;

pub fn protect_process() -> Result<(), String> {
    let limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &limit) } != 0
        || unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0) } != 0
    {
        return Err("Cannot disable credential-process dumps".into());
    }
    Ok(())
}

struct SecretBytes {
    bytes: Zeroizing<Vec<u8>>,
    locked: bool,
}
impl SecretBytes {
    fn new(password: &str) -> Self {
        // Reserve the terminator too: Vec growth must not free a password copy.
        let mut bytes = Zeroizing::new(Vec::<u8>::with_capacity(password.len() + 1));
        let locked = unsafe { libc::mlock(bytes.as_ptr().cast(), password.len() + 1) } == 0;
        bytes.extend_from_slice(password.as_bytes());
        bytes.push(0);
        Self { bytes, locked }
    }
}
impl Drop for SecretBytes {
    fn drop(&mut self) {
        let len = self.bytes.len();
        self.bytes.as_mut_slice().zeroize();
        if self.locked {
            unsafe {
                libc::munlock(self.bytes.as_ptr().cast(), len);
            }
        }
    }
}

/// Also scrub ignored/extra bridge fields; extracting only `password` is not
/// enough for malformed messages. Parser/JSC-managed internal copies remain
/// outside Rust's ownership, so this is not a claim of whole-process erasure.
pub struct SecretMessage(pub serde_json::Value);
impl std::ops::Deref for SecretMessage {
    type Target = serde_json::Value;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for SecretMessage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for SecretMessage {
    fn drop(&mut self) {
        fn scrub(v: &mut serde_json::Value) {
            match v {
                serde_json::Value::String(s) => s.zeroize(),
                serde_json::Value::Array(a) => a.iter_mut().for_each(scrub),
                serde_json::Value::Object(o) => {
                    for (mut key, mut value) in std::mem::take(o) {
                        key.zeroize();
                        scrub(&mut value);
                    }
                }
                _ => {}
            }
        }
        scrub(&mut self.0);
    }
}

pub fn take_secret(raw: gtk::glib::GString) -> Zeroizing<String> {
    use gtk::glib::translate::IntoGlibPtr;
    let secret = Zeroizing::new(raw.to_string());
    let len = raw.len();
    let ptr: *mut c_char = raw.into_glib_ptr();
    unsafe {
        std::slice::from_raw_parts_mut(ptr.cast::<u8>(), len).zeroize();
        gtk::glib::ffi::g_free(ptr.cast());
    }
    secret
}
unsafe extern "C" fn conversation(
    n: c_int,
    messages: *mut *const PamMessage,
    responses: *mut *mut PamResponse,
    data: *mut c_void,
) -> c_int {
    if !(1..=32).contains(&n) || messages.is_null() || responses.is_null() || data.is_null() {
        return 19;
    }
    // PAM owns successful response allocations and frees them after the conversation.
    unsafe {
        let context = &*(data as *const Conversation<'_>);
        let out = libc::calloc(n as usize, std::mem::size_of::<PamResponse>()) as *mut PamResponse;
        if out.is_null() {
            return 5;
        }
        for i in 0..n as usize {
            let msg = *messages.add(i);
            let mut valid = !msg.is_null();
            if valid {
                match (*msg).style {
                    1 if context.password.is_none() => valid = false,
                    1 | 2 => {
                        let bytes = if (*msg).style == 1 {
                            context
                                .password
                                .as_ref()
                                .map(|p| p.bytes.as_slice())
                                .unwrap_or_default()
                        } else {
                            context.user.to_bytes_with_nul()
                        };
                        let ptr = libc::malloc(bytes.len()) as *mut c_char;
                        if ptr.is_null() {
                            valid = false;
                        } else {
                            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.cast(), bytes.len());
                            (*out.add(i)).resp = ptr;
                        }
                    }
                    3 | 4 => {
                        if context.messages.get() >= 32 {
                            continue;
                        }
                        context.messages.set(context.messages.get() + 1);
                        let text = if (*msg).msg.is_null() {
                            String::new()
                        } else {
                            CStr::from_ptr((*msg).msg)
                                .to_string_lossy()
                                .chars()
                                .take(1024)
                                .collect()
                        };
                        let _ = context.tx.send(Event::Pam {
                            kind: if (*msg).style == 3 { "error" } else { "info" }.into(),
                            text,
                        });
                    }
                    _ => valid = false,
                }
            }
            if !valid {
                for j in 0..=i {
                    let p = (*out.add(j)).resp;
                    if !p.is_null() {
                        let len = libc::strlen(p);
                        std::slice::from_raw_parts_mut(p.cast::<u8>(), len).zeroize();
                        libc::free(p.cast());
                    }
                }
                libc::free(out.cast());
                return 19;
            }
        }
        *responses = out;
    }
    0
}
pub fn pam(
    service: &str,
    user: &str,
    password: Zeroizing<String>,
    tx: &Sender<Event>,
) -> Result<(), Failure> {
    run_pam(service, user, Some(password), tx, |_, _, _| {})
}

/// This path deliberately skips pam_authenticate and refuses password prompts.
pub fn check_pam(service: &str, user: &str) -> Result<(), Failure> {
    let (tx, _rx) = std::sync::mpsc::channel();
    run_pam(service, user, None, &tx, |stage, status, message| {
        println!(
            "{}: {} ({status}) — {message}",
            stage.name(),
            if status == 0 { "OK" } else { "FAILED" }
        );
    })
}

fn steps(
    authenticate: bool,
    mut call: impl FnMut(Stage) -> Result<(), Failure>,
) -> Result<(), Failure> {
    call(Stage::Start)?;
    if authenticate {
        call(Stage::Authenticate)?;
    }
    call(Stage::Account)?;
    call(Stage::Credentials)
}

fn run_pam(
    service: &str,
    user: &str,
    password: Option<Zeroizing<String>>,
    tx: &Sender<Event>,
    mut report: impl FnMut(Stage, c_int, &str),
) -> Result<(), Failure> {
    let service =
        CString::new(service).map_err(|_| Failure::new(Stage::Start, "Invalid PAM service"))?;
    let user = CString::new(user).map_err(|_| Failure::new(Stage::Start, "Invalid username"))?;
    if password
        .as_ref()
        .is_some_and(|p| p.contains('\0') || p.len() > MAX_PASSWORD_BYTES)
    {
        return Err(Failure::new(Stage::Authenticate, "Invalid password"));
    }
    let authenticate = password.is_some();
    let bytes = password.as_ref().map(|password| SecretBytes::new(password));
    drop(password);
    let mut data = Conversation {
        user: &user,
        password: bytes,
        tx,
        messages: std::cell::Cell::new(0),
    };
    let conv = PamConv {
        conv: conversation,
        data: (&mut data as *mut Conversation<'_>).cast(),
    };
    let mut handle = std::ptr::null_mut();
    unsafe {
        let mut status = 0;
        let result = steps(authenticate, |stage| {
            status = match stage {
                Stage::Start => pam_start(service.as_ptr(), user.as_ptr(), &conv, &mut handle),
                Stage::Authenticate => pam_authenticate(handle, 0),
                Stage::Account => pam_acct_mgmt(handle, 0),
                Stage::Credentials => pam_setcred(handle, 0x10), // PAM_REFRESH_CRED
            };
            let ptr = pam_strerror(handle, status);
            let message = if ptr.is_null() {
                format!("PAM status {status}")
            } else {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            };
            report(stage, status, &message);
            if status == 0 {
                Ok(())
            } else {
                Err(Failure::new(stage, message))
            }
        });
        if !handle.is_null() {
            pam_end(handle, status);
        }
        result
    }
}
pub fn pad_failure(start: Instant, result: &Result<(), Failure>) {
    if result.is_err() {
        std::thread::sleep(Duration::from_millis(1200).saturating_sub(start.elapsed()));
    }
}
#[cfg(feature = "mock-auth")]
pub fn mock(password: &str, expected: &str) -> Result<(), Failure> {
    use subtle::ConstantTimeEq;
    let mut a = Zeroizing::new([0u8; 4096]);
    let mut b = Zeroizing::new([0u8; 4096]);
    if password.len() > a.len() || expected.len() > b.len() {
        return Err(Failure::new(Stage::Authenticate, "Password too long"));
    }
    a[..password.len()].copy_from_slice(password.as_bytes());
    b[..expected.len()].copy_from_slice(expected.as_bytes());
    if bool::from(a.as_slice().ct_eq(b.as_slice()) & password.len().ct_eq(&expected.len())) {
        Ok(())
    } else {
        Err(Failure::new(Stage::Authenticate, "Authentication failed"))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pam_stages_short_circuit_and_preserve_failure() {
        let all = [
            Stage::Start,
            Stage::Authenticate,
            Stage::Account,
            Stage::Credentials,
        ];
        for (index, failing) in all.into_iter().enumerate() {
            let mut called = vec![];
            let failure = steps(true, |stage| {
                called.push(stage);
                if stage == failing {
                    Err(Failure::new(stage, "PAM diagnostic"))
                } else {
                    Ok(())
                }
            })
            .expect_err("injected failure");
            assert_eq!(called, all[..=index]);
            assert_eq!(failure.stage, failing);
            assert_eq!(failure.message, "PAM diagnostic");
            assert_eq!(
                serde_json::to_value(failure.stage).expect("stage JSON"),
                failing.name()
            );
        }
    }
    #[test]
    fn preflight_never_authenticates_and_stops_on_failure() {
        let all = [Stage::Start, Stage::Account, Stage::Credentials];
        for fail_at in [
            None,
            Some(Stage::Start),
            Some(Stage::Account),
            Some(Stage::Credentials),
        ] {
            let mut called = vec![];
            let result = steps(false, |stage| {
                called.push(stage);
                if Some(stage) == fail_at {
                    Err(Failure::new(stage, "denied"))
                } else {
                    Ok(())
                }
            });
            assert!(!called.contains(&Stage::Authenticate));
            assert_eq!(result.is_err(), fail_at.is_some());
            assert_eq!(called, all[..called.len()]);
            if let Some(stage) = fail_at {
                assert_eq!(called.last(), Some(&stage));
            } else {
                assert_eq!(called, all);
            }
        }
    }
    #[test]
    fn pam_nonexistent_user() {
        assert!(
            unsafe { libc::getpwnam(c"rhine-nobody".as_ptr()) }.is_null(),
            "Refusing to authenticate an existing fixture account"
        );
        let (tx, _) = std::sync::mpsc::channel();
        let failure = pam(
            "swaylock",
            "rhine-nobody",
            Zeroizing::new("test-only".into()),
            &tx,
        )
        .expect_err("nonexistent user must be rejected");
        assert_eq!(failure.stage, Stage::Authenticate);
        assert!(!failure.message.is_empty());
    }
    #[cfg(feature = "mock-auth")]
    #[test]
    fn mock_exact_match() {
        assert!(mock("", "").is_ok());
        assert!(mock("abc", "abc").is_ok());
        assert!(mock("abc\0", "abc").is_err());
        assert!(mock("ab", "abc").is_err());
        assert_eq!(
            mock("wrong", "abc").expect_err("wrong key").stage,
            Stage::Authenticate
        );
        assert_eq!(
            mock(&"x".repeat(4097), "abc")
                .expect_err("oversize key")
                .stage,
            Stage::Authenticate
        );
    }
}
