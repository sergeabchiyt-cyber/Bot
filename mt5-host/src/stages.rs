//! The boot stages and the supervision loop.
//!
//! Every step is idempotent and driven purely from configuration, so a cold
//! container (Render gives us an ephemeral disk) can go from "empty" to "demo
//! terminal with the bridge EA attached, talking to Node 4" with no human
//! interaction and no GUI. This is the Linux/Wine shape of the terminal host and
//! it supersedes the earlier Windows (Wine-with-a-desktop) deployment
//! assumption, where those steps were performed by an operator on a machine
//! somebody could log into:
//!
//! 1. **display** — start `Xvfb` on `MT5_HOST_DISPLAY` (Wine needs an X server;
//!    nothing is ever rendered to a screen).
//! 2. **prefix** — create the 64-bit Wine prefix (`wineboot -u`), or unpack a
//!    prepared one (`MT5_HOST_PREFIX_ARCHIVE_URL`) to skip the slow part.
//! 3. **install** — run `mt5setup.exe /auto` unattended if no terminal is found.
//! 4. **ea** — install `Mt5BridgeEA.mq5` into `MQL5/Experts/Node4` and compile
//!    it headlessly with `metaeditor64 /compile`.
//! 5. **config** — write the terminal startup ini (account + `[Experts]` +
//!    `[StartUp] Expert=…`) and the EA preset into `MQL5/Presets`.
//! 6. **run** — start the bridge, start `terminal64.exe /portable /config:…`,
//!    then supervise both: restart on exit, restart when no EA ever attaches,
//!    and keep `/diagnostics` honest about what is actually running.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use crate::config::Config;
use crate::health::{lock, SharedStatus};
use crate::process;
use crate::state::{now_ms, Node4View};
use crate::templates;

pub struct Supervisor {
    cfg: Config,
    status: SharedStatus,
    xvfb: Option<Child>,
    bridge: Option<Child>,
    terminal: Option<Child>,
    terminal_started_ms: i64,
    /// Timestamps of recent terminal restarts (pruned to one hour).
    restarts: Vec<i64>,
    /// Do not respawn the terminal before this instant.
    terminal_hold_until_ms: i64,
    install_dir: Option<PathBuf>,
    terminal_exe: Option<PathBuf>,
    last_node4_poll_ms: i64,
    last_memory_warn_ms: i64,
}

fn log(message: &str) {
    println!("mt5-host: {message}");
}

impl Supervisor {
    pub fn new(cfg: Config, status: SharedStatus) -> Self {
        Self {
            cfg,
            status,
            xvfb: None,
            bridge: None,
            terminal: None,
            terminal_started_ms: 0,
            restarts: Vec::new(),
            terminal_hold_until_ms: 0,
            install_dir: None,
            terminal_exe: None,
            last_node4_poll_ms: 0,
            last_memory_warn_ms: 0,
        }
    }

    /// Boot, then supervise forever (or prepare and exit, in archive-build mode).
    pub fn run(&mut self) -> ! {
        for (key, value) in self.cfg.summary() {
            log(&format!("config {key} = {value}"));
        }
        log(&format!(
            "bridge history file: {}",
            self.cfg.history_file().display()
        ));
        if !self.cfg.trading_enabled {
            log("MT5_TRADING_ENABLED is off: the bridge starts halted (monitoring only)");
        }

        let mut attempt = 0u32;
        loop {
            match self.bootstrap() {
                Ok(()) => break,
                Err(error) => {
                    attempt += 1;
                    log(&format!("boot attempt {attempt} failed: {error}"));
                    lock(&self.status).record_error("boot", &error);
                    if self.cfg.prepare_only {
                        // A prepared prefix is a build artifact: fail loudly so
                        // the workflow that requested it goes red.
                        eprintln!("mt5-host: prepare-only run failed: {error}");
                        std::process::exit(1);
                    }
                    // Never spin: a wrong password is not fixed by retrying, but
                    // a transient download failure is. 15s doubling to 5 minutes,
                    // so a free instance that is still downloading settles down
                    // instead of hammering download.mql5.com.
                    let delay = std::cmp::min(300, 15 * 2u64.pow(attempt.min(4)));
                    log(&format!("retrying in {delay}s"));
                    std::thread::sleep(Duration::from_secs(delay));
                }
            }
        }

        if self.cfg.prepare_only {
            log("prepare-only run complete; exiting with a credential-free prefix");
            std::process::exit(0);
        }

        self.supervise();
    }

