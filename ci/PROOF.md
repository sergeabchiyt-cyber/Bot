# CI proof — run 40391942180
commit: 6a1a33057820d54ba023b87f93bea50f691a2fdd
generated: 2026-10-26T20:02:53Z

## Unit tests
   Compiling brotli-decompressor v6.2.0
   Compiling brotli v9.1.0
   Compiling compression-codecs v0.4.46
   Compiling async-compression v0.4.51
   Compiling tower-http v0.6.11
   Compiling reqwest v0.12.28
   Compiling xauusd-engine v0.4.0 (/home/runner/work/Bot/Bot)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 8.83s
     Running unittests src/main.rs (target/debug/deps/xauusd_engine-c9365006b296bef2)

running 106 tests
test calendar::live_tests::real_forexfactory_feed_returns_events ... ignored, requires network
test calendar::tests::handles_empty_feed ... ok
test calendar::tests::impact_colors_are_normalized ... ok
test calendar::tests::parses_forexfactory_weekly_json ... ok
test calendar::tests::markdown_fallback_skips_headers_and_separators ... ok
test calendar::tests::parses_sifting_economic_calendar_json ... ok
test calendar::tests::rejects_html_rate_limit_page ... ok
test calendar::tests::honest_error_state_when_all_configured_sources_fail ... ok
test candles::tests::a_full_series_evicts_the_oldest_bar_never_the_newest ... ok
test candles::tests::a_late_bar_takes_its_own_slot_when_there_is_room ... ok
test calendar::tests::source_uses_live_urls_and_optional_sifting_fallback ... ok
test candles::tests::a_closed_bar_is_published_exactly_once_and_before_the_next_bucket ... ok
test candles::tests::a_seed_row_never_reopens_a_bar ... ok
test candles::tests::a_resubscribe_snapshot_tick_is_not_counted_twice ... ok
test candles::tests::counters_add_up ... ok
test candles::tests::late_ticks_do_not_rewrite_a_closed_bucket ... ok
test candles::tests::folded_coarse_bars_exclude_incomplete_current_bucket ... ok
test candles::tests::non_finite_and_non_positive_prices_are_dropped ... ok
test candles::tests::second_based_timestamps_are_dropped ... ok
test candles::tests::the_book_reports_edges_and_retains_the_recent_tail ... ok
test candles::tests::the_three_resolutions_roll_together ... ok
test candles::tests::the_forming_bucket_is_not_in_the_candle_cache ... ok
test candles::tests::ticks_more_than_five_seconds_in_the_future_are_dropped ... ok
test candles::tests::tick_volume_ticks_equal_sum_of_1m_volumes_over_full_15m_bucket ... ok
test candles::tests::unaligned_seed_rows_are_rejected_and_duplicates_merge_ascending ... ok
test config::tests::cors_origin_is_normalized_for_header_comparison ... ok
test config::tests::default_origin_is_already_in_header_form ... ok
test calendar::tests::sifting_calendar_recovers_when_direct_feed_is_blocked ... ok
test candles::tests::tick_volume_ticks_equal_the_candle_volume ... ok
test config::tests::from_env_falls_back_to_the_node2_site ... ok
test config::tests::mcp_fallback_is_auto_enabled_only_when_mcp_url_is_configured ... ok
test publish::tests::a_full_broadcast_buffer_never_blocks_a_closed_candle ... ok
test config::tests::the_calendar_poll_cannot_be_configured_below_a_minute ... ok
test publish::tests::a_rejected_tick_publishes_nothing ... ok
test publish::tests::a_book_feeds_the_publisher_end_to_end ... ok
test publish::tests::a_tick_publishes_closed_bars_before_the_next_bucket ... ok
test publish::tests::calendar_and_status_share_the_same_counter ... ok
test session::tests::a_session_spans_the_dst_change_without_a_fixed_24_hours ... ok
test session::tests::reopening_monday_counts_as_open ... ok
test session::tests::the_anchor_follows_daylight_saving_time ... ok
test session::tests::the_week_closes_friday_1700_new_york ... ok
test session::tests::sessions_are_contiguous_and_ordered ... ok
test session::tests::the_week_opens_sunday_1800_new_york ... ok
test publish::tests::freshness_is_marked_from_the_accepted_tick ... ok
test sifting_rest::tests::an_empty_page_is_not_an_error ... ok
test sifting_rest::tests::a_page_is_sorted_ascending_and_typed ... ok
test sifting_rest::tests::earliest_date_is_parsed_across_upstream_formats ... ok
test sifting_rest::tests::impossible_bars_and_duplicate_timestamps_are_rejected ... ok
test sifting_rest::tests::history_requests_carry_interval_order_limit_and_cursor ... ok
test sifting_rest::tests::response_validation_rejects_wrong_symbol_or_interval ... ok
test sifting_rest::tests::the_seed_lookback_covers_the_bars_it_asks_for ... ok
test sifting_rest::tests::truncate_never_splits_a_utf8_boundary ... ok
test sifting_rest::tests::fetch_seed_clamps_422_data_unavailable_to_earliest_and_drops_forming_bar ... ok
test sifting_ws::tests::a_bucket_roll_publishes_the_closed_bar_first ... ok
test sifting_ws::tests::a_second_based_timestamp_is_counted_as_a_drop ... ok
test sifting_ws::tests::a_tick_ahead_of_the_clock_is_dropped ... ok
test sifting_ws::tests::a_tick_without_any_usable_price_is_ignored ... ok
test sifting_ws::tests::a_tick_is_folded_into_every_resolution ... ok
test sifting_ws::tests::an_invalid_price_is_counted_as_a_drop ... ok
test sifting_ws::tests::control_frames_and_other_symbols_are_ignored ... ok
test sifting_ws::tests::the_bid_ask_mid_is_used_when_no_last_trade_is_printed ... ok
test sifting_ws::tests::the_drop_reason_names_are_stable ... ok
test status::tests::boot_identity_is_stable_and_unique_looking ... ok
test status::tests::feed_states_are_reported_per_feed ... ok
test status::tests::freshness_moves_through_offline_live_stale_and_closed ... ok
test status::tests::the_fingerprint_is_order_independent ... ok
test status::tests::the_fingerprint_only_moves_on_a_real_transition ... ok
test status::tests::ticks_never_move_freshness_backwards ... ok
test sifting_rest::tests::rate_limited_page_honors_retry_after_and_succeeds ... ok
test sifting_rest::tests::desc_seed_stops_after_first_full_page_instead_of_walking_all_cursors ... ok
test synthetic::tests::history_ends_before_the_forming_bucket ... ok
test tick_volume::tests::an_empty_store_snapshots_empty ... ok
test tick_volume::tests::capacity_is_enforced_oldest_first ... ok
test tick_volume::tests::out_of_order_bars_stay_sorted ... ok
test tick_volume::tests::upsert_replaces_the_live_bar_and_appends_new_ones ... ok
test types::tests::a_well_formed_candle_is_publishable ... ok
test types::tests::candle_wire_format_carries_every_required_field ... ok
test types::tests::every_execution_shaped_frame_is_recognised_and_refused ... ok
test types::tests::freshness_serializes_with_the_contract_state_names ... ok
test types::tests::freshness_states_follow_the_contract_table ... ok
test types::tests::impossible_candles_are_rejected ... ok
test types::tests::interval_spellings_round_trip_and_align ... ok
test types::tests::live_frames_have_a_seq_and_replay_frames_do_not ... ok
test types::tests::no_published_frame_can_carry_trade_vocabulary ... ok
test synthetic::tests::coarse_bars_are_the_fold_of_the_minute_bars ... ok
test types::tests::second_based_timestamps_are_recognised ... ok
test types::tests::the_sequencer_only_moves_forward ... ok
test ws_server::tests::a_lagged_subscriber_is_told_how_much_it_lost ... ok
test types::tests::only_subscribe_and_heartbeat_parse_as_inbound_frames ... ok
test ws_server::tests::boot_id_is_stable_across_requests ... ok
test ws_server::tests::candle_frames_are_filtered_by_the_requested_intervals ... ok
test ws_server::tests::allowed_origin_may_read_every_rest_route ... ok
test ws_server::tests::candles_are_served_per_interval_and_reject_an_unknown_one ... ok
test ws_server::tests::candles_limit_returns_last_n_closed_bars_and_preserves_default ... ok
test ws_server::tests::every_interval_replays_a_usable_tail ... ok
test ws_server::tests::malformed_origin_panics_at_boot ... ok
test ws_server::tests::cors_grant_is_scoped_to_the_configured_origin ... ok
test ws_server::tests::only_the_configured_origin_is_ever_allowed ... ok
test ws_server::tests::preflight_is_answered_for_the_allowed_origin_only ... ok
test ws_server::tests::no_route_accepts_a_broker_write ... ok
test ws_server::tests::removed_routes_are_not_served ... ok
test ws_server::tests::tick_volume_is_served_uncached_with_its_envelope ... ok
test ws_server::tests::status_reports_the_c1_envelope ... ok
test synthetic::tests::the_set_has_all_three_resolutions_and_the_documented_spans ... ok
test ws_server::tests::ws_broadcast_overflow_emits_gap_frame_over_the_socket ... ok
test synthetic::tests::the_series_is_reproducible ... ok

