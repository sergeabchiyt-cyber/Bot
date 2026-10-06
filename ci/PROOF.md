# CI proof — run 37523541632
commit: 219901f41acdc451f956e057c2ac310511db7c37
generated: 2026-10-06T20:04:33Z

## Unit tests
   Compiling libc v0.2.190
   Compiling smallvec v1.16.2
   Compiling yoke v0.8.3
   Compiling zeroize v1.9.1
   Compiling rustls-pki-types v1.15.1
   Compiling zerovec v0.11.8
   Compiling zerotrie v0.2.5
   Compiling zerocopy v0.8.60
   Compiling alloc-no-stdlib v3.0.0
   Compiling tinystr v0.8.4
   Compiling potential_utf v0.1.6
   Compiling icu_collections v2.3.0
   Compiling icu_locale_core v2.3.0
   Compiling parking_lot_core v0.9.12
   Compiling errno v0.3.14
   Compiling signal-hook-registry v1.4.8
   Compiling parking_lot v0.12.5
   Compiling mio v1.2.4
   Compiling socket2 v0.6.5
   Compiling tokio v1.53.2
   Compiling getrandom v0.2.17
   Compiling getrandom v0.3.4
   Compiling ring v0.17.14
   Compiling rand_core v0.9.5
   Compiling icu_provider v2.3.1
   Compiling alloc-stdlib v0.3.0
   Compiling want v0.3.2
   Compiling icu_properties v2.3.0
   Compiling ppv-lite86 v0.2.21
   Compiling rustls-webpki v0.103.15
   Compiling rand_chacha v0.9.0
   Compiling rand v0.9.5
   Compiling icu_normalizer v2.3.0
   Compiling rustls v0.23.45
   Compiling brotli-decompressor v6.0.1
   Compiling idna_adapter v1.2.2
   Compiling webpki-roots v1.0.9
   Compiling base64 v0.23.1
   Compiling idna v1.1.0
   Compiling hyper v1.12.0
   Compiling brotli v9.0.0
   Compiling hyper-util v0.1.21
   Compiling tokio-rustls v0.26.6
   Compiling tower v0.5.3
   Compiling compression-codecs v0.4.45
   Compiling siphasher v1.0.4
   Compiling phf_shared v0.12.1
   Compiling url v2.5.8
   Compiling async-compression v0.4.50
   Compiling tokio-util v0.7.19
   Compiling tungstenite v0.29.0
   Compiling lazy_static v1.5.1
   Compiling sharded-slab v0.1.7
   Compiling tower-http v0.6.11
   Compiling tokio-tungstenite v0.29.0
   Compiling tungstenite v0.26.2
   Compiling phf v0.12.1
   Compiling hyper-rustls v0.27.10
   Compiling webpki-roots v0.26.11
   Compiling tracing-subscriber v0.3.23
   Compiling reqwest v0.12.28
   Compiling axum v0.8.9
   Compiling tokio-tungstenite v0.26.2
   Compiling dashmap v6.2.1
   Compiling chrono-tz v0.10.4
   Compiling xauusd-engine v0.3.0 (/home/runner/work/Bot/Bot)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 23.56s
     Running unittests src/main.rs (target/debug/deps/xauusd_engine-8d98b85d3dff5ec5)

