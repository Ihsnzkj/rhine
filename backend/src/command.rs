//! Small, bounded best-effort OS helpers. Never wait indefinitely on niri.
use std::{
    io::Read,
    os::{fd::AsRawFd, unix::process::CommandExt},
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

pub fn output(program: &str, args: &[&str], budget: Duration, limit: usize) -> Option<Vec<u8>> {
    completed_output(program, args, budget, limit)
        .and_then(|(status, bytes)| status.success().then_some(bytes))
}

pub fn completed_output(
    program: &str,
    args: &[&str],
    budget: Duration,
    limit: usize,
) -> Option<(ExitStatus, Vec<u8>)> {
    let parent = unsafe { libc::getpid() };
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0);
    // Only async-signal-safe syscalls between fork and exec. Helpers must not
    // outlive a locker that exits while their supervising thread is running.
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::getppid() != parent {
                libc::_exit(1);
            }
            Ok(())
        });
    }
    let mut child = command.spawn().ok()?;
    let result = (|| {
        let mut stdout = child.stdout.take()?;
        if unsafe { libc::fcntl(stdout.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) } < 0 {
            return None;
        }
        let start = Instant::now();
        let mut bytes = Vec::new();
        let mut eof = false;
        while start.elapsed() < budget {
            let mut buffer = [0; 4096];
            loop {
                if start.elapsed() >= budget {
                    return None;
                }
                match stdout.read(&mut buffer) {
                    Ok(0) => {
                        eof = true;
                        break;
                    }
                    Ok(n) if bytes.len() + n <= limit => bytes.extend_from_slice(&buffer[..n]),
                    Ok(_) => return None,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => return None,
                }
            }
            if exited_without_reaping(child.id()).ok()? && eof {
                return Some(bytes);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        None
    })();
    let status = kill_and_reap(&mut child).ok()?;
    result.map(|bytes| (status, bytes))
}

/// Keep the process-group leader reserved until its descendants are killed.
/// Child::try_wait reaps it, losing the group on early exit and risking PID reuse.
pub fn exited_without_reaping(pid: u32) -> std::io::Result<bool> {
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            pid,
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if result < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(unsafe { info.si_pid() } != 0)
    }
}

pub fn kill_and_reap(child: &mut Child) -> std::io::Result<ExitStatus> {
    // The unreaped leader still owns this PID, even if it exited before the
    // deadline while a descendant kept stdout open.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    child.wait()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_output_and_deadline() {
        assert_eq!(
            output("/bin/echo", &["fixture"], Duration::from_secs(1), 64).unwrap(),
            b"fixture\n"
        );
        assert!(output("/bin/echo", &["fixture"], Duration::from_secs(1), 2).is_none());
        let start = Instant::now();
        assert!(output("/bin/sleep", &["5"], Duration::from_millis(30), 64).is_none());
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn exited_leader_does_not_leave_a_running_descendant() {
        let dir =
            tempfile::tempdir_in(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target"))
                .unwrap();
        let pid_file = dir.path().join("descendant");
        assert!(
            output(
                "/bin/sh",
                &[
                    "-c",
                    "sleep 30 & echo $! > \"$1\"",
                    "fixture",
                    pid_file.to_str().unwrap()
                ],
                Duration::from_millis(100),
                64,
            )
            .is_none()
        );
        let pid: i32 = std::fs::read_to_string(pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let proc_stat = format!("/proc/{pid}/stat");
        let running = || {
            std::fs::read_to_string(&proc_stat)
                .is_ok_and(|s| s.split_whitespace().nth(2) != Some("Z"))
        };
        let start = Instant::now();
        while running() && start.elapsed() < Duration::from_secs(1) {
            std::thread::sleep(Duration::from_millis(5));
        }
        let leaked = running();
        if leaked {
            // Clean up the exact fixture before reporting the regression.
            unsafe {
                libc::kill(pid, libc::SIGKILL);
            }
        }
        assert!(
            !leaked,
            "helper leader exited but its descendant survived the deadline"
        );
    }
}