test result: ok. 105 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.24s


## Live calendar feed test
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.11s
     Running unittests src/main.rs (target/debug/deps/xauusd_engine-c9365006b296bef2)

running 1 test
LIVE CALENDAR: 97 events
  2026-10-25T19:00:00+00:00 | NZD | Holiday | Bank Holiday | forecast=None previous=None gold=false
  2026-10-25T21:10:00+00:00 | AUD | Low | RBA Assist Gov Jones Speaks | forecast=None previous=None gold=false
  2026-10-26T09:00:00+00:00 | EUR | Low | German ifo Business Climate | forecast=Some("90.0") previous=Some("89.9") gold=false
  2026-10-26T11:00:00+00:00 | GBP | Low | CBI Realized Sales | forecast=Some("-47") previous=Some("-55") gold=false
  2026-10-26T23:50:00+00:00 | JPY | Low | SPPI y/y | forecast=Some("3.7%") previous=Some("3.7%") gold=false
  2026-10-27T00:01:00+00:00 | GBP | Low | BRC Shop Price Index y/y | forecast=Some("1.6%") previous=Some("1.4%") gold=false
  2026-10-27T05:00:00+00:00 | JPY | Low | BOJ Core CPI y/y | forecast=Some("1.9%") previous=Some("1.8%") gold=false
  2026-10-27T08:00:00+00:00 | EUR | Low | Spanish Unemployment Rate | forecast=Some("9.9%") previous=Some("9.9%") gold=false
  2026-10-27T09:00:00+00:00 | EUR | Low | M3 Money Supply y/y | forecast=Some("3.5%") previous=Some("3.5%") gold=false
  2026-10-27T09:00:00+00:00 | EUR | Low | Private Loans y/y | forecast=Some("3.1%") previous=Some("3.1%") gold=false
