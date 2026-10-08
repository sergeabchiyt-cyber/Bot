# mt5-host — the headless MT5 demo terminal host

Node 4 executes on a Linux host. MetaTrader 5 is a Windows GUI application. This
crate is the bridge between those two facts: **one Rust binary** that turns a
plain Linux container into a machine that runs an MT5 demo terminal with the
bridge Expert Advisor attached, fully unattended, with no desktop and no
inbound control port.

It supersedes the earlier Windows / Wine-desktop deployment assumption for this
repository. There is no RDP, no VNC, no `wine explorer` desktop, no manual
"attach the EA" step, and no operator clicking through an installer at three in
the morning: everything below the bridge's WebSocket session to Node 4 is
performed by this binary from environment configuration.

```
Render web service (Linux container, 0.1 CPU / 512 MB on the free plan)
┌──────────────────────────────────────────────────────────────────────────┐
│ mt5-host            supervisor + GET /health, /readyz, /diagnostics       │
│  ├── Xvfb :99       virtual display; nothing is ever rendered anywhere    │
│  ├── mt5-bridge     WSS client of Node 4   ◄── trade_intent / reports     │
│  └── wine terminal64.exe /portable /config:mt5-start.ini                  │
│        └── Mt5BridgeEA.ex5 ───loopback TCP :5055───► mt5-bridge           │
└──────────────────────────────────────────────────────────────────────────┘
                                   │ outbound only
                                   ▼
                        Node 4  wss://…/mt5/bridge
```

The host never trades. It has no order path at all: it starts processes, writes
two configuration files, compiles the EA, and reports what it sees. Orders exist
only on the bridge's Node 4 session, exactly as they do when the bridge runs on
a Windows terminal host.

## What "automated" means here — the boot stages

`mt5-host` runs these stages in order at startup, retrying a failed stage with a
15 s → 5 min backoff and reporting progress on `/diagnostics`:

| # | stage | what it does | ready when |
|---|-------|--------------|-----------|
| 1 | `state_dir` | creates `$MT5_HOST_STATE_DIR` (`/data`) | directory exists |
| 2 | `display` | cleans stale `/tmp/.X99-lock` + `/tmp/.X11-unix/X99`, starts `Xvfb :99 -screen 0 640x480x16 -ac -nolisten tcp -noreset` | the X socket exists |
| 3 | `prefix` | unpacks `$MT5_HOST_PREFIX_ARCHIVE_URL` if set, else runs `wineboot -u` with `WINEPREFIX=/data/wine`, `WINEARCH=win64`, `WINEDEBUG=-all`, `WINEDLLOVERRIDES="mscoree,mshtml="` | `drive_c` exists |
| 4 | `terminal` | downloads `mt5setup.exe` if the image did not bake it, runs **`wine mt5setup.exe /auto`**, then finds `terminal64.exe`/`terminal.exe` by bounded case-insensitive search under the prefix | a terminal executable is found |
| 5 | `ea` | writes the EA to `<install>/MQL5/Experts/Node4/Mt5BridgeEA.mq5` and compiles it with `wine metaeditor64.exe /compile:Z:\… /log:Z:\…`, parsing the `; N errors` summary. A prefix whose `.ex5` already matches the embedded source is skipped | `.ex5` exists, 0 errors |
| 6 | `startup_config` | writes `$MT5_HOST_STATE_DIR/mt5-start.ini` (`[Common]` Login/Password/Server, `[Experts]` AllowLiveTrading=1/AllowDllImport=0/Enabled=1/Account=1, `[StartUp]` Expert/Symbol/Period/ExpertParameters) plus the EA preset in `<install>/MQL5/Presets/`, both mode 0600 | file written |
| 7 | *run* | starts `mt5-bridge`, then `wine terminal64.exe /portable /config:Z:\…mt5-start.ini` | `ea_connections > 0` |

The startup ini is the only place the terminal password exists on disk. It is
written 0600 and **deleted as soon as the EA connects** — after that the terminal
holds the session.

Then supervision runs every 2 s: it reaps children, respawns the bridge, restarts
the terminal when it exits (and when no EA has connected within
`MT5_HOST_EA_WAIT_SECS`), holds restarts for 5 minutes after
`MT5_HOST_MAX_RESTARTS_PER_HOUR` in an hour, refreshes `ea_connections` from
`/proc/net/tcp` and the memory footprint, and polls Node 4's `/mt5/status` +
`/mt5/account` so `/diagnostics` shows what Node 4 thinks of this bridge.

