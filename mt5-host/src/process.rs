//! Linux primitives — `/proc`, signals, children — and the reason a terminal
//! host can be a headless container at all, replacing the Windows/Wine-desktop
//! machine this deployment used to assume: every fact the supervisor needs
//! (established loopback connections, resident memory, descendants, liveness) is
//! readable from the kernel instead of from a window.
//! Process and Linux introspection helpers.
//!
//! Everything the supervisor needs to observe the container comes from `std`
//! plus `/proc`: no shell, no extra tooling beyond the two commands the
//! Dockerfile installs on purpose (`curl` to fetch the installer/prefix
//! archive, `tar` to unpack it).

use std::collections::HashSet;
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum ProcError {
    Spawn { what: String, detail: String },
    Timeout { what: String, secs: u64 },
    Failed { what: String, code: Option<i32> },
}

impl fmt::Display for ProcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProcError::Spawn { what, detail } => write!(f, "cannot start {what}: {detail}"),
            ProcError::Timeout { what, secs } => {
                write!(f, "{what} did not finish within {secs}s")
            }
            ProcError::Failed { what, code } => match code {
                Some(code) => write!(f, "{what} failed with exit code {code}"),
                None => write!(f, "{what} was killed by a signal"),
            },
        }
    }
}

impl std::error::Error for ProcError {}

/// Is `bin` on `PATH` (or an absolute path that exists)?
pub fn which(bin: &str) -> Option<PathBuf> {
    let candidate = Path::new(bin);
    if candidate.is_absolute() {
        return candidate.exists().then(|| candidate.to_path_buf());
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(bin))
        .find(|full| full.is_file())
}

/// Run a command to completion, inheriting stdio so its output lands in the
/// container log, and kill it if it overruns `timeout`.
pub fn run(what: &str, command: &mut Command, timeout: Duration) -> Result<(), ProcError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let mut child = command.spawn().map_err(|error| ProcError::Spawn {
        what: what.to_string(),
        detail: error.to_string(),
    })?;
    wait_for_exit(what, &mut child, timeout)
}

/// Like [`run`], but captures stdout (used for `curl -o -` style probes where
/// the output must be parsed rather than streamed). stderr still goes to the
/// container log.
pub fn run_capture(
    what: &str,
    command: &mut Command,
    timeout: Duration,
) -> Result<(bool, String), ProcError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let mut child = command.spawn().map_err(|error| ProcError::Spawn {
        what: what.to_string(),
        detail: error.to_string(),
    })?;

    // Read on a worker thread so a chatty child cannot deadlock on a full pipe
    // while we are polling for its exit.
    let mut stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut buffer = String::new();
        if let Some(stream) = stdout.as_mut() {
            let _ = stream.read_to_string(&mut buffer);
        }
        buffer
    });

    let outcome = wait_for_exit(what, &mut child, timeout);
    let captured = reader.join().unwrap_or_default();
    match outcome {
        Ok(()) => Ok((true, captured)),
        Err(ProcError::Failed { .. }) => Ok((false, captured)),
        Err(other) => Err(other),
    }
}

fn wait_for_exit(what: &str, child: &mut Child, timeout: Duration) -> Result<(), ProcError> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    Ok(())
                } else {
                    Err(ProcError::Failed {
                        what: what.to_string(),
                        code: status.code(),
                    })
                };
            }
            Ok(None) => {}
            Err(error) => {
                return Err(ProcError::Spawn {
                    what: what.to_string(),
                    detail: format!("cannot poll {what}: {error}"),
                })
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ProcError::Timeout {
                what: what.to_string(),
                secs: timeout.as_secs(),
            });
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Poll `check` until it returns true or the deadline passes.
pub fn wait_until<F>(what: &str, timeout: Duration, mut check: F) -> Result<(), ProcError>
where
    F: FnMut() -> bool,
{
    let deadline = Instant::now() + timeout;
    loop {
        if check() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(ProcError::Timeout {
                what: what.to_string(),
                secs: timeout.as_secs(),
            });
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

pub fn wait_for_file(what: &str, path: &Path, timeout: Duration) -> Result<(), ProcError> {
    let target = path.to_path_buf();
    wait_until(what, timeout, move || target.exists())
}

/// Established TCP connections whose *local* port is `port`.
///
/// Used to answer "is an Expert Advisor attached to the bridge's loopback
/// listener?" without parsing the bridge's log or trusting a timer.
pub fn loopback_established(port: u16) -> usize {
    std::fs::read_to_string("/proc/net/tcp")
        .map(|table| parse_proc_net_tcp(&table, port))
        .unwrap_or(0)
}

/// Count `ESTABLISHED` (state `01`) rows in a `/proc/net/tcp` table whose local
/// port matches. Kept pure so it can be tested without a socket.
pub fn parse_proc_net_tcp(table: &str, port: u16) -> usize {
    let wanted = format!("{port:04X}");
    table
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            // Rows look like `0: 0100007F:13BF 00000000:0000 01 ...` — the
            // first column is the slot number, so the local address is the
            // *second* field (`0a` lines from `/proc/net/tcp6` parse the same
            // way, just with longer hex addresses).
            let _slot = fields.next()?;
            let local = fields.next()?;
            let _remote = fields.next()?;
            let state = fields.next()?;
            if state != "01" {
                return None;
            }
            let local_port = local.rsplit(':').next()?;
            (local_port.eq_ignore_ascii_case(&wanted)).then_some(())
        })
        .count()
}