test calendar::live_tests::real_forexfactory_feed_returns_events ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 105 filtered out; finished in 0.05s


## Runtime smoke test (C1 v2 contract matrix)
--- starting the calendar fixture on 127.0.0.1:9099 ---
calendar stub on 127.0.0.1:9099 (4 events, 10s delay)

--- booting the engine (synthetic seed, no market key) ---
--- waiting for /health ---
ok <- /health ok

--- /ws: replay burst, live seq, heartbeats, execution-frame refusal ---
handshake (full): HTTP/1.1 101 Switching Protocols
  sec-websocket-accept: 6W80kqPg6JokV9SLVu6PItBrCb4=
  access-control-allow-origin: https://static-dash-frontend.onrender.com
  vary: origin, access-control-request-method, access-control-request-headers
replay: 15m=96, 1h=48, 1m=240, calendar=0 events
handshake (status-only): HTTP/1.1 101 Switching Protocols
  sec-websocket-accept: +L8EYHRXpo+wwAlfYE9jphar1sk=
  access-control-allow-origin: https://static-dash-frontend.onrender.com
  vary: origin, access-control-request-method, access-control-request-headers
live frames: 2 on the full socket, 1 on the status socket
no broker write path: 6 execution-shaped inbound frames were ignored
heartbeat: answered in both directions, no seq, no payload
re-subscribe: 48 1h candles replayed, all replay:true
recorded 439 frames to /tmp/c1/frames.jsonl
WS CHECK PASSED

