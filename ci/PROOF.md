# CI proof — run 37569395049
commit: 0e1757ff60d126545ee3cc5edac73443eb2ae7fc
generated: 2026-10-07T04:03:04Z

## Unit tests
   Compiling smallvec v1.16.2
   Compiling libc v0.2.190
   Compiling yoke v0.8.3
   Compiling zeroize v1.9.1
   Compiling zerovec v0.11.8
   Compiling rustls-pki-types v1.15.1
   Compiling zerocopy v0.8.60
   Compiling tinystr v0.8.4
   Compiling potential_utf v0.1.6
   Compiling zerotrie v0.2.5
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
   Compiling icu_collections v2.3.0
   Compiling ring v0.17.14
   Compiling rand_core v0.9.5
   Compiling icu_provider v2.3.1
   Compiling alloc-no-stdlib v3.0.0
   Compiling alloc-stdlib v0.3.0
   Compiling want v0.3.2
   Compiling icu_normalizer v2.3.0
   Compiling ppv-lite86 v0.2.21
   Compiling rustls-webpki v0.103.15
   Compiling rand_chacha v0.9.0
   Compiling rustls v0.23.45
   Compiling rand v0.9.5
   Compiling icu_properties v2.3.0
   Compiling brotli-decompressor v6.0.1
   Compiling hyper v1.12.0
   Compiling idna_adapter v1.2.2
   Compiling brotli v9.0.0
   Compiling webpki-roots v1.0.9
   Compiling base64 v0.23.1
   Compiling hyper-util v0.1.21
   Compiling tokio-rustls v0.26.6
   Compiling compression-codecs v0.4.45
   Compiling idna v1.1.0
   Compiling tower v0.5.3
   Compiling siphasher v1.0.4
   Compiling url v2.5.8
   Compiling phf_shared v0.12.1
   Compiling async-compression v0.4.50
   Compiling tungstenite v0.29.0
   Compiling tokio-util v0.7.19
   Compiling lazy_static v1.5.1
   Compiling tungstenite v0.26.2
   Compiling tower-http v0.6.11
   Compiling tokio-tungstenite v0.29.0
   Compiling sharded-slab v0.1.7
   Compiling phf v0.12.1
   Compiling hyper-rustls v0.27.10
   Compiling webpki-roots v0.26.11
   Compiling tracing-subscriber v0.3.23
   Compiling tokio-tungstenite v0.26.2
   Compiling dashmap v6.2.1
   Compiling axum v0.8.9
   Compiling reqwest v0.12.28
   Compiling chrono-tz v0.10.4
   Compiling xauusd-engine v0.3.0 (/home/runner/work/Bot/Bot)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 24.98s
     Running unittests src/main.rs (target/debug/deps/xauusd_engine-8d98b85d3dff5ec5)

