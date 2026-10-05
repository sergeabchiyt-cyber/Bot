# CI proof — run 37328970500
commit: f6e2a9578bb4ecc5629c3dc0a6fb5d8de57b3ac3
generated: 2026-10-05T14:59:30Z

## Unit tests
   Compiling smallvec v1.16.2
   Compiling libc v0.2.190
   Compiling yoke v0.8.3
   Compiling zerocopy v0.8.59
   Compiling zerovec v0.11.8
   Compiling zerotrie v0.2.5
   Compiling alloc-no-stdlib v3.0.0
   Compiling alloc-stdlib v0.3.0
   Compiling brotli-decompressor v6.0.1
   Compiling tinystr v0.8.4
   Compiling icu_locale_core v2.3.0
   Compiling parking_lot_core v0.9.12
   Compiling errno v0.3.14
   Compiling signal-hook-registry v1.4.8
   Compiling parking_lot v0.12.5
   Compiling socket2 v0.6.5
   Compiling mio v1.2.4
   Compiling getrandom v0.3.4
   Compiling tokio v1.53.2
   Compiling getrandom v0.2.17
   Compiling potential_utf v0.1.6
   Compiling ring v0.17.14
   Compiling icu_collections v2.3.0
   Compiling rand_core v0.9.5
   Compiling icu_provider v2.3.1
   Compiling ppv-lite86 v0.2.21
   Compiling icu_normalizer v2.3.0
   Compiling rustls-webpki v0.103.15
   Compiling rand_chacha v0.9.0
   Compiling rand v0.9.5
   Compiling rustls v0.23.45
   Compiling icu_properties v2.3.0
   Compiling brotli v9.0.0
   Compiling idna_adapter v1.2.2
   Compiling base64 v0.23.1
   Compiling hyper v1.11.1
   Compiling hyper-util v0.1.21
   Compiling tokio-rustls v0.26.6
   Compiling compression-codecs v0.4.45
   Compiling tower v0.5.3
   Compiling idna v1.1.0
   Compiling siphasher v1.0.4
   Compiling url v2.5.8
   Compiling phf_shared v0.12.1
   Compiling async-compression v0.4.50
   Compiling tokio-util v0.7.19
   Compiling tungstenite v0.29.0
   Compiling lazy_static v1.5.1
   Compiling sharded-slab v0.1.7
   Compiling tungstenite v0.26.2
   Compiling tokio-tungstenite v0.29.0
   Compiling tower-http v0.6.11
   Compiling phf v0.12.1
   Compiling hyper-rustls v0.27.10
   Compiling tracing-subscriber v0.3.23
   Compiling dashmap v6.2.1
   Compiling reqwest v0.12.28
   Compiling axum v0.8.9
   Compiling tokio-tungstenite v0.26.2
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

warning: methods `ingest_profile_candles` and `ingest_swing_candles` are never used
   --> src/volume_profile.rs:197:12
    |
