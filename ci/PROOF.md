# CI proof — run 36223765455
commit: a5f91e91cc1c9d4e9a89d4d91eb7c30402ec3f81
generated: 2026-09-26T06:28:10Z

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
   Compiling tracing-subscriber v0.3.23
   Compiling tungstenite v0.29.0
   Compiling hyper v1.11.1
   Compiling hyper-util v0.1.21
   Compiling tokio-rustls v0.26.5
   Compiling tower v0.5.3
   Compiling async-compression v0.4.48
   Compiling tokio-util v0.7.19
   Compiling hyper-rustls v0.27.10
   Compiling tower-http v0.6.11
   Compiling tungstenite v0.26.2
   Compiling tokio-tungstenite v0.29.0
   Compiling axum v0.8.9
   Compiling dashmap v6.2.1
   Compiling tokio-tungstenite v0.26.2
   Compiling reqwest v0.12.28
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
    Finished `test` profile [unoptimized + debuginfo] target(s) in 13.51s
     Running unittests src/main.rs (target/debug/deps/xauusd_engine-b8aab877ac07cfa8)

running 39 tests
test calendar::live_tests::real_forexfactory_feed_returns_events ... ignored, requires network
test calendar::tests::handles_empty_feed ... ok
test calendar::tests::impact_colors_are_normalized ... ok
test calendar::tests::rejects_html_rate_limit_page ... ok
test calendar::tests::markdown_fallback_skips_headers_and_separators ... ok
test calendar::tests::parses_forexfactory_weekly_json ... ok
test calendar::tests::source_has_fallback_urls ... ok
test config::tests::cors_origin_is_normalized_for_header_comparison ... ok
test config::tests::from_env_falls_back_to_the_node2_site ... ok
test config::tests::default_origin_is_already_in_header_form ... ok
test sifting_rest::tests::a_short_response_is_rejected_instead_of_seeding_a_partial_profile ... ok
test sifting_rest::tests::history_request_is_a_single_fixed_two_thousand_bar_page ... ok
test sifting_ws::tests::candle_volume_is_the_tick_count_like_sifting_history ... ok
test sifting_ws::tests::bucket_roll_emits_one_closed_bar_then_a_fresh_live_bar ... ok
test sifting_ws::tests::resubscribe_snapshot_and_late_ticks_are_not_counted ... ok
test sifting_ws::tests::handle_text_writes_the_store_and_broadcasts_candle_plus_tick_volume ... ok
test sifting_ws::tests::rolling_rate_counts_the_last_ten_seconds ... ok
test sifting_ws::tests::tick_rule_splits_up_down_and_flat ... ok
test sifting_ws::tests::tick_volume_frame_wire_shape ... ok
test tick_volume::tests::capacity_is_enforced_oldest_first ... ok
test tick_volume::tests::out_of_order_bars_stay_sorted ... ok
test tick_volume::tests::upsert_replaces_the_live_bar_and_appends_new_ones ... ok
test volume_profile::tests::bearish_swing_is_anchored_high_to_low ... ok
test volume_profile::tests::bullish_swing_is_anchored_low_to_high ... ok
test volume_profile::tests::duplicate_candle_timestamp_is_replaced ... ok
test volume_profile::tests::ingesting_a_candle_after_a_close_refreshes_ps ... ok
test volume_profile::tests::ps_skips_the_weekend_gap ... ok
test volume_profile::tests::ps_rolls_forward_at_every_session_close ... ok
test volume_profile::tests::session_close_is_1700_new_york ... ok
test volume_profile::tests::week_windows_do_not_overlap_the_session_window ... ok
test volume_profile::tests::session_boundary_survives_dst_change ... ok
test ws_server::tests::malformed_origin_panics_at_boot ... ok
test ws_server::tests::allowed_origin_may_read_every_rest_route ... ok
test ws_server::tests::cors_grant_is_scoped_to_the_configured_origin ... ok
test volume_profile::tests::pw_excludes_weekend_candles_and_is_stable_all_week ... ok
test ws_server::tests::payload_shape_is_untouched_by_the_layer ... ok
test ws_server::tests::preflight_is_answered_for_the_allowed_origin_only ... ok
test ws_server::tests::tick_volume_route_serves_the_live_bars_uncached ... ok
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
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.07s
     Running unittests src/main.rs (target/debug/deps/xauusd_engine-b8aab877ac07cfa8)