running 89 tests
test calendar::live_tests::real_forexfactory_feed_returns_events ... ignored, requires network
test ai_cache::tests::health_is_kept_verbatim_plus_timestamp ... ok
test calendar::tests::handles_empty_feed ... ok
test ai_cache::tests::records_transcripts_and_reports_todays_counts ... ok
test ai_cache::tests::stale_entries_are_pruned ... ok
test calendar::tests::impact_colors_are_normalized ... ok
test calendar::tests::markdown_fallback_skips_headers_and_separators ... ok
test calendar::tests::parses_forexfactory_weekly_json ... ok
test config::tests::cors_origin_is_normalized_for_header_comparison ... ok
test config::tests::default_origin_is_already_in_header_form ... ok
test econ_monitor::tests::all_currency_matches_everything ... ok
test calendar::tests::rejects_html_rate_limit_page ... ok
test config::tests::from_env_falls_back_to_the_node2_site ... ok
test econ_monitor::tests::b64_matches_known_vectors ... ok
test calendar::tests::source_has_fallback_urls ... ok
test econ_monitor::tests::tone_chunk_is_f32le_of_expected_length ... ok
test econ_monitor::tests::fed_events_are_recognized ... ok
test econ_monitor::tests::extracts_live_urls_and_prefers_live ... ok
test econ_monitor::tests::window_selects_only_active_high_impact_usd_events ... ok
test sifting_rest::tests::a_short_chart_response_is_rejected_instead_of_seeding_a_partial_page ... ok
test sifting_rest::tests::a_sparse_profile_page_is_rejected_even_when_it_reaches_both_ends ... ok
test sifting_rest::tests::chart_history_request_is_a_single_fixed_two_thousand_bar_page ... ok
test sifting_rest::tests::history_tail_drops_the_boundary_bar_and_keeps_the_newer_ones ... ok
test sifting_rest::tests::profile_history_request_carries_interval_and_encoded_cursor ... ok
test sifting_ws::tests::bucket_roll_emits_one_closed_bar_then_a_fresh_live_bar ... ok
test sifting_rest::tests::response_validation_rejects_wrong_symbol_or_interval ... ok
test sifting_ws::tests::candle_volume_is_the_tick_count_like_sifting_history ... ok
test sifting_ws::tests::handle_text_writes_the_store_and_broadcasts_candle_plus_tick_volume ... ok
test sifting_ws::tests::profile_candle_closes_on_one_minute_boundaries ... ok
test sifting_ws::tests::resubscribe_snapshot_and_late_ticks_are_not_counted ... ok
test sifting_ws::tests::rolling_rate_counts_the_last_ten_seconds ... ok
test sifting_ws::tests::tick_volume_frame_wire_shape ... ok
test sifting_ws::tests::tick_rule_splits_up_down_and_flat ... ok
test tick_volume::tests::capacity_is_enforced_oldest_first ... ok
test tick_volume::tests::out_of_order_bars_stay_sorted ... ok
test tick_volume::tests::upsert_replaces_the_live_bar_and_appends_new_ones ... ok
test types::tests::levels_wire_format_is_tagged_and_includes_the_sunday_open_price ... ok
test sifting_rest::tests::complete_profile_history_passes_the_density_check ... ok
test types::tests::trade_frames_are_not_part_of_the_node1_wire_contract ... ok
test volume_profile::histogram::tests::absurd_bin_sizes_are_rejected_instead_of_allocating ... ok
test volume_profile::histogram::tests::levels_payload_carries_the_audit_metadata ... ok
test volume_profile::histogram::tests::poc_sits_in_the_heaviest_band_and_value_area_contains_it ... ok
test volume_profile::histogram::tests::price_rows_are_aligned_to_the_fixed_half_dollar_grid ... ok
test volume_profile::histogram::tests::rows_are_not_snapped_to_the_half_dollar_grid_in_rows_mode ... ok
test volume_profile::histogram::tests::rows_layout_creates_a_short_top_row_for_a_partial_range ... ok
test volume_profile::histogram::tests::rows_layout_rounds_row_height_to_whole_ticks_like_tradingview ... ok
test volume_profile::histogram::tests::up_and_down_volume_follow_the_bar_direction ... ok
test volume_profile::histogram::tests::value_area_tie_break_prefers_the_row_closer_to_the_poc ... ok
test volume_profile::session::tests::session_boundary_survives_dst_change ... ok
test volume_profile::session::tests::session_close_is_1800_new_york ... ok
test volume_profile::session::tests::filler_only_sessions_are_skipped_for_the_previous_session ... ok
test volume_profile::session::tests::session_shift_keeps_the_1800_anchor_across_dst ... ok
test volume_profile::swing::tests::bearish_swing_is_anchored_high_to_low ... ok
test volume_profile::swing::tests::plateau_highs_anchor_at_the_most_recent_touch ... ok
test volume_profile::swing::tests::bullish_swing_is_anchored_low_to_high ... ok
test volume_profile::swing::tests::fallback_leg_is_bounded_to_recent_bars ... ok
test volume_profile::tests::custom_range_profiles_an_arbitrary_window ... ok
test volume_profile::tests::duplicate_candle_timestamp_is_replaced ... ok
test volume_profile::tests::custom_range_rejects_inverted_and_unretained_ranges ... ok
test volume_profile::histogram::tests::empty_or_flat_windows_produce_no_levels ... ok
test volume_profile::tests::ingesting_a_candle_after_a_close_refreshes_ps ... ok
test volume_profile::tests::cw_appears_once_monday_closes_and_freezes_intraday ... ok
test volume_profile::tests::ps_skips_the_weekend_gap ... ok
test volume_profile::tests::ps_rolls_forward_at_every_session_close ... ok
test volume_profile::tests::time_profiles_and_swing_can_use_separate_history_resolutions ... ok
test volume_profile::tests::pw_marks_the_sunday_start_candle_open_price ... ok
test volume_profile::timeframe::tests::aggregation_merges_out_of_order_chunks ... ok
test volume_profile::tests::refresh_fires_once_per_close_and_ignores_empty_weekend_sessions ... ok
test volume_profile::timeframe::tests::aggregation_sums_volume_and_keeps_the_bucket_ohlc ... ok
test volume_profile::timeframe::tests::labels_round_trip ... ok
test volume_profile::timeframe::tests::prepare_input_never_downscales_coarse_history ... ok
test volume_profile::tests::week_windows_do_not_overlap_the_session_window ... ok
test volume_profile::timeframe::tests::prepare_input_reports_one_minute_for_short_windows ... ok
test volume_profile::weekly::tests::previous_week_runs_sunday_open_to_friday_close ... ok
test volume_profile::timeframe::tests::weekly_windows_use_5m_and_sessions_use_1m_like_tradingview ... ok
test volume_profile::weekly::tests::week_start_is_the_sunday_1800_open ... ok
test ws_server::tests::an_execution_frame_cannot_be_submitted_to_node1 ... ok
test ws_server::tests::ai_route_reports_todays_digest ... ok
test ws_server::tests::envelope_normalization_wraps_loose_producer_payloads ... ok
test ws_server::tests::malformed_origin_panics_at_boot ... ok
test ws_server::tests::cors_grant_is_scoped_to_the_configured_origin ... ok
test ws_server::tests::allowed_origin_may_read_every_rest_route ... ok
test ws_server::tests::payload_shape_is_untouched_by_the_layer ... ok
test ws_server::tests::preflight_is_answered_for_the_allowed_origin_only ... ok
test ws_server::tests::tick_volume_route_serves_the_live_bars_uncached ... ok
test volume_profile::tests::pw_excludes_weekend_candles_and_is_stable_all_week ... ok
test ws_server::tests::vp_custom_range_validates_its_parameters ... ok
test ws_server::tests::only_the_configured_origin_is_ever_allowed ... ok
test ws_server::tests::no_route_accepts_a_broker_write ... ok

