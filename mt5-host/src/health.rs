//! This endpoint set replaces the operator's eyes on a Windows/Wine desktop: the
//! Linux host assumes nobody can see the GUI, so the facts a human used to check
//! by looking — is the terminal logged in, is the EA attached, is the bridge up —
//! are reported here instead, in a form a monitor can alert on.
//! The host's HTTP surface, on `0.0.0.0:$PORT` (Render requires the service to
//! listen for its health checks).
//!
//! Public, read-only, and deliberately tiny:
//!
//! * `GET /health`      -> always `200 ok` while the process is alive. This is
//!   what the platform health check points at, so a long Wine install never
//!   makes the deploy look dead.
//! * `GET /readyz`      -> `200` only when a terminal is running with an EA
//!   attached and the bridge is up; otherwise `503` with the reasons, one per
//!   line. Use this one for monitoring.
//! * `GET /diagnostics` -> JSON view of the same state, plus Node 4's view of
//!   this bridge, the restart counters and the memory footprint.
//!
//! There is no control endpoint: stopping or restarting the terminal is a
//! redeploy, and order-level control lives on Node 4 (`POST /mt5/control`)
//! behind its own token. Nothing here can place, modify or close a trade.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crate::state::HostStatus;

pub type SharedStatus = Arc<Mutex<HostStatus>>;

/// Lock helper that survives a panicking writer instead of poisoning the server.
pub fn lock(status: &SharedStatus) -> MutexGuard<'_, HostStatus> {
    status
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn serve(port: u16, status: SharedStatus) -> std::io::Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    eprintln!("mt5-host: HTTP resources on 0.0.0.0:{port} (/health, /readyz, /diagnostics)");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(error) = handle(stream, &status) {
                    eprintln!("mt5-host: health request failed: {error}");
                }
            }
            Err(error) => eprintln!("mt5-host: accept failed: {error}"),
        }
    }
    Ok(())
}

fn handle(mut stream: TcpStream, status: &SharedStatus) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    let mut buffer = [0u8; 2048];
    let read = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let path = target.split('?').next().unwrap_or("/").to_string();

    let (code, reason, content_type, body) = match (method.as_str(), path.as_str()) {
        ("GET", "/health") | ("HEAD", "/health") => {
            (200, "OK", "text/plain; charset=utf-8", "ok\n".to_string())
        }
        ("GET", "/readyz") => match lock(status).ready() {
            Ok(()) => (
                200,
                "OK",
                "text/plain; charset=utf-8",
                "ready\n".to_string(),
            ),
            Err(reasons) => (
                503,
                "Service Unavailable",
                "text/plain; charset=utf-8",
                format!("{}\n", reasons.join("\n")),
            ),
        },
        ("GET", "/diagnostics") => (
            200,
            "OK",
            "application/json; charset=utf-8",
            format!("{}\n", lock(status).to_json()),
        ),
        ("GET", _) | ("HEAD", _) => (
            404,
            "Not Found",
            "text/plain; charset=utf-8",
            "not found\n".to_string(),
        ),
        _ => (
            405,
            "Method Not Allowed",
            "text/plain; charset=utf-8",
            "read-only service\n".to_string(),
        ),
    };

    let head_only = method == "HEAD";
    let response = format!(
        "HTTP/1.1 {code} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {length}\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\
         \r\n",
        length = body.len(),
    );
    stream.write_all(response.as_bytes())?;
    if !head_only {
        stream.write_all(body.as_bytes())?;
    }
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy_status() -> SharedStatus {
        let mut status = HostStatus::new();
        status.display_ready = true;
        status.prefix_ready = true;
        status.mt5_installed = true;
        status.ea_compiled = true;
        status.startup_config_written = true;
        status.terminal_pid = Some(1);
        status.bridge_pid = Some(2);
        status.ea_connections = 1;
        Arc::new(Mutex::new(status))
    }

    fn get(port: u16, path: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    fn spawn_server(status: SharedStatus, port: u16) {
        std::thread::spawn(move || {
            let _ = serve(port, status);
        });
        std::thread::sleep(Duration::from_millis(150));
    }

    #[test]
    fn health_is_always_ok_and_readyz_reflects_the_stack() {
        let port = 18_991;
        spawn_server(healthy_status(), port);

        let health = get(port, "/health");
        assert!(health.starts_with("HTTP/1.1 200 OK"), "{health}");
        assert!(health.ends_with("ok\n"), "{health}");

        let ready = get(port, "/readyz");
        assert!(ready.starts_with("HTTP/1.1 200 OK"), "{ready}");
        assert!(ready.ends_with("ready\n"));
    }

    #[test]
    fn readyz_reports_why_it_is_not_ready() {
        let status = Arc::new(Mutex::new(HostStatus::new()));
        let port = 18_992;
        spawn_server(status, port);

        let ready = get(port, "/readyz");
        assert!(ready.starts_with("HTTP/1.1 503"), "{ready}");
        assert!(ready.contains("Wine prefix is not initialised"));
        assert!(ready.contains("terminal64.exe is not running"));
    }

    #[test]
    fn diagnostics_is_json_without_secret_shaped_fields() {
        let port = 18_993;
        spawn_server(healthy_status(), port);

        let response = get(port, "/diagnostics");
        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        let body = response.split("\r\n\r\n").nth(1).unwrap_or_default();
        let parsed: serde_json::Value = serde_json::from_str(body.trim()).unwrap();
        assert_eq!(parsed["service"], "mt5-host");
        assert!(!body.to_ascii_lowercase().contains("password"));
    }

    #[test]
    fn unknown_paths_are_404_and_writes_are_405() {
        let port = 18_994;
        spawn_server(healthy_status(), port);

        let missing = get(port, "/restart");
        assert!(missing.starts_with("HTTP/1.1 404"), "{missing}");

        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .write_all(b"POST /diagnostics HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 405"), "{response}");
    }
}