running 1 test
LIVE CALENDAR: 82 events
  2026-09-20T23:00:00+00:00 | JPY | Holiday | Bank Holiday | forecast=None previous=None gold=false
  2026-09-20T23:01:00+00:00 | GBP | Low | Rightmove HPI m/m | forecast=None previous=Some("-2.0%") gold=false
  2026-09-21T03:00:00+00:00 | NZD | Low | Credit Card Spending y/y | forecast=None previous=Some("5.3%") gold=false
  2026-09-21T10:00:00+00:00 | EUR | Low | German Buba Monthly Report | forecast=None previous=None gold=false
  2026-09-21T10:30:00+00:00 | USD | Low | FOMC Member Goolsbee Speaks | forecast=None previous=None gold=false
  2026-09-21T15:00:00+00:00 | EUR | Medium | ECB President Lagarde Speaks | forecast=None previous=None gold=false
  2026-09-21T15:05:00+00:00 | CAD | Medium | BOC Gov Macklem Speaks | forecast=None previous=None gold=false
  2026-09-21T19:00:00+00:00 | AUD | Low | RBA Assist Gov Hunter Speaks | forecast=None previous=None gold=false
  2026-09-21T23:00:00+00:00 | JPY | Holiday | Bank Holiday | forecast=None previous=None gold=false
  2026-09-22T03:10:00+00:00 | AUD | High | RBA Gov Bullock Speaks | forecast=None previous=None gold=false
test calendar::live_tests::real_forexfactory_feed_returns_events ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 38 filtered out; finished in 0.10s


## Runtime smoke test
--- waiting for /health ---
ok <- /health ok

--- /status ---
{"status":{"feeds":{"alltick":{"last_msg":0,"msgs":0,"state":"off (no token)"},"binance":{"last_msg":0,"msgs":0,"state":"connecting"},"bitget":{"last_msg":0,"msgs":0,"state":"connecting"},"bybit":{"last_msg":0,"msgs":0,"state":"connecting"},"calendar":{"last_msg":0,"msgs":0,"state":"starting"},"gate":{"last_msg":0,"msgs":0,"state":"connecting"},"itick":{"last_msg":0,"msgs":0,"state":"off (no token)"},"kraken":{"last_msg":0,"msgs":0,"state":"connecting"},"okx":{"last_msg":0,"msgs":0,"state":"connecting"},"sifting":{"last_msg":0,"msgs":0,"state":"off (no key)"}},"ts":1790404089498},"venue":"None","version":"0.3.0"}

--- /levels ---
[{"window":"PW","poc":3378.75,"vah":3396.0,"val":3358.5,"start":1789336800000,"end":1789765200000,"timestamp":1790404089479,"direction":"neutral","swing_high":null,"swing_low":null},{"window":"PS","poc":3460.75,"vah":3463.5,"val":3435.0,"start":1790283600000,"end":1790370000000,"timestamp":1790404089479,"direction":"neutral","swing_high":null,"swing_low":null},{"window":"CW","poc":3431.25,"vah":3455.5,"val":3418.5,"start":1789941600000,"end":1790404089479,"timestamp":1790404089479,"direction":"neutral","swing_high":null,"swing_low":null},{"window":"SWING_BULL","poc":3431.75,"vah":3457.5,"val":3429.0,"start":1790310489477,"end":1790354589477,"timestamp":1790404089479,"direction":"bullish","swing_high":3463.5,"swing_low":3429.0}]
windows present: ['CW', 'PS', 'PW', 'SWING_BULL']
PS window ends at 2026-09-25T17:00:00-04:00 (New York)
PS session window: 2026-09-24T21:00:00+00:00 -> 2026-09-25T21:00:00+00:00 (24.0h)
PS poc=3460.750 vah=3463.500 val=3435.000
PW direction=neutral poc=3378.750 vah=3396.000 val=3358.500
CW direction=neutral poc=3431.250 vah=3455.500 val=3418.500
SWING_BULL direction=bullish poc=3431.750 vah=3457.500 val=3429.000
LEVELS CHECK PASSED

