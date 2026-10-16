use gtk::{gdk, gio, glib};
use std::{
    cell::RefCell,
    collections::HashMap,
    fs::{File, OpenOptions},
    io::Read,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    },
    path::{Component, Path, PathBuf},
    rc::Rc,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

const CONNECTOR: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');
pub const CSP: &str = "default-src rhine:; script-src rhine: 'unsafe-inline' 'wasm-unsafe-eval'; style-src rhine: 'unsafe-inline'; img-src rhine: data: blob:; media-src rhine: blob:; connect-src rhine:; font-src rhine: data:; worker-src blob: rhine:; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'";
pub type Art = Arc<Mutex<Option<(Vec<u8>, String)>>>;
pub struct Screenshot {
    pub bytes: glib::Bytes,
    pub texture: gdk::Texture,
}
#[derive(Clone)]
pub struct Resources {
    pub root: Option<PathBuf>,
    directory: Option<Arc<File>>,
    pub avatar: Option<PathBuf>,
    pub art: Art,
    pub screenshots: Rc<RefCell<HashMap<String, Screenshot>>>,
}
impl Resources {
    pub fn new(root: &Path, user: &str) -> Self {
        let mut root = root
            .canonicalize()
            .ok()
            .filter(|p| safe_path(p, "index.html").is_some());
        let directory = root
            .as_ref()
            .and_then(|p| {
                OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                    .open(p)
                    .ok()
            })
            .map(Arc::new);
        if directory.is_none() {
            root = None;
        }
        let mut choices = vec![PathBuf::from("/var/lib/AccountsService/icons").join(user)];
        if let Some(home) = std::env::var_os("HOME") {
            choices.push(PathBuf::from(&home).join(".face"));
            choices.push(PathBuf::from(home).join(".face.icon"));
        }
        Self {
            root,
            directory,
            avatar: choices.into_iter().find(|p| p.is_file()),
            art: Arc::new(Mutex::new(None)),
            screenshots: Rc::new(RefCell::new(HashMap::new())),
        }
    }
    pub fn screenshot_uri(&self, name: &str) -> Option<String> {
        self.screenshots.borrow().contains_key(name).then(|| {
            format!(
                "rhine://screenshot/{}",
                percent_encoding::utf8_percent_encode(name, CONNECTOR)
            )
        })
    }
    pub fn install(&self, context: &webkit::WebContext) {
        if let Some(security) = context.security_manager() {
            security.register_uri_scheme_as_secure("rhine");
            security.register_uri_scheme_as_cors_enabled("rhine");
        }
        let resources = self.clone();
        let files = FileService::new(
            self.root.clone(),
            self.directory.clone(),
            self.avatar.clone(),
        );
        context.register_uri_scheme("rhine", move |request| {
            let uri = request.uri().map(|s| s.to_string()).unwrap_or_default();
            if uri.len() > 8192 {
                finish(request, None);
                return;
            }
            let disk = url::Url::parse(&uri).ok().is_some_and(|u| {
                u.scheme() == "rhine" && matches!(u.host_str(), Some("app" | "avatar"))
            });
            if !disk {
                finish(request, resources.read_memory(&uri));
                return;
            }
            let (tx, rx) = mpsc::channel();
            if files.0.try_send((uri, tx)).is_err() {
                finish(request, None);
                return;
            }
            let request = request.clone();
            let start = Instant::now();
            glib::timeout_add_local(Duration::from_millis(5), move || match rx.try_recv() {
                Ok(content) => {
                    finish(
                        &request,
                        content.map(|(b, m)| (glib::Bytes::from_owned(b), m)),
                    );
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    finish(&request, None);
                    glib::ControlFlow::Break
                }
                Err(_) if start.elapsed() >= Duration::from_secs(3) => {
                    finish(&request, None);
                    glib::ControlFlow::Break
                }
                Err(_) => glib::ControlFlow::Continue,
            });
        });
    }
    fn read_memory(&self, uri: &str) -> Option<(glib::Bytes, String)> {
        let url = url::Url::parse(uri).ok()?;
        if url.scheme() != "rhine" {
            return None;
        }
        match url.host_str()? {
            "media-art" => self
                .art
                .lock()
                .ok()?
                .clone()
                .map(|(b, m)| (glib::Bytes::from_owned(b), m)),
            "screenshot" => {
                let name = percent_encoding::percent_decode_str(url.path().strip_prefix('/')?)
                    .decode_utf8()
                    .ok()?;
                self.screenshots
                    .borrow()
                    .get(name.as_ref())
                    .map(|s| (s.bytes.clone(), "image/png".into()))
            }
            _ => None,
        }
    }
}

