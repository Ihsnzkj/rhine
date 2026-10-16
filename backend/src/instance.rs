//! A private, per-compositor flock plus a live readiness reply.
//!
//! A held flock alone only proves that another process is starting, not that
//! the compositor has locked. Never persist a readiness bit: it survives crashes.
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{FileTypeExt, MetadataExt, OpenOptionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub struct Instance {
    _directory: File,
    _lock: File,
    listener: UnixListener,
    socket: PathBuf,
    pub confirmed: Arc<AtomicBool>,
    stopping: Arc<AtomicBool>,
}

impl Instance {
    pub fn acquire() -> Result<Option<Self>, String> {
        // libwayland prefers an inherited connection over WAYLAND_DISPLAY.
        // Our coordination key identifies the named listening socket, so an
        // inherited fd could otherwise receive confirmation for another
        // compositor (or report success even when that fd is invalid).
        if std::env::var_os("WAYLAND_SOCKET").is_some() {
            return Err("WAYLAND_SOCKET is unsupported for locking; unset it and select the compositor with WAYLAND_DISPLAY".into());
        }
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR is required")?;
        let display = std::env::var_os("WAYLAND_DISPLAY").unwrap_or_else(|| "wayland-0".into());
        let path = Path::new(&runtime).join(display);
        let metadata = std::fs::metadata(path).map_err(|e| format!("Wayland socket: {e}"))?;
        if !metadata.file_type().is_socket() {
            return Err("WAYLAND_DISPLAY must identify a Unix socket".into());
        }
        let key = format!("rhine-lock-{:x}-{:x}", metadata.dev(), metadata.ino());
        Self::acquire_at(Path::new(&runtime), &key, Duration::from_secs(30))
    }

    fn acquire_at(runtime: &Path, key: &str, timeout: Duration) -> Result<Option<Self>, String> {
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(runtime)
            .map_err(|e| format!("Runtime directory: {e}"))?;
        let meta = directory.metadata().map_err(|e| e.to_string())?;
        if meta.uid() != unsafe { libc::getuid() } || meta.mode() & 0o077 != 0 {
            return Err("XDG_RUNTIME_DIR must be owned by this UID and private (0700)".into());
        }
        // Linux resolves the directory fd before looking up the socket. This
        // also works with long test/runtime paths beyond sockaddr_un's limit.
        let base = PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(base.join(format!("{key}.lock")))
            .map_err(|e| e.to_string())?;
        let meta = file.metadata().map_err(|e| e.to_string())?;
        if !meta.is_file()
            || meta.uid() != unsafe { libc::getuid() }
            || meta.mode() & 0o077 != 0
            || meta.nlink() != 1
        {
            return Err("Unsafe singleton lock file".into());
        }
        let socket = base.join(format!("{key}.ready"));
        let start = Instant::now();
        loop {
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                match std::fs::symlink_metadata(&socket) {
                    Ok(m) if m.file_type().is_socket() && m.uid() == unsafe { libc::getuid() } => {
                        std::fs::remove_file(&socket).map_err(|e| e.to_string())?;
                    }
                    Ok(_) => return Err("Unsafe singleton readiness path".into()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.to_string()),
                }
                let listener = UnixListener::bind(&socket).map_err(|e| e.to_string())?;
                listener.set_nonblocking(true).map_err(|e| e.to_string())?;
                return Ok(Some(Self {
                    _directory: directory,
                    _lock: file,
                    listener,
                    socket,
                    confirmed: Arc::new(AtomicBool::new(false)),
                    stopping: Arc::new(AtomicBool::new(false)),
                }));
            }
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EWOULDBLOCK) {
                return Err(error.to_string());
            }
            let remaining = timeout.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                return Err("Existing locker did not confirm locking".into());
            }
            if let Ok(mut stream) = connect_ready(&socket) {
                stream
                    .set_read_timeout(Some(remaining))
                    .map_err(|e| e.to_string())?;
                let mut reply = [0; 6];
                if stream.read_exact(&mut reply).is_ok() && &reply == b"LOCKED" {
                    return Ok(None);
                }
                // A dead owner may have closed its connection. Retry the flock;
                // an abandoned compositor lock can be safely taken over.
            }
            std::thread::sleep(Duration::from_millis(20).min(remaining));
        }
    }

    /// Call only after daemonization: never fork with our worker running.
    pub fn serve(&self) -> Result<(), String> {
        let listener = self.listener.try_clone().map_err(|e| e.to_string())?;
        let confirmed = self.confirmed.clone();
        let stopping = self.stopping.clone();
        std::thread::Builder::new()
            .name("lock-readiness".into())
            .spawn(move || {
                while !stopping.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let start = Instant::now();
                            while !confirmed.load(Ordering::SeqCst)
                                && !stopping.load(Ordering::SeqCst)
                                && start.elapsed() < Duration::from_secs(30)
                            {
                                std::thread::sleep(Duration::from_millis(10));
                            }
                            if confirmed.load(Ordering::SeqCst) && !stopping.load(Ordering::SeqCst)
                            {
                                let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
                                let _ = stream.write_all(b"LOCKED");
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(_) => break,
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn connect_ready(path: &Path) -> std::io::Result<UnixStream> {
    // A blocking Unix connect can wait indefinitely when the owner's accept
    // backlog is full. A nonblocking attempt lets acquire_at enforce its own
    // deadline; a full backlog is retried like an owner that is still starting.
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    let bytes = path.as_os_str().as_bytes();
    if bytes.len() >= address.sun_path.len() {
        return Err(std::io::Error::from_raw_os_error(libc::ENAMETOOLONG));
    }
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (slot, byte) in address.sun_path.iter_mut().zip(bytes) {
        *slot = *byte as libc::c_char;
    }
    let fd = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
            0,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let stream = unsafe { UnixStream::from_raw_fd(fd) };
    if unsafe {
        libc::connect(
            fd,
            (&address as *const libc::sockaddr_un).cast(),
            std::mem::size_of_val(&address) as libc::socklen_t,
        )
    } < 0
    {
        return Err(std::io::Error::last_os_error());
    }
    stream.set_nonblocking(false)?;
    Ok(stream)
}

impl Drop for Instance {
    fn drop(&mut self) {
        self.confirmed.store(false, Ordering::SeqCst);
        self.stopping.store(true, Ordering::SeqCst);
        let _ = std::fs::remove_file(&self.socket);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn directory() -> tempfile::TempDir {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target")).unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        dir
    }
    #[test]
    fn contention_waits_for_live_confirmation_and_scopes_independently() {
        let dir = directory();
        let owner = Instance::acquire_at(dir.path(), "one", Duration::from_secs(1))
            .unwrap()
            .unwrap();
        owner.serve().unwrap();
        let path = dir.path().to_path_buf();
        let (tx, rx) = std::sync::mpsc::channel();
        let contender = std::thread::spawn(move || {
            tx.send(
                Instance::acquire_at(&path, "one", Duration::from_secs(2)).map(|v| v.is_none()),
            )
            .unwrap();
        });
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
        assert!(
            Instance::acquire_at(dir.path(), "two", Duration::from_secs(1))
                .unwrap()
                .is_some()
        );
        owner.confirmed.store(true, Ordering::SeqCst);
        assert!(rx.recv_timeout(Duration::from_secs(1)).unwrap().unwrap());
        contender.join().unwrap();
        drop(owner);
        assert!(
            Instance::acquire_at(dir.path(), "one", Duration::from_secs(1))
                .unwrap()
                .is_some()
        );
    }
    #[test]
    fn unsafe_files_are_rejected() {
        let dir = directory();
        std::os::unix::fs::symlink("missing", dir.path().join("one.lock")).unwrap();
        assert!(Instance::acquire_at(dir.path(), "one", Duration::from_millis(20)).is_err());
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(Instance::acquire_at(dir.path(), "two", Duration::from_millis(20)).is_err());
    }
    #[test]
    fn a_full_readiness_backlog_respects_the_deadline() {
        let dir = directory();
        let owner = Instance::acquire_at(dir.path(), "busy", Duration::from_secs(1))
            .unwrap()
            .unwrap();
        // Keep the saturation fixture small regardless of the host's somaxconn.
        assert_eq!(unsafe { libc::listen(owner.listener.as_raw_fd(), 1) }, 0);
        let mut peers = Vec::new();
        loop {
            match connect_ready(&owner.socket) {
                Ok(peer) => peers.push(peer),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => panic!("unexpected connect failure: {e}"),
            }
            assert!(peers.len() < 4096, "backlog did not fill");
        }
        let start = Instant::now();
        assert!(Instance::acquire_at(dir.path(), "busy", Duration::from_millis(40)).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