128 | impl VolumeProfileEngine {
    | ------------------------ methods in this implementation
...
197 |     pub fn ingest_profile_candles(&mut self, candles: Vec<VpCandle>) {
    |            ^^^^^^^^^^^^^^^^^^^^^^
...
211 |     pub fn ingest_swing_candles(&mut self, candles: Vec<VpCandle>) {
    |            ^^^^^^^^^^^^^^^^^^^^

warning: field `subscriptions` is never read
  --> src/ws_server.rs:34:9
   |
32 | pub struct AppState {
   |            -------- field in this struct
33 |     pub tx: broadcast::Sender<WsFrame>,
34 |     pub subscriptions: Arc<DashMap<String, Vec<String>>>,
   |         ^^^^^^^^^^^^^
   |
   = note: `AppState` has a derived impl for the trait `Clone`, but this is intentionally ignored during dead code analysis

warning: `xauusd-engine` (bin "xauusd-engine" test) generated 3 warnings
    Finished `test` profile [unoptimized + debuginfo] target(s) in 16.74s
     Running unittests src/main.rs (target/debug/deps/xauusd_engine-a43c736f72c15a5f)

running 82 tests
test calendar::live_tests::real_forexfactory_feed_returns_events ... ignored, requires network
test ai_cache::tests::stale_entries_are_pruned ... ok
test ai_cache::tests::health_is_kept_verbatim_plus_timestamp ... ok
test calendar::tests::handles_empty_feed ... ok
test calendar::tests::impact_colors_are_normalized ... ok
test calendar::tests::markdown_fallback_skips_headers_and_separators ... ok
test calendar::tests::rejects_html_rate_limit_page ... ok
test calendar::tests::parses_forexfactory_weekly_json ... ok
test calendar::tests::source_has_fallback_urls ... ok
test config::tests::cors_origin_is_normalized_for_header_comparison ... ok
test config::tests::default_origin_is_already_in_header_form ... ok
test econ_monitor::tests::all_currency_matches_everything ... ok
test ai_cache::tests::records_transcripts_and_reports_todays_counts ... ok
test econ_monitor::tests::b64_matches_known_vectors ... ok
test econ_monitor::tests::extracts_live_urls_and_prefers_live ... ok
test econ_monitor::tests::fed_events_are_recognized ... ok
test config::tests::from_env_falls_back_to_the_node2_site ... ok
test econ_monitor::tests::tone_chunk_is_f32le_of_expected_length ... ok
test econ_monitor::tests::window_selects_only_active_high_impact_usd_events ... ok
test sifting_rest::tests::a_short_chart_response_is_rejected_instead_of_seeding_a_partial_page ... ok
test sifting_rest::tests::a_sparse_profile_page_is_rejected_even_when_it_reaches_both_ends ... ok
test sifting_rest::tests::chart_history_request_is_a_single_fixed_two_thousand_bar_page ... ok
test sifting_ws::tests::bucket_roll_emits_one_closed_bar_then_a_fresh_live_bar ... ok
test sifting_ws::tests::candle_volume_is_the_tick_count_like_sifting_history ... ok
test sifting_ws::tests::handle_text_writes_the_store_and_broadcasts_candle_plus_tick_volume ... ok
test sifting_ws::tests::profile_candle_closes_on_one_minute_boundaries ... ok
test sifting_ws::tests::resubscribe_snapshot_and_late_ticks_are_not_counted ... ok
test sifting_rest::tests::profile_history_request_carries_interval_and_encoded_cursor ... ok
test sifting_ws::tests::rolling_rate_counts_the_last_ten_seconds ... ok
test sifting_ws::tests::tick_rule_splits_up_down_and_flat ... ok
test sifting_ws::tests::tick_volume_frame_wire_shape ... ok
test tick_volume::tests::out_of_order_bars_stay_sorted ... ok
test tick_volume::tests::capacity_is_enforced_oldest_first ... ok
test sifting_rest::tests::response_validation_rejects_wrong_symbol_or_interval ... ok
test types::tests::levels_wire_format_is_tagged_and_includes_the_sunday_open_price ... ok
test tick_volume::tests::upsert_replaces_the_live_bar_and_appends_new_ones ... ok
test volume_profile::histogram::tests::empty_or_flat_windows_produce_no_levels ... ok
test types::tests::node3_trade_event_uses_the_tagged_trades_envelope ... ok
test volume_profile::histogram::tests::absurd_bin_sizes_are_rejected_instead_of_allocating ... ok
test volume_profile::histogram::tests::levels_payload_carries_the_audit_metadata ... ok
test volume_profile::histogram::tests::poc_sits_in_the_heaviest_band_and_value_area_contains_it ... ok
test sifting_rest::tests::complete_profile_history_passes_the_density_check ... ok
test volume_profile::histogram::tests::price_rows_are_aligned_to_the_fixed_half_dollar_grid ... ok
test volume_profile::histogram::tests::rows_are_not_snapped_to_the_half_dollar_grid_in_rows_mode ... ok
test volume_profile::histogram::tests::rows_layout_creates_a_short_top_row_for_a_partial_range ... ok
test volume_profile::histogram::tests::value_area_tie_break_prefers_the_row_closer_to_the_poc ... ok
test volume_profile::histogram::tests::rows_layout_rounds_row_height_to_whole_ticks_like_tradingview ... ok
test volume_profile::histogram::tests::up_and_down_volume_follow_the_bar_direction ... ok
test volume_profile::session::tests::session_shift_keeps_the_1800_anchor_across_dst ... ok
test volume_profile::session::tests::session_boundary_survives_dst_change ... ok
test volume_profile::session::tests::session_close_is_1800_new_york ... ok
test volume_profile::swing::tests::bearish_swing_is_anchored_high_to_low ... ok
test volume_profile::swing::tests::bullish_swing_is_anchored_low_to_high ... ok
test volume_profile::swing::tests::plateau_highs_anchor_at_the_most_recent_touch ... ok
test volume_profile::swing::tests::fallback_leg_is_bounded_to_recent_bars ... ok
test volume_profile::tests::cw_appears_once_monday_closes_and_freezes_intraday ... ok
test volume_profile::tests::duplicate_candle_timestamp_is_replaced ... ok
test volume_profile::tests::ingesting_a_candle_after_a_close_refreshes_ps ... ok
test volume_profile::tests::ps_skips_the_weekend_gap ... ok
test volume_profile::tests::ps_rolls_forward_at_every_session_close ... ok
test volume_profile::tests::time_profiles_and_swing_can_use_separate_history_resolutions ... ok
test volume_profile::tests::pw_marks_the_sunday_start_candle_open_price ... ok
test volume_profile::timeframe::tests::aggregation_merges_out_of_order_chunks ... ok
test volume_profile::timeframe::tests::aggregation_sums_volume_and_keeps_the_bucket_ohlc ... ok
test volume_profile::timeframe::tests::labels_round_trip ... ok
test volume_profile::timeframe::tests::prepare_input_never_downscales_coarse_history ... ok
test volume_profile::timeframe::tests::prepare_input_reports_one_minute_for_short_windows ... ok
test volume_profile::tests::refresh_fires_once_per_close_and_ignores_empty_weekend_sessions ... ok
test volume_profile::weekly::tests::previous_week_runs_sunday_open_to_friday_close ... ok
test volume_profile::weekly::tests::week_start_is_the_sunday_1800_open ... ok
test volume_profile::tests::week_windows_do_not_overlap_the_session_window ... ok
test ws_server::tests::ai_route_reports_todays_digest ... ok
test volume_profile::timeframe::tests::weekly_windows_use_5m_and_sessions_use_1m_like_tradingview ... ok
test ws_server::tests::envelope_normalization_wraps_loose_producer_payloads ... ok
test ws_server::tests::malformed_origin_panics_at_boot ... ok
test ws_server::tests::cors_grant_is_scoped_to_the_configured_origin ... ok
test ws_server::tests::payload_shape_is_untouched_by_the_layer ... ok
test ws_server::tests::allowed_origin_may_read_every_rest_route ... ok
test ws_server::tests::preflight_is_answered_for_the_allowed_origin_only ... ok
test ws_server::tests::tick_volume_route_serves_the_live_bars_uncached ... ok
test volume_profile::tests::pw_excludes_weekend_candles_and_is_stable_all_week ... ok
test ws_server::tests::only_the_configured_origin_is_ever_allowed ... ok

test result: ok. 81 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.02s


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

warning: methods `ingest_profile_candles` and `ingest_swing_candles` are never used
   --> src/volume_profile.rs:197:12
    |
128 | impl VolumeProfileEngine {
    | ------------------------ methods in this implementation
...
197 |     pub fn ingest_profile_candles(&mut self, candles: Vec<VpCandle>) {
    |            ^^^^^^^^^^^^^^^^^^^^^^
...
211 |     pub fn ingest_swing_candles(&mut self, candles: Vec<VpCandle>) {
    |            ^^^^^^^^^^^^^^^^^^^^

warning: field `subscriptions` is never read
  --> src/ws_server.rs:34:9
   |
32 | pub struct AppState {
   |            -------- field in this struct
33 |     pub tx: broadcast::Sender<WsFrame>,
34 |     pub subscriptions: Arc<DashMap<String, Vec<String>>>,
   |         ^^^^^^^^^^^^^
   |
   = note: `AppState` has a derived impl for the trait `Clone`, but this is intentionally ignored during dead code analysis

warning: `xauusd-engine` (bin "xauusd-engine" test) generated 3 warnings
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.07s
     Running unittests src/main.rs (target/debug/deps/xauusd_engine-a43c736f72c15a5f)

running 1 test
LIVE CALENDAR: 79 events
  2026-10-04T09:15:00+00:00 | ALL | Medium | OPEC-JMMC Meetings | forecast=None previous=None gold=false
  2026-10-04T20:00:00+00:00 | AUD | Holiday | Bank Holiday | forecast=None previous=None gold=false
  2026-10-04T23:01:00+00:00 | CNY | Holiday | Bank Holiday | forecast=None previous=None gold=false
  2026-10-05T00:00:00+00:00 | AUD | Low | MI Inflation Gauge m/m | forecast=None previous=Some("0.5%") gold=false
  2026-10-05T00:00:00+00:00 | NZD | Low | ANZ Commodity Prices m/m | forecast=None previous=Some("-0.4%") gold=false
  2026-10-05T05:00:00+00:00 | JPY | Low | Consumer Confidence | forecast=Some("35.3") previous=Some("35.5") gold=false
  2026-10-05T07:15:00+00:00 | EUR | Low | Spanish Services PMI | forecast=Some("57.1") previous=Some("57.8") gold=false
  2026-10-05T07:45:00+00:00 | EUR | Low | German Buba President Nagel Speaks | forecast=None previous=None gold=false
  2026-10-05T07:45:00+00:00 | EUR | Low | Italian Services PMI | forecast=Some("54.6") previous=Some("55.2") gold=false
  2026-10-05T07:50:00+00:00 | EUR | Low | French Final Services PMI | forecast=Some("51.4") previous=Some("51.4") gold=false
test calendar::live_tests::real_forexfactory_feed_returns_events ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 81 filtered out; finished in 0.14s


## Runtime smoke test
--- waiting for /health ---
ok <- /health ok

--- /status ---
{"candles":{"interval":"15m","seed_bars":2000},"status":{"feeds":{"alltick":{"last_msg":0,"msgs":0,"state":"off (no token)"},"binance":{"last_msg":0,"msgs":0,"state":"connecting"},"bitget":{"last_msg":0,"msgs":0,"state":"connecting"},"bybit":{"last_msg":0,"msgs":0,"state":"connecting"},"calendar":{"last_msg":0,"msgs":0,"state":"starting"},"econ_monitor":{"last_msg":0,"msgs":0,"state":"idle (no active event windows)"},"gate":{"last_msg":0,"msgs":0,"state":"connecting"},"itick":{"last_msg":0,"msgs":0,"state":"off (no token)"},"kraken":{"last_msg":0,"msgs":0,"state":"connecting"},"okx":{"last_msg":0,"msgs":0,"state":"connecting"},"sifting":{"last_msg":0,"msgs":0,"state":"off (no key)"}},"ts":1791212366636},"venue":"None","version":"0.3.0","volume_profile":{"audit":"/vp?window=PW","bin_size":0.5,"input":"tv","row_mode":"rows","rows":128,"va_pct":70.0}}

--- /levels ---
[{"window":"PW","poc":3416.3999999999996,"vah":3431.52,"val":3399.36,"start":1790546400000,"end":1790978400000,"timestamp":1791212366620,"direction":"neutral","swing_high":null,"swing_low":null,"meta":{"row_mode":"rows","row_height":0.48,"rows":128,"range_high":3444.9852813742386,"range_low":3384.0,"input_interval":"15m","input_bars":480,"total_volume":59478.00000000222,"va_pct":0.7}},{"window":"PS","poc":3453.12,"vah":3458.2799999999997,"val":3435.0,"start":1791064800000,"end":1791151200000,"timestamp":1791212366620,"direction":"neutral","swing_high":null,"swing_low":null,"meta":{"row_mode":"rows","row_height":0.24,"rows":130,"range_high":3459.9852813742386,"range_low":3429.0,"input_interval":"15m","input_bars":96,"total_volume":11837.99999998932,"va_pct":0.7}},{"window":"SWING_BEAR","poc":3460.755,"vah":3463.38,"val":3442.59,"start":1791161966619,"end":1791206066619,"timestamp":1791212366620,"direction":"bearish","swing_high":3463.5,"swing_low":3436.5,"meta":{"row_mode":"rows","row_height":0.21,"rows":129,"range_high":3463.5,"range_low":3436.5,"input_interval":"15m","input_bars":49,"total_volume":6097.000000001011,"va_pct":0.7}}]
windows present: ['PS', 'PW', 'SWING_BEAR']
PS window ends at 2026-10-04T18:00:00-04:00 (New York)
PS session window: 2026-10-03T22:00:00+00:00 -> 2026-10-04T22:00:00+00:00 (24.0h)
PS poc=3453.120 vah=3458.280 val=3435.000
PW direction=neutral poc=3416.400 vah=3431.520 val=3399.360
SWING_BEAR direction=bearish poc=3460.755 vah=3463.380 val=3442.590
LEVELS CHECK PASSED

--- /vp histogram audit (TradingView row model + input resolution) ---
window=PS row_mode=rows rows=130 row_height=0.2400
range 3429.000..3459.985 input=15m (96 bars) total_volume=11838
poc=3453.120 (row 100) val=3435.000 vah=3458.280
VP CHECK PASSED

--- /calendar (waiting for first fetch) ---
source: ff_json | count: 79
--- first 15 events ---
  2026-10-04T09:15:00+00:00    |  ALL | Medium   | OPEC-JMMC Meetings  (F:None P:None)
  2026-10-04T20:00:00+00:00    |  AUD | Holiday  | Bank Holiday  (F:None P:None)
  2026-10-04T23:01:00+00:00    |  CNY | Holiday  | Bank Holiday  (F:None P:None)
  2026-10-05T00:00:00+00:00    |  AUD | Low      | MI Inflation Gauge m/m  (F:None P:0.5%)
  2026-10-05T00:00:00+00:00    |  NZD | Low      | ANZ Commodity Prices m/m  (F:None P:-0.4%)
  2026-10-05T05:00:00+00:00    |  JPY | Low      | Consumer Confidence  (F:35.3 P:35.5)
  2026-10-05T07:15:00+00:00    |  EUR | Low      | Spanish Services PMI  (F:57.1 P:57.8)
  2026-10-05T07:45:00+00:00    |  EUR | Low      | German Buba President Nagel Speaks  (F:None P:None)
  2026-10-05T07:45:00+00:00    |  EUR | Low      | Italian Services PMI  (F:54.6 P:55.2)
  2026-10-05T07:50:00+00:00    |  EUR | Low      | French Final Services PMI  (F:51.4 P:51.4)
  2026-10-05T07:55:00+00:00    |  EUR | Low      | German Final Services PMI  (F:52.9 P:52.9)
  2026-10-05T08:00:00+00:00    |  EUR | Low      | Final Services PMI  (F:53.0 P:53.0)
  2026-10-05T08:30:00+00:00    |  EUR | Low      | Sentix Investor Confidence  (F:4.5 P:5.1)
  2026-10-05T08:30:00+00:00    |  GBP | Low      | Final Services PMI  (F:51.7 P:51.7)
  2026-10-05T09:00:00+00:00    |  EUR | Low      | PPI m/m  (F:1.9% P:1.6%)
TOTAL EVENTS: 79
WITH TIMESTAMPS: 79
GOLD-RELEVANT (high-impact USD): 1
CALENDAR CHECK PASSED

--- /candles payload shape (must be untouched by the CORS layer) ---
candles: 2000 bars, sources=['synthetic']
first: time=1789412366619 close=3300.25
last:  time=1791211466619 close=3439.4875271016076
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
  /ai -> access-control-allow-origin: https://static-dash-frontend.onrender.com
  /status -> access-control-allow-origin: https://static-dash-frontend.onrender.com
  /vp -> access-control-allow-origin: https://static-dash-frontend.onrender.com
2) preflight (OPTIONS) is authorised for that origin
  HTTP/1.1 200 OK|vary: origin, access-control-request-method, access-control-request-headers|access-control-allow-methods: GET,OPTIONS|access-control-allow-headers: content-type,accept,origin|access-control-max-age: 600|access-control-allow-origin: https://static-dash-frontend.onrender.com
3) every other origin gets NO allow-header
  /health (foreign) -> HTTP/1.1 200 OK, no CORS grant
  /levels (foreign) -> HTTP/1.1 200 OK, no CORS grant
  /candles (foreign) -> HTTP/1.1 200 OK, no CORS grant
  /tick-volume (foreign) -> HTTP/1.1 200 OK, no CORS grant
  /calendar (foreign) -> HTTP/1.1 200 OK, no CORS grant
  /ai (foreign) -> HTTP/1.1 200 OK, no CORS grant
  /status (foreign) -> HTTP/1.1 200 OK, no CORS grant
  /vp (foreign) -> HTTP/1.1 200 OK, no CORS grant
  OPTIONS /candles (foreign) -> HTTP/1.1 200 OK, no CORS grant
4) no wildcard, no credentials, no secrets in the response
  headers are CORS-only
5) /ws still upgrades and replays frames through the layer
handshake: HTTP/1.1 101 Switching Protocols
  sec-websocket-accept: qc+S0FZ98UROrSr5LbR/EIXu6uE=
  access-control-allow-origin: https://static-dash-frontend.onrender.com
  vary: origin, access-control-request-method, access-control-request-headers
replay: 3 levels, 15 candles
candle: source=synthetic close=3439.4875271016076
levels: PW poc=3416.3999999999996
handshake: HTTP/1.1 101 Switching Protocols
  sec-websocket-accept: WpPsOXsirroZlCmgLqHrsqq//LY=
  access-control-allow-origin: https://static-dash-frontend.onrender.com
  vary: origin, access-control-request-method, access-control-request-headers
trade fan-out: Node3 TradeEvent reached the trades subscriber unchanged
WS CHECK PASSED

6) econ-news audio pipeline serves audio_chunk/learn and caches /ai digests
 <- /ws upgraded (HTTP/1.1 101 Switching Protocols)
 -> subscribed ["audio_chunk","learn","transcript"]
 <- 3 audio_chunk frames (valid base64 f32le 16kHz, non-silent)
    e.g. event='test tone' source='econ_monitor:tone'
 <- learn frame rebroadcast in canonical envelope {data:{features,target}}
 <- transcript frame fanned out on the `transcript` topic
 <- /ai digest ok (today: 1 transcripts, 0 sentiments)
ECON AUDIO CHECK OK

--- engine log: session/level/calendar/CORS lines ---
2026-10-05T14:59:26.620404Z  INFO xauusd_engine: PW: poc=3416.400 vah=3431.520 val=3399.360 direction=neutral
2026-10-05T14:59:26.620410Z  INFO xauusd_engine: PS: poc=3453.120 vah=3458.280 val=3435.000 direction=neutral
2026-10-05T14:59:26.620412Z  INFO xauusd_engine: SWING_BEAR: poc=3460.755 vah=3463.380 val=3442.590 direction=bearish
2026-10-05T14:59:26.620681Z  INFO xauusd_engine::ws_server: CORS: browser access to the REST API allowed for this origin origin=https://static-dash-frontend.onrender.com
2026-10-05T14:59:26.620806Z  INFO xauusd_engine: Listening on 0.0.0.0:3000 — /health /status /levels /candles /tick-volume /calendar /ai /ws
2026-10-05T14:59:26.669375Z  INFO xauusd_engine::calendar: Calendar: 79 events from direct feed
2026-10-05T14:59:27.009775Z  INFO xauusd_engine::ws_server: WS session sess-1791212367009766023 opened
2026-10-05T14:59:27.009829Z  INFO xauusd_engine::ws_server: WS session sess-1791212367009766023 subscribed to ["candle", "levels", "trades"]
2026-10-05T14:59:27.050873Z  INFO xauusd_engine::ws_server: WS session sess-1791212367050867095 opened
2026-10-05T14:59:27.050947Z  INFO xauusd_engine::ws_server: WS session sess-1791212367050867095 subscribed to ["trades"]
2026-10-05T14:59:27.091481Z  INFO xauusd_engine::ws_server: WS session sess-1791212367009766023 disconnected
2026-10-05T14:59:27.091549Z  INFO xauusd_engine::ws_server: WS session sess-1791212367050867095 disconnected

SMOKE TEST PASSED