--- /status ---
  service          : xauusd-engine v0.4.0
  contract_version : 2.0
  boot_id          : boot-1793044954698-3028
  market_open      : True
  freshness        : {'feed': 'sifting', 'state': 'offline', 'age_ms': 1793044968812}
  session          : {'anchor': '18:00 America/New_York', 'close_ms': 1793052000000, 'market_open': True, 'open_ms': 1792965600000}
  intervals        : {'15m': 2000, '1h': 2000, '1m': 43200}
  ws topics        : ['candle', 'tick_volume', 'calendar', 'status']

--- /candles: three resolutions, one contract ---
interval: 1m (1m buckets)
grid: 43199/43199 gaps are exactly one bucket
history edge: 2 minutes behind now
candles: 43200 bars, sources=['synthetic']
first: time=1790452920000 close=3389.866026062453
last:  time=1793044860000 close=3434.8923048454135
CANDLES CHECK PASSED (1m)

interval: 15m (15m buckets)
grid: 1999/1999 gaps are exactly one bucket
history edge: 3 minutes behind now
candles: 2000 bars, sources=['synthetic']
first: time=1791245700000 close=3391.5375678791384
last:  time=1793044800000 close=3434.8923048454135
CANDLES CHECK PASSED (15m)

interval: 1h (60m buckets)
grid: 1999/1999 gaps are exactly one bucket
history edge: 3 minutes behind now
candles: 2000 bars, sources=['synthetic']
first: time=1785848400000 close=3305.9090827212417
last:  time=1793044800000 close=3434.8923048454135
CANDLES CHECK PASSED (1h)

--- /candles interval and limit handling ---
  ?interval=5m -> 400, ?interval=60m -> 200, ?interval=1m&limit=10 -> last 10 closed bars, ?limit=0 -> 400

--- /tick-volume ---
tick-volume: 0 live bar(s) (no Sifting feed in this run)
TICK VOLUME CHECK PASSED

--- /calendar (hermetic fixture, waiting for the first fetch) ---
source: ff_json | count: 4
--- first 15 events ---
  2026-10-27T12:30:00+00:00    |  USD | High     | Core CPI m/m  (F:0.3% P:0.2%)
  2026-10-27T16:15:00+00:00    |  USD | Medium   | FOMC Member Barr Speaks  (F:None P:None)
  2026-10-28T09:00:00+00:00    |  EUR | Low      | German ifo Business Climate  (F:88.0 P:87.6)
  2026-10-29T04:00:00+00:00    |  ALL | Holiday  | Bank Holiday  (F:None P:None)
TOTAL EVENTS: 4
WITH TIMESTAMPS: 4
GOLD-RELEVANT (high-impact USD): 1
CALENDAR CHECK PASSED

--- CORS: only the Node 2 origin may read the REST API ---
1) allowed origin gets access-control-allow-origin on every route
  /health -> access-control-allow-origin: https://static-dash-frontend.onrender.com
  /status -> access-control-allow-origin: https://static-dash-frontend.onrender.com
  /candles -> access-control-allow-origin: https://static-dash-frontend.onrender.com
  /tick-volume -> access-control-allow-origin: https://static-dash-frontend.onrender.com
  /calendar -> access-control-allow-origin: https://static-dash-frontend.onrender.com
2) preflight (OPTIONS) is authorised for that origin, GET only
  HTTP/1.1 200 OK|vary: origin, access-control-request-method, access-control-request-headers|access-control-allow-methods: GET,OPTIONS|access-control-allow-headers: content-type,accept,origin|access-control-max-age: 600|access-control-allow-origin: https://static-dash-frontend.onrender.com
3) every other origin gets NO allow-header
  /health (foreign) -> HTTP/1.1 200 OK, no CORS grant
  /status (foreign) -> HTTP/1.1 200 OK, no CORS grant
  /candles (foreign) -> HTTP/1.1 200 OK, no CORS grant
  /tick-volume (foreign) -> HTTP/1.1 200 OK, no CORS grant
  /calendar (foreign) -> HTTP/1.1 200 OK, no CORS grant
  OPTIONS /candles (foreign) -> HTTP/1.1 200 OK, no CORS grant