/// Resident set size of a pid in megabytes (Linux only).
pub fn rss_mb(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kb = rest
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u64>().ok())?;
            return Some(kb / 1024);
        }
    }
    None
}

/// Total RSS of these pids, skipping any that have already exited.
pub fn total_rss_mb(pids: &[u32]) -> Option<u64> {
    let mut total = 0;
    let mut seen = false;
    for pid in pids {
        if let Some(mb) = rss_mb(*pid) {
            total += mb;
            seen = true;
        }
    }
    seen.then_some(total)
}

/// Every descendant pid of `root`, so a Wine session's children are accounted
/// for in the memory report.
pub fn descendants(root: u32) -> Vec<u32> {
    let mut children: Vec<(u32, u32)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(pid) = name.to_str().and_then(|value| value.parse::<u32>().ok()) else {
                continue;
            };
            let stat = match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
                Ok(stat) => stat,
                Err(_) => continue,
            };
            // `pid (comm) state ppid ...` — comm may contain spaces, so parse
            // from the last ')'.
            let Some(close) = stat.rfind(')') else {
                continue;
            };
            let mut fields = stat[close + 1..].split_whitespace();
            let _state = fields.next();
            let Some(parent) = fields.next().and_then(|value| value.parse::<u32>().ok()) else {
                continue;
            };
            children.push((pid, parent));
        }
    }

    let mut found: HashSet<u32> = HashSet::new();
    let mut frontier = vec![root];
    while let Some(current) = frontier.pop() {
        for (pid, parent) in &children {
            if *parent == current && found.insert(*pid) {
                frontier.push(*pid);
            }
        }
    }
    let mut list: Vec<u32> = found.into_iter().collect();
    list.sort_unstable();
    list
}

/// Size of a file in bytes, if it exists.
pub fn file_len(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|meta| meta.len())
}

/// Write a file with `0600` permissions, creating parent directories.
///
/// Used for the terminal startup ini, which is the one place a credential ever
/// touches the disk on this host.
pub fn write_secret_file(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path)?.permissions();
        permissions.set_mode(0o600);
        std::fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

/// Remove a file, ignoring "already gone".
pub fn remove_file(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ports are hex: 13BF = 5055 (the EA port), 0050 = 80, 1F90 = 8080.
    const TABLE: &str = "\
  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:13BF 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 1 1
   1: 0100007F:13BF 0100007F:E4A2 01 00000000:00000000 00:00000000 00000000  1000        0 2 1
   2: 0100007F:13BF 0100007F:8B1C 01 00000000:00000000 00:00000000 00000000  1000        0 3 1
   3: 00000000:0050 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 4 1
   4: 0100007F:1F90 0100007F:E4A2 06 00000000:00000000 00:00000000 00000000  1000        0 5 1
";

    #[test]
    fn established_rows_are_counted_by_local_port() {
        assert_eq!(parse_proc_net_tcp(TABLE, 5055), 2);
        assert_eq!(parse_proc_net_tcp(TABLE, 80), 0);
        assert_eq!(parse_proc_net_tcp(TABLE, 8080), 0);
        assert_eq!(parse_proc_net_tcp("", 5055), 0);
    }

    #[test]
    fn rss_reads_a_real_process() {
        let own = std::process::id();
        assert!(rss_mb(own).unwrap_or(0) > 0);
        assert!(rss_mb(999_999).is_none());
        assert!(total_rss_mb(&[own]).unwrap_or(0) > 0);
    }

    #[test]
    fn descendants_of_this_process_are_found() {
        // A sleeping child is enough to prove the /proc walk works.
        let mut child = Command::new("sleep").arg("2").spawn().expect("spawn sleep");
        let found = descendants(std::process::id());
        assert!(
            found.contains(&child.id()),
            "child pid not found: {found:?}"
        );
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn timeouts_kill_the_child() {
        let mut command = Command::new("sleep");
        command.arg("30");
        let error = run("sleep 30", &mut command, Duration::from_millis(600)).unwrap_err();
        assert!(matches!(error, ProcError::Timeout { .. }), "{error}");
    }

    #[test]
    fn a_missing_binary_is_a_spawn_error() {
        let mut command = Command::new("/nonexistent/binary");
        let error = run("missing", &mut command, Duration::from_secs(1)).unwrap_err();
        assert!(matches!(error, ProcError::Spawn { .. }), "{error}");
    }

    #[test]
    fn capture_reports_exit_status_and_output() {
        let mut command = Command::new("sh");
        command.args(["-c", "printf hello"]);
        let (ok, out) = run_capture("printf", &mut command, Duration::from_secs(5)).unwrap();
        assert!(ok);
        assert_eq!(out, "hello");

        let mut failing = Command::new("sh");
        failing.args(["-c", "exit 3"]);
        let (ok, _) = run_capture("exit 3", &mut failing, Duration::from_secs(5)).unwrap();
        assert!(!ok);
    }

    #[test]
    fn wait_until_gives_up() {
        let error = wait_until("never", Duration::from_millis(600), || false).unwrap_err();
        assert!(matches!(error, ProcError::Timeout { .. }));
    }

    #[test]
    #[cfg(unix)]
    fn secret_files_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let mut path = std::env::temp_dir();
        path.push(format!("mt5-host-secret-{}.ini", std::process::id()));
        write_secret_file(&path, "Password=hidden\n").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        remove_file(&path);
    }
}
