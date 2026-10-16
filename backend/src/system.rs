use crate::resources::{Art, image_mime};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::Read,
    sync::mpsc::{self, Receiver, SyncSender},
    time::{Duration, Instant},
};
use zbus::{
    blocking::{Connection, Proxy},
    zvariant::OwnedValue,
};

pub enum Command {
    Power(String),
    Media(String),
    CycleLayout,
}
pub struct Update {
    pub capabilities: Value,
    pub media: Value,
    pub battery: Value,
}
fn proxy<'a>(
    connection: &'a Connection,
    name: &'a str,
    path: &'a str,
    interface: &'a str,
) -> zbus::Result<Proxy<'a>> {
    Proxy::new(connection, name, path, interface)
}
fn manager(c: &Connection) -> zbus::Result<Proxy<'_>> {
    proxy(
        c,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )
}
fn capabilities(c: Option<&Connection>, allow_power: bool) -> Value {
    let start = Instant::now();
    let can = |method| {
        if start.elapsed() >= Duration::from_secs(2) {
            return false;
        }
        c.and_then(|c| manager(c).ok())
            .and_then(|p| p.call::<_, _, String>(method, &()).ok())
            .is_some_and(|s| s == "yes")
    };
    json!({"suspend":can("CanSuspend"),"hibernate":can("CanHibernate"),"reboot":allow_power && can("CanReboot"),"powerOff":allow_power && can("CanPowerOff"),"media":false,"layouts":std::env::var_os("NIRI_SOCKET").is_some()})
}
fn battery(c: Option<&Connection>) -> Value {
    let get = || -> Option<Value> {
        let c = c?;
        let root = proxy(
            c,
            "org.freedesktop.UPower",
            "/org/freedesktop/UPower",
            "org.freedesktop.UPower",
        )
        .ok()?;
        let path: zbus::zvariant::OwnedObjectPath = root.call("GetDisplayDevice", &()).ok()?;
        let p = proxy(
            c,
            "org.freedesktop.UPower",
            path.as_str(),
            "org.freedesktop.UPower.Device",
        )
        .ok()?;
        if !p.get_property::<bool>("IsPresent").ok()? {
            return None;
        }
        let percent: f64 = p.get_property("Percentage").ok()?;
        let state: u32 = p.get_property("State").ok()?;
        Some(json!({"percent":percent.clamp(0.0,100.0),"charging": matches!(state,1|4|5)}))
    };
    json!({"type":"battery","battery":get()})
}
struct Player {
    bus: String,
    data: Value,
    art: String,
}
fn players(c: Option<&Connection>) -> Vec<Player> {
    let Some(c) = c else {
        return vec![];
    };
    let names = proxy(
        c,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .and_then(|p| p.call::<_, _, Vec<String>>("ListNames", &()))
    .unwrap_or_default();
    let start = Instant::now();
    names.into_iter().filter(|n| n.starts_with("org.mpris.MediaPlayer2.")).take(32).filter_map(|name| {
        if start.elapsed() >= Duration::from_secs(2) { return None; }
        let p = proxy(c,&name,"/org/mpris/MediaPlayer2","org.mpris.MediaPlayer2.Player").ok()?;
        let meta: HashMap<String,OwnedValue> = p.get_property("Metadata").ok()?;
        let string = |key: &str| meta.get(key).and_then(|v| <&str>::try_from(v).ok()).unwrap_or("").chars().take(if key=="mpris:artUrl" {8192} else {512}).collect::<String>();
        let artists = meta.get("xesam:artist").and_then(|v| v.try_clone().ok()).and_then(|v| Vec::<String>::try_from(v).ok()).unwrap_or_default().into_iter().take(8).map(|s|s.chars().take(128).collect::<String>()).collect::<Vec<_>>().join(", ");
        let identity = proxy(c,&name,"/org/mpris/MediaPlayer2","org.mpris.MediaPlayer2").and_then(|p| p.get_property::<String>("Identity")).unwrap_or_else(|_| name.trim_start_matches("org.mpris.MediaPlayer2.").into());
        let identity: String = identity.chars().take(128).collect();
        let playing = p.get_property::<String>("PlaybackStatus").ok()? == "Playing";
        let data = json!({"name":identity,"title":string("xesam:title"),"artist":artists,"album":string("xesam:album"),"playing":playing,"artUrl":null,"canNext":p.get_property::<bool>("CanGoNext").unwrap_or(false),"canPrevious":p.get_property::<bool>("CanGoPrevious").unwrap_or(false),"canPlayPause":p.get_property::<bool>("CanControl").unwrap_or(false) && p.get_property::<bool>(if playing {"CanPause"} else {"CanPlay"}).unwrap_or(false)});
        Some(Player { bus: name.clone(), data, art:string("mpris:artUrl") })
    }).collect()
}
fn fetch_art(uri: &str) -> Option<(Vec<u8>, String)> {
    const LIMIT: u64 = 4 * 1024 * 1024;
    if uri.len() > 8192 {
        return None;
    }
    let mut url = url::Url::parse(uri).ok()?;
    let mut bytes = vec![];
    match url.scheme() {
        "file" => {
            bytes = crate::resources::read_regular(&url.to_file_path().ok()?, LIMIT)?;
        }
        "https" | "http" => {
            let start = Instant::now();
            let mut final_response = None;
            for _ in 0..4 {
                if !art_url_allowed(&url) {
                    return None;
                }
                let remaining = Duration::from_secs(5).checked_sub(start.elapsed())?;
                let config = ureq::Agent::config_builder()
                    .timeout_global(Some(remaining))
                    .proxy(None)
                    .max_redirects(0)
                    .http_status_as_error(false)
                    .build();
                let agent = ureq::Agent::with_parts(
                    config,
                    ureq::unversioned::transport::DefaultConnector::default(),
                    PublicResolver,
                );
                let response = agent.get(url.as_str()).call().ok()?;
                if response.status().is_redirection() {
                    url = art_redirect(&url, response.headers().get("location")?.to_str().ok()?)?;
                } else if response.status().is_success() {
                    final_response = Some(response);
                    break;
                } else {
                    return None;
                }
            }
            let mut response = final_response?;
            if !response
                .headers()
                .get("content-type")?
                .to_str()
                .ok()?
                .to_ascii_lowercase()
                .starts_with("image/")
            {
                return None;
            }
            response
                .body_mut()
                .as_reader()
                .take(LIMIT + 1)
                .read_to_end(&mut bytes)
                .ok()?;
        }
        _ => return None,
    }
    if bytes.len() as u64 > LIMIT {
        return None;
    }
    let mime = image_mime(&bytes)?.to_string();
    Some((bytes, mime))
}
// Validate the addresses used by the connector, not an earlier DNS lookup
// which could be rebound between validation and connection. Redirects repeat
// this check, and proxy environment variables cannot route around it.
#[derive(Debug)]
struct PublicResolver;
impl ureq::unversioned::resolver::Resolver for PublicResolver {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: ureq::unversioned::transport::NextTimeout,
    ) -> Result<ureq::unversioned::resolver::ResolvedSocketAddrs, ureq::Error> {
        use ureq::unversioned::resolver::DefaultResolver;
        let addresses = DefaultResolver::default().resolve(uri, config, timeout)?;
        if addresses.iter().any(|a| !public_address(a.ip())) {
            return Err(ureq::Error::HostNotFound);
        }
        Ok(addresses)
    }
}