    // ---- boot stages -------------------------------------------------------

    fn bootstrap(&mut self) -> Result<(), String> {
        self.ensure_state_dir()?;
        self.ensure_display()?;
        self.ensure_prefix()?;
        self.ensure_terminal()?;
        self.ensure_ea()?;
        if !self.cfg.prepare_only {
            self.write_startup_config()?;
        }
        Ok(())
    }

    fn ensure_state_dir(&self) -> Result<(), String> {
        let mut status = lock(&self.status);
        status.advance("state_dir");
        std::fs::create_dir_all(&self.cfg.state_dir).map_err(|error| {
            format!(
                "cannot create state directory {}: {error}",
                self.cfg.state_dir.display()
            )
        })?;
        let probe = self.cfg.state_dir.join(".mt5-host-write-probe");
        std::fs::write(&probe, b"probe").map_err(|error| {
            format!(
                "state directory {} is not writable: {error} (the platform disk is ephemeral; \
                 mount a disk here for a durable Wine prefix and history)",
                self.cfg.state_dir.display()
            )
        })?;
        let _ = std::fs::remove_file(&probe);
        Ok(())
    }

    fn ensure_display(&mut self) -> Result<(), String> {
        let socket = display_socket_path(&self.cfg.display);
        if process::which(&self.cfg.xvfb_bin).is_none() {
            return Err(format!(
                "{} is not installed in this image, so Wine has no display to run on",
                self.cfg.xvfb_bin
            ));
        }

        reap(&mut self.xvfb);
        if self.xvfb.is_some() && socket.exists() {
            lock(&self.status).display_ready = true;
            return Ok(());
        }

        if socket.exists() {
            // A socket without a live Xvfb is a container-restart leftover and
            // starting a second server for the same display would fail.
            log(&format!(
                "removing stale display {} left by a previous run",
                socket.display()
            ));
            process::remove_file(&socket);
            if let Some(number) = display_number(&self.cfg.display) {
                process::remove_file(Path::new(&format!("/tmp/.X{number}-lock")));
            }
        }

        let mut command = Command::new(&self.cfg.xvfb_bin);
        command
            .arg(&self.cfg.display)
            .arg("-screen")
            .arg("0")
            .arg(&self.cfg.screen)
            .arg("-ac")
            .arg("-nolisten")
            .arg("tcp")
            .arg("-noreset")
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        log(&format!(
            "starting {} on {} ({})",
            self.cfg.xvfb_bin, self.cfg.display, self.cfg.screen
        ));
        let child = command
            .spawn()
            .map_err(|error| format!("cannot start {}: {error}", self.cfg.xvfb_bin))?;
        self.xvfb = Some(child);

        process::wait_for_file(
            "the X socket",
            &socket,
            Duration::from_secs(self.cfg.timeouts.display_secs),
        )
        .map_err(|error| {
            format!(
                "{error} — the virtual display never came up on {}",
                self.cfg.display
            )
        })?;

        lock(&self.status).display_ready = true;
        lock(&self.status).advance("display");
        Ok(())
    }

    fn ensure_prefix(&mut self) -> Result<(), String> {
        let marker = self.cfg.wine_prefix.join("system.reg");
        if marker.exists() {
            lock(&self.status).prefix_ready = true;
            return Ok(());
        }

        // Fast path: a prefix prepared elsewhere (see the prepare workflow) is
        // one download + untar instead of a full unattended install.
        if let Some(url) = self.cfg.prefix_archive_url.clone() {
            let archive = self.cfg.state_dir.join("mt5-prefix.tar.gz");
            log(&format!("fetching prepared Wine prefix from {url}"));
            self.download(&url, &archive)?;
            log("unpacking the prepared prefix");
            let mut command = Command::new(&self.cfg.tar_bin);
            command
                .arg("-xzf")
                .arg(&archive)
                .arg("-C")
                .arg(&self.cfg.state_dir)
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit());
            let outcome = process::run(
                "tar -xzf mt5-prefix.tar.gz",
                &mut command,
                Duration::from_secs(self.cfg.timeouts.prefix_download_secs),
            );
            process::remove_file(&archive);
            if let Err(error) = outcome {
                log(&format!("prepared prefix could not be unpacked ({error}); falling back to an unattended install"));
            } else if marker.exists() {
                lock(&self.status).prefix_ready = true;
                lock(&self.status).advance("prefix");
                log("prepared prefix unpacked");
                return Ok(());
            } else {
                log("the prepared prefix did not contain system.reg; falling back to an unattended install");
            }
        }