`MT5_HOST_PREPARE_ONLY=1` stops after stage 6 and exits — that is how the
pre-baked prefix image is built, and how you smoke-test an install with no
credentials and no Node 4.

## Deploy on Render (free plan)

### 1. Build the prefix bundle (recommended, and the difference between a 3-minute and a 20-minute boot)

```sh
docker build -f mt5-host/Dockerfile -t mt5-host:local .
docker build -f ci/mt5/Dockerfile.bundle-image \
    --build-arg MT5_HOST_IMAGE=mt5-host:local -t registry.example/mt5-host:prefix .
docker push registry.example/mt5-host:prefix
```

`ci/mt5/Dockerfile.bundle-image` runs the boot stages once *at build time*
(installing the terminal into the Wine prefix) and bakes the result at
`/opt/mt5-prefix`. On a Render free instance — where `/data` is wiped on every
deploy and every wake-up — the entrypoint copies that prefix into `/data/wine`
in seconds instead of downloading and installing MT5 again. The prefix is
credential-free by construction (`MT5_HOST_PREPARE_ONLY=1`, enforced by
`validate()` and by `ci/mt5/protocol_lint.py`), so the image is safe to publish.

Deploy that image. If it is not in a registry the Render builder can pull, use
the plain `mt5-host/Dockerfile` image instead and accept the cold install.

The bundle also ships the same prefix as a gzipped tarball for the
`MT5_HOST_PREFIX_ARCHIVE_URL` path:

```sh
docker run --rm -v "$PWD/out:/out" --entrypoint cp mt5-host:prefix \
    /opt/mt5/mt5-prefix.tar.gz /out/
# host it somewhere reachable, then set MT5_HOST_PREFIX_ARCHIVE_URL to it
```

### 2. Create the service

