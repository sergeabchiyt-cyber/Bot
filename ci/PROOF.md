# CI proof — run 37327208729
commit: 707f51688228dd0e14d63bc81d3fd1070ee8824c
generated: 2026-10-05T14:45:59Z

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
   Compiling mio v1.2.4
   Compiling socket2 v0.6.5
   Compiling getrandom v0.3.4
   Compiling getrandom v0.2.17
   Compiling tokio v1.53.2
   Compiling potential_utf v0.1.6
   Compiling icu_collections v2.3.0
   Compiling ring v0.17.14
   Compiling rand_core v0.9.5
   Compiling icu_provider v2.3.1
   Compiling icu_normalizer v2.3.0
   Compiling ppv-lite86 v0.2.21
   Compiling rustls-webpki v0.103.15
   Compiling rand_chacha v0.9.0
   Compiling rustls v0.23.45
   Compiling rand v0.9.5
   Compiling icu_properties v2.3.0
   Compiling brotli v9.0.0
   Compiling hyper v1.11.1
   Compiling idna_adapter v1.2.2
   Compiling base64 v0.23.1
   Compiling hyper-util v0.1.21
   Compiling compression-codecs v0.4.45
   Compiling idna v1.1.0
   Compiling tokio-rustls v0.26.6
   Compiling tower v0.5.3
   Compiling siphasher v1.0.4
   Compiling url v2.5.8
   Compiling phf_shared v0.12.1
   Compiling async-compression v0.4.50
   Compiling tokio-util v0.7.19
   Compiling tungstenite v0.29.0
   Compiling lazy_static v1.5.1
   Compiling tungstenite v0.26.2
   Compiling tower-http v0.6.11
   Compiling sharded-slab v0.1.7
   Compiling tokio-tungstenite v0.29.0
   Compiling phf v0.12.1
   Compiling hyper-rustls v0.27.10
   Compiling tracing-subscriber v0.3.23
   Compiling chrono-tz v0.10.4
   Compiling reqwest v0.12.28
   Compiling dashmap v6.2.1
   Compiling axum v0.8.9
   Compiling tokio-tungstenite v0.26.2
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

warning: methods `lower_tf`, `ingest_profile_candles`, and `ingest_swing_candles` are never used
   --> src/volume_profile.rs:156:12
    |