        if process::which(&self.cfg.wine_bin).is_none() {
            return Err(format!(
                "{} is not installed in this image",
                self.cfg.wine_bin
            ));
        }

        let wineboot = process::which("wineboot");
        let mut command = Command::new(&self.cfg.wine_bin);
        for (key, value) in self.cfg.wine_env() {
            command.env(key, value);
        }
        match wineboot {
            Some(wineboot) => {
                command = Command::new(wineboot);
                for (key, value) in self.cfg.wine_env() {
                    command.env(key, value);
                }
                command.arg("-u");
            }
            None => {
                command.arg("wineboot").arg("-u");
            }
        }
        for extra in &self.cfg.wineboot_extra_args {
            command.arg(extra);
        }
        log("initialising the Wine prefix (this is the slow step on 0.1 CPU)");
        lock(&self.status).advance("prefix");
        process::run(
            "wineboot -u",
            &mut command,
            Duration::from_secs(self.cfg.timeouts.wineboot_secs),
        )
        .map_err(|error| format!("provisioning the Wine prefix failed: {error}"))?;

        process::wait_for_file(
            "system.reg",
            &marker,
            Duration::from_secs(self.cfg.timeouts.wineboot_secs),
        )
        .map_err(|error| format!("{error} — the Wine prefix was not created"))?;
        lock(&self.status).prefix_ready = true;
        Ok(())
    }

    fn ensure_terminal(&mut self) -> Result<(), String> {
        if let Some(exe) = self.find_terminal() {
            self.adopt_terminal(exe);
            return Ok(());
        }

        if let Some(url) = self.cfg.prefix_archive_url.clone() {
            // The archive is fetched during the prefix stage only; if it did not
            // contain a terminal, fall through to the installer.
            log(&format!("no terminal after the prepared prefix from {url}"));
        }

        let installer = self.cfg.installer_path.clone();
        if !installer.exists() {
            log(&format!(
                "no installer at {}; downloading it",
                installer.display()
            ));
            self.download(&self.cfg.installer_url.clone(), &installer)?;
        }

        let mut command = Command::new(&self.cfg.wine_bin);
        for (key, value) in self.cfg.wine_env() {
            command.env(key, value);
        }
        let argv = installer.display().to_string();
        command.arg(&installer).arg("/auto");
        log(&format!(
            "installing MetaTrader 5 unattended ({argv} /auto)"
        ));
        lock(&self.status).advance("install");
        process::run(
            "mt5setup.exe /auto",
            &mut command,
            Duration::from_secs(self.cfg.timeouts.install_secs),
        )
        .map_err(|error| format!("the MT5 installer failed: {error}"))?;

        let exe = self.find_terminal().ok_or_else(|| {
            format!(
                "the installer reported success but no terminal64.exe was found below {}",
                self.cfg.wine_prefix.join("drive_c").display()
            )
        })?;
        self.adopt_terminal(exe);
        Ok(())
    }

    fn adopt_terminal(&mut self, exe: PathBuf) {
        let install_dir = exe
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.cfg.mt5_install_dir_hint());
        log(&format!("MT5 terminal found at {}", exe.display()));
        let mut status = lock(&self.status);
        status.mt5_installed = true;
        status.install_dir = Some(install_dir.display().to_string());
        status.advance("install");
        drop(status);
        self.install_dir = Some(install_dir);
        self.terminal_exe = Some(exe);
    }

    fn ensure_ea(&mut self) -> Result<(), String> {
        let install_dir = self
            .install_dir
            .clone()
            .ok_or_else(|| "the EA cannot be installed before the terminal exists".to_string())?;
        let source = self
            .cfg
            .ea_source
            .clone()
            .map(|path| {
                std::fs::read_to_string(&path).map_err(|error| {
                    format!("cannot read MT5_HOST_EA_SOURCE {}: {error}", path.display())
                })
            })
            .transpose()?
            .unwrap_or_else(|| templates::EA_SOURCE.to_string());

        let experts = install_dir
            .join("MQL5")
            .join("Experts")
            .join(crate::config::EA_SUBDIR);
        std::fs::create_dir_all(&experts)
            .map_err(|error| format!("cannot create {}: {error}", experts.display()))?;
        let mq5 = experts.join(format!("{}.mq5", self.cfg.ea_name));
        let ex5 = experts.join(format!("{}.ex5", self.cfg.ea_name));

        let unchanged = ex5.exists()
            && std::fs::read_to_string(&mq5)
                .map(|existing| existing == source)
                .unwrap_or(false);
        if unchanged {
            log(&format!(
                "the EA is already compiled in this prefix ({})",
                ex5.display()
            ));
            lock(&self.status).ea_compiled = true;
            lock(&self.status).advance("ea");
            return Ok(());
        }

        std::fs::write(&mq5, &source)
            .map_err(|error| format!("cannot write {}: {error}", mq5.display()))?;
        log(&format!("compiling the EA ({})", mq5.display()));
        lock(&self.status).advance("ea");

        let metaeditor = find_sibling(&install_dir, "metaeditor64.exe")
            .or_else(|| find_sibling(&install_dir, "metaeditor.exe"))
            .ok_or_else(|| {
                format!(
                    "MetaEditor is not next to the terminal in {} — ship a precompiled \
                     {}.ex5 in the prefix or set MT5_HOST_EA_SOURCE to a precompiled EA",
                    install_dir.display(),
                    self.cfg.ea_name
                )
            })?;

        let log_path = install_dir
            .join("MQL5")
            .join("Experts")
            .join(format!("{}-compile.log", self.cfg.ea_name));
        let mut command = Command::new(&self.cfg.wine_bin);
        for (key, value) in self.cfg.wine_env() {
            command.env(key, value);
        }
        command
            .arg(win_path(&metaeditor))
            .arg(format!("/compile:{}", win_path(&mq5)))
            .arg(format!("/log:{}", win_path(&log_path)))
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        let outcome = process::run(
            "metaeditor64 /compile",
            &mut command,
            Duration::from_secs(self.cfg.timeouts.compile_secs),
        );

        let report = std::fs::read_to_string(&log_path).unwrap_or_default();
        process::remove_file(&log_path);

        if let Err(error) = outcome {
            return Err(format!("compiling the EA failed: {error}{}", tail(&report)));
        }
        if !ex5.exists() {
            return Err(format!(
                "MetaEditor produced no {}.ex5{}",
                self.cfg.ea_name,
                tail(&report)
            ));
        }
        if let Some(errors) = compile_error_count(&report) {
            if errors > 0 {
                return Err(format!(
                    "MetaEditor reported {errors} error(s){}",
                    tail(&report)
                ));
            }
        }

        log(&format!("EA compiled to {}", ex5.display()));
        lock(&self.status).ea_compiled = true;
        Ok(())
    }

    fn write_startup_config(&mut self) -> Result<(), String> {
        let install_dir = self
            .install_dir
            .clone()
            .ok_or_else(|| "no terminal to configure".to_string())?;

        let ini = templates::start_ini(&self.cfg);
        let ini_path = self.cfg.start_ini_path();
        process::write_secret_file(&ini_path, &ini)
            .map_err(|error| format!("cannot write {}: {error}", ini_path.display()))?;

        let preset_path = self.cfg.preset_path(&install_dir);
        if let Some(parent) = preset_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
        std::fs::write(&preset_path, templates::preset(&self.cfg))
            .map_err(|error| format!("cannot write {}: {error}", preset_path.display()))?;

        if self.cfg.write_profile {
            let profile_dir = install_dir
                .join("MQL5")
                .join("Profiles")
                .join("Charts")
                .join("Default");
            std::fs::create_dir_all(&profile_dir)
                .map_err(|error| format!("cannot create {}: {error}", profile_dir.display()))?;
            let chart = profile_dir.join("chart01.chr");
            std::fs::write(&chart, templates::chart_profile(&self.cfg))
                .map_err(|error| format!("cannot write {}: {error}", chart.display()))?;
            log(&format!("wrote fallback chart profile {}", chart.display()));
        }

        lock(&self.status).startup_config_written = true;
        lock(&self.status).advance("config");
        log(&format!(
            "terminal startup configuration written ({} + {})",
            ini_path.display(),
            preset_path.display()
        ));
        Ok(())
    }

    // ---- supervision -------------------------------------------------------

    fn supervise(&mut self) -> ! {
        log("supervising the terminal and the bridge");
        let mut wine_version_logged = false;
        loop {
            self.reap_children();
            if !wine_version_logged {
                let version = self.wine_version();
                if let Some(version) = version {
                    log(&format!("wine reports {version}"));
                    lock(&self.status).wine_version = Some(version);
                    wine_version_logged = true;
                }
            }
            if let Err(error) = self.ensure_display() {
                log(&format!("display problem: {error}"));
                lock(&self.status).record_error("display", &error);
            }
            if let Err(error) = self.ensure_bridge() {
                log(&format!("bridge problem: {error}"));
                lock(&self.status).record_error("bridge", &error);
            }
            if let Err(error) = self.ensure_terminal_running() {
                log(&format!("terminal problem: {error}"));
                lock(&self.status).record_error("terminal", &error);
            }
            self.refresh_observations();
            self.maybe_remove_start_ini();
            std::thread::sleep(Duration::from_secs(2));
        }
    }

    fn reap_children(&mut self) {
        if let Some(status) = reap(&mut self.terminal) {
            log(&format!("terminal64.exe exited ({status})"));
            lock(&self.status).terminal_pid = None;
            lock(&self.status).terminal_uptime_secs = None;
            if self.install_dir.is_some() {
                self.note_restart();
            }
        }
        if let Some(status) = reap(&mut self.bridge) {
            log(&format!("mt5-bridge exited ({status})"));
            lock(&self.status).bridge_pid = None;
        }
        if let Some(status) = reap(&mut self.xvfb) {
            log(&format!("Xvfb exited ({status})"));
            lock(&self.status).display_ready = false;
        }
    }

    fn note_restart(&mut self) {
        let now = now_ms();
        self.restarts.push(now);
        self.restarts.retain(|stamp| now - *stamp <= 3_600_000);
        let count = self.restarts.len() as u32;
        lock(&self.status).restarts_last_hour = count;
        if count > self.cfg.max_restarts_per_hour {
            let hold = 300_000;
            self.terminal_hold_until_ms = now + hold;
            log(&format!(
                "{count} terminal restarts in the last hour: holding restarts for {}s to stop \
                 thrashing on a 0.1 CPU instance",
                hold / 1000
            ));
        }
    }

    fn ensure_bridge(&mut self) -> Result<(), String> {
        if self.bridge.is_some() {
            return Ok(());
        }
        let mut command = Command::new(self.cfg.bridge_bin.clone());
        for (key, value) in self.cfg.bridge_env() {
            command.env(key, value);
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        let child = command.spawn().map_err(|error| {
            format!(
                "cannot start {}: {error} (it must be on PATH in this image)",
                self.cfg.bridge_bin.display()
            )
        })?;
        let mut status = lock(&self.status);
        status.bridge_pid = Some(child.id());
        status.bridge_starts += 1;
        status.advance("bridge");
        drop(status);
        log(&format!("mt5-bridge started (pid {})", child.id()));
        self.bridge = Some(child);
        Ok(())
    }

    fn ensure_terminal_running(&mut self) -> Result<(), String> {
        if self.terminal.is_some() {
            // The terminal is up but no EA ever reached the bridge: the EA did
            // not attach (or the terminal never finished logging in). Restart it
            // once the patience window has passed.
            let started = self.terminal_started_ms;
            let age_secs = ((now_ms() - started).max(0) / 1000) as u64;
            let ea_connections = lock(&self.status).ea_connections;
            if ea_connections == 0 && age_secs > self.cfg.timeouts.ea_wait_secs {
                log(&format!(
                    "no Expert Advisor connected after {age_secs}s: restarting the terminal"
                ));
                self.stop_terminal();
                self.note_restart();
                return Ok(());
            }
            return Ok(());
        }

        if now_ms() < self.terminal_hold_until_ms {
            return Ok(());
        }

        let exe = self
            .terminal_exe
            .clone()
            .ok_or_else(|| "the terminal executable is unknown".to_string())?;
        let ini_path = self.cfg.start_ini_path();
        if !ini_path.exists() {
            // Restarting without the startup ini would open an unconfigured,
            // un-logged-in terminal: rewrite it first.
            self.write_startup_config()?;
        }

        let mut command = Command::new(&self.cfg.wine_bin);
        for (key, value) in self.cfg.wine_env() {
            command.env(key, value);
        }
        command
            .arg(win_path(&exe))
            .arg("/portable")
            .arg(format!("/config:{}", win_path(&ini_path)));
        for extra in &self.cfg.extra_terminal_args {
            command.arg(extra);
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        log(&format!("starting the terminal ({})", win_path(&exe)));
        let child = command
            .spawn()
            .map_err(|error| format!("cannot start the terminal: {error}"))?;
        let mut status = lock(&self.status);
        status.terminal_pid = Some(child.id());
        status.terminal_starts += 1;
        status.advance("terminal");
        drop(status);
        self.terminal_started_ms = now_ms();
        self.terminal = Some(child);
        Ok(())
    }

    fn stop_terminal(&mut self) {
        if let Some(mut child) = self.terminal.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        // Wine keeps helper processes (and the terminal's own children) alive;
        // killing the session avoids two terminals fighting over one data dir.
        self.kill_wine_session();
        lock(&self.status).terminal_pid = None;
        lock(&self.status).terminal_uptime_secs = None;
    }

    fn kill_wine_session(&self) {
        let Some(wineserver) = process::which("wineserver") else {
            return;
        };
        let mut command = Command::new(wineserver);
        for (key, value) in self.cfg.wine_env() {
            command.env(key, value);
        }
        command.arg("-k");
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        match process::run("wineserver -k", &mut command, Duration::from_secs(30)) {
            Ok(()) => log("wine session killed"),
            Err(error) => log(&format!("wineserver -k: {error}")),
        }
    }

    fn refresh_observations(&mut self) {
        let ea_connections = process::loopback_established(self.cfg.ea_port);
        let own_pid = std::process::id();
        let mut pids = vec![own_pid];
        pids.extend(process::descendants(own_pid));
        let memory_mb = process::total_rss_mb(&pids);
        let terminal_uptime = self
            .terminal
            .as_ref()
            .map(|_| ((now_ms() - self.terminal_started_ms).max(0) / 1000) as u64);

        {
            let mut status = lock(&self.status);
            status.ea_connections = ea_connections;
            status.memory_mb = memory_mb;
            status.terminal_uptime_secs = terminal_uptime;
            status.trading_enabled = self.cfg.trading_enabled;
        }

        if let Some(mb) = memory_mb {
            if mb > self.cfg.rss_warn_mb {
                log_once(
                    &mut self.last_memory_warn_ms,
                    &format!(
                        "memory warning: {mb} MB resident across this container's processes \
                         (threshold {} MB) — MT5 under Wine is at the edge of a 512 MB instance",
                        self.cfg.rss_warn_mb
                    ),
                );
            }
        }

        let poll_due =
            now_ms() - self.last_node4_poll_ms >= (self.cfg.node4_poll_secs as i64) * 1000;
        if poll_due {
            self.last_node4_poll_ms = now_ms();
            self.poll_node4();
        }
    }

    fn poll_node4(&mut self) {
        let Some(base) = self.cfg.node4_ws_url.as_deref().and_then(node4_http_base) else {
            return;
        };
        let view = self.fetch_node4_view(&base);
        lock(&self.status).node4 = view;
    }

    fn fetch_node4_view(&self, base: &str) -> Node4View {
        let mut view = Node4View::default();
        let status = match self.get_json(&format!("{base}/mt5/status")) {
            Some(value) => value,
            None => {
                view.error = Some("Node 4 status unreachable".into());
                return view;
            }
        };
        view.reachable = true;
        let account = self.get_json(&format!("{base}/mt5/account"));

        view.configured = json_bool(&status, "/configured");
        view.connected = json_bool(&status, "/connected");
        view.authorized = json_bool(&status, "/authorized");
        view.ea_connected = json_bool(&status, "/ea_connected");
        view.ea_mode = json_str(&status, "/ea_mode");
        view.halted = json_bool(&status, "/halted");
        view.halt_reason = json_str(&status, "/halt_reason");
        view.orders_sent = json_u64(&status, "/orders_sent");
        view.orders_filled = json_u64(&status, "/orders_filled");
        view.error = json_str(&status, "/last_error");

        if let Some(account) = account {
            view.login = json_i64(&account, "/login");
            view.account_type = json_str(&account, "/account_type");
        }
        view
    }

    fn get_json(&self, url: &str) -> Option<serde_json::Value> {
        let mut command = Command::new(&self.cfg.curl_bin);
        command
            .arg("-fsS")
            .arg("-m")
            .arg("10")
            .arg("-H")
            .arg("accept: application/json")
            .arg(url);
        match process::run_capture("curl", &mut command, Duration::from_secs(15)) {
            Ok((true, body)) => serde_json::from_str(&body).ok(),
            Ok((false, _)) => None,
            Err(_) => None,
        }
    }

    fn maybe_remove_start_ini(&self) {
        // Once the EA is talking to the bridge the terminal has obviously read
        // the startup ini, so the one file that ever held the account password
        // can go away.
        let ini = self.cfg.start_ini_path();
        if ini.exists() && lock(&self.status).ea_connections > 0 {
            process::remove_file(&ini);
            log("removed the terminal startup ini (the password is no longer on disk)");
        }
    }

    fn wine_version(&self) -> Option<String> {
        let mut command = Command::new(&self.cfg.wine_bin);
        command.arg("--version");
        match process::run_capture("wine --version", &mut command, Duration::from_secs(30)) {
            Ok((true, output)) => Some(output.trim().to_string()).filter(|v| !v.is_empty()),
            _ => None,
        }
    }

    // ---- helpers -----------------------------------------------------------

    fn download(&self, url: &str, destination: &Path) -> Result<(), String> {
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
        let mut command = Command::new(&self.cfg.curl_bin);
        command
            .arg("-fsSL")
            .arg("--retry")
            .arg("3")
            .arg("--retry-delay")
            .arg("5")
            .arg("-o")
            .arg(destination)
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        process::run(
            &format!("curl {url}"),
            &mut command,
            Duration::from_secs(self.cfg.timeouts.prefix_download_secs),
        )
        .map_err(|error| format!("downloading {url} failed: {error}"))?;
        let size = process::file_len(destination).unwrap_or(0);
        if size == 0 {
            return Err(format!("downloading {url} produced an empty file"));
        }
        log(&format!("downloaded {url} ({size} bytes)"));
        Ok(())
    }

    fn find_terminal(&self) -> Option<PathBuf> {
        let drive_c = self.cfg.wine_prefix.join("drive_c");
        if !drive_c.is_dir() {
            return None;
        }
        search_tree(&drive_c, 4, "terminal64.exe")
            .or_else(|| search_tree(&drive_c, 4, "terminal.exe"))
    }
}

// ---- free functions --------------------------------------------------------

/// Reap a child if it has exited; returns its status and clears the slot.
fn reap(slot: &mut Option<Child>) -> Option<std::process::ExitStatus> {
    let status = match slot.as_mut() {
        Some(child) => match child.try_wait() {
            Ok(Some(status)) => Some(status),
            _ => None,
        },
        None => None,
    };
    if status.is_some() {
        *slot = None;
    }
    status
}

/// Breadth-limited search for a file name, case-insensitively.
fn search_tree(dir: &Path, depth: usize, name: &str) -> Option<PathBuf> {
    if depth == 0 {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    let mut directories = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            directories.push(path);
            continue;
        }
        if path
            .file_name()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case(name))
            .unwrap_or(false)
        {
            return Some(path);
        }
    }
    for directory in directories {
        if let Some(found) = search_tree(&directory, depth - 1, name) {
            return Some(found);
        }
    }
    None
}