test result: ok. 88 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.04s


## Live calendar feed test
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.11s
     Running unittests src/main.rs (target/debug/deps/xauusd_engine-8d98b85d3dff5ec5)

running 1 test
LIVE CALENDAR: 83 events
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

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 88 filtered out; finished in 0.05s


## Runtime smoke test
--- waiting for /health ---
ok <- /health ok

--- /status ---
{"candles":{"bars":2000,"edge_lag_minutes":17,"first_bar":1789545600000,"interval":"15m","last_bar":1791344700000,"seed_bars":2000},"status":{"feeds":{"alltick":{"last_msg":0,"msgs":0,"state":"off (no token)"},"binance":{"last_msg":0,"msgs":0,"state":"connecting"},"bitget":{"last_msg":0,"msgs":0,"state":"connecting"},"bybit":{"last_msg":0,"msgs":0,"state":"connecting"},"calendar":{"last_msg":0,"msgs":0,"state":"starting"},"econ_monitor":{"last_msg":0,"msgs":0,"state":"idle (no active event windows)"},"gate":{"last_msg":0,"msgs":0,"state":"connecting"},"itick":{"last_msg":0,"msgs":0,"state":"off (no token)"},"kraken":{"last_msg":0,"msgs":0,"state":"connecting"},"okx":{"last_msg":0,"msgs":0,"state":"connecting"},"sifting":{"last_msg":0,"msgs":0,"state":"off (no key)"}},"ts":1791345775737},"version":"0.3.0","volume_profile":{"audit":"/vp?window=PW","bin_size":0.5,"input":"tv","row_mode":"rows","rows":128,"va_pct":70.0}}

--- /levels ---
[{"window":"PW","poc":3401.25,"vah":3421.0,"val":3384.0,"start":1790546400000,"end":1790978400000,"timestamp":1791345775718,"direction":"neutral","swing_high":null,"swing_low":null,"sunday_open":3376.5,"meta":{"row_mode":"rows","row_height":0.5,"rows":129,"range_high":3433.5,"range_low":3369.0,"input_interval":"15m","input_bars":480,"total_volume":59525.99999999998,"va_pct":0.7}},{"window":"PS","poc":3460.7250000000004,"vah":3463.5,"val":3435.48,"start":1791237600000,"end":1791324000000,"timestamp":1791345775718,"direction":"neutral","swing_high":null,"swing_low":null,"meta":{"row_mode":"rows","row_height":0.27,"rows":128,"range_high":3463.5,"range_low":3429.0,"input_interval":"15m","input_bars":96,"total_volume":11930.999999999212,"va_pct":0.7}},{"window":"CW","poc":3453.345,"vah":3463.08,"val":3432.72,"start":1791151200000,"end":1791324000000,"timestamp":1791345775718,"direction":"neutral","swing_high":null,"swing_low":null,"meta":{"row_mode":"rows","row_height":0.33,"rows":128,"range_high":3463.5,"range_low":3421.5,"input_interval":"15m","input_bars":192,"total_volume":23753.999999994936,"va_pct":0.7}},{"window":"SWING_BEAR","poc":3460.755,"vah":3463.38,"val":3442.59,"start":1791295200000,"end":1791339300000,"timestamp":1791345775718,"direction":"bearish","swing_high":3463.5,"swing_low":3436.5,"meta":{"row_mode":"rows","row_height":0.21,"rows":129,"range_high":3463.5,"range_low":3436.5,"input_interval":"15m","input_bars":49,"total_volume":6097.000000001011,"va_pct":0.7}}]
windows present: ['CW', 'PS', 'PW', 'SWING_BEAR']
PS window ends at 2026-10-06T18:00:00-04:00 (New York)
PS session window: 2026-10-05T22:00:00+00:00 -> 2026-10-06T22:00:00+00:00 (24.0h)
PS poc=3460.725 vah=3463.500 val=3435.480
PW direction=neutral poc=3401.250 vah=3421.000 val=3384.000
CW direction=neutral poc=3453.345 vah=3463.080 val=3432.720
SWING_BEAR direction=bearish poc=3460.755 vah=3463.380 val=3442.590
LEVELS CHECK PASSED