4) no wildcard, no credentials, no secrets in the response
  headers are CORS-only, bodies carry no credentials

5) no write route: every non-GET method is 405 on every registered path
  /health: POST/PUT/PATCH/DELETE -> 405
  /status: POST/PUT/PATCH/DELETE -> 405
  /candles: POST/PUT/PATCH/DELETE -> 405
  /tick-volume: POST/PUT/PATCH/DELETE -> 405
  /calendar: POST/PUT/PATCH/DELETE -> 405

--- C1 contract matrix (contracts/c1-market-feed.schema.json) ---
frames: 4 kind(s) ['calendar', 'candle', 'heartbeat', 'status'], 435 replay, 2 live (seq [2]..[3])
C1 CONTRACT MATRIX PASSED

--- engine log: seed / calendar / CORS / rejection lines ---
2026-10-26T20:02:34.745399Z  WARN xauusd_engine: No upstream history — seeded 43200 SYNTHETIC 1m bars (SEED_SYNTHETIC_CANDLES=1)
2026-10-26T20:02:34.749028Z  WARN xauusd_engine: No upstream history — seeded 2000 SYNTHETIC 15m bars (SEED_SYNTHETIC_CANDLES=1)
2026-10-26T20:02:34.749746Z  WARN xauusd_engine: No upstream history — seeded 2000 SYNTHETIC 1h bars (SEED_SYNTHETIC_CANDLES=1)
2026-10-26T20:02:34.749996Z  INFO xauusd_engine::ws_server: CORS: browser access to the REST API allowed for this origin origin=https://static-dash-frontend.onrender.com
2026-10-26T20:02:34.750102Z  INFO xauusd_engine: Listening on 0.0.0.0:3000 — /health /status /candles /tick-volume /calendar /ws
2026-10-26T20:02:35.286379Z  INFO xauusd_engine::ws_server: WS session sess-1793044955286369473 opened
2026-10-26T20:02:35.286504Z  INFO xauusd_engine::ws_server: WS session sess-1793044955286369473 subscribed to [Candle, TickVolume, Calendar, Status] intervals [M1, M15, H1]
2026-10-26T20:02:35.297923Z  INFO xauusd_engine::ws_server: WS session sess-1793044955297919413 opened
2026-10-26T20:02:35.298039Z  INFO xauusd_engine::ws_server: WS session sess-1793044955297919413 subscribed to [Status] intervals [M1, M15, H1]
2026-10-26T20:02:44.751255Z  INFO xauusd_engine::calendar: Calendar: 4 events from direct feed
2026-10-26T20:02:45.751511Z  INFO xauusd_engine::ws_server: WS session sess-1793044955286369473 ignored an execution-shaped frame
2026-10-26T20:02:45.791687Z  INFO xauusd_engine::ws_server: WS session sess-1793044955286369473 ignored an execution-shaped frame
2026-10-26T20:02:45.791715Z  INFO xauusd_engine::ws_server: WS session sess-1793044955286369473 ignored an execution-shaped frame
2026-10-26T20:02:45.791720Z  INFO xauusd_engine::ws_server: WS session sess-1793044955286369473 ignored an execution-shaped frame
2026-10-26T20:02:45.791726Z  INFO xauusd_engine::ws_server: WS session sess-1793044955286369473 ignored an execution-shaped frame
2026-10-26T20:02:45.791730Z  INFO xauusd_engine::ws_server: WS session sess-1793044955286369473 ignored an execution-shaped frame
2026-10-26T20:02:48.755140Z  INFO xauusd_engine::ws_server: WS session sess-1793044955286369473 subscribed to [Candle] intervals [H1]
2026-10-26T20:02:48.797621Z  INFO xauusd_engine::ws_server: WS session sess-1793044955286369473 error: WebSocket protocol error: Connection reset without closing handshake
2026-10-26T20:02:48.797737Z  INFO xauusd_engine::ws_server: WS session sess-1793044955297919413 error: WebSocket protocol error: Connection reset without closing handshake

SMOKE TEST PASSED

## Clippy
    Checking xauusd-engine v0.4.0 (/home/runner/work/Bot/Bot)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.21s