fn find_sibling(dir: &Path, name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    entries.flatten().map(|entry| entry.path()).find(|path| {
        path.file_name()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case(name))
            .unwrap_or(false)
    })
}

/// Wine needs Windows-style paths for arguments that carry a colon (a Unix path
/// after `/compile:` would be read as a drive spec), and `Z:` maps to `/`.
pub fn win_path(path: &Path) -> String {
    let unix = path.display().to_string().replace('/', "\\");
    if unix.starts_with('\\') {
        format!("Z:{unix}")
    } else {
        format!("Z:\\{unix}")
    }
}

/// `/tmp/.X11-unix/X99` for display `:99` (and `:99.0`).
pub fn display_socket_path(display: &str) -> PathBuf {
    match display_number(display) {
        Some(number) => PathBuf::from(format!("/tmp/.X11-unix/X{number}")),
        None => PathBuf::from("/tmp/.X11-unix/X99"),
    }
}

pub fn display_number(display: &str) -> Option<u32> {
    let trimmed = display.trim_start_matches(':');
    let number = trimmed.split('.').next().unwrap_or(trimmed);
    number.parse::<u32>().ok()
}

/// HTTP base of Node 4, derived from the bridge's WebSocket URL:
/// `wss://host/mt5/bridge` -> `https://host`.
pub fn node4_http_base(ws_url: &str) -> Option<String> {
    let (scheme, rest) = if let Some(rest) = ws_url.strip_prefix("wss://") {
        ("https", rest)
    } else if let Some(rest) = ws_url.strip_prefix("ws://") {
        ("http", rest)
    } else {
        return None;
    };
    let host = rest.split('/').next().unwrap_or(rest);
    if host.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{host}"))
}

