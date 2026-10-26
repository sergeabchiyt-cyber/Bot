# CI proof — run 40342445872
commit: 624be4f7d790fd8d3116618ba5778070c96dc0dd
generated: 2026-10-26T14:45:02Z

## Unit tests
   Compiling brotli-decompressor v6.2.0
   Compiling brotli v9.1.0
   Compiling compression-codecs v0.4.46
   Compiling async-compression v0.4.51
   Compiling tower-http v0.6.11
   Compiling reqwest v0.12.28
   Compiling xauusd-engine v0.4.0 (/home/runner/work/Bot/Bot)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 5.10s
     Running unittests src/main.rs (target/debug/deps/xauusd_engine-c9365006b296bef2)

running 93 tests
test calendar::live_tests::real_forexfactory_feed_returns_events ... ignored, requires network
test calendar::tests::handles_empty_feed ... ok
test calendar::tests::impact_colors_are_normalized ... ok
test calendar::tests::markdown_fallback_skips_headers_and_separators ... ok
test calendar::tests::parses_forexfactory_weekly_json ... ok
test calendar::tests::rejects_html_rate_limit_page ... ok
test calendar::tests::source_has_fallback_urls ... ok
test candles::tests::a_closed_bar_is_published_exactly_once_and_before_the_next_bucket ... ok