--- /vp histogram audit (TradingView row model + input resolution) ---
window=PS row_mode=rows rows=128 row_height=0.2700
range 3429.000..3463.500 input=15m (96 bars) total_volume=11931
poc=3460.725 (row 117) val=3435.480 vah=3463.500
VP CHECK PASSED

--- /vp custom range (profile exactly the range the chart drew) ---
window=CUSTOM row_mode=rows rows=129 row_height=0.5000
range 3369.000..3433.500 input=15m (480 bars) total_volume=59526
poc=3401.250 (row 64) val=3384.000 vah=3421.000
VP CHECK PASSED
custom range: rows=129 row_height=0.5000 input=15m (480 bars) poc=3401.25
CUSTOM RANGE CHECK PASSED
malformed / inverted / half ranges -> 400

--- /calendar (waiting for first fetch) ---
source: ff_json | count: 83
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
TOTAL EVENTS: 83
WITH TIMESTAMPS: 83
GOLD-RELEVANT (high-impact USD): 1
CALENDAR CHECK PASSED

--- /candles payload shape (must be untouched by the CORS layer) ---
grid: 15m (1999/1999 gaps are exactly one bucket)
history edge: 18 minutes behind now
candles: 2000 bars, sources=['synthetic']
first: time=1789545600000 close=3300.25
last:  time=1791344700000 close=3439.4875271016076
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
  sec-websocket-accept: PiaXhJpCLDB11BfESAmmJVb+BKM=
  access-control-allow-origin: https://static-dash-frontend.onrender.com
  vary: origin, access-control-request-method, access-control-request-headers
replay: 4 levels, 15 candles
candle: source=synthetic close=3439.4875271016076
levels: PW poc=3401.25
handshake: HTTP/1.1 101 Switching Protocols
  sec-websocket-accept: WxkggT7tCykxHsem3Oyp89D8H9I=
  access-control-allow-origin: https://static-dash-frontend.onrender.com
  vary: origin, access-control-request-method, access-control-request-headers
no broker write path: an inbound `trades` frame is never rebroadcast
market topics intact: `levels` still replays on re-subscribe
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
2026-10-07T04:02:55.718883Z  INFO xauusd_engine: PW: poc=3401.250 vah=3421.000 val=3384.000 direction=neutral
2026-10-07T04:02:55.718893Z  INFO xauusd_engine: PS: poc=3460.725 vah=3463.500 val=3435.480 direction=neutral
2026-10-07T04:02:55.718896Z  INFO xauusd_engine: CW: poc=3453.345 vah=3463.080 val=3432.720 direction=neutral
2026-10-07T04:02:55.718899Z  INFO xauusd_engine: SWING_BEAR: poc=3460.755 vah=3463.380 val=3442.590 direction=bearish
2026-10-07T04:02:55.719150Z  INFO xauusd_engine::ws_server: CORS: browser access to the REST API allowed for this origin origin=https://static-dash-frontend.onrender.com
2026-10-07T04:02:55.719274Z  INFO xauusd_engine: Listening on 0.0.0.0:3000 — /health /status /levels /candles /tick-volume /calendar /ai /ws
2026-10-07T04:02:55.770696Z  INFO xauusd_engine::calendar: Calendar: 83 events from direct feed
2026-10-07T04:02:56.307126Z  INFO xauusd_engine::ws_server: WS session sess-1791345776307118969 opened
2026-10-07T04:02:56.307260Z  INFO xauusd_engine::ws_server: WS session sess-1791345776307118969 subscribed to ["candle", "levels"]
2026-10-07T04:02:56.348892Z  INFO xauusd_engine::ws_server: WS session sess-1791345776348885086 opened
2026-10-07T04:02:56.349007Z  INFO xauusd_engine::ws_server: WS session sess-1791345776348885086 subscribed to ["levels", "bubbles"]
2026-10-07T04:03:01.394840Z  INFO xauusd_engine::ws_server: WS session sess-1791345776348885086 subscribed to ["levels"]
2026-10-07T04:03:01.395144Z  INFO xauusd_engine::ws_server: WS session sess-1791345776307118969 disconnected
2026-10-07T04:03:01.395276Z  INFO xauusd_engine::ws_server: WS session sess-1791345776348885086 disconnected

SMOKE TEST PASSED

## Clippy
    Checking xauusd-engine v0.3.0 (/home/runner/work/Bot/Bot)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.51s

