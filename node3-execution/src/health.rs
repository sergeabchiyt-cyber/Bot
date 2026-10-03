//! Minimal HTTP health endpoint.
//!
//! Node 3 is a pure outbound WebSocket client, but hosting platforms (Render,
//! Docker, Kubernetes probes) need a listening TCP port to consider the service
//! alive. This serves a dependency-free `GET /health` -> `200 ok` on
//! `0.0.0.0:$PORT` (default 10000) and nothing else.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{error, info};

const OK: &str = "HTTP/1.1 200 OK\r\n\
Content-Type: text/plain\r\n\
Content-Length: 2\r\n\
Connection: close\r\n\
\r\n\
ok";

const NOT_FOUND: &str = "HTTP/1.1 404 Not Found\r\n\
Content-Type: text/plain\r\n\
Content-Length: 9\r\n\
Connection: close\r\n\
\r\n\
not found";

/// Response for a request path: only `/health` is served (query strings such as
/// `/health?t=1` used for cache-busting probes are ignored).
fn response_for(path: &str) -> &'static str {
    if path.split('?').next().unwrap_or(path) == "/health" {
        OK
    } else {
        NOT_FOUND
    }
}

/// Serves `GET /health` on `0.0.0.0:{port}` until the process exits.
pub async fn serve(port: u16) {
    let listener = match TcpListener::bind(("0.0.0.0", port)).await {
        Ok(listener) => listener,
        Err(e) => {
            error!("Failed to bind health server on 0.0.0.0:{port}: {e}");
            return;
        }
    };

    info!("Health endpoint listening on http://0.0.0.0:{port}/health");

    loop {
        match listener.accept().await {
            Ok((mut stream, _)) => {
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let n = match stream.read(&mut buf).await {
                        Ok(n) => n,
                        Err(_) => return,
                    };

                    // First line of the request: "GET /health HTTP/1.1".
                    let request = String::from_utf8_lossy(&buf[..n]);
                    let path = request
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .unwrap_or("/");

                    let _ = stream.write_all(response_for(path).as_bytes()).await;
                    let _ = stream.shutdown().await;
                });
            }
            Err(e) => error!("Health server accept error: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_path_is_ok() {
        assert!(response_for("/health").starts_with("HTTP/1.1 200 OK"));
    }

    #[test]
    fn query_string_is_ignored() {
        assert!(response_for("/health?t=1").starts_with("HTTP/1.1 200 OK"));
    }

    #[test]
    fn other_paths_are_not_found() {
        assert!(response_for("/").starts_with("HTTP/1.1 404"));
        assert!(response_for("/status").starts_with("HTTP/1.1 404"));
        assert!(response_for("/healthz").starts_with("HTTP/1.1 404"));
    }
}