fn art_url_allowed(url: &url::Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && url.username().is_empty()
        && url.password().is_none()
        && match url.host() {
            Some(url::Host::Ipv4(ip)) => public_address(ip.into()),
            Some(url::Host::Ipv6(ip)) => public_address(ip.into()),
            Some(url::Host::Domain(_)) => true, // checked at actual DNS resolution
            None => false,
        }
}

fn art_redirect(current: &url::Url, location: &str) -> Option<url::Url> {
    let next = current.join(location).ok()?;
    (art_url_allowed(&next) && !(current.scheme() == "https" && next.scheme() != "https"))
        .then_some(next)
}

fn public_address(address: std::net::IpAddr) -> bool {
    match address {
        std::net::IpAddr::V4(ip) => !matches!(
            ip.octets(),
            [0 | 10 | 127, ..]
                | [100, 64..=127, ..]
                | [169, 254, ..]
                | [172, 16..=31, ..]
                | [192, 168, ..]
                | [192, 0, 0 | 2, _]
                | [192, 88, 99, _]
                | [198, 18..=19, ..]
                | [198, 51, 100, _]
                | [203, 0, 113, _]
                | [224..=255, ..]
        ),
        std::net::IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return public_address(v4.into());
            }
            let s = ip.segments();
            // Conservative global-unicast allowlist; exclude transition,
            // documentation and special-purpose allocations as well.
            s[0] & 0xe000 == 0x2000
                && s[0] != 0x2002
                && !(s[0] == 0x2001 && (s[1] < 0x0200 || s[1] == 0x0db8))
                && !(s[0] == 0x3fff && s[1] < 0x1000)
        }
    }
}

