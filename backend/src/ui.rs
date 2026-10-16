use crate::{
    auth,
    config::{Args, Config},
    resources::{Resources, Screenshot},
    system,
};
use gtk::{gdk, gio, glib, prelude::*};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    fs::File,
    io::Write,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use webkit::prelude::*;
use zeroize::Zeroizing;

type Shared = Rc<App>;
fn auth_label(message: &Value) -> String {
    match message["state"].as_str().unwrap_or("idle") {
        "verifying" => "Verifying…".into(),
        "success" => "Unlocked".into(),
        "failure" => {
            let prefix = match message["stage"].as_str() {
                Some("account") => "Password accepted; account policy refused access.",
                Some("credentials") => "Password accepted; credential refresh was refused.",
                Some("start") => "Authentication service unavailable.",
                _ => "Authentication failed.",
            };
            let detail: String = message["message"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(512)
                .collect();
            format!("{prefix} {detail}")
        }
        _ => "Enter your password to unlock".into(),
    }
}
pub struct Output {
    pub window: gtk::Window,
    backdrop: gtk::Overlay,
    pub view: RefCell<Option<webkit::WebView>>,
    pub monitor: gdk::Monitor,
    pub name: String,
    pub hello: Cell<bool>,
    queue: RefCell<VecDeque<Value>>,
    crashes: RefCell<VecDeque<Instant>>,
    pub entry: RefCell<Option<gtk::PasswordEntry>>,
    pub status: RefCell<Option<gtk::Label>>,
    button: RefCell<Option<gtk::Button>>,
    generation: Cell<u64>,
    probe_started: Cell<Option<Instant>>,
    last_probe: Cell<Instant>,
    probe_cancel: RefCell<Option<gio::Cancellable>>,
}
impl Output {
    pub fn name(&self) -> String {
        self.monitor
            .connector()
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.name.clone())
    }
}
pub struct App {
    pub args: Args,
    config: Config,
    resources: Resources,
    context: webkit::WebContext,
    network: webkit::NetworkSession,
    pub lock: Option<session_lock::Instance>,
    pub outputs: RefCell<Vec<Rc<Output>>>,
    pub main_loop: glib::MainLoop,
    pub auth_state: RefCell<Value>,
    keyboard: RefCell<Value>,
    media: RefCell<Option<Value>>,
    battery: RefCell<Value>,
    capabilities: RefCell<Value>,
    auth_tx: mpsc::Sender<auth::Event>,
    system_tx: mpsc::SyncSender<system::Command>,
    user: String,
    real_name: String,
    primary: RefCell<String>,
    locked_at: Cell<u64>,
    pub locked: Cell<bool>,
    pub exiting: Cell<bool>,
    exit_code: Cell<i32>,
    confirmed: Option<Arc<AtomicBool>>,
}
fn epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn primary_output() -> String {
    if std::env::var_os("NIRI_SOCKET").is_some()
        && let Some(out) = crate::command::output(
            "niri",
            &["msg", "--json", "focused-output"],
            Duration::from_millis(300),
            65536,
        )
        && let Ok(v) = serde_json::from_slice::<Value>(&out)
    {
        return v["name"].as_str().unwrap_or("").into();
    }
    String::new()
}
pub fn run(
    args: Args,
    ready: Option<File>,
    confirmed: Option<Arc<AtomicBool>>,
) -> Result<i32, String> {
    let display = gdk::Display::default().ok_or("No display")?;
    if !args.preview && !session_lock::is_supported() {
        return Err("Compositor does not support ext-session-lock-v1".into());
    }
    let (user, real_name) = if args.is_mock() {
        ("test-user".into(), "Test Operator".into())
    } else {
        auth::identity()?
    };
    let config = Config::load();
    let mut resources = Resources::new(&args.frontend, &user);
    if args.is_mock() {
        resources.avatar = None;
    }
    let names: Vec<String> = (0..display.monitors().n_items())
        .filter_map(|i| display.monitors().item(i).and_downcast::<gdk::Monitor>())
        .filter_map(|m| m.connector().map(|n| n.to_string()))
        .collect();
    resources.screenshots = Rc::new(RefCell::new(
        crate::screenshot::capture(&names)
            .into_iter()
            .filter_map(|(name, png)| {
                let bytes = glib::Bytes::from_owned(png);
                gdk::Texture::from_bytes(&bytes)
                    .ok()
                    .map(|texture| (name, Screenshot { bytes, texture }))
            })
            .collect(),
    ));
    let context = webkit::WebContext::new();
    resources.install(&context);
    let network = webkit::NetworkSession::new_ephemeral();
    network.connect_download_started(|_, download| download.cancel());
    let (auth_tx, auth_rx) = mpsc::channel();
    let isolated = cfg!(feature = "mock-auth");
    let (system_tx, system_rx) =
        system::start(config.lock.allow_power, resources.art.clone(), isolated);
    let lock = if args.preview {
        None
    } else {
        Some(session_lock::Instance::new())
    };
    let app = Rc::new(App {
        args,
        config,
        resources,
        context,
        network,
        lock,
        outputs: RefCell::new(vec![]),
        main_loop: glib::MainLoop::new(None, false),
        auth_state: RefCell::new(json!({"type":"auth","state":"idle","attempts":0})),
        keyboard: RefCell::new(
            json!({"type":"keyboard","capsLock":false,"numLock":false,"layout":"","layouts":[]}),
        ),
        media: RefCell::new(None),
        battery: RefCell::new(json!({"type":"battery","battery":null})),
        capabilities: RefCell::new(
            json!({"suspend":false,"hibernate":false,"reboot":false,"powerOff":false,"media":false,"layouts":!isolated && std::env::var_os("NIRI_SOCKET").is_some()}),
        ),
        auth_tx,
        system_tx,
        user,
        real_name,
        primary: RefCell::new(primary_output()),
        locked_at: Cell::new(epoch()),
        locked: Cell::new(false),
        exiting: Cell::new(false),
        exit_code: Cell::new(0),
        confirmed,
    });
    let css = gtk::CssProvider::new();
    css.load_from_string("window {background:#e7e5e1;color:#111214;font-family:Montserrat,MiSans,sans-serif;} .fallback {padding:32px;} .fallback entry {min-width:260px;min-height:42px;background:#e7e5e1;color:#111214;border:2px solid #111214;} .fallback button {background:#ff7b00;color:#111214;min-height:42px;} .fallback entry:focus-within, .fallback button:focus-visible {outline:3px solid #b9e432;outline-offset:3px;box-shadow:none;}");
    gtk::style_context_add_provider_for_display(
        &display,
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    if let Some(keyboard) = display.default_seat().and_then(|s| s.keyboard()) {
        app.update_keyboard(&keyboard);
        let a = Rc::downgrade(&app);
        keyboard.connect_notify_local(None, move |keyboard, _| {
            if let Some(a) = a.upgrade() {
                a.update_keyboard(keyboard);
            }
        });
    }
    let a = Rc::downgrade(&app);
    glib::timeout_add_local(Duration::from_millis(20), move || {
        let Some(a) = a.upgrade() else {
            return glib::ControlFlow::Break;
        };
        while let Ok(event) = auth_rx.try_recv() {
            a.auth_event(event);
        }
        while let Ok(update) = system_rx.try_recv() {
            if *a.capabilities.borrow() != update.capabilities {
                *a.capabilities.borrow_mut() = update.capabilities;
                a.refresh_init();
            }
            if a.media.borrow().as_ref() != Some(&update.media) {
                a.broadcast(&update.media);
                *a.media.borrow_mut() = Some(update.media);
            }
            if *a.battery.borrow() != update.battery {
                a.broadcast(&update.battery);
                *a.battery.borrow_mut() = update.battery;
            }
        }
        a.check_frontends();
        glib::ControlFlow::Continue
    });
    let a = Rc::downgrade(&app);
    display
        .monitors()
        .connect_items_changed(move |monitors, _, _, _| {
            if let Some(a) = a.upgrade() {
                let current: Vec<gdk::Monitor> = (0..monitors.n_items())
                    .filter_map(|i| monitors.item(i).and_downcast::<gdk::Monitor>())
                    .collect();
                a.outputs.borrow_mut().retain(|o| {
                    let present = current.contains(&o.monitor);
                    if !present {
                        a.resources.screenshots.borrow_mut().remove(&o.name());
                    }
                    present
                });
                let primary_alive = a
                    .outputs
                    .borrow()
                    .iter()
                    .any(|o| o.name() == *a.primary.borrow());
                if !primary_alive {
                    *a.primary.borrow_mut() = a
                        .outputs
                        .borrow()
                        .first()
                        .map(|o| o.name())
                        .unwrap_or_default();
                }
                a.refresh_init();
            }
        });
    #[cfg(feature = "mock-auth")]
    crate::test_control::install(&app);
    // GTK's first native realization initializes the GPU renderer synchronously.
    // Pay that cold-start cost before requesting the lock, on an unmapped window.
    // No desktop surface is ever presented by this warm-up.
    let warmup = gtk::Window::new();
    WidgetExt::realize(&warmup);
    if let Some(renderer) = warmup.renderer() {
        let bounds = gtk::graphene::Rect::new(0.0, 0.0, 32.0, 32.0);
        let node = gtk::gsk::ColorNode::new(
            &gdk::RGBA::new(231.0 / 255.0, 229.0 / 255.0, 225.0 / 255.0, 1.0),
            &bounds,
        );
        let _ = renderer.render_texture(&node, Some(&bounds));
        for screenshot in app.resources.screenshots.borrow().values() {
            let node = gtk::gsk::TextureNode::new(&screenshot.texture, &bounds);
            let _ = renderer.render_texture(&node, Some(&bounds));
        }
    }
    warmup.destroy();
    if let Some(lock) = &app.lock {
        let a = Rc::downgrade(&app);
        lock.connect_monitor(move |_, monitor| {
            if let Some(a) = a.upgrade() {
                a.add_output(monitor);
            }
        });
        let a = Rc::downgrade(&app);
        let ready = RefCell::new(ready);
        lock.connect_locked(move |_| {
            if let Some(a) = a.upgrade() {
                a.locked.set(true);
                if let Some(confirmed) = &a.confirmed {
                    confirmed.store(true, Ordering::SeqCst);
                }
                a.locked_at.set(epoch());
                eprintln!("Session lock acquired");
                if let Some(mut fd) = ready.borrow_mut().take() {
                    let _ = fd.write_all(&[0]);
                }
                a.refresh_init();
            }
        });
        let a = Rc::downgrade(&app);
        lock.connect_failed(move |_| {
            if let Some(a) = a.upgrade() {
                eprintln!("Compositor refused session lock");
                a.exit_code.set(1);
                a.exiting.set(true);
                a.main_loop.quit();
            }
        });
        let a = Rc::downgrade(&app);
        lock.connect_unlocked(move |_| {
            if let Some(a) = a.upgrade() {
                a.locked.set(false);
                if let Some(confirmed) = &a.confirmed {
                    confirmed.store(false, Ordering::SeqCst);
                }
                a.main_loop.quit();
            }
        });
        lock.lock();
    } else {
        if let Some(monitor) = display.monitors().item(0).and_downcast::<gdk::Monitor>() {
            app.add_output(&monitor);
        } else {
            return Err("No monitors available".into());
        }
    }
    if !app.exiting.get() {
        app.main_loop.run();
    }
    Ok(app.exit_code.get())
}
impl App {
    fn check_frontends(self: &Shared) {
        let outputs = self.outputs.borrow().clone();
        for output in outputs {
            if !output.hello.get() {
                continue;
            }
            let Some(view) = output.view.borrow().clone() else {
                continue;
            };
            if let Some(started) = output.probe_started.get() {
                if started.elapsed() >= Duration::from_secs(8) {
                    self.fallback(&output, "Frontend stopped responding");
                }
                continue;
            }
            if output.last_probe.get().elapsed() < Duration::from_secs(2) {
                continue;
            }
            let now = Instant::now();
            output.last_probe.set(now);
            output.probe_started.set(Some(now));
            let cancel = gio::Cancellable::new();
            *output.probe_cancel.borrow_mut() = Some(cancel.clone());
            let a = Rc::downgrade(self);
            let o = Rc::downgrade(&output);
            let generation = output.generation.get();
            view.evaluate_javascript("true", None, None, Some(&cancel), move |result| {
                if let (Some(a), Some(o)) = (a.upgrade(), o.upgrade())
                    && o.generation.get() == generation
                    && o.view.borrow().is_some()
                {
                    o.probe_started.set(None);
                    o.probe_cancel.borrow_mut().take();
                    if result.is_err() && o.hello.get() {
                        a.fallback(&o, "Frontend responsiveness check failed");
                    }
                }
            });
        }
    }
    fn init(&self, output: &Output) -> Value {
        let geometry = output.monitor.geometry();
        let host = if self.args.is_mock() {
            "test-host".to_owned()
        } else {
            glib::host_name().to_string()
        };
        let session = if self.args.is_mock() {
            "headless-test".into()
        } else {
            std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default()
        };
        json!({"type":"init","protocol":1,"version":env!("CARGO_PKG_VERSION"),"output":{"name":output.name(),"width":geometry.width(),"height":geometry.height(),"scale":output.monitor.scale_factor(),"primary":self.is_primary(&output.name())},"outputs":self.outputs.borrow().len(),"user":{"name":self.user,"realName":self.real_name,"avatar":self.resources.avatar.as_ref().map(|_|"rhine://avatar")},"screenshot":self.resources.screenshot_uri(&output.name()),"host":host,"session":session,"lockedAt":self.locked_at.get(),"capabilities":*self.capabilities.borrow(),"config":self.config.frontend,"mode":if self.args.preview {"preview"} else {"lock"},"mock":cfg!(feature="mock-auth")})
    }
    fn refresh_init(&self) {
        for o in self.outputs.borrow().iter() {
            if o.hello.get() {
                self.send_now(o, &self.init(o));
            }
        }
    }
    fn send_now(&self, output: &Output, message: &Value) {
        if let Some(view) = output.view.borrow().as_ref() {
            let script = format!("window.__rhine && window.__rhine.receive({message})");
            view.evaluate_javascript(&script, None, None, gio::Cancellable::NONE, |result| {
                if let Err(e) = result {
                    eprintln!("Bridge delivery failed: {e}");
                }
            });
        }
    }
    pub fn broadcast(&self, message: &Value) {
        if self.args.debug {
            eprintln!("backend → {message}");
        }
        for output in self.outputs.borrow().iter() {
            if output.hello.get() {
                self.send_now(output, message);
            } else if output.view.borrow().is_some() {
                let mut queue = output.queue.borrow_mut();
                if queue.len() == 256 {
                    queue.pop_front();
                }
                queue.push_back(message.clone());
            }
            if message["type"] == "auth" {
                let state = message["state"].as_str().unwrap_or("idle");
                let enabled = !matches!(state, "verifying" | "success");
                if let Some(label) = output.status.borrow().as_ref() {
                    label.set_text(&auth_label(message));
                }
                if let Some(entry) = output.entry.borrow().as_ref() {
                    entry.set_sensitive(enabled);
                    if state == "failure" {
                        entry.set_text("");
                        entry.grab_focus();
                    }
                }
                if let Some(button) = output.button.borrow().as_ref() {
                    button.set_sensitive(enabled);
                }
            }
        }
    }
    pub fn message(self: &Shared, output: &Rc<Output>, message: Value) {
        let mut message = auth::SecretMessage(message);
        match message["type"].as_str().unwrap_or("") {
            "hello" => {
                if message["protocol"] != 1 {
                    self.fallback(output, "Unsupported frontend protocol");
                    return;
                }
                output.hello.set(true);
                self.send_now(output, &self.init(output));
                for pending in output.queue.borrow_mut().drain(..) {
                    self.send_now(output, &pending);
                }
                for state in [&self.auth_state, &self.keyboard, &self.battery] {
                    self.send_now(output, &state.borrow());
                }
                // Unknown is not an empty player list: optional music waits
                // for the first completed system poll, including slow art I/O.
                if let Some(media) = self.media.borrow().as_ref() {
                    self.send_now(output, media);
                }
                eprintln!("Frontend ready: {}", output.name());
            }
            "authenticate" => {
                if let Some(Value::String(password)) =
                    message.as_object_mut().and_then(|m| m.remove("password"))
                {
                    self.authenticate(Zeroizing::new(password));
                }
            }
            "unlock-ready" => {
                if self.auth_state.borrow()["state"] == "success" {
                    self.unlock();
                }
            }
            "power" => {
                if let Some(action) = message["action"].as_str()
                    && matches!(action, "suspend" | "hibernate" | "reboot" | "powerOff")
                    && self.capabilities.borrow()[action] == true
                {
                    let _ = self
                        .system_tx
                        .try_send(system::Command::Power(action.into()));
                }
            }
            "media" => {
                if let Some(action) = message["action"].as_str()
                    && matches!(action, "play-pause" | "next" | "previous")
                {
                    let _ = self
                        .system_tx
                        .try_send(system::Command::Media(action.into()));
                }
            }
            "cycle-layout" => {
                if self.capabilities.borrow()["layouts"] == true {
                    let _ = self.system_tx.try_send(system::Command::CycleLayout);
                }
            }
            "log" if self.args.debug => {
                eprintln!(
                    "frontend [{}]: {}",
                    message["level"].as_str().unwrap_or("info"),
                    message["message"].as_str().unwrap_or("")
                );
            }
            "log" => {}
            other => {
                if self.args.debug {
                    eprintln!("Ignored message type: {other}");
                }
            }
        }
    }
    pub fn authenticate(self: &Shared, password: Zeroizing<String>) {
        if matches!(
            self.auth_state.borrow()["state"].as_str(),
            Some("verifying" | "success")
        ) {
            return;
        }
        let attempts = self.auth_state.borrow()["attempts"].as_u64().unwrap_or(0);
        *self.auth_state.borrow_mut() =
            json!({"type":"auth","state":"verifying","attempts":attempts});
        self.broadcast(&self.auth_state.borrow());
        let tx = self.auth_tx.clone();
        let service = self.config.lock.pam_service.clone();
        #[cfg(feature = "mock-auth")]
        let mock = self.args.mock_password.clone();
        let worker = std::thread::Builder::new()
            .name("auth-supervisor".into())
            .spawn(move || {
                let start = Instant::now();
                let result = crate::auth_worker::verify(
                    &service,
                    password,
                    &tx,
                    #[cfg(feature = "mock-auth")]
                    mock,
                );
                auth::pad_failure(start, &result);
                let _ = tx.send(auth::Event::Done(result));
            });
        if worker.is_err() {
            let a = Rc::downgrade(self);
            glib::timeout_add_local_once(Duration::from_millis(1200), move || {
                if let Some(a) = a.upgrade() {
                    a.auth_event(auth::Event::Done(Err(auth::Failure::new(
                        auth::Stage::Start,
                        "Cannot start authentication worker",
                    ))));
                }
            });
        }
    }
    fn auth_event(self: &Shared, event: auth::Event) {
        match event {
            auth::Event::Pam { kind, text } => {
                self.broadcast(&json!({"type":"pam","kind":kind,"text":text}))
            }
            auth::Event::Done(result) => {
                let attempts = self.auth_state.borrow()["attempts"].as_u64().unwrap_or(0);
                let success = result.is_ok();
                *self.auth_state.borrow_mut() = match result {
                    Ok(()) => json!({"type":"auth","state":"success","attempts":attempts}),
                    Err(failure) => {
                        eprintln!(
                            "Authentication failed at {}: {}",
                            failure.stage.name(),
                            failure.message
                        );
                        json!({"type":"auth","state":"failure","attempts":attempts.saturating_add(1),"message":failure.message,"stage":failure.stage})
                    }
                };
                self.broadcast(&self.auth_state.borrow());
                if success {
                    let a = Rc::downgrade(self);
                    glib::timeout_add_local_once(Duration::from_millis(1600), move || {
                        if let Some(a) = a.upgrade() {
                            a.unlock();
                        }
                    });
                }
            }
        }
    }
    /// The focused output from niri if it is one of ours, else the first output —
    /// a stale or foreign name must never leave a lock without a primary.
    fn is_primary(&self, name: &str) -> bool {
        let outputs = self.outputs.borrow();
        let primary = self.primary.borrow();
        if outputs.iter().any(|o| o.name() == *primary) {
            name == *primary
        } else {
            outputs.first().is_none_or(|o| o.name() == name)
        }
    }
    fn unlock(&self) {
        if self.auth_state.borrow()["state"] != "success" || self.exiting.replace(true) {
            return;
        }
        self.broadcast(&json!({"type":"unlocking"}));
        if let Some(confirmed) = &self.confirmed {
            confirmed.store(false, Ordering::SeqCst);
        }
        if let Some(lock) = &self.lock {
            lock.unlock();
        }
        for output in self.outputs.borrow().iter() {
            output.window.destroy();
        }
        eprintln!("Session unlocked");
        self.main_loop.quit();
    }
    fn update_keyboard(&self, device: &gdk::Device) {
        let layouts: Vec<String> = device
            .layout_names()
            .iter()
            .map(|s| crate::keyboard::short_name(s))
            .collect();
        let layout = layouts
            .get(device.active_layout_index().max(0) as usize)
            .cloned()
            .unwrap_or_default();
        let state = json!({"type":"keyboard","capsLock":device.is_caps_locked(),"numLock":device.is_num_locked(),"layout":layout,"layouts":layouts});
        if *self.keyboard.borrow() != state {
            self.broadcast(&state);
            *self.keyboard.borrow_mut() = state;
        }
    }
    fn add_output(self: &Shared, monitor: &gdk::Monitor) {
        let name = monitor
            .connector()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("output-{}", self.outputs.borrow().len()));
        if self.outputs.borrow().iter().any(|o| o.name() == name) {
            return;
        }
        if self.primary.borrow().is_empty() {
            *self.primary.borrow_mut() = name.clone();
        }
        let window = gtk::Window::builder()
            .title("Rhine Lock")
            .default_width(if self.args.preview {
                1024
            } else {
                monitor.geometry().width()
            })
            .default_height(if self.args.preview {
                720
            } else {
                monitor.geometry().height()
            })
            .build();
        window.set_widget_name("rhine-lock-window");
        let output = Rc::new(Output {
            window,
            backdrop: gtk::Overlay::new(),
            monitor: monitor.clone(),
            name,
            view: RefCell::new(None),
            hello: Cell::new(false),
            queue: RefCell::new(VecDeque::new()),
            crashes: RefCell::new(VecDeque::new()),
            entry: RefCell::new(None),
            status: RefCell::new(None),
            button: RefCell::new(None),
            generation: Cell::new(0),
            probe_started: Cell::new(None),
            last_probe: Cell::new(Instant::now()),
            probe_cancel: RefCell::new(None),
        });
        self.outputs.borrow_mut().push(output.clone());
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let a = Rc::downgrade(self);
        let o = Rc::downgrade(&output);
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            if key == gdk::Key::Escape
                && modifiers
                    .contains(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK)
                && let (Some(a), Some(o)) = (a.upgrade(), o.upgrade())
            {
                a.fallback(&o, "Native input requested with Ctrl+Shift+Esc");
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        output.window.add_controller(keys);
        let a = Rc::downgrade(self);
        output.window.connect_close_request(move |_| {
            if let Some(a) = a.upgrade()
                && a.args.preview
            {
                a.main_loop.quit();
            }
            glib::Propagation::Stop
        });
        let backdrop = gtk::Picture::new();
        backdrop.set_content_fit(gtk::ContentFit::Fill);
        backdrop.set_can_shrink(true);
        if let Some(screenshot) = self.resources.screenshots.borrow().get(&output.name()) {
            backdrop.set_paintable(Some(&screenshot.texture));
        }
        output.backdrop.set_child(Some(&backdrop));
        output.window.set_child(Some(&output.backdrop));
        if let Some(lock) = &self.lock {
            lock.assign_window_to_monitor(&output.window, monitor);
        }
        if monitor.connector().is_some() {
            let a = Rc::downgrade(self);
            let o = Rc::downgrade(&output);
            output.window.add_tick_callback(move |window, clock| {
                let a = a.clone();
                let o = o.clone();
                let handler = Rc::new(RefCell::new(None));
                let saved = handler.clone();
                *handler.borrow_mut() = Some(clock.connect_after_paint(move |clock| {
                    if let Some(handler) = saved.borrow_mut().take() {
                        clock.disconnect(handler);
                    }
                    let a = a.clone();
                    let o = o.clone();
                    glib::idle_add_local_once(move || {
                        if let (Some(a), Some(o)) = (a.upgrade(), o.upgrade()) {
                            a.finish_output(&o);
                        }
                    });
                }));
                window.queue_draw();
                glib::ControlFlow::Break
            });
        } else {
            let a = Rc::downgrade(self);
            let o = Rc::downgrade(&output);
            monitor.connect_notify_local(Some("connector"), move |monitor, _| {
                if monitor.connector().is_some()
                    && let (Some(a), Some(o)) = (a.upgrade(), o.upgrade())
                    && o.view.borrow().is_none()
                    && o.entry.borrow().is_none()
                {
                    a.finish_output(&o);
                }
            });
            let a = Rc::downgrade(self);
            let o = Rc::downgrade(&output);
            glib::timeout_add_local_once(Duration::from_secs(2), move || {
                if let (Some(a), Some(o)) = (a.upgrade(), o.upgrade())
                    && o.view.borrow().is_none()
                    && o.entry.borrow().is_none()
                {
                    a.finish_output(&o);
                }
            });
        }
        output.window.present();
        if let Some(view) = output.view.borrow().as_ref() {
            view.grab_focus();
        }
        if let Some(entry) = output.entry.borrow().as_ref() {
            entry.grab_focus();
        }
        self.refresh_init();
        eprintln!("Lock window created: {}", output.name());
    }
    fn finish_output(self: &Shared, output: &Rc<Output>) {
        if output.view.borrow().is_some() || output.entry.borrow().is_some() {
            return;
        }
        if self.resources.root.is_some() && std::env::var_os("RHINE_REEXEC").is_none() {
            self.webview(output);
        } else {
            self.fallback(output, "Frontend files unavailable");
        }
        if let Some(view) = output.view.borrow().as_ref() {
            view.grab_focus();
        }
        if let Some(entry) = output.entry.borrow().as_ref() {
            entry.grab_focus();
        }
        self.refresh_init();
    }
    pub fn fallback(self: &Shared, output: &Rc<Output>, reason: &str) {
        if output.entry.borrow().is_some() {
            return;
        }
        eprintln!("Native fallback on {}: {reason}", output.name());
        output.hello.set(false);
        output
            .generation
            .set(output.generation.get().wrapping_add(1));
        output.probe_started.set(None);
        let cancel = output.probe_cancel.borrow_mut().take();
        if let Some(cancel) = cancel {
            cancel.cancel();
        }
        let view = output.view.borrow_mut().take();
        if let Some(view) = view {
            output.backdrop.remove_overlay(&view);
            view.terminate_web_process();
        }
        output.queue.borrow_mut().clear();
        let container = gtk::Box::new(gtk::Orientation::Vertical, 18);
        container.add_css_class("fallback");
        container.set_halign(gtk::Align::Center);
        container.set_valign(gtk::Align::Center);
        let title = gtk::Label::new(Some("RHINE LAB · Session locked"));
        let status = gtk::Label::new(Some("Enter your password to unlock"));
        status.set_wrap(true);
        let entry = gtk::PasswordEntry::builder().show_peek_icon(false).build();
        if let Some(delegate) = entry.delegate().and_downcast::<gtk::Text>() {
            // Keep one overflow character so the byte validator rejects rather
            // than authenticating a silently truncated overlong password.
            delegate.set_max_length((auth::MAX_PASSWORD_BYTES + 1) as i32);
        }
        entry.set_widget_name("unlock-password");
        entry.update_property(&[gtk::accessible::Property::Label("Password")]);
        let button = gtk::Button::with_label("Unlock");
        button.set_widget_name("unlock-submit");
        let a = Rc::downgrade(self);
        entry.connect_activate(move |entry| {
            let secret = auth::take_secret(entry.text());
            entry.set_text("");
            if let Some(a) = a.upgrade() {
                a.authenticate(secret);
            }
        });
        let clear_key = gtk::EventControllerKey::new();
        let weak = entry.downgrade();
        clear_key.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape
                && let Some(entry) = weak.upgrade()
            {
                entry.set_text("");
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        entry.add_controller(clear_key);
        let entry_weak = entry.downgrade();
        button.connect_clicked(move |_| {
            if let Some(entry) = entry_weak.upgrade() {
                entry.emit_activate();
            }
        });
        container.append(&title);
        container.append(&entry);
        container.append(&button);
        container.append(&status);
        *output.entry.borrow_mut() = Some(entry.clone());
        *output.status.borrow_mut() = Some(status);
        *output.button.borrow_mut() = Some(button);
        output.window.set_child(Some(&container));
        entry.grab_focus();
        self.broadcast(&self.auth_state.borrow());
    }
    fn webview(self: &Shared, output: &Rc<Output>) {
        let manager = webkit::UserContentManager::new();
        let settings = webkit::Settings::new();
        settings.set_enable_developer_extras(self.args.debug);
        settings.set_enable_webgl(true);
        settings.set_enable_webaudio(true);
        settings.set_hardware_acceleration_policy(webkit::HardwareAccelerationPolicy::Always);
        settings.set_media_playback_requires_user_gesture(false);
        settings.set_javascript_can_access_clipboard(false);
        settings.set_javascript_can_open_windows_automatically(false);
        settings.set_allow_file_access_from_file_urls(false);
        settings.set_allow_universal_access_from_file_urls(false);
        settings.set_enable_html5_database(false);
        settings.set_enable_html5_local_storage(false);
        settings.set_enable_webrtc(false);
        let policies = webkit::WebsitePolicies::builder()
            .autoplay(webkit::AutoplayPolicy::Allow)
            .build();
        let view = webkit::WebView::builder()
            .web_context(&self.context)
            .network_session(&self.network)
            .user_content_manager(&manager)
            .settings(&settings)
            .website_policies(&policies)
            .default_content_security_policy(crate::resources::CSP)
            .build();
        view.set_widget_name("lock-frontend");
        view.set_background_color(&gdk::RGBA::new(0.0, 0.0, 0.0, 0.0));
        let a = Rc::downgrade(self);
        let o = Rc::downgrade(output);
        manager.connect_script_message_received(Some("rhine"), move |_, value| {
            if let (Some(a), Some(o), Some(raw)) = (a.upgrade(), o.upgrade(), value.to_json(0)) {
                // JSC hands us a GLib-owned JSON buffer. Scrub that original buffer,
                // too, instead of copying it to an ordinary temporary String.
                use glib::translate::IntoGlibPtr;
                use zeroize::Zeroize;
                let len = raw.len();
                let ptr: *mut std::ffi::c_char = raw.into_glib_ptr();
                let message = unsafe {
                    let bytes = std::slice::from_raw_parts_mut(ptr.cast::<u8>(), len);
                    let parsed = if len <= 65536 {
                        serde_json::from_slice::<Value>(bytes).ok()
                    } else {
                        None
                    };
                    bytes.zeroize();
                    glib::ffi::g_free(ptr.cast());
                    parsed
                };
                if let Some(message) = message {
                    a.message(&o, message);
                }
            }
        });
        manager.register_script_message_handler("rhine", None);
        if self.args.debug {
            manager.add_script(&webkit::UserScript::new("for(const level of ['debug','info','warn','error','log']){const old=console[level];console[level]=(...args)=>{old.apply(console,args);window.webkit.messageHandlers.rhine.postMessage({type:'log',level,message:args.map(String).join(' ')})}};addEventListener('error',e=>console.error(e.message));addEventListener('unhandledrejection',e=>console.error(String(e.reason)));",webkit::UserContentInjectedFrames::TopFrame,webkit::UserScriptInjectionTime::Start,&[],&[]));
        }
        view.connect_context_menu(|_, _, _| true);
        view.connect_create(|_, _| None);
        view.connect_permission_request(|_, request| {
            request.deny();
            true
        });
        view.connect_run_file_chooser(|_, request| {
            request.cancel();
            true
        });
        view.connect_script_dialog(|_, dialog| {
            dialog.close();
            true
        });
        let uri = format!(
            "rhine://app/index.html?output={}",
            url::form_urlencoded::byte_serialize(output.name().as_bytes()).collect::<String>()
        );
        let allowed_uri = uri.clone();
        view.connect_decide_policy(move |_, decision, kind| {
            let allowed = kind == webkit::PolicyDecisionType::Response
                || (kind == webkit::PolicyDecisionType::NavigationAction
                    && decision
                        .downcast_ref::<webkit::NavigationPolicyDecision>()
                        .and_then(|p| p.navigation_action())
                        .is_some_and(|nav| {
                            matches!(
                                nav.navigation_type(),
                                webkit::NavigationType::Other | webkit::NavigationType::Reload
                            ) && nav
                                .request()
                                .and_then(|r| r.uri())
                                .is_some_and(|u| u == allowed_uri)
                        }));
            if allowed {
                decision.use_();
            } else {
                eprintln!("Blocked navigation ({kind:?})");
                decision.ignore();
            }
            true
        });
        let a = Rc::downgrade(self);
        let o = Rc::downgrade(output);
        view.connect_load_changed(move |_, event| {
            if event == webkit::LoadEvent::Started
                && let (Some(a), Some(o)) = (a.upgrade(), o.upgrade())
            {
                o.hello.set(false);
                o.generation.set(o.generation.get().wrapping_add(1));
                o.probe_started.set(None);
                let cancel = o.probe_cancel.borrow_mut().take();
                if let Some(cancel) = cancel {
                    cancel.cancel();
                }
                let generation = o.generation.get();
                let a = Rc::downgrade(&a);
                let o = Rc::downgrade(&o);
                glib::timeout_add_local_once(Duration::from_secs(15), move || {
                    if let (Some(a), Some(o)) = (a.upgrade(), o.upgrade())
                        && !o.hello.get()
                        && o.generation.get() == generation
                        && o.view.borrow().is_some()
                    {
                        a.fallback(&o, "Frontend handshake timed out");
                    }
                });
            }
        });
        let a = Rc::downgrade(self);
        let o = Rc::downgrade(output);
        view.connect_load_failed(move |_, _, _, error| {
            eprintln!("Page load failed: {error}");
            let a = a.clone();
            let o = o.clone();
            glib::idle_add_local_once(move || {
                if let (Some(a), Some(o)) = (a.upgrade(), o.upgrade()) {
                    a.fallback(&o, "Page load failed");
                }
            });
            true
        });
        let a = Rc::downgrade(self);
        let o = Rc::downgrade(output);
        view.connect_web_process_terminated(move |_, reason| {
            eprintln!("Web process terminated: {reason:?}");
            let a = a.clone();
            let o = o.clone();
            glib::idle_add_local_once(move || {
                if let (Some(a), Some(o)) = (a.upgrade(), o.upgrade()) {
                    if o.view.borrow().is_none() {
                        return;
                    }
                    let now = Instant::now();
                    {
                        let mut crashes = o.crashes.borrow_mut();
                        crashes.retain(|t| now.duration_since(*t) < Duration::from_secs(30));
                        crashes.push_back(now);
                    }
                    if o.crashes.borrow().len() >= 3 {
                        a.fallback(&o, "Repeated web process crashes");
                    } else {
                        o.hello.set(false);
                        if let Some(view) = o.view.borrow().as_ref() {
                            view.load_uri(&format!(
                                "rhine://app/index.html?output={}",
                                url::form_urlencoded::byte_serialize(o.name().as_bytes())
                                    .collect::<String>()
                            ));
                        }
                    }
                }
            });
        });
        view.connect_resource_load_started(|_, resource, _| {
            resource.connect_failed(|resource, error| {
                eprintln!(
                    "Resource load failed {}: {error}",
                    resource.uri().unwrap_or_default()
                );
            });
        });
        *output.view.borrow_mut() = Some(view.clone());
        output.backdrop.add_overlay(&view);
        view.set_hexpand(true);
        view.set_vexpand(true);
        view.load_uri(&uri);
    }
}
