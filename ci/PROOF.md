# CI proof — run 36464959381
commit: 616d374c7d31c81753aedb82701f9cb557242605
generated: 2026-09-28T18:26:09Z

## Unit tests
   Compiling smallvec v1.16.2
   Compiling zerocopy v0.8.59
   Compiling ring v0.17.14
   Compiling base64 v0.23.1
   Compiling parking_lot_core v0.9.12
   Compiling parking_lot v0.12.5
   Compiling icu_normalizer v2.3.0
   Compiling tokio v1.53.1
   Compiling idna_adapter v1.2.2
   Compiling idna v1.1.0
   Compiling rustls-webpki v0.103.15
   Compiling siphasher v1.0.4
   Compiling rustls v0.23.45
   Compiling phf_shared v0.12.1
   Compiling url v2.5.8
   Compiling ppv-lite86 v0.2.21
   Compiling rand_chacha v0.9.0
   Compiling rand v0.9.5
   Compiling phf v0.12.1
   Compiling dashmap v6.2.1
   Compiling tungstenite v0.29.0
   Compiling tracing-subscriber v0.3.23
   Compiling hyper v1.11.1
   Compiling hyper-util v0.1.21
   Compiling tokio-rustls v0.26.6
   Compiling tower v0.5.3
   Compiling tokio-util v0.7.19
   Compiling async-compression v0.4.48
   Compiling tower-http v0.6.11
   Compiling tungstenite v0.26.2
   Compiling hyper-rustls v0.27.10
   Compiling tokio-tungstenite v0.29.0
   Compiling reqwest v0.12.28
   Compiling tokio-tungstenite v0.26.2
   Compiling axum v0.8.9
   Compiling chrono-tz v0.10.4
   Compiling xauusd-engine v0.3.0 (/home/runner/work/Bot/Bot)
