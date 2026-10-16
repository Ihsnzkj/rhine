//! Best-effort, memory-only capture with one shared deadline for all outputs.
use std::{
    collections::HashMap,
    io::Read,
    os::fd::AsRawFd,
    process::{Child, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};

const BUDGET: Duration = Duration::from_millis(800);
// Bound memory even if a broken capture helper produces an endless stream.
const MAX_PNG: usize = 128 * 1024 * 1024;
struct Capture {
    name: String,
    child: Child,
    stdout: ChildStdout,
    bytes: Vec<u8>,
    eof: bool,
}
impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn capture(names: &[String]) -> HashMap<String, Vec<u8>> {
    capture_with(names, "grim", BUDGET)
}
fn capture_with(names: &[String], program: &str, budget: Duration) -> HashMap<String, Vec<u8>> {
    let start = Instant::now();
    // Leave room within the 800 ms envelope to kill and reap timed-out helpers.
    let budget = budget.saturating_sub(Duration::from_millis(25));
    let mut pending = Vec::new();
    let mut images = HashMap::new();
    for name in names {
        if start.elapsed() >= budget {
            break;
        }
        let Ok(mut child) = Command::new(program)
            .args(["-o", name, "-t", "png", "-l", "1", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        else {
            continue;
        };
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            continue;
        };
        // Drain every pipe without waiting for another output's encoder.
        if unsafe { libc::fcntl(stdout.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) } < 0 {
            let _ = child.kill();
            let _ = child.wait();
            continue;
        }
        pending.push(Capture {
            name: name.clone(),
            child,
            stdout,
            bytes: Vec::new(),
            eof: false,
        });
    }
    while !pending.is_empty() && start.elapsed() < budget {
        pending.retain_mut(|capture| {
            let mut buffer = [0u8; 65536];
            while !capture.eof && start.elapsed() < budget {
                match capture.stdout.read(&mut buffer) {
                    Ok(0) => capture.eof = true,
                    Ok(n) => {
                        if capture.bytes.len() + n > MAX_PNG {
                            return false;
                        }
                        capture.bytes.extend_from_slice(&buffer[..n]);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => return false,
                }
            }
            match capture.child.try_wait() {
                Ok(Some(status)) if capture.eof => {
                    if status.success() && capture.bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                        images.insert(capture.name.clone(), std::mem::take(&mut capture.bytes));
                    }
                    false
                }
                Err(_) => false,
                _ => true,
            }
        });
        if !pending.is_empty() {
            std::thread::sleep(
                Duration::from_millis(1).min(budget.saturating_sub(start.elapsed())),
            );
        }
    }
    // Kill/reap every timed-out helper; never leave a pipe reader or child behind.
    drop(pending);
    eprintln!(
        "Pre-lock capture: {}/{} outputs in {} ms",
        images.len(),
        names.len(),
        start.elapsed().as_millis()
    );
    images
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{os::unix::fs::PermissionsExt, path::Path};
    #[test]
    fn parallel_deadline_failure_and_reaping() {
        let dir = tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target"))
            .expect("temp");
        let helper = dir.path().join("grim");
        std::fs::write(&helper, "#!/bin/sh\ncase \"$2\" in\ngood) printf '\\211PNG\\r\\n\\032\\nfixture';;\nbad) exit 1;;\n*) exec /usr/bin/sleep 5;;\nesac\n").expect("helper");
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).expect("mode");
        let names = ["slow1", "good", "bad", "slow2"].map(str::to_string);
        let start = Instant::now();
        let images = capture_with(
            &names,
            helper.to_str().expect("path"),
            Duration::from_millis(100),
        );
        assert_eq!(images.len(), 1);
        assert!(images.contains_key("good"));
        assert!(
            start.elapsed() < Duration::from_millis(400),
            "one deadline, not one per output"
        );
        assert!(capture_with(&names, "/nonexistent/rhine-grim", BUDGET).is_empty());
    }
}