thread 'candles::tests::a_full_series_evicts_the_oldest_bar_never_the_newest' (2458) panicked at src/candles.rs:770:9:
assertion `left == right` failed
  left: [1700000100000, 1700001000000, 1700001900000, 1700002800000]
 right: [1700000100000, 1700001000000, 1700001900000]
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test candles::tests::counters_add_up ... ok
test candles::tests::late_ticks_do_not_rewrite_a_closed_bucket ... ok
test candles::tests::non_finite_and_non_positive_prices_are_dropped ... ok
test candles::tests::a_full_series_evicts_the_oldest_bar_never_the_newest ... FAILED
test candles::tests::second_based_timestamps_are_dropped ... ok
test candles::tests::a_resubscribe_snapshot_tick_is_not_counted_twice ... ok
test candles::tests::a_seed_row_never_reopens_a_bar ... ok
test candles::tests::the_book_reports_edges_and_retains_the_recent_tail ... ok
test candles::tests::the_forming_bucket_is_not_in_the_candle_cache ... ok
test candles::tests::the_three_resolutions_roll_together ... ok
test candles::tests::tick_volume_ticks_equal_the_candle_volume ... ok
test candles::tests::ticks_more_than_five_seconds_in_the_future_are_dropped ... ok
test candles::tests::unaligned_seed_rows_are_rejected_and_duplicates_merge_ascending ... ok
test config::tests::cors_origin_is_normalized_for_header_comparison ... ok
test config::tests::default_origin_is_already_in_header_form ... ok
test config::tests::from_env_falls_back_to_the_node2_site ... ok
test config::tests::the_calendar_poll_cannot_be_configured_below_a_minute ... ok
test publish::tests::a_book_feeds_the_publisher_end_to_end ... ok
test publish::tests::a_full_broadcast_buffer_never_blocks_a_closed_candle ... ok
test publish::tests::a_rejected_tick_publishes_nothing ... ok
test publish::tests::freshness_is_marked_from_the_accepted_tick ... ok
test publish::tests::a_tick_publishes_closed_bars_before_the_next_bucket ... ok
test publish::tests::calendar_and_status_share_the_same_counter ... ok
test session::tests::a_session_spans_the_dst_change_without_a_fixed_24_hours ... ok
test session::tests::reopening_monday_counts_as_open ... ok
test session::tests::sessions_are_contiguous_and_ordered ... ok
test session::tests::the_anchor_follows_daylight_saving_time ... ok
test session::tests::the_week_closes_friday_1700_new_york ... ok
test sifting_rest::tests::an_empty_page_is_not_an_error ... ok
test session::tests::the_week_opens_sunday_1800_new_york ... ok
test sifting_rest::tests::a_page_is_sorted_ascending_and_typed ... ok
test sifting_rest::tests::response_validation_rejects_wrong_symbol_or_interval ... ok
test sifting_rest::tests::history_requests_carry_interval_order_limit_and_cursor ... ok
test sifting_rest::tests::impossible_bars_and_duplicate_timestamps_are_rejected ... ok
test sifting_rest::tests::the_seed_lookback_covers_the_bars_it_asks_for ... ok
test sifting_rest::tests::truncate_never_splits_a_utf8_boundary ... ok
test sifting_ws::tests::a_second_based_timestamp_is_counted_as_a_drop ... ok
test sifting_ws::tests::a_tick_ahead_of_the_clock_is_dropped ... ok
test sifting_ws::tests::a_bucket_roll_publishes_the_closed_bar_first ... ok
test sifting_ws::tests::a_tick_without_any_usable_price_is_ignored ... ok
test sifting_ws::tests::a_tick_is_folded_into_every_resolution ... ok
test sifting_ws::tests::an_invalid_price_is_counted_as_a_drop ... ok
test sifting_ws::tests::the_drop_reason_names_are_stable ... ok
test sifting_ws::tests::the_bid_ask_mid_is_used_when_no_last_trade_is_printed ... ok
test sifting_ws::tests::control_frames_and_other_symbols_are_ignored ... ok
test status::tests::boot_identity_is_stable_and_unique_looking ... ok
test status::tests::freshness_moves_through_offline_live_stale_and_closed ... ok
test status::tests::feed_states_are_reported_per_feed ... ok
test status::tests::the_fingerprint_only_moves_on_a_real_transition ... ok
test status::tests::the_fingerprint_is_order_independent ... ok
test status::tests::ticks_never_move_freshness_backwards ... ok
test synthetic::tests::history_ends_before_the_forming_bucket ... ok
test tick_volume::tests::an_empty_store_snapshots_empty ... ok
test tick_volume::tests::capacity_is_enforced_oldest_first ... ok
test synthetic::tests::coarse_bars_are_the_fold_of_the_minute_bars ... ok
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
test types::tests::only_subscribe_and_heartbeat_parse_as_inbound_frames ... ok
test types::tests::second_based_timestamps_are_recognised ... ok
test types::tests::the_sequencer_only_moves_forward ... ok
test ws_server::tests::a_lagged_subscriber_is_told_how_much_it_lost ... ok
test ws_server::tests::boot_id_is_stable_across_requests ... ok
test ws_server::tests::candle_frames_are_filtered_by_the_requested_intervals ... ok
test ws_server::tests::allowed_origin_may_read_every_rest_route ... ok
test ws_server::tests::candles_are_served_per_interval_and_reject_an_unknown_one ... ok
test ws_server::tests::every_interval_replays_a_usable_tail ... ok
test synthetic::tests::the_set_has_all_three_resolutions_and_the_documented_spans ... ok
test ws_server::tests::cors_grant_is_scoped_to_the_configured_origin ... ok
test ws_server::tests::malformed_origin_panics_at_boot ... ok
test ws_server::tests::preflight_is_answered_for_the_allowed_origin_only ... ok
test ws_server::tests::removed_routes_are_not_served ... ok
test ws_server::tests::only_the_configured_origin_is_ever_allowed ... ok
test ws_server::tests::status_reports_the_c1_envelope ... ok
test ws_server::tests::tick_volume_is_served_uncached_with_its_envelope ... ok
test ws_server::tests::no_route_accepts_a_broker_write ... ok
test synthetic::tests::the_series_is_reproducible ... ok

failures:

failures:
    candles::tests::a_full_series_evicts_the_oldest_bar_never_the_newest

test result: FAILED. 91 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.14s

error: test failed, to rerun pass `--bin xauusd-engine`

## Live calendar feed test
(missing)

## Runtime smoke test (C1 v2 contract matrix)
(missing)

## Clippy
    Checking xauusd-engine v0.4.0 (/home/runner/work/Bot/Bot)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.03s

