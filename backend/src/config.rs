use serde::Deserialize;
use serde_json::Value;
use std::{
    env,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LockConfig {
    pub pam_service: String,
    pub allow_power: bool,
}
impl Default for LockConfig {
    fn default() -> Self {
        Self {
            pam_service: "rhine-lock".into(),
            allow_power: true,
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub lock: LockConfig,
    pub frontend: serde_json::Map<String, Value>,
}
impl Config {
    fn conservative_defaults() -> Self {
        Self {
            lock: LockConfig {
                allow_power: false,
                ..LockConfig::default()
            },
            ..Self::default()
        }
    }
    fn parse(text: &str) -> Result<Self, ()> {
        let config: Self = toml::from_str(text).map_err(|_| ())?;
        if !valid_service(&config.lock.pam_service) {
            return Err(());
        }
        Ok(config)
    }
    pub fn load() -> Self {
        let base = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")));
        let path = base
            .map(|p| p.join("rhine-lock/config.toml"))
            .filter(|p| p.try_exists().unwrap_or(true))
            .unwrap_or_else(|| PathBuf::from("/etc/rhine-lock/config.toml"));
        match std::fs::read_to_string(path) {
            Ok(s) => Self::parse(&s).unwrap_or_else(|()| {
                eprintln!(
                    "Invalid config; using default PAM service with reboot/power-off disabled"
                );
                Self::conservative_defaults()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(_) => {
                eprintln!(
                    "Cannot read config; using default PAM service with reboot/power-off disabled"
                );
                Self::conservative_defaults()
            }
        }
    }
}
pub fn valid_service(service: &str) -> bool {
    !service.is_empty()
        && service.len() <= 128
        && service.as_bytes()[0].is_ascii_alphanumeric()
        && service
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}
#[derive(Default)]
pub struct Args {
    pub auth_worker: bool,
    pub check_pam_worker: bool,
    pub preview: bool,
    pub daemonize: bool,
    pub debug: bool,
    pub check_pam: bool,
    pub frontend: PathBuf,
    #[cfg(feature = "mock-auth")]
    pub mock_password: Option<zeroize::Zeroizing<String>>,
}
impl Args {
    pub fn is_mock(&self) -> bool {
        #[cfg(feature = "mock-auth")]
        {
            self.mock_password.is_some()
        }
        #[cfg(not(feature = "mock-auth"))]
        {
            false
        }
    }
    pub fn parse() -> Result<Self, String> {
        let mut args = Self::default();
        let mut input = env::args().skip(1);
        while let Some(arg) = input.next() {
            match arg.as_str() {
                "--auth-worker" => args.auth_worker = true,
                "--check-pam-worker" => args.check_pam_worker = true,
                "--preview" => args.preview = true,
                "--daemonize" => args.daemonize = true,
                "--debug" => args.debug = true,
                "--check-pam" => args.check_pam = true,
                "--frontend" => {
                    args.frontend = input.next().ok_or("--frontend needs a directory")?.into()
                }
                "--version" => {
                    println!("rhine-lock {}", env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                "--help" | "-h" => {
                    println!(
                        "rhine-lock [--preview] [--daemonize] [--frontend DIR] [--debug] [--version] [--check-pam]"
                    );
                    std::process::exit(0);
                }
                #[cfg(feature = "mock-auth")]
                "--mock-password" => {
                    args.mock_password = Some(zeroize::Zeroizing::new(
                        input.next().ok_or("--mock-password needs a value")?,
                    ))
                }
                _ => return Err(format!("Unknown option: {arg}")),
            }
        }
        if args.preview && args.daemonize {
            return Err("--preview and --daemonize cannot be combined".into());
        }
        if args.frontend.as_os_str().is_empty() {
            args.frontend = default_frontend();
        }
        Ok(args)
    }
}
fn default_frontend() -> PathBuf {
    if let Some(p) = env::var_os("RHINE_LOCK_FRONTEND") {
        return p.into();
    }
    find_frontend(
        env::current_exe().ok().as_deref(),
        Path::new("/usr/share/rhine-lock/frontend"),
    )
}
fn find_frontend(exe: Option<&Path>, system: &Path) -> PathBuf {
    let mut dirs = Vec::new();
    if let Some(exe) = exe
        && let Some(parent) = exe.parent()
    {
        dirs.push(parent.join("../share/rhine-lock/frontend"));
    }
    dirs.push(system.to_path_buf());
    if let Some(exe) = exe
        && let Some(repo) = exe.ancestors().nth(4)
    {
        dirs.push(repo.join("frontend/dist"));
    }
    dirs.into_iter()
        .find(|p| p.join("index.html").is_file())
        .unwrap_or_else(|| system.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_passthrough() {
        let c: Config = toml::from_str("[lock]\nallow_power=false\n[frontend]\nmusic=true\nmusic_volume=0.5\n[frontend.custom]\nx='y'").expect("fixture");
        assert!(!c.lock.allow_power);
        assert_eq!(c.lock.pam_service, "rhine-lock");
        assert_eq!(c.frontend["custom"]["x"], "y");
    }
    #[test]
    fn invalid_security_config_is_conservative() {
        for text in [
            "[lock]\nallow_power='no'",
            "[lock]\nallow_powre=false",
            "[lock]\npam_service='../other'",
            "[lock]\npam_service=''",
            "[frontend]\nbroken=[",
        ] {
            assert!(Config::parse(text).is_err());
        }
        assert!(!Config::conservative_defaults().lock.allow_power);
        assert!(valid_service("rhine-lock"));
        assert!(!valid_service("/etc/pam.d/login"));
    }
    #[test]
    fn executable_install_wins_over_an_older_system_frontend() {
        let dir =
            tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target")).unwrap();
        let prefix = dir.path().join("prefix");
        let local = prefix.join("share/rhine-lock/frontend");
        let system = dir.path().join("system");
        std::fs::create_dir_all(prefix.join("bin")).unwrap();
        for root in [&local, &system] {
            std::fs::create_dir_all(root).unwrap();
            std::fs::write(root.join("index.html"), b"fixture").unwrap();
        }
        let exe = prefix.join("bin/rhine-lock");
        assert_eq!(
            find_frontend(Some(&exe), &system).canonicalize().unwrap(),
            local
        );
        std::fs::remove_file(local.join("index.html")).unwrap();
        assert_eq!(find_frontend(Some(&exe), &system), system);
    }
}