128 | impl VolumeProfileEngine {
    | ------------------------ methods in this implementation
...
156 |     pub fn lower_tf(&self) -> LowerTf {
    |            ^^^^^^^^
...
201 |     pub fn ingest_profile_candles(&mut self, candles: Vec<VpCandle>) {
    |            ^^^^^^^^^^^^^^^^^^^^^^
...
215 |     pub fn ingest_swing_candles(&mut self, candles: Vec<VpCandle>) {
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
    Finished `test` profile [unoptimized + debuginfo] target(s) in 21.72s
     Running unittests src/main.rs (target/debug/deps/xauusd_engine-a43c736f72c15a5f)

running 82 tests
test ai_cache::tests::records_transcripts_and_reports_todays_counts ... ok
test ai_cache::tests::health_is_kept_verbatim_plus_timestamp ... ok
test calendar::live_tests::real_forexfactory_feed_returns_events ... ignored, requires network
test ai_cache::tests::stale_entries_are_pruned ... ok
test calendar::tests::handles_empty_feed ... ok
test calendar::tests::impact_colors_are_normalized ... ok
test calendar::tests::markdown_fallback_skips_headers_and_separators ... ok
test calendar::tests::parses_forexfactory_weekly_json ... ok
test calendar::tests::rejects_html_rate_limit_page ... ok
test calendar::tests::source_has_fallback_urls ... ok
test config::tests::cors_origin_is_normalized_for_header_comparison ... ok
test config::tests::default_origin_is_already_in_header_form ... ok
test config::tests::from_env_falls_back_to_the_node2_site ... ok
test econ_monitor::tests::all_currency_matches_everything ... ok
test econ_monitor::tests::fed_events_are_recognized ... ok
test econ_monitor::tests::b64_matches_known_vectors ... ok
test econ_monitor::tests::extracts_live_urls_and_prefers_live ... ok
test econ_monitor::tests::tone_chunk_is_f32le_of_expected_length ... ok
test sifting_rest::tests::a_short_chart_response_is_rejected_instead_of_seeding_a_partial_page ... ok
test econ_monitor::tests::window_selects_only_active_high_impact_usd_events ... ok
test sifting_rest::tests::a_sparse_profile_page_is_rejected_even_when_it_reaches_both_ends ... ok
test sifting_rest::tests::chart_history_request_is_a_single_fixed_two_thousand_bar_page ... ok
test sifting_rest::tests::profile_history_request_carries_interval_and_encoded_cursor ... ok
test sifting_rest::tests::response_validation_rejects_wrong_symbol_or_interval ... ok
test sifting_ws::tests::bucket_roll_emits_one_closed_bar_then_a_fresh_live_bar ... ok
test sifting_ws::tests::candle_volume_is_the_tick_count_like_sifting_history ... ok
test sifting_ws::tests::handle_text_writes_the_store_and_broadcasts_candle_plus_tick_volume ... ok
test sifting_ws::tests::profile_candle_closes_on_one_minute_boundaries ... ok
test sifting_ws::tests::resubscribe_snapshot_and_late_ticks_are_not_counted ... ok
test sifting_ws::tests::rolling_rate_counts_the_last_ten_seconds ... ok
test sifting_ws::tests::tick_rule_splits_up_down_and_flat ... ok
test tick_volume::tests::capacity_is_enforced_oldest_first ... ok
test sifting_ws::tests::tick_volume_frame_wire_shape ... ok
test types::tests::levels_wire_format_is_tagged_and_includes_the_sunday_open_price ... ok
test tick_volume::tests::out_of_order_bars_stay_sorted ... ok
test tick_volume::tests::upsert_replaces_the_live_bar_and_appends_new_ones ... ok
test types::tests::node3_trade_event_uses_the_tagged_trades_envelope ... ok
test volume_profile::histogram::tests::absurd_bin_sizes_are_rejected_instead_of_allocating ... ok
test volume_profile::histogram::tests::levels_payload_carries_the_audit_metadata ... ok
test volume_profile::histogram::tests::poc_sits_in_the_heaviest_band_and_value_area_contains_it ... ok
test volume_profile::histogram::tests::price_rows_are_aligned_to_the_fixed_half_dollar_grid ... ok
test volume_profile::histogram::tests::rows_are_not_snapped_to_the_half_dollar_grid_in_rows_mode ... ok

thread 'volume_profile::histogram::tests::rows_layout_creates_a_short_top_row_for_a_partial_range' (3804) panicked at src/volume_profile/histogram.rs:549:9:
assertion `left == right` failed
  left: 11
 right: 10
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test volume_profile::histogram::tests::rows_layout_rounds_row_height_to_whole_ticks_like_tradingview ... ok
test volume_profile::histogram::tests::empty_or_flat_windows_produce_no_levels ... ok

thread 'volume_profile::histogram::tests::value_area_tie_break_prefers_the_row_closer_to_the_poc' (3807) panicked at src/volume_profile/histogram.rs:584:9:
assertion `left == right` failed
  left: 8
 right: 7
test sifting_rest::tests::complete_profile_history_passes_the_density_check ... ok
test volume_profile::histogram::tests::rows_layout_creates_a_short_top_row_for_a_partial_range ... FAILED
test volume_profile::histogram::tests::up_and_down_volume_follow_the_bar_direction ... ok
test volume_profile::histogram::tests::value_area_tie_break_prefers_the_row_closer_to_the_poc ... FAILED
test volume_profile::session::tests::session_boundary_survives_dst_change ... ok
test volume_profile::session::tests::session_close_is_1800_new_york ... ok
test volume_profile::session::tests::session_shift_keeps_the_1800_anchor_across_dst ... ok
test volume_profile::swing::tests::bearish_swing_is_anchored_high_to_low ... ok
test volume_profile::swing::tests::bullish_swing_is_anchored_low_to_high ... ok
test volume_profile::swing::tests::fallback_leg_is_bounded_to_recent_bars ... ok
test volume_profile::swing::tests::plateau_highs_anchor_at_the_most_recent_touch ... ok
test volume_profile::tests::ingesting_a_candle_after_a_close_refreshes_ps ... ok
test volume_profile::tests::duplicate_candle_timestamp_is_replaced ... ok
test volume_profile::tests::cw_appears_once_monday_closes_and_freezes_intraday ... ok
test volume_profile::tests::ps_skips_the_weekend_gap ... ok
test volume_profile::tests::ps_rolls_forward_at_every_session_close ... ok
test volume_profile::tests::time_profiles_and_swing_can_use_separate_history_resolutions ... ok
test volume_profile::tests::pw_marks_the_sunday_start_candle_open_price ... ok
test volume_profile::tests::refresh_fires_once_per_close_and_ignores_empty_weekend_sessions ... ok

thread 'volume_profile::timeframe::tests::aggregation_sums_volume_and_keeps_the_bucket_ohlc' (3826) panicked at src/volume_profile/timeframe.rs:275:9:
assertion `left == right` failed
  left: 4103.0
 right: 4102.0
test volume_profile::timeframe::tests::aggregation_merges_out_of_order_chunks ... ok
test volume_profile::timeframe::tests::aggregation_sums_volume_and_keeps_the_bucket_ohlc ... FAILED
test volume_profile::timeframe::tests::labels_round_trip ... ok
test volume_profile::timeframe::tests::prepare_input_never_downscales_coarse_history ... ok
test volume_profile::timeframe::tests::prepare_input_reports_one_minute_for_short_windows ... ok
test volume_profile::tests::week_windows_do_not_overlap_the_session_window ... ok
test volume_profile::weekly::tests::previous_week_runs_sunday_open_to_friday_close ... ok
test volume_profile::weekly::tests::week_start_is_the_sunday_1800_open ... ok
test ws_server::tests::ai_route_reports_todays_digest ... ok
test volume_profile::timeframe::tests::weekly_windows_use_5m_and_sessions_use_1m_like_tradingview ... ok
test ws_server::tests::envelope_normalization_wraps_loose_producer_payloads ... ok
test ws_server::tests::cors_grant_is_scoped_to_the_configured_origin ... ok
test ws_server::tests::malformed_origin_panics_at_boot ... ok
test ws_server::tests::allowed_origin_may_read_every_rest_route ... ok
test ws_server::tests::payload_shape_is_untouched_by_the_layer ... ok
test ws_server::tests::preflight_is_answered_for_the_allowed_origin_only ... ok
test ws_server::tests::tick_volume_route_serves_the_live_bars_uncached ... ok
test volume_profile::tests::pw_excludes_weekend_candles_and_is_stable_all_week ... ok
test ws_server::tests::only_the_configured_origin_is_ever_allowed ... ok

failures:

failures:
    volume_profile::histogram::tests::rows_layout_creates_a_short_top_row_for_a_partial_range
    volume_profile::histogram::tests::value_area_tie_break_prefers_the_row_closer_to_the_poc
    volume_profile::timeframe::tests::aggregation_sums_volume_and_keeps_the_bucket_ohlc

test result: FAILED. 78 passed; 3 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.03s

error: test failed, to rerun pass `--bin xauusd-engine`

## Live calendar feed test
(missing)

## Runtime smoke test
(missing)