fn finish(request: &webkit::URISchemeRequest, content: Option<(glib::Bytes, String)>) {
    let (data, mime, status) = match content {
        Some((bytes, mime)) => (bytes, mime, 200),
        None => (
            glib::Bytes::from_static(b"Not found"),
            "text/plain".into(),
            404,
        ),
    };
    let stream = gio::MemoryInputStream::from_bytes(&data);
    let response = webkit::URISchemeResponse::new(&stream, data.len() as i64);
    response.set_status(status, None);
    response.set_content_type(&mime);
    let headers = webkit::soup::MessageHeaders::new(webkit::soup::MessageHeadersType::Response);
    headers.append("Content-Type", &mime);
    headers.append("Content-Security-Policy", CSP);
    headers.append("X-Content-Type-Options", "nosniff");
    headers.append("Cache-Control", "no-store");
    headers.append("Access-Control-Allow-Origin", "rhine://app");
    response.set_http_headers(headers);
    request.finish_with_response(&response);
}

type FileContent = Option<(Vec<u8>, String)>;
type FileJob = (String, mpsc::Sender<FileContent>);
struct FileService(mpsc::SyncSender<FileJob>);
impl FileService {
    fn new(root: Option<PathBuf>, directory: Option<Arc<File>>, avatar: Option<PathBuf>) -> Self {
        let (tx, rx) = mpsc::sync_channel::<FileJob>(64);
        let rx = Arc::new(Mutex::new(rx));
        for _ in 0..2 {
            let rx = rx.clone();
            let root = root.clone();
            let directory = directory.clone();
            let avatar = avatar.clone();
            let _ = std::thread::Builder::new()
                .name("scheme-files".into())
                .spawn(move || {
                    loop {
                        let job = rx.lock().ok().and_then(|r| r.recv().ok());
                        let Some((uri, reply)) = job else {
                            break;
                        };
                        let content = (|| {
                            let url = url::Url::parse(&uri).ok()?;
                            match url.host_str()? {
                                "app" => {
                                    let raw = uri
                                        .strip_prefix("rhine://app/")?
                                        .split(['?', '#'])
                                        .next()?;
                                    let root = root.as_ref()?;
                                    let resolved = safe_path(root, raw)?;
                                    let file = open_contained(
                                        directory.as_ref()?,
                                        resolved.strip_prefix(root).ok()?,
                                    )?;
                                    Some((read_file(file, 64 * 1024 * 1024)?, mime(&resolved)))
                                }
                                "avatar" => {
                                    let bytes = read_regular(avatar.as_ref()?, 4 * 1024 * 1024)?;
                                    let mime = image_mime(&bytes)?.to_owned();
                                    Some((bytes, mime))
                                }
                                _ => None,
                            }
                        })();
                        let _ = reply.send(content);
                    }
                });
        }
        Self(tx)
    }
}

// Resolve ordinary in-root symlinks first, then walk the resulting relative
// path from the held root fd with NOFOLLOW at every component. A rename or
// symlink replacement between validation/open cannot escape that directory.
fn open_contained(root: &File, relative: &Path) -> Option<File> {
    let parts: Vec<_> = relative.components().collect();
    if parts.is_empty() {
        return None;
    }
    let mut directory = root.try_clone().ok()?;
    for (i, part) in parts.iter().enumerate() {
        let Component::Normal(name) = part else {
            return None;
        };
        let name = std::ffi::CString::new(name.as_bytes()).ok()?;
        let flags = libc::O_RDONLY
            | libc::O_NOFOLLOW
            | libc::O_CLOEXEC
            | libc::O_NONBLOCK
            | if i + 1 < parts.len() {
                libc::O_DIRECTORY
            } else {
                0
            };
        let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return None;
        }
        directory = unsafe { File::from_raw_fd(fd) };
    }
    Some(directory)
}