/// Trailing part of a MetaEditor log, for error messages.
fn tail(report: &str) -> String {
    let lines: Vec<&str> = report
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if lines.is_empty() {
        return String::new();
    }
    let start = lines.len().saturating_sub(8);
    format!("\n  {}", lines[start..].join("\n  "))
}

/// MetaEditor prints `; N errors, M warnings`. `None` when the summary line is
/// missing (a localised log, or a compile that never ran).
pub fn compile_error_count(report: &str) -> Option<u32> {
    for line in report.lines().rev() {
        let lowered = line.to_ascii_lowercase();
        if let Some(position) = lowered.find("error") {
            let prefix = &line[..position];
            if let Some(number) = prefix
                .split(|c: char| !c.is_ascii_digit())
                .filter(|part| !part.is_empty())
                .next_back()
            {
                if let Ok(value) = number.parse::<u32>() {
                    return Some(value);
                }
            }
        }
    }
    None
}

fn json_bool(value: &serde_json::Value, pointer: &str) -> Option<bool> {
    value.pointer(pointer).and_then(|item| item.as_bool())
}

fn json_str(value: &serde_json::Value, pointer: &str) -> Option<String> {
    value
        .pointer(pointer)
        .and_then(|item| item.as_str())
        .map(|item| item.to_string())
}