warning: field `symbol_type` is never read
  --> src/types.rs:30:9
   |
 8 | pub struct AggTrade {
   |            -------- field in this struct
...
30 |     pub symbol_type: Option<i32>,
   |         ^^^^^^^^^^^
   |
   = note: `AggTrade` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: field `subscriptions` is never read
  --> src/ws_server.rs:28:9
   |
26 | pub struct AppState {
   |            -------- field in this struct
27 |     pub tx: broadcast::Sender<WsFrame>,
28 |     pub subscriptions: Arc<DashMap<String, Vec<String>>>,
   |         ^^^^^^^^^^^^^
   |
   = note: `AppState` has a derived impl for the trait `Clone`, but this is intentionally ignored during dead code analysis

warning: `xauusd-engine` (bin "xauusd-engine" test) generated 2 warnings
    Finished `test` profile [unoptimized + debuginfo] target(s) in 17.19s
     Running unittests src/main.rs (target/debug/deps/xauusd_engine-77f3e241cd78650f)

running 39 tests
test calendar::live_tests::real_forexfactory_feed_returns_events ... ignored, requires network
test calendar::tests::handles_empty_feed ... ok
test calendar::tests::impact_colors_are_normalized ... ok
test calendar::tests::markdown_fallback_skips_headers_and_separators ... ok
test calendar::tests::parses_forexfactory_weekly_json ... ok
test calendar::tests::rejects_html_rate_limit_page ... ok
test config::tests::cors_origin_is_normalized_for_header_comparison ... ok
test config::tests::default_origin_is_already_in_header_form ... ok
test config::tests::from_env_falls_back_to_the_node2_site ... ok
test sifting_rest::tests::a_short_response_is_rejected_instead_of_seeding_a_partial_profile ... ok
test calendar::tests::source_has_fallback_urls ... ok
test sifting_rest::tests::history_request_is_a_single_fixed_two_thousand_bar_page ... ok
test sifting_ws::tests::candle_volume_is_the_tick_count_like_sifting_history ... ok
test sifting_ws::tests::bucket_roll_emits_one_closed_bar_then_a_fresh_live_bar ... ok
test sifting_ws::tests::resubscribe_snapshot_and_late_ticks_are_not_counted ... ok
test sifting_ws::tests::handle_text_writes_the_store_and_broadcasts_candle_plus_tick_volume ... ok
test sifting_ws::tests::rolling_rate_counts_the_last_ten_seconds ... ok
test tick_volume::tests::capacity_is_enforced_oldest_first ... ok
test sifting_ws::tests::tick_volume_frame_wire_shape ... ok
test sifting_ws::tests::tick_rule_splits_up_down_and_flat ... ok
test tick_volume::tests::upsert_replaces_the_live_bar_and_appends_new_ones ... ok
test tick_volume::tests::out_of_order_bars_stay_sorted ... ok
test volume_profile::tests::bearish_swing_is_anchored_high_to_low ... ok
test volume_profile::tests::duplicate_candle_timestamp_is_replaced ... ok
test volume_profile::tests::bullish_swing_is_anchored_low_to_high ... ok
test volume_profile::tests::ingesting_a_candle_after_a_close_refreshes_ps ... ok
test volume_profile::tests::session_boundary_survives_dst_change ... ok
test volume_profile::tests::ps_skips_the_weekend_gap ... ok
test volume_profile::tests::session_close_is_1700_new_york ... ok
test volume_profile::tests::ps_rolls_forward_at_every_session_close ... ok
test ws_server::tests::allowed_origin_may_read_every_rest_route ... ok
test volume_profile::tests::week_windows_do_not_overlap_the_session_window ... ok
test ws_server::tests::malformed_origin_panics_at_boot ... ok
test ws_server::tests::cors_grant_is_scoped_to_the_configured_origin ... ok
test ws_server::tests::preflight_is_answered_for_the_allowed_origin_only ... ok
test ws_server::tests::payload_shape_is_untouched_by_the_layer ... ok
test ws_server::tests::tick_volume_route_serves_the_live_bars_uncached ... ok
test volume_profile::tests::pw_excludes_weekend_candles_and_is_stable_all_week ... ok
test ws_server::tests::only_the_configured_origin_is_ever_allowed ... ok

test result: ok. 38 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.01s


## Live calendar feed test
warning: field `symbol_type` is never read
  --> src/types.rs:30:9
   |
 8 | pub struct AggTrade {
   |            -------- field in this struct
...
30 |     pub symbol_type: Option<i32>,
   |         ^^^^^^^^^^^
   |
   = note: `AggTrade` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: field `subscriptions` is never read
  --> src/ws_server.rs:28:9
   |
26 | pub struct AppState {
   |            -------- field in this struct
27 |     pub tx: broadcast::Sender<WsFrame>,
28 |     pub subscriptions: Arc<DashMap<String, Vec<String>>>,
   |         ^^^^^^^^^^^^^
   |
   = note: `AppState` has a derived impl for the trait `Clone`, but this is intentionally ignored during dead code analysis

warning: `xauusd-engine` (bin "xauusd-engine" test) generated 2 warnings
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.10s
     Running unittests src/main.rs (target/debug/deps/xauusd_engine-77f3e241cd78650f)

running 1 test
LIVE CALENDAR: 141 events
  2026-09-27T23:50:00+00:00 | JPY | Low | Monetary Policy Meeting Minutes | forecast=None previous=None gold=false
  2026-09-27T23:50:00+00:00 | JPY | Low | SPPI y/y | forecast=Some("3.6%") previous=Some("3.6%") gold=false
  2026-09-28T10:00:00+00:00 | GBP | Low | MPC Member Ramsden Speaks | forecast=None previous=None gold=false
  2026-09-28T12:15:00+00:00 | USD | Low | FOMC Member Bowman Speaks | forecast=None previous=None gold=false
  2026-09-28T13:30:00+00:00 | EUR | Medium | ECB President Lagarde Speaks | forecast=None previous=None gold=false
  2026-09-28T17:25:00+00:00 | USD | Low | FOMC Member Cook Speaks | forecast=None previous=None gold=false
  2026-09-28T17:30:00+00:00 | USD | Low | FOMC Member Barkin Speaks | forecast=None previous=None gold=false
  2026-09-28T23:01:00+00:00 | GBP | Low | BRC Shop Price Index y/y | forecast=Some("1.5%") previous=Some("1.5%") gold=false
  2026-09-29T01:30:00+00:00 | AUD | Low | Household Spending m/m | forecast=Some("0.4%") previous=Some("1.1%") gold=false
  2026-09-29T04:30:00+00:00 | AUD | High | Cash Rate | forecast=Some("4.60%") previous=Some("4.35%") gold=false
test calendar::live_tests::real_forexfactory_feed_returns_events ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 38 filtered out; finished in 0.27s


## Runtime smoke test
--- waiting for /health ---
ok <- /health ok

--- /status ---
{"status":{"feeds":{"alltick":{"last_msg":0,"msgs":0,"state":"off (no token)"},"binance":{"last_msg":0,"msgs":0,"state":"connecting"},"bitget":{"last_msg":0,"msgs":0,"state":"connecting"},"bybit":{"last_msg":0,"msgs":0,"state":"connecting"},"calendar":{"last_msg":0,"msgs":0,"state":"starting"},"gate":{"last_msg":0,"msgs":0,"state":"connecting"},"itick":{"last_msg":0,"msgs":0,"state":"off (no token)"},"kraken":{"last_msg":0,"msgs":0,"state":"connecting"},"okx":{"last_msg":0,"msgs":0,"state":"connecting"},"sifting":{"last_msg":0,"msgs":0,"state":"off (no key)"}},"ts":1790619969257},"venue":"None","version":"0.3.0"}

--- /levels ---
[{"window":"PW","poc":3416.25,"vah":3426.0,"val":3395.0,"start":1789941600000,"end":1790370000000,"timestamp":1790619969241,"direction":"neutral","swing_high":null,"swing_low":null},{"window":"PS","poc":3431.75,"vah":3449.5,"val":3429.0,"start":1790456400000,"end":1790542800000,"timestamp":1790619969241,"direction":"neutral","swing_high":null,"swing_low":null},{"window":"SWING_BULL","poc":3431.75,"vah":3457.5,"val":3429.0,"start":1790526369238,"end":1790570469238,"timestamp":1790619969241,"direction":"bullish","swing_high":3463.5,"swing_low":3429.0}]
windows present: ['PS', 'PW', 'SWING_BULL']
PS window ends at 2026-09-27T17:00:00-04:00 (New York)
PS session window: 2026-09-26T21:00:00+00:00 -> 2026-09-27T21:00:00+00:00 (24.0h)
PS poc=3431.750 vah=3449.500 val=3429.000
PW direction=neutral poc=3416.250 vah=3426.000 val=3395.000
SWING_BULL direction=bullish poc=3431.750 vah=3457.500 val=3429.000
LEVELS CHECK PASSED

--- /calendar (waiting for first fetch) ---
source: ff_json | count: 141
--- first 15 events ---
  2026-09-27T23:50:00+00:00    |  JPY | Low      | Monetary Policy Meeting Minutes  (F:None P:None)
  2026-09-27T23:50:00+00:00    |  JPY | Low      | SPPI y/y  (F:3.6% P:3.6%)
  2026-09-28T10:00:00+00:00    |  GBP | Low      | MPC Member Ramsden Speaks  (F:None P:None)
  2026-09-28T12:15:00+00:00    |  USD | Low      | FOMC Member Bowman Speaks  (F:None P:None)
  2026-09-28T13:30:00+00:00    |  EUR | Medium   | ECB President Lagarde Speaks  (F:None P:None)
  2026-09-28T17:25:00+00:00    |  USD | Low      | FOMC Member Cook Speaks  (F:None P:None)
  2026-09-28T17:30:00+00:00    |  USD | Low      | FOMC Member Barkin Speaks  (F:None P:None)
  2026-09-28T23:01:00+00:00    |  GBP | Low      | BRC Shop Price Index y/y  (F:1.5% P:1.5%)
  2026-09-29T01:30:00+00:00    |  AUD | Low      | Household Spending m/m  (F:0.4% P:1.1%)
  2026-09-29T04:30:00+00:00    |  AUD | High     | Cash Rate  (F:4.60% P:4.35%)
  2026-09-29T04:30:00+00:00    |  AUD | High     | RBA Rate Statement  (F:None P:None)
  2026-09-29T05:30:00+00:00    |  AUD | Medium   | RBA Press Conference  (F:None P:None)
  2026-09-29T07:00:00+00:00    |  CHF | Low      | KOF Economic Barometer  (F:106.0 P:106.7)
  2026-09-29T07:00:00+00:00    |  EUR | Low      | Spanish Flash CPI y/y  (F:4.7% P:4.3%)
  2026-09-29T08:30:00+00:00    |  GBP | Low      | M4 Money Supply m/m  (F:0.1% P:-0.3%)
TOTAL EVENTS: 141
WITH TIMESTAMPS: 141
GOLD-RELEVANT (high-impact USD): 5
CALENDAR CHECK PASSED

--- /candles payload shape (must be untouched by the CORS layer) ---
candles: 2000 bars, sources=['synthetic']
first: time=1788819969238 close=3300.25
last:  time=1790619069238 close=3439.4875271016076
CANDLES CHECK PASSED

--- /tick-volume payload shape ---
tick-volume: 0 live bar(s) (no Sifting feed in this run)
TICK VOLUME CHECK PASSED

--- CORS: only the Node2 origin may read the REST API ---
1) allowed origin gets access-control-allow-origin on every route
  /health -> access-control-allow-origin: https://static-dash-frontend.onrender.com
  /levels -> access-control-allow-origin: https://static-dash-frontend.onrender.com
  /candles -> access-control-allow-origin: https://static-dash-frontend.onrender.com
  /tick-volume -> access-control-allow-origin: https://static-dash-frontend.onrender.com
  /calendar -> access-control-allow-origin: https://static-dash-frontend.onrender.com
  /status -> access-control-allow-origin: https://static-dash-frontend.onrender.com
2) preflight (OPTIONS) is authorised for that origin
  HTTP/1.1 200 OK|vary: origin, access-control-request-method, access-control-request-headers|access-control-allow-methods: GET,OPTIONS|access-control-allow-headers: content-type,accept,origin|access-control-max-age: 600|access-control-allow-origin: https://static-dash-frontend.onrender.com
3) every other origin gets NO allow-header
  /health (foreign) -> HTTP/1.1 200 OK, no CORS grant
  /levels (foreign) -> HTTP/1.1 200 OK, no CORS grant
  /candles (foreign) -> HTTP/1.1 200 OK, no CORS grant
  /tick-volume (foreign) -> HTTP/1.1 200 OK, no CORS grant
  /calendar (foreign) -> HTTP/1.1 200 OK, no CORS grant
  /status (foreign) -> HTTP/1.1 200 OK, no CORS grant
  OPTIONS /candles (foreign) -> HTTP/1.1 200 OK, no CORS grant
4) no wildcard, no credentials, no secrets in the response
  headers are CORS-only