fn read_file(file: File, limit: u64) -> Option<Vec<u8>> {
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > limit {
        return None;
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(limit + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= limit).then_some(bytes)
}

pub fn read_regular(path: &Path, limit: u64) -> Option<Vec<u8>> {
    let path = path.canonicalize().ok()?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .ok()?;
    read_file(file, limit)
}
pub fn safe_path(root: &Path, encoded: &str) -> Option<PathBuf> {
    let decoded = percent_encoding::percent_decode_str(encoded)
        .decode_utf8()
        .ok()?;
    let relative = Path::new(decoded.as_ref());
    if decoded.contains('\0')
        || decoded.contains('\\')
        || relative
            .components()
            .any(|p| !matches!(p, Component::Normal(_)))
    {
        return None;
    }
    let resolved = root.join(relative).canonicalize().ok()?;
    (resolved.starts_with(root) && resolved.is_file()).then_some(resolved)
}
pub fn mime(path: &Path) -> String {
    match path.extension().and_then(|s| s.to_str()).unwrap_or("") {
        "js" | "mjs" => "text/javascript".into(),
        "opus" => "audio/ogg".into(),
        "hdr" => "application/octet-stream".into(),
        _ => mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string(),
    }
}
pub fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("image/webp")
    } else {
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_and_mimes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/stub-frontend")
            .canonicalize()
            .expect("fixture");
        assert!(safe_path(&root, "index.html").is_some());
        for path in [
            "../index.html",
            "%2e%2e/Cargo.toml",
            "/etc/passwd",
            "%2fetc/passwd",
            "foo\\bar",
            "index.html%00",
        ] {
            assert!(safe_path(&root, path).is_none(), "{path}");
        }
        assert_eq!(mime(Path::new("a.js")), "text/javascript");
        assert_eq!(mime(Path::new("a.opus")), "audio/ogg");
        assert_eq!(mime(Path::new("a.wasm")), "application/wasm");
    }
    #[test]
    fn refuses_symlinks_outside_root() {
        let temp = tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target"))
            .expect("temp");
        let root = temp.path().join("root");
        std::fs::create_dir(&root).expect("root");
        std::fs::write(temp.path().join("secret"), "outside").expect("fixture");
        std::os::unix::fs::symlink(temp.path().join("secret"), root.join("escape"))
            .expect("symlink");
        assert!(safe_path(&root, "escape").is_none());
        std::fs::write(root.join("index.html"), "inside").expect("fixture");
        std::os::unix::fs::symlink(root.join("index.html"), root.join("valid")).expect("symlink");
        assert!(safe_path(&root, "valid").is_some());
    }
    #[test]
    fn descriptor_walk_rejects_replaced_ancestors_and_special_files() {
        let temp =
            tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target")).unwrap();
        let root = temp.path().join("root");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/index"), b"inside").unwrap();
        let directory = File::open(&root).unwrap();
        let resolved = safe_path(&root, "sub/index").unwrap();
        std::fs::rename(root.join("sub"), temp.path().join("outside")).unwrap();
        std::os::unix::fs::symlink(temp.path().join("outside"), root.join("sub")).unwrap();
        assert!(open_contained(&directory, resolved.strip_prefix(&root).unwrap()).is_none());
        let fifo = temp.path().join("fifo");
        let raw = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(raw.as_ptr(), 0o600) }, 0);
        let started = Instant::now();
        assert!(read_regular(&fifo, 4096).is_none());
        assert!(started.elapsed() < Duration::from_secs(1));
        let large = temp.path().join("large");
        File::create(&large).unwrap().set_len(4097).unwrap();
        assert!(read_regular(&large, 4096).is_none());
    }
}