--- /calendar (waiting for first fetch) ---
source: ff_json | count: 82
--- first 15 events ---
  2026-09-20T23:00:00+00:00    |  JPY | Holiday  | Bank Holiday  (F:None P:None)
  2026-09-20T23:01:00+00:00    |  GBP | Low      | Rightmove HPI m/m  (F:None P:-2.0%)
  2026-09-21T03:00:00+00:00    |  NZD | Low      | Credit Card Spending y/y  (F:None P:5.3%)
  2026-09-21T10:00:00+00:00    |  EUR | Low      | German Buba Monthly Report  (F:None P:None)
  2026-09-21T10:30:00+00:00    |  USD | Low      | FOMC Member Goolsbee Speaks  (F:None P:None)
  2026-09-21T15:00:00+00:00    |  EUR | Medium   | ECB President Lagarde Speaks  (F:None P:None)
  2026-09-21T15:05:00+00:00    |  CAD | Medium   | BOC Gov Macklem Speaks  (F:None P:None)
  2026-09-21T19:00:00+00:00    |  AUD | Low      | RBA Assist Gov Hunter Speaks  (F:None P:None)
  2026-09-21T23:00:00+00:00    |  JPY | Holiday  | Bank Holiday  (F:None P:None)
  2026-09-22T03:10:00+00:00    |  AUD | High     | RBA Gov Bullock Speaks  (F:None P:None)
  2026-09-22T06:00:00+00:00    |  GBP | Low      | Public Sector Net Borrowing  (F:15.2B P:1.8B)
  2026-09-22T08:30:00+00:00    |  EUR | Low      | German Buba President Nagel Speaks  (F:None P:None)
  2026-09-22T10:00:00+00:00    |  GBP | Low      | CBI Industrial Order Expectations  (F:-33 P:-25)
  2026-09-22T11:00:00+00:00    |  EUR | Medium   | ECB President Lagarde Speaks  (F:None P:None)
  2026-09-22T12:15:00+00:00    |  USD | Low      | ADP Weekly Employment Change  (F:None P:16.3K)
TOTAL EVENTS: 82
WITH TIMESTAMPS: 82
GOLD-RELEVANT (high-impact USD): 0
CALENDAR CHECK PASSED

--- /candles payload shape (must be untouched by the CORS layer) ---
candles: 2000 bars, sources=['synthetic']
first: time=1788604089477 close=3300.25
last:  time=1790403189477 close=3439.4875271016076
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
  sec-websocket-accept: nsBiroG9/Um/8GSAOCubgeVRC9Q=
  access-control-allow-origin: https://static-dash-frontend.onrender.com
  vary: origin, access-control-request-method, access-control-request-headers
frames received: ['candle', 'levels']
candle: source=synthetic close=3300.25
levels: PW poc=3378.75
WS CHECK PASSED

--- engine log: session/level/calendar/CORS lines ---
2026-09-26T06:28:09.479428Z  INFO xauusd_engine: PW: poc=3378.750 vah=3396.000 val=3358.500 direction=neutral
2026-09-26T06:28:09.479437Z  INFO xauusd_engine: PS: poc=3460.750 vah=3463.500 val=3435.000 direction=neutral
2026-09-26T06:28:09.479440Z  INFO xauusd_engine: CW: poc=3431.250 vah=3455.500 val=3418.500 direction=neutral
2026-09-26T06:28:09.479442Z  INFO xauusd_engine: SWING_BULL: poc=3431.750 vah=3457.500 val=3429.000 direction=bullish
2026-09-26T06:28:09.479618Z  INFO xauusd_engine::ws_server: CORS: browser access to the REST API allowed for this origin origin=https://static-dash-frontend.onrender.com
2026-09-26T06:28:09.479704Z  INFO xauusd_engine: Listening on 0.0.0.0:3000 — /health /status /levels /candles /tick-volume /calendar /ws
2026-09-26T06:28:09.520534Z  INFO xauusd_engine::calendar: Calendar: 82 events from direct feed
2026-09-26T06:28:09.779819Z  INFO xauusd_engine::ws_server: WS session sess-1790404089779816418 opened
2026-09-26T06:28:09.780129Z  INFO xauusd_engine::ws_server: WS session sess-1790404089779816418 subscribed to ["levels", "candle", "tick_volume", "status"]

SMOKE TEST PASSED