running 89 tests
test calendar::live_tests::real_forexfactory_feed_returns_events ... ignored, requires network
test ai_cache::tests::records_transcripts_and_reports_todays_counts ... ok
test ai_cache::tests::health_is_kept_verbatim_plus_timestamp ... ok
test calendar::tests::handles_empty_feed ... ok
test ai_cache::tests::stale_entries_are_pruned ... ok
test calendar::tests::impact_colors_are_normalized ... ok
test calendar::tests::markdown_fallback_skips_headers_and_separators ... ok
test calendar::tests::rejects_html_rate_limit_page ... ok
test calendar::tests::parses_forexfactory_weekly_json ... ok
test config::tests::cors_origin_is_normalized_for_header_comparison ... ok
test config::tests::default_origin_is_already_in_header_form ... ok
test econ_monitor::tests::all_currency_matches_everything ... ok
test calendar::tests::source_has_fallback_urls ... ok
test config::tests::from_env_falls_back_to_the_node2_site ... ok
test econ_monitor::tests::extracts_live_urls_and_prefers_live ... ok
test econ_monitor::tests::fed_events_are_recognized ... ok
test econ_monitor::tests::b64_matches_known_vectors ... ok
test econ_monitor::tests::tone_chunk_is_f32le_of_expected_length ... ok
test sifting_rest::tests::a_sparse_profile_page_is_rejected_even_when_it_reaches_both_ends ... ok
test econ_monitor::tests::window_selects_only_active_high_impact_usd_events ... ok
test sifting_rest::tests::a_short_chart_response_is_rejected_instead_of_seeding_a_partial_page ... ok
test sifting_rest::tests::profile_history_request_carries_interval_and_encoded_cursor ... ok
test sifting_rest::tests::chart_history_request_is_a_single_fixed_two_thousand_bar_page ... ok
test sifting_rest::tests::response_validation_rejects_wrong_symbol_or_interval ... ok
test sifting_ws::tests::bucket_roll_emits_one_closed_bar_then_a_fresh_live_bar ... ok
test sifting_rest::tests::history_tail_drops_the_boundary_bar_and_keeps_the_newer_ones ... ok
test sifting_ws::tests::candle_volume_is_the_tick_count_like_sifting_history ... ok
test sifting_ws::tests::handle_text_writes_the_store_and_broadcasts_candle_plus_tick_volume ... ok
test sifting_ws::tests::profile_candle_closes_on_one_minute_boundaries ... ok
test sifting_ws::tests::rolling_rate_counts_the_last_ten_seconds ... ok
test sifting_ws::tests::resubscribe_snapshot_and_late_ticks_are_not_counted ... ok
test sifting_ws::tests::tick_rule_splits_up_down_and_flat ... ok
test tick_volume::tests::capacity_is_enforced_oldest_first ... ok
test sifting_ws::tests::tick_volume_frame_wire_shape ... ok
test tick_volume::tests::out_of_order_bars_stay_sorted ... ok
test tick_volume::tests::upsert_replaces_the_live_bar_and_appends_new_ones ... ok
test types::tests::levels_wire_format_is_tagged_and_includes_the_sunday_open_price ... ok
test types::tests::trade_frames_are_not_part_of_the_node1_wire_contract ... ok
test volume_profile::histogram::tests::absurd_bin_sizes_are_rejected_instead_of_allocating ... ok
test volume_profile::histogram::tests::empty_or_flat_windows_produce_no_levels ... ok
test volume_profile::histogram::tests::levels_payload_carries_the_audit_metadata ... ok
test volume_profile::histogram::tests::poc_sits_in_the_heaviest_band_and_value_area_contains_it ... ok
test sifting_rest::tests::complete_profile_history_passes_the_density_check ... ok
test volume_profile::histogram::tests::price_rows_are_aligned_to_the_fixed_half_dollar_grid ... ok
test volume_profile::histogram::tests::rows_layout_creates_a_short_top_row_for_a_partial_range ... ok
test volume_profile::histogram::tests::rows_are_not_snapped_to_the_half_dollar_grid_in_rows_mode ... ok
test volume_profile::histogram::tests::rows_layout_rounds_row_height_to_whole_ticks_like_tradingview ... ok
test volume_profile::histogram::tests::up_and_down_volume_follow_the_bar_direction ... ok
test volume_profile::session::tests::filler_only_sessions_are_skipped_for_the_previous_session ... ok
test volume_profile::histogram::tests::value_area_tie_break_prefers_the_row_closer_to_the_poc ... ok
test volume_profile::session::tests::session_boundary_survives_dst_change ... ok
test volume_profile::swing::tests::bearish_swing_is_anchored_high_to_low ... ok
test volume_profile::session::tests::session_close_is_1800_new_york ... ok
test volume_profile::session::tests::session_shift_keeps_the_1800_anchor_across_dst ... ok
test volume_profile::swing::tests::bullish_swing_is_anchored_low_to_high ... ok
test volume_profile::tests::custom_range_profiles_an_arbitrary_window ... ok
test volume_profile::swing::tests::plateau_highs_anchor_at_the_most_recent_touch ... ok
test volume_profile::swing::tests::fallback_leg_is_bounded_to_recent_bars ... ok
test volume_profile::tests::custom_range_rejects_inverted_and_unretained_ranges ... ok
test volume_profile::tests::duplicate_candle_timestamp_is_replaced ... ok
test volume_profile::tests::ingesting_a_candle_after_a_close_refreshes_ps ... ok
test volume_profile::tests::ps_skips_the_weekend_gap ... ok
test volume_profile::tests::cw_appears_once_monday_closes_and_freezes_intraday ... ok
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
test ws_server::tests::an_execution_frame_cannot_be_submitted_to_node1 ... ok
test ws_server::tests::cors_grant_is_scoped_to_the_configured_origin ... ok
test ws_server::tests::envelope_normalization_wraps_loose_producer_payloads ... ok
test volume_profile::timeframe::tests::weekly_windows_use_5m_and_sessions_use_1m_like_tradingview ... ok
test ws_server::tests::malformed_origin_panics_at_boot ... ok
test ws_server::tests::allowed_origin_may_read_every_rest_route ... ok
test ws_server::tests::payload_shape_is_untouched_by_the_layer ... ok
test ws_server::tests::preflight_is_answered_for_the_allowed_origin_only ... ok
test ws_server::tests::tick_volume_route_serves_the_live_bars_uncached ... ok
test ws_server::tests::only_the_configured_origin_is_ever_allowed ... ok
test volume_profile::tests::pw_excludes_weekend_candles_and_is_stable_all_week ... ok
test ws_server::tests::vp_custom_range_validates_its_parameters ... ok
test ws_server::tests::no_route_accepts_a_broker_write ... ok

test result: ok. 88 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.03s


## Live calendar feed test
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.10s
     Running unittests src/main.rs (target/debug/deps/xauusd_engine-8d98b85d3dff5ec5)

running 1 test
LIVE CALENDAR: 82 events
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

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 88 filtered out; finished in 0.22s


## Runtime smoke test
(missing)

## Clippy
    Checking xauusd-engine v0.3.0 (/home/runner/work/Bot/Bot)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.18s