pub fn start(
    allow_power: bool,
    art: Art,
    isolated: bool,
) -> (SyncSender<Command>, Receiver<Update>) {
    start_with_connector(allow_power, art, isolated, move || {
        if isolated {
            #[cfg(feature = "mock-auth")]
            if std::env::var("RHINE_TEST_CONTROL").as_deref() == Ok("1")
                && let Ok(delay) = std::env::var("RHINE_TEST_MEDIA_DELAY_MS")
                && let Ok(delay) = delay.parse::<u64>()
            {
                std::thread::sleep(Duration::from_millis(delay.min(10_000)));
            }
            (None, None)
        } else {
            let system = zbus::blocking::connection::Builder::system()
                .ok()
                .and_then(|b| b.method_timeout(Duration::from_secs(2)).build().ok());
            let session = zbus::blocking::connection::Builder::session()
                .ok()
                .and_then(|b| b.method_timeout(Duration::from_secs(2)).build().ok());
            (system, session)
        }
    })
}
fn start_with_connector<F>(
    allow_power: bool,
    art: Art,
    isolated: bool,
    mut connect: F,
) -> (SyncSender<Command>, Receiver<Update>)
where
    F: FnMut() -> (Option<Connection>, Option<Connection>) + Send + 'static,
{
    let (tx, commands) = mpsc::sync_channel(32);
    let (updates, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let (mut system, mut session) = connect();
        let mut connected_at = Instant::now();
        let mut last_art = String::new();
        let mut art_version: u64 = 0;
        let mut history: HashMap<String, (Value, u64)> = HashMap::new();
        let mut sequence = 0u64;
        loop {
            if !isolated && connected_at.elapsed() >= Duration::from_secs(30) {
                // Retry absent or disconnected buses. Polling carries no
                // subscriptions that need to survive replacing a connection.
                (system, session) = connect();
                connected_at = Instant::now();
            }
            let mut caps = capabilities(system.as_ref(), allow_power);
            let mut available = players(session.as_ref());
            history.retain(|bus, _| available.iter().any(|p| &p.bus == bus));
            for p in &available {
                if history.get(&p.bus).is_none_or(|(data, _)| data != &p.data) {
                    sequence = sequence.saturating_add(1);
                    history.insert(p.bus.clone(), (p.data.clone(), sequence));
                }
            }
            available.sort_by_key(|p| {
                (
                    p.data["playing"].as_bool().unwrap_or(false),
                    history.get(&p.bus).map(|(_, n)| *n).unwrap_or(0),
                )
            });
            let mut selected = available.pop();
            caps["media"] = json!(selected.is_some());
            let uri = selected.as_ref().map(|p| p.art.clone()).unwrap_or_default();
            if uri != last_art {
                let fetched = fetch_art(&uri);
                if let Ok(mut cache) = art.lock() {
                    *cache = fetched;
                }
                last_art = uri;
                art_version = art_version.wrapping_add(1);
            }
            if let Some(p) = selected.as_mut()
                && art.lock().is_ok_and(|a| a.is_some())
            {
                p.data["artUrl"] = json!(format!("rhine://media-art?v={art_version}"));
            }
            let media = json!({"type":"media","player":selected.as_ref().map(|p| p.data.clone())});
            if matches!(
                updates.try_send(Update {
                    capabilities: caps.clone(),
                    media,
                    battery: battery(system.as_ref()),
                }),
                Err(mpsc::TrySendError::Disconnected(_))
            ) {
                break;
            }
            match commands.recv_timeout(Duration::from_secs(2)) {
                Ok(Command::Power(action)) if !isolated && caps[&action] == true => {
                    let method = match action.as_str() {
                        "suspend" => "Suspend",
                        "hibernate" => "Hibernate",
                        "reboot" => "Reboot",
                        "powerOff" => "PowerOff",
                        _ => continue,
                    };
                    if let Some(c) = system.as_ref()
                        && let Ok(p) = manager(c)
                        && let Err(e) = p.call::<_, _, ()>(method, &(false,))
                    {
                        eprintln!("Power action failed: {e}");
                    }
                }
                Ok(Command::Media(action)) if !isolated => {
                    if let (Some(c), Some(player)) = (session.as_ref(), selected.as_ref()) {
                        let (method, cap) = match action.as_str() {
                            "play-pause" => ("PlayPause", "canPlayPause"),
                            "next" => ("Next", "canNext"),
                            "previous" => ("Previous", "canPrevious"),
                            _ => continue,
                        };
                        if player.data[cap] == true
                            && let Ok(p) = proxy(
                                c,
                                &player.bus,
                                "/org/mpris/MediaPlayer2",
                                "org.mpris.MediaPlayer2.Player",
                            )
                        {
                            let _ = p.call::<_, _, ()>(method, &());
                        }
                    }
                }
                Ok(Command::CycleLayout) if !isolated && caps["layouts"] == true => {
                    let _ = crate::command::output(
                        "niri",
                        &["msg", "action", "switch-layout", "next"],
                        Duration::from_millis(500),
                        65536,
                    );
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                _ => {}
            }
        }
    });
    (tx, rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::BufRead,
        process::{Child, Command as Process, Stdio},
        sync::{Arc, Mutex, mpsc::Sender},
    };
    struct Bus(Child);
    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    struct Login {
        calls: Sender<String>,
    }
    #[zbus::interface(name = "org.freedesktop.login1.Manager")]
    impl Login {
        fn can_suspend(&self) -> &str {
            "yes"
        }
        fn can_hibernate(&self) -> &str {
            "challenge"
        }
        fn can_reboot(&self) -> &str {
            "yes"
        }
        fn can_power_off(&self) -> &str {
            "yes"
        }
        fn suspend(&self, interactive: bool) {
            assert!(!interactive);
            self.calls.send("Suspend".into()).expect("test receiver");
        }
        fn reboot(&self, _interactive: bool) {
            self.calls.send("Reboot".into()).expect("test receiver");
        }
    }
    struct Upower;
    #[zbus::interface(name = "org.freedesktop.UPower")]
    impl Upower {
        fn get_display_device(&self) -> zbus::zvariant::ObjectPath<'_> {
            zbus::zvariant::ObjectPath::try_from("/org/freedesktop/UPower/devices/DisplayDevice")
                .expect("path")
        }
    }
    struct Battery;
    #[zbus::interface(name = "org.freedesktop.UPower.Device")]
    impl Battery {
        #[zbus(property)]
        fn is_present(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn percentage(&self) -> f64 {
            72.5
        }
        #[zbus(property)]
        fn state(&self) -> u32 {
            1
        }
    }
    struct Identity;
    #[zbus::interface(name = "org.mpris.MediaPlayer2")]
    impl Identity {
        #[zbus(property)]
        fn identity(&self) -> &str {
            "Fixture player"
        }
    }
    struct Media {
        calls: Sender<String>,
        art: String,
    }
    #[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
    impl Media {
        #[zbus(property)]
        fn playback_status(&self) -> &str {
            "Playing"
        }
        #[zbus(property)]
        fn can_control(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn can_pause(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn can_play(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn can_go_next(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn can_go_previous(&self) -> bool {
            false
        }
        #[zbus(property)]
        fn metadata(&self) -> HashMap<String, OwnedValue> {
            [
                (
                    "xesam:title",
                    OwnedValue::from(zbus::zvariant::Str::from("Fixture title")),
                ),
                (
                    "mpris:artUrl",
                    OwnedValue::from(zbus::zvariant::Str::from(self.art.as_str())),
                ),
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), v))
            .collect()
        }
        fn next(&self) {
            self.calls.send("Next".into()).expect("test receiver");
        }
        fn previous(&self) {
            self.calls.send("Previous".into()).expect("test receiver");
        }
    }
    #[test]
    fn private_bus_integration() {
        let temp =
            tempfile::tempdir_in(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target"))
                .expect("temp");
        let art_path = temp.path().join("art.png");
        std::fs::write(&art_path, b"\x89PNG\r\n\x1a\nfixture").expect("art");
        let mut bus = Bus(Process::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1", "--address"])
            .arg(format!("unix:path={}", temp.path().join("bus").display()))
            .stdout(Stdio::piped())
            .spawn()
            .expect("private dbus"));
        let mut address = String::new();
        std::io::BufReader::new(bus.0.stdout.take().expect("pipe"))
            .read_line(&mut address)
            .expect("address");
        let address = address.trim();
        let (calls, received) = mpsc::channel();
        let _server = zbus::blocking::connection::Builder::address(address)
            .expect("address")
            .name("org.freedesktop.login1")
            .expect("name")
            .name("org.freedesktop.UPower")
            .expect("name")
            .name("org.mpris.MediaPlayer2.fixture")
            .expect("name")
            .serve_at(
                "/org/freedesktop/login1",
                Login {
                    calls: calls.clone(),
                },
            )
            .expect("login")
            .serve_at("/org/freedesktop/UPower", Upower)
            .expect("upower")
            .serve_at("/org/freedesktop/UPower/devices/DisplayDevice", Battery)
            .expect("battery")
            .serve_at("/org/mpris/MediaPlayer2", Identity)
            .expect("identity")
            .serve_at(
                "/org/mpris/MediaPlayer2",
                Media {
                    calls,
                    art: url::Url::from_file_path(&art_path)
                        .expect("url")
                        .to_string(),
                },
            )
            .expect("media")
            .build()
            .expect("server");
        let c = zbus::blocking::connection::Builder::address(address)
            .expect("address")
            .build()
            .expect("client");
        let art = Arc::new(Mutex::new(None));
        let cache = art.clone();
        let (commands, updates) = start_with_connector(false, art, false, move || {
            (Some(c.clone()), Some(c.clone()))
        });
        let update = updates
            .recv_timeout(Duration::from_secs(5))
            .expect("initial update");
        assert_eq!(update.capabilities["suspend"], true);
        assert_eq!(update.capabilities["hibernate"], false);
        assert_eq!(update.capabilities["reboot"], false);
        assert_eq!(update.capabilities["powerOff"], false);
        assert_eq!(update.media["player"]["title"], "Fixture title");
        assert_eq!(update.media["player"]["artUrl"], "rhine://media-art?v=1");
        assert_eq!(update.battery["battery"]["percent"], 72.5);
        assert_eq!(update.battery["battery"]["charging"], true);
        assert!(cache.lock().expect("cache").is_some());
        commands
            .send(Command::Power("suspend".into()))
            .expect("command");
        assert_eq!(
            received
                .recv_timeout(Duration::from_secs(5))
                .expect("suspend"),
            "Suspend"
        );
        commands
            .send(Command::Media("next".into()))
            .expect("command");
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).expect("next"),
            "Next"
        );
        commands
            .send(Command::Power("reboot".into()))
            .expect("command");
        commands
            .send(Command::Media("previous".into()))
            .expect("command");
        commands
            .send(Command::Media("next".into()))
            .expect("command");
        assert_eq!(
            received
                .recv_timeout(Duration::from_secs(5))
                .expect("only next allowed"),
            "Next"
        );
        assert!(received.try_recv().is_err());
        drop(commands);
        drop(updates);
    }
    #[test]
    fn art_limits_and_type_checks() {
        let temp =
            tempfile::tempdir_in(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target"))
                .expect("temp");
        let path = temp.path().join("art");
        let uri = url::Url::from_file_path(&path).expect("url").to_string();
        std::fs::write(&path, b"not an image").expect("fixture");
        assert!(fetch_art(&uri).is_none());
        let mut large = vec![0; 4 * 1024 * 1024 + 1];
        large[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        std::fs::write(&path, large).expect("fixture");
        assert!(fetch_art(&uri).is_none());
        assert!(fetch_art("ftp://invalid/art").is_none());
    }
    #[test]
    fn art_rejects_private_destinations_and_unsafe_redirects() {
        for ip in [
            "0.0.0.0",
            "127.0.0.1",
            "10.1.2.3",
            "169.254.169.254",
            "172.16.0.1",
            "192.168.1.2",
            "100.64.1.2",
            "192.0.2.1",
            "224.0.0.1",
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "::ffff:127.0.0.1",
            "2001:db8::1",
            "2002:7f00:1::",
        ] {
            assert!(!public_address(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
            assert!(public_address(ip.parse().unwrap()), "{ip}");
        }
        let current = url::Url::parse("https://art.example/cover").unwrap();
        assert!(art_redirect(&current, "/next").is_some());
        for next in [
            "http://art.example/next",
            "https://user:pass@art.example/next",
            "file:///tmp/image",
            "https://127.0.0.1/image",
            "https://[::ffff:127.0.0.1]/image",
        ] {
            assert!(art_redirect(&current, next).is_none(), "{next}");
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        assert!(
            fetch_art(&format!(
                "http://localhost:{}/image",
                listener.local_addr().unwrap().port()
            ))
            .is_none()
        );
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
    #[test]
    fn command_queue_never_blocks_the_ui_when_full() {
        let (release, gate) = mpsc::channel();
        let (commands, updates) =
            start_with_connector(false, Arc::new(Mutex::new(None)), true, move || {
                gate.recv().unwrap();
                (None, None)
            });
        for _ in 0..32 {
            assert!(commands.try_send(Command::CycleLayout).is_ok());
        }
        assert!(matches!(
            commands.try_send(Command::CycleLayout),
            Err(mpsc::TrySendError::Full(_))
        ));
        drop(commands);
        drop(updates);
        release.send(()).unwrap();
    }
}
