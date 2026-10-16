//! Stdio JSON control is compiled only into mock-auth builds and requires an explicit mock password.
//! It shares the UI command path. No listening sockets, production D-Bus, PAM or shell access.
use crate::ui::App;
use gtk::{gio, glib, prelude::*};
use serde_json::{Value, json};
use std::{
    io::{BufRead, Write},
    rc::Rc,
    sync::mpsc,
    time::Duration,
};
use webkit::prelude::*;
use zeroize::Zeroizing;
fn reply(request: &Value, result: Result<Value, String>) {
    let response = match result {
        Ok(data) => {
            json!({"protocol":1,"id":request["id"],"op":request["op"],"ok":true,"code":"OK","message":"Completed","data":data})
        }
        Err(message) => {
            json!({"protocol":1,"id":request["id"],"op":request["op"],"ok":false,"code":"FAILED","message":message})
        }
    };
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{response}");
    let _ = stdout.flush();
}
pub fn install(app: &Rc<App>) {
    if std::env::var("RHINE_TEST_CONTROL").as_deref() != Ok("1") || app.args.mock_password.is_none()
    {
        return;
    }
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        // Dedicated inherited stdin/stdout belong exclusively to the harness.
        let mut input = std::io::stdin().lock();
        loop {
            let mut bytes = Vec::new();
            match input.read_until(b'\n', &mut bytes) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if bytes.len() > 65536 {
                        break;
                    }
                    let bytes = Zeroizing::new(bytes);
                    if let Ok(value) = serde_json::from_slice::<Value>(&bytes)
                        && tx.send(value).is_err()
                    {
                        break;
                    }
                }
            }
        }
    });
    let app = Rc::downgrade(app);
    glib::timeout_add_local(Duration::from_millis(10), move || {
        let Some(app) = app.upgrade() else {
            return glib::ControlFlow::Break;
        };
        while let Ok(mut request) = rx.try_recv() {
            if request["protocol"] != 1 {
                reply(&request, Err("Protocol must be 1".into()));
                continue;
            }
            let index = request["params"]["output"].as_u64().unwrap_or(0) as usize;
            let output = app.outputs.borrow().get(index).cloned();
            let result = match request["op"].as_str().unwrap_or("") {
                "health" | "get_state" => Ok(
                    json!({"dumpable":unsafe {libc::prctl(libc::PR_GET_DUMPABLE)},"locked":app.lock.as_ref().is_some_and(|l|l.is_locked()),"auth":*app.auth_state.borrow(),"pid":std::process::id(),"windows":app.outputs.borrow().iter().map(|o|json!({"name":o.name(),"ready":o.hello.get(),"fallback":o.entry.borrow().is_some(),"status":o.status.borrow().as_ref().map(|s|s.text().to_string()),"passwordBytes":o.entry.borrow().as_ref().map(|e|crate::auth::take_secret(e.text()).len()),"width":o.window.width(),"height":o.window.height(),"focused":GtkWindowExt::focus(&o.window).map(|w|w.widget_name().to_string())})).collect::<Vec<_>>()}),
                ),
                "evaluate" => {
                    if let Some(view) = output.and_then(|o| o.view.borrow().clone()) {
                        let script = request["params"]["script"]
                            .as_str()
                            .unwrap_or("")
                            .to_string();
                        view.evaluate_javascript(
                            &script,
                            None,
                            None,
                            gio::Cancellable::NONE,
                            move |result| {
                                reply(
                                    &request,
                                    result
                                        .map(|v| {
                                            v.to_json(0)
                                                .and_then(|s| serde_json::from_str(&s).ok())
                                                .unwrap_or(Value::Null)
                                        })
                                        .map_err(|e| e.to_string()),
                                );
                            },
                        );
                        continue;
                    } else {
                        Err("No WebView".into())
                    }
                }
                "reload" | "crash_web" => {
                    if let Some(view) = output.and_then(|o| o.view.borrow().clone()) {
                        if request["op"] == "reload" {
                            view.reload();
                        } else {
                            view.terminate_web_process();
                        }
                        Ok(Value::Null)
                    } else {
                        Err("No WebView".into())
                    }
                }
                "set_value" => {
                    if request["params"]["control"] != "unlock-password" {
                        Err("Unknown control".into())
                    } else if let Some(entry) = output.and_then(|o| o.entry.borrow().clone()) {
                        if let Some(Value::String(value)) = request["params"]
                            .as_object_mut()
                            .and_then(|p| p.remove("value"))
                        {
                            let value = Zeroizing::new(value);
                            entry.set_text(&value);
                            Ok(Value::Null)
                        } else {
                            Err("Missing value".into())
                        }
                    } else {
                        Err("No native password entry".into())
                    }
                }
                "invoke_action" => {
                    if request["params"]["control"] != "unlock-submit" {
                        Err("Unknown control".into())
                    } else if let Some(entry) = output.and_then(|o| o.entry.borrow().clone()) {
                        entry.emit_activate();
                        Ok(Value::Null)
                    } else {
                        Err("No native password entry".into())
                    }
                }
                "close_window" => {
                    if let Some(o) = output {
                        o.window.close();
                        Ok(Value::Null)
                    } else {
                        Err("No window".into())
                    }
                }
                "panic" => panic!("mock-only recovery probe"),
                _ => Err("Unknown operation".into()),
            };
            reply(&request, result);
        }
        glib::ControlFlow::Continue
    });
}