`mt5-host/render.yaml` is the whole configuration (copy it to the repository
root if you want Render's Blueprint flow; Render only reads `/render.yaml`).
By hand, the settings that matter:

| setting | value |
|---------|-------|
| Type / runtime | Web Service, Docker |
| Dockerfile path | `mt5-host/Dockerfile` (or `ci/mt5/Dockerfile.bundle-image`) |
| Docker build context | `.` (the repository root — the build needs `mt5-bridge/`) |
| Instance type | Free |
| Health check path | **`/health`** — never `/readyz`, see below |
| Region | as close as possible to Node 4; every intent crosses that link |

Then set the environment from `mt5-host/.env.example`. The minimum for a
monitor-only deployment is:

```sh
MT5_HOST_STATE_DIR=/data
MT5_LOGIN=…            # MQL5 demo account
MT5_PASSWORD=…
MT5_SERVER=…           # exact name from the terminal's login box
NODE4_WS_URL=wss://your-node4-service.onrender.com/mt5/bridge
MT5_BRIDGE_TOKEN=…     # must equal Node 4's MT5_BRIDGE_TOKEN
MT5_SYMBOL=XAUUSD
MT5_TRADING_ENABLED=0
```

`NODE4_WS_URL` and `MT5_BRIDGE_TOKEN` are required (the host exits `2` without
them, launching nothing). `MT5_EA_TOKEN` is optional: without it the bridge is
read-only, which is a perfectly good monitoring deployment. Everything else has
a default — see `mt5-host/.env.example`, which is checked against the code by
`ci/mt5/protocol_lint.py` in both directions so the two cannot drift.

> The environment is read from the *process*, never from a `.env` file. On
> Render that is automatic; locally, `set -a; . mt5-host/.env; set +a`.

### 3. Watch it come up

```sh
curl -s https://<service>.onrender.com/health      # 200 "ok" as soon as the process is alive
curl -s https://<service>.onrender.com/readyz      # 503 + one reason per line until it is done
curl -s https://<service>.onrender.com/diagnostics | python3 -m json.tool
```

`/health` is the platform health check on purpose: a first boot spends minutes
inside Wine, and a health check pointed at `/readyz` would fail the deploy for a
container that is busy doing exactly what it should. `/readyz` is for monitoring
and returns `503` with the reasons, e.g.:

```
no Expert Advisor is attached to the bridge port
terminal64.exe is not running
MetaTrader 5 terminal is not installed
```

## Free-plan realities (checked against Render's documentation)

These are properties of the plan, not bugs in this crate. Each one has a
mitigation, and the mitigations are why the bundle image exists.

| reality | consequence for MT5 | mitigation |
|---------|--------------------|-----------|
| A free instance **spins down after 15 minutes without inbound traffic**, and local filesystem changes are lost | the terminal, its login session, the EA and the bridge all die; `/data` is wiped; Node 4 sees the bridge disconnect | point a monitor (or a Node 4 side job) at `GET /health` every 5–10 minutes to keep it awake; otherwise accept the cycle and rely on the bundle image for a fast re-boot |
| **750 free instance hours per workspace per month**, and spun-down services do not consume them | a 31-day month is 744 hours, so *one* always-on free service fits and *two* do not; Node 4 + host both always-on will run out near the end of the month and be suspended until the 1st | let the terminal host sleep when nobody is trading, or budget the hours deliberately |
| **No persistent disk** on free instances | the prefix, the terminal's own state, the bridge history and the execution ledger live in the container and disappear on redeploy/restart/spin-down | bake the prefix into the image; accept that the login is repeated; keep durable records on Node 4 (its ledger is the authoritative one) |
| Render **may restart a free instance at any time** | the same as above, unplanned | the host's stages are idempotent: a restart re-runs them from wherever the prefix is |
| **No shell access, no one-off jobs** on free | you cannot `docker exec` in to debug, and you cannot run the prepare step as a Render job | build the bundle image locally or in CI and push it; debug locally with `docker run` |
| Service-initiated traffic thresholds | the terminal's initial market-data download is a burst of outbound traffic | it happens once per fresh prefix; the bundle image avoids re-downloading on every deploy |
| Free instances are 0.1 CPU (a shared fraction) | first boot is slow in wall-clock terms, and everything CPU-bound is slow | the bundle image removes the biggest CPU cost (install + compile) from the boot path |

Two operational notes that follow from the table:

* **Use unattended login** (`MT5_LOGIN`/`MT5_PASSWORD`/`MT5_SERVER`) rather than
  `MT5_HOST_MANUAL_LOGIN=1`. A manual login does not survive a spin-down or a
  redeploy, so on the free plan it means a terminal that is logged out every
  morning. Note that the MetaQuotes terminal accepts MQL5 demo accounts; Deriv
  demo credentials are not guaranteed to work in it.
* **Trading is off by default** and should stay off until the demo integration
  gate in `docs/mt5/EXECUTION_ARCHITECTURE.md` passes.

## Resource budget (0.1 CPU / 512 MB)

The container holds Xvfb, a Wine server, the MT5 terminal and two small Rust
processes. Nothing renders, there is one chart and one fixed period, and trading
is off. Rough expectations for RSS — **not measured in this repository**, these
are order-of-magnitude figures for the pieces involved:

| piece | RSS |
|-------|-----|
| `mt5-host` + `mt5-bridge` | 10–20 MB together |
| `Xvfb` (640×480×16) | 10–20 MB |
| `wineserver` + Wine services | 20–50 MB |
| `terminal64.exe` with one chart, market watch, history | 150–300 MB |

Total ≈ 200–400 MB, which fits — with little headroom. The host warns on
`/diagnostics` (`memory_mb`, and a log line) past `MT5_HOST_RSS_WARN_MB`
(default 420). If the container is OOM-killed, Render restarts it and the
supervisor starts over; nothing here can prevent that, only avoid it. What blows
the budget, in order of likelihood: extra charts/periods, a large market watch
with many symbols, Wine debug output (`WINEDEBUG` must stay `-all`), a second
terminal instance, and the installer running on top of a live terminal. If you
need more headroom, a paid instance or a small VPS with a persistent disk (which
also keeps the login across restarts) is the honest answer.

## HTTP surface

Read-only by construction. There is no control endpoint: stopping or restarting
the terminal is a redeploy, and order-level control lives on Node 4
(`POST /mt5/control` behind its own token).

| endpoint | response |
|----------|----------|
| `GET /health` | `200 ok` while the process is alive, whatever state the terminal is in |
| `GET /readyz` | `200 ready` only when the display, prefix, install, EA, startup ini, terminal pid, bridge pid and at least one EA connection all hold — and when Node 4, if reachable, does not report the EA link as down or an error. Otherwise `503` with one reason per line |
| `GET /diagnostics` | JSON: `stage`, `stages_done`, `install_dir`, `wine_version`, `checks`, `processes` (pids, starts, `ea_connections`, `restarts_last_hour`), `node4` (Node 4's view: `connected`, `authorized`, `ea_connected`, `ea_mode`, `halted`, `halt_reason`, `login`, `account_type`, `orders_sent`, `orders_filled`), `memory_mb`, `trading_enabled` |
| anything else | `404`; non-`GET` methods get `405` |

No credential-shaped field can appear in `/diagnostics` — asserted by unit tests
in `state.rs` and `config.rs`.

## Boot timeline on a cold free instance

Order-of-magnitude, 0.1 CPU, and the reason the bundle image is the recommended
path:

| | with the prefix bundle | plain image |
|---|---|---|
| prefix copy / `wineboot` | 10–60 s | 30–90 s |
| terminal install (`mt5setup.exe /auto`) | — | 5–20 min |
| EA compile (first time only) | — | 1–3 min |
| terminal start, login, EA attach | 30–120 s | 30–120 s |
| **to `/readyz` 200** | **≈ 1–4 min** | **≈ 10–25 min** |

## Troubleshooting

`/readyz` names the reason; match it here.

| reason | usual cause | what to do |
|--------|-------------|-----------|
| `virtual display is not up` | `Xvfb` missing or a stale lock/socket from a previous run | the host cleans `/tmp/.X99-lock` and `/tmp/.X11-unix/X99` itself; check `MT5_HOST_XVFB_BIN` and the image |
| `Wine prefix is not initialised` | `wineboot` failed or timed out | raise `MT5_HOST_WINEBOOT_TIMEOUT_SECS`; check the log for the Wine version line; the Debian/WineHQ flavor build argument exists for a reason |
| `MetaTrader 5 terminal is not installed` | installer unreachable, failed, or installed outside the prefix | `/diagnostics` shows whether the download happened; check `MT5_HOST_INSTALLER_URL` reachability from Render, or bake the installer/bundle |
| `the bridge Expert Advisor is not compiled` | `metaeditor64` reported errors | the reason (with the tail of the MetaEditor log) is in the container's log stream and in `last_error` on `/diagnostics`; the compiler log itself is read, reported and removed |
| `terminal64.exe is not running` | login rejected, terminal crashed, or restarts are being held | look for the restart hold (`restarts_last_hour` ≥ cap → 5-minute hold) and for `invalid account`/`authorization failed` in the terminal log; a wrong `MT5_SERVER` string is the usual culprit |
| `no Expert Advisor is attached to the bridge port` | terminal up but the EA did not attach | the EA must be in `MQL5/Experts/Node4/`, `[StartUp] Expert` must use the `Node4\` reference, and `[Experts] Enabled=1` must hold — all written by the host; check `mt5-bridge` log lines about the EA |
| `mt5-bridge is not running` | bridge refused to start (bad config, bad token, malformed symbol map) | the bridge logs its reason on stderr, which is this container's log stream |
| `Node 4 reports the EA link as down` | the bridge connected but Node 4 does not see the EA frames | compare `/diagnostics.node4` with Node 4's own `/mt5/status`; the loopback path EA → bridge may be broken while the WSS path is fine |
| `Node 4 reports: …` | Node 4 refused the bridge (token, protocol version, account not demo, halted) | fix it on Node 4; the host deliberately does not retry its way around an authorization failure |

## What is verified, and what is not

Honest status of this deployment path:

**Verified in this repository (CI and local `cargo`):**

* `mt5-host` compiles with no warnings under `cargo check --all-targets`,
  `cargo clippy --all-targets -- -D warnings`, and `cargo fmt --all -- --check`.
* 37 unit tests pass: configuration validation and the fail-closed rules,
  readiness and JSON rendering, the `/proc` helpers (including the
  established-connection count used for `ea_connections`, and the `SLOT` column
  parsing bug that made it always zero), timeouts that kill children, the
  startup ini / EA preset / fallback profile generators, the EA source that is
  compiled into the binary, and the HTTP surface (`/health`, `/readyz`,
  `/diagnostics`, `404`, `405`).
* `ci/mt5/protocol_lint.py` cross-checks the deployment surface and fails the
  build if the environment documentation and the code drift, if the embedded EA
  is not the one the bridge contract tests use, if the bundle image could bake a
  credential into a published prefix, if the image loses a binary or the
  entrypoint grows a second supervisor, if the host ever grows an order path, or
  if the Render health check points at `/readyz`.
* The release binary was **run** on a Linux host: with no configuration it
  exits `2` naming every missing variable and launching nothing; with a valid
  configuration and a deliberately broken `Xvfb` it serves `GET /health` → `200
  ok`, `GET /readyz` → `503` with its eight reasons one per line, `GET
  /diagnostics` → the JSON documented above (including `last_error` naming the
  display failure), `POST /health` → `405`, `GET /nope` → `404`, and the
  password appears nowhere in either the responses or the log stream.

**Not verified — no terminal, no Wine and no container runtime exist in this
sandbox:**

* That either Dockerfile builds: there is no container runtime here, so
  `mt5-host/Dockerfile` and `ci/mt5/Dockerfile.bundle-image` have been written
  and reviewed by hand but never built. Expect the usual first-build fixups
  (package names, Wine flavor, an installer fetch that fails behind a proxy).
* That `download.mql5.com` serves `mt5setup.exe` in your build/runtime
  environment (URL and the `/auto` switch are the documented, widely used ones,
  but the fetch itself is not exercised here).
* That Wine + `terminal64.exe` boots and stays inside 512 MB, and how long a
  cold install actually takes on 0.1 CPU. The figures above are estimates.
* That a given MQL5 demo account logs in unattended from `[Common]` with the
  server string you use.
* That `metaeditor64 /compile` succeeds inside the image for the embedded EA
  (the source is the same one the bridge contract tests and the protocol lint
  agree on, but it has never been compiled by MetaTrader).
* That Node 4 and the host complete a real `execution_hello`/snapshot exchange
  through this container. Everything up to the WebSocket is tested; the session
  itself needs a deployment.
* The sandbox had no crates.io, so the checks above ran against a faithful API
  stand-in for `serde_json` instead of the real crate: `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets` (37
  tests), `cargo build --release` and the runtime smoke test all pass with it,
  but the first `cargo test` on a machine with registry access is the
  authoritative run. The crate uses nothing beyond `Value`, the `json!` macro,
  `from_str::<Value>`, `pointer` and the `as_bool`/`as_str`/`as_u64`/`as_i64`
  accessors, and has no `Deserialize` derive anywhere — so there is no API
  surface a stand-in could have faked into passing.

The honest summary: the orchestration logic is tested, the wiring between the
crate, the bridge, the EA and the image is linted, and the terminal itself has
not been touched. Treat the first deployment as the integration test it is, and
watch `/readyz` rather than `/health`.

## Local development

```sh
# unit tests, no Wine and no terminal required
cd mt5-host && cargo test

# the deployment lint (from the repository root)
python3 ci/mt5/protocol_lint.py

# a real container: the whole path, one port
docker build -f mt5-host/Dockerfile -t mt5-host .
docker run --rm -p 10000:10000 --env-file mt5-host/.env mt5-host
curl -s localhost:10000/readyz
```

To exercise the boot stages without credentials and without Node 4:

```sh
docker run --rm -e MT5_HOST_PREPARE_ONLY=1 -e MT5_LOGIN= -e MT5_PASSWORD= -e MT5_SERVER= \
    -v "$PWD/prefix:/data" mt5-host
```

That installs the terminal into `./prefix/wine`, compiles the EA, writes the
startup ini and exits with `0` (or `1`, with the reason, if any stage failed).
It is the fastest way to see whether an installer or a Wine version is the
problem before blaming Render.

## Layout

| path | what it is |
|------|-----------|
| `src/config.rs` | the environment contract, validation, `wine_env()`/`bridge_env()`, secret-free `summary()` |
| `src/state.rs` | what `/health`, `/readyz` and `/diagnostics` report, and what "ready" means |
| `src/process.rs` | Linux process helpers: `/proc` reading, timeouts, `ESTABLISHED` counting for the EA link, 0600 secret files |
| `src/templates.rs` | the startup ini, the EA preset and the fallback chart profile; the EA source embedded at compile time |
| `src/stages.rs` | the supervisor: boot stages, restarts, observations, Node 4 polling |
| `src/health.rs` | the read-only HTTP surface on `$PORT` |
| `Dockerfile` | Xvfb + Wine + both binaries; the image Render builds |
| `docker-entrypoint.sh` | seeds a writable prefix from a baked one, then `exec mt5-host` |
| `render.yaml` | the service definition (health check, image, secrets) |
| `ci/mt5/Dockerfile.bundle-image` | the pre-installed prefix image / tarball artifact |
