//! `mt5-host` — the Deriv MT5 **demo** terminal host for a Linux container.
//!
//! Node 4 needs something that owns a MetaTrader 5 terminal. On Windows that is
//! a person clicking "attach EA"; this binary is the Linux equivalent: it makes
//! the terminal headless (Xvfb), makes it install itself (`mt5setup.exe /auto`
//! under Wine), makes it attach the bridge EA (`[StartUp] Expert=` in the
//! startup ini, compiled with `metaeditor64 /compile`), starts `mt5-bridge`
//! next to it and then supervises both — all from environment configuration,
//! all inside the 512 MB / 0.1 CPU shape of a free web-service instance.
//!
//! It never trades itself. It has no order path: orders arrive from Node 4 over
//! the bridge's WebSocket session, exactly as they do on a Windows host.
//!
//! ```text
//!   Render web service (this container)
//!   ┌──────────────────────────────────────────────────────────────┐
//!   │ mt5-host            supervisor + /health, /readyz, /diagnostics
//!   │  ├── Xvfb :99       virtual display (nothing is rendered)
//!   │  ├── mt5-bridge     WSS client of Node 4  ◄── intents / reports
//!   │  └── wine terminal64.exe /portable /config:mt5-start.ini
//!   │        └── Mt5BridgeEA.ex5  ──loopback TCP──► mt5-bridge
//!   └──────────────────────────────────────────────────────────────┘
//! ```
//!
//! Exit codes: `2` configuration error (nothing was started), `1` a
//! prepare-only run failed. Once the supervisor loop is reached the process
//! stays up through failures — Render would only restart it into the same
//! broken configuration — and reports the reasons on `/readyz`.

mod config;
mod health;
mod process;
mod stages;
mod state;
mod templates;

use std::sync::{Arc, Mutex};

use config::Config;
use state::HostStatus;

fn main() {
    let cfg = Config::from_env();
    if let Err(errors) = cfg.validate() {
        for error in &errors {
            eprintln!("mt5-host: configuration error: {error}");
        }
        eprintln!(
            "mt5-host: refusing to start — fix the environment (see mt5-host/README.md) and \
             redeploy; nothing was launched"
        );
        std::process::exit(2);
    }

    let status: health::SharedStatus = Arc::new(Mutex::new(HostStatus::new()));

    // The HTTP surface comes up first: the platform health check must see the
    // service while Wine is still installing (that can take minutes on 0.1 CPU).
    {
        let status = Arc::clone(&status);
        let port = cfg.port;
        std::thread::spawn(move || {
            if let Err(error) = health::serve(port, Arc::clone(&status)) {
                eprintln!("mt5-host: HTTP surface failed to bind: {error}");
                health::lock(&status).record_error("http", &error.to_string());
            }
        });
    }

    let mut supervisor = stages::Supervisor::new(cfg, status);
    supervisor.run();
}