fn json_u64(value: &serde_json::Value, pointer: &str) -> Option<u64> {
    value.pointer(pointer).and_then(|item| item.as_u64())
}

fn json_i64(value: &serde_json::Value, pointer: &str) -> Option<i64> {
    value.pointer(pointer).and_then(|item| item.as_i64())
}

/// Emit a message at most once per hour, using the shared poll timestamp slot.
fn log_once(slot: &mut i64, message: &str) {
    let now = now_ms();
    if now - *slot >= 3_600_000 {
        *slot = now;
        log(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_paths_use_the_z_drive_mapping() {
        assert_eq!(
            win_path(Path::new("/data/wine/x.exe")),
            "Z:\\data\\wine\\x.exe"
        );
        assert_eq!(win_path(Path::new("relative/x")), "Z:\\relative\\x");
    }

    #[test]
    fn display_sockets_and_numbers() {
        assert_eq!(
            display_socket_path(":99"),
            PathBuf::from("/tmp/.X11-unix/X99")
        );
        assert_eq!(
            display_socket_path(":99.0"),
            PathBuf::from("/tmp/.X11-unix/X99")
        );
        assert_eq!(display_number(":1"), Some(1));
        assert_eq!(display_number("nonsense"), None);
    }

    #[test]
    fn node4_http_base_is_derived_from_the_websocket_url() {
        assert_eq!(
            node4_http_base("wss://exec.onrender.com/mt5/bridge").unwrap(),
            "https://exec.onrender.com"
        );
        assert_eq!(
            node4_http_base("ws://127.0.0.1:10000/mt5/bridge").unwrap(),
            "http://127.0.0.1:10000"
        );
        assert_eq!(
            node4_http_base("wss://exec.onrender.com").unwrap(),
            "https://exec.onrender.com"
        );
        assert!(node4_http_base("https://exec.onrender.com").is_none());
    }

    #[test]
    fn the_compile_summary_is_parsed() {
        let log = "; MetaEditor 5 build 4000\r\nMt5BridgeEA.mq5 : information: compiling\r\n; 0 errors, 0 warnings\r\n";
        assert_eq!(compile_error_count(log), Some(0));
        let bad = "; 3 errors, 1 warning\r\n";
        assert_eq!(compile_error_count(bad), Some(3));
        assert_eq!(compile_error_count("no summary here"), None);
    }

    #[test]
    fn the_tree_search_finds_a_terminal_by_name_case_insensitively() {
        let root = std::env::temp_dir().join(format!("mt5-host-tree-{}", std::process::id()));
        let nested = root.join("Program Files").join("MetaTrader 5");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("Terminal64.EXE"), b"fake").unwrap();
        assert_eq!(
            search_tree(&root, 4, "terminal64.exe"),
            Some(nested.join("Terminal64.EXE"))
        );
        assert!(search_tree(&root, 4, "metaeditor64.exe").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sibling_lookup_is_case_insensitive() {
        let dir = std::env::temp_dir().join(format!("mt5-host-sibling-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("MetaEditor64.exe"), b"fake").unwrap();
        assert!(find_sibling(&dir, "metaeditor64.exe").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