5) /ws still upgrades and replays frames through the layer
handshake: HTTP/1.1 101 Switching Protocols
  sec-websocket-accept: kDn7a2JXrktNnmX7ALE8jT+GBXs=
  access-control-allow-origin: https://static-dash-frontend.onrender.com
  vary: origin, access-control-request-method, access-control-request-headers
frames received: ['candle', 'levels']
candle: source=synthetic close=3300.25
levels: PW poc=3416.25
WS CHECK PASSED

--- engine log: session/level/calendar/CORS lines ---
2026-09-28T18:26:09.241981Z  INFO xauusd_engine: PW: poc=3416.250 vah=3426.000 val=3395.000 direction=neutral
2026-09-28T18:26:09.242002Z  INFO xauusd_engine: PS: poc=3431.750 vah=3449.500 val=3429.000 direction=neutral
2026-09-28T18:26:09.242006Z  INFO xauusd_engine: SWING_BULL: poc=3431.750 vah=3457.500 val=3429.000 direction=bullish
2026-09-28T18:26:09.242218Z  INFO xauusd_engine::ws_server: CORS: browser access to the REST API allowed for this origin origin=https://static-dash-frontend.onrender.com
2026-09-28T18:26:09.242352Z  INFO xauusd_engine: Listening on 0.0.0.0:3000 — /health /status /levels /candles /tick-volume /calendar /ws
2026-09-28T18:26:09.287528Z  INFO xauusd_engine::calendar: Calendar: 141 events from direct feed
2026-09-28T18:26:09.632592Z  INFO xauusd_engine::ws_server: WS session sess-1790619969632582605 opened
2026-09-28T18:26:09.632671Z  INFO xauusd_engine::ws_server: WS session sess-1790619969632582605 subscribed to ["levels", "candle", "tick_volume", "status"]

SMOKE TEST PASSED
