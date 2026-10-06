# CI proof — run 37522240654
commit: 103dd3f62d487aeec2fedc71f969c934aecd3766
generated: 2026-10-06T19:54:02Z

## Unit tests
(missing)

## Live calendar feed test
(missing)

## Runtime smoke test
(missing)

## Clippy
    Checking xauusd-engine v0.3.0 (/home/runner/work/Bot/Bot)
error: digits grouped inconsistently by underscores
   --> src/econ_monitor.rs:534:66
    |
534 |             ev("FOMC Press Conference", "USD", "High", now - 2 * 3600_000), // after window
    |                                                                  ^^^^^^^^ help: consider: `3_600_000`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#inconsistent_digit_grouping
    = note: `-D clippy::inconsistent-digit-grouping` implied by `-D warnings`
    = help: to override `-D warnings` add `#[allow(clippy::inconsistent_digit_grouping)]`

error: unnecessary `>= y + 1` or `x - 1 >=`
   --> src/volume_profile/swing.rs:132:8
    |
132 |     if candles.len() >= SWING_PIVOT_RADIUS * 2 + 1 {
    |        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ help: change it to: `candles.len() > SWING_PIVOT_RADIUS * 2`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#int_plus_one
    = note: `-D clippy::int-plus-one` implied by `-D warnings`
    = help: to override `-D warnings` add `#[allow(clippy::int_plus_one)]`

error: field `symbol_type` is never read
  --> src/types.rs:30:9
   |
 8 | pub struct AggTrade {
   |            -------- field in this struct
...
30 |     pub symbol_type: Option<i32>,
   |         ^^^^^^^^^^^
   |
   = note: `AggTrade` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis
   = note: `-D dead-code` implied by `-D warnings`
   = help: to override `-D warnings` add `#[expect(dead_code)]` or `#[allow(dead_code)]`

error: associated items `new`, `ingest_profile_candles`, `ingest_swing_candles`, `candle_count`, `last_session_close_utc`, and `most_recent_week_start_utc` are never used
   --> src/volume_profile.rs:147:12
    |
146 | impl VolumeProfileEngine {
    | ------------------------ associated items in this implementation
147 |     pub fn new() -> Self {
    |            ^^^
...
211 |     pub fn ingest_profile_candles(&mut self, candles: Vec<VpCandle>) {
    |            ^^^^^^^^^^^^^^^^^^^^^^
...
225 |     pub fn ingest_swing_candles(&mut self, candles: Vec<VpCandle>) {
    |            ^^^^^^^^^^^^^^^^^^^^
...
503 |     pub fn candle_count(&self) -> usize {
    |            ^^^^^^^^^^^^
...
508 |     pub fn last_session_close_utc(now_ms: i64) -> i64 {
    |            ^^^^^^^^^^^^^^^^^^^^^^
...
512 |     pub fn most_recent_week_start_utc(now_ms: i64) -> i64 {
    |            ^^^^^^^^^^^^^^^^^^^^^^^^^^

error: field `subscriptions` is never read
  --> src/ws_server.rs:34:9
   |
32 | pub struct AppState {
   |            -------- field in this struct
33 |     pub tx: broadcast::Sender<WsFrame>,
34 |     pub subscriptions: Arc<DashMap<String, Vec<String>>>,
   |         ^^^^^^^^^^^^^
   |
   = note: `AppState` has a derived impl for the trait `Clone`, but this is intentionally ignored during dead code analysis

error: this `if` statement can be collapsed
  --> src/ai_cache.rs:69:9
   |
69 | /         if let Some(obj) = entry.as_object_mut() {
70 | |             if !matches!(obj.get("ts"), Some(serde_json::Value::Number(_))) {
71 | |                 obj.insert("ts".into(), chrono::Utc::now().timestamp_millis().into());
72 | |             }
73 | |         }
   | |_________^
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#collapsible_if
   = note: `-D clippy::collapsible-if` implied by `-D warnings`
   = help: to override `-D warnings` add `#[allow(clippy::collapsible_if)]`
help: collapse nested if block
   |
69 ~         if let Some(obj) = entry.as_object_mut()
70 ~             && !matches!(obj.get("ts"), Some(serde_json::Value::Number(_))) {
71 |                 obj.insert("ts".into(), chrono::Utc::now().timestamp_millis().into());
72 ~             }
   |

error: manually reimplementing `div_ceil`
   --> src/econ_monitor.rs:169:41
    |
169 |     let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    |                                         ^^^^^^^^^^^^^^^^^^^^ help: consider using `.div_ceil()`: `data.len().div_ceil(3)`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#manual_div_ceil
    = note: `-D clippy::manual-div-ceil` implied by `-D warnings`
    = help: to override `-D warnings` add `#[allow(clippy::manual_div_ceil)]`

error: this `if` statement can be collapsed
   --> src/mcp_client.rs:114:17
    |
114 | /                 if let Ok(v) = serde_json::from_str::<Value>(data) {
115 | |                     if body.get("id").is_some() && v.get("id") == body.get("id") {
116 | |                         return Ok(Some(v));
117 | |                     }
118 | |                 }
    | |_________________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#collapsible_if
help: collapse nested if block
    |
114 ~                 if let Ok(v) = serde_json::from_str::<Value>(data)
115 ~                     && body.get("id").is_some() && v.get("id") == body.get("id") {
116 |                         return Ok(Some(v));
117 ~                     }
    |

error: this `if` statement can be collapsed
   --> src/mcp_client.rs:190:9
    |
190 | /         if let Ok(name) = std::env::var(env_key) {
191 | |             if !name.trim().is_empty() {
192 | |                 return Some(name.trim().to_string());
193 | |             }
194 | |         }
    | |_________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#collapsible_if
help: collapse nested if block
    |
190 ~         if let Ok(name) = std::env::var(env_key)
191 ~             && !name.trim().is_empty() {
192 |                 return Some(name.trim().to_string());
193 ~             }
    |

error: this `if` statement can be collapsed
   --> src/mcp_client.rs:293:9
    |
293 | /         if item.get("type").and_then(|t| t.as_str()) == Some("text") {
294 | |             if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
295 | |                 out.push_str(text);
296 | |                 out.push('\n');
297 | |             }
298 | |         }
    | |_________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#collapsible_if
help: collapse nested if block
    |
293 ~         if item.get("type").and_then(|t| t.as_str()) == Some("text")
294 ~             && let Some(text) = item.get("text").and_then(|t| t.as_str()) {
295 |                 out.push_str(text);
296 |                 out.push('\n');
297 ~             }
    |

error: methods `ingest_profile_candles` and `ingest_swing_candles` are never used
   --> src/volume_profile.rs:211:12
    |
146 | impl VolumeProfileEngine {
    | ------------------------ methods in this implementation
...
211 |     pub fn ingest_profile_candles(&mut self, candles: Vec<VpCandle>) {
    |            ^^^^^^^^^^^^^^^^^^^^^^
...
225 |     pub fn ingest_swing_candles(&mut self, candles: Vec<VpCandle>) {
    |            ^^^^^^^^^^^^^^^^^^^^

error: this `if` statement can be collapsed
   --> src/multi_exchange.rs:766:25
    |
766 | /                         if let Some(s) = d["s"].as_str() {
767 | |                             if s != symbol {
768 | |                                 continue;
769 | |                             }
770 | |                         }
    | |_________________________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#collapsible_if
help: collapse nested if block
    |
766 ~                         if let Some(s) = d["s"].as_str()
767 ~                             && s != symbol {
768 |                                 continue;
769 ~                             }
    |

error: this `if` statement can be collapsed
   --> src/order_flow.rs:153:9
    |
153 | /         if let Some(&last) = self.last_emit.get(kind) {
154 | |             if now_ms - last < DEBOUNCE_MS {
155 | |                 return false;
156 | |             }
157 | |         }
    | |_________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#collapsible_if
help: collapse nested if block
    |
153 ~         if let Some(&last) = self.last_emit.get(kind)
154 ~             && now_ms - last < DEBOUNCE_MS {
155 |                 return false;
156 ~             }
    |

error: this function has too many arguments (8/7)
  --> src/sifting_rest.rs:54:1
   |
54 | / fn history_url(
55 | |     base_url: &str,
56 | |     symbol: &str,
57 | |     start: DateTime<Utc>,
...  |
62 | |     cursor: Option<&str>,
63 | | ) -> Result<reqwest::Url> {
   | |_________________________^
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#too_many_arguments
   = note: `-D clippy::too-many-arguments` implied by `-D warnings`
   = help: to override `-D warnings` add `#[allow(clippy::too_many_arguments)]`

error: this `if` statement can be collapsed
  --> src/sifting_rest.rs:90:5
   |
90 | /     if let Some(response_symbol) = response.meta.symbol.as_deref() {
91 | |         if !response_symbol.eq_ignore_ascii_case(symbol) {
92 | |             anyhow::bail!("SiftingIO returned symbol {response_symbol}, expected {symbol}");
93 | |         }
94 | |     }
   | |_____^
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#collapsible_if
help: collapse nested if block
   |
90 ~     if let Some(response_symbol) = response.meta.symbol.as_deref()
91 ~         && !response_symbol.eq_ignore_ascii_case(symbol) {
92 |             anyhow::bail!("SiftingIO returned symbol {response_symbol}, expected {symbol}");
93 ~         }
   |

error: this `if` statement can be collapsed
  --> src/sifting_rest.rs:95:5
   |
95 | /     if let Some(interval) = response.meta.interval.as_deref() {
96 | |         if interval != expected_interval {
97 | |             anyhow::bail!("SiftingIO returned interval {interval}, expected {expected_interval}");
98 | |         }
99 | |     }
   | |_____^
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#collapsible_if
help: collapse nested if block
   |
95 ~     if let Some(interval) = response.meta.interval.as_deref()
96 ~         && interval != expected_interval {
97 |             anyhow::bail!("SiftingIO returned interval {interval}, expected {expected_interval}");
98 ~         }
   |

error: using `clone` on type `DateTime<Utc>` which implements the `Copy` trait
   --> src/sifting_rest.rs:280:13
    |
280 |             start.clone(),
    |             ^^^^^^^^^^^^^ help: try removing the `clone` call: `start`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#clone_on_copy
    = note: `-D clippy::clone-on-copy` implied by `-D warnings`
    = help: to override `-D warnings` add `#[allow(clippy::clone_on_copy)]`

error: using `clone` on type `DateTime<Utc>` which implements the `Copy` trait
   --> src/sifting_rest.rs:281:13
    |
281 |             end.clone(),
    |             ^^^^^^^^^^^ help: try removing the `clone` call: `end`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#clone_on_copy

error: this function has too many arguments (8/7)
   --> src/sifting_ws.rs:254:1
    |
254 | / pub async fn run_sifting_stream(
255 | |     base_url: String,
256 | |     api_key: String,
257 | |     symbol: String,
...   |
262 | |     status: FeedStatus,
263 | | ) -> anyhow::Result<()> {
    | |_______________________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#too_many_arguments

error: this function has too many arguments (9/7)
   --> src/volume_profile/histogram.rs:174:5
    |
174 | /     pub fn levels(
175 | |         &self,
176 | |         window: &str,
177 | |         start: i64,
...   |
183 | |         input_bars: usize,
184 | |     ) -> VpLevels {
    | |_________________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#too_many_arguments

error: this `if` statement can be collapsed
   --> src/volume_profile/swing.rs:250:5
    |
250 | /     if let Some(previous) = pivots.last_mut() {
251 | |         if previous.kind == kind {
252 | |             let replace = match kind {
253 | |                 PivotKind::High => candles[index].high >= candles[previous.index].high,
...   |
261 | |     }
    | |_____^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#collapsible_if
help: collapse nested if block
    |
250 ~     if let Some(previous) = pivots.last_mut()
251 ~         && previous.kind == kind {
252 |             let replace = match kind {
...
259 |             return;
260 ~         }
    |

error: manual implementation of an assign operation
  --> src/volume_profile/weekly.rs:37:9
   |
37 |         sunday = sunday - Duration::days(7);
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ help: replace it with: `sunday -= Duration::days(7)`
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#assign_op_pattern
   = note: `-D clippy::assign-op-pattern` implied by `-D warnings`
   = help: to override `-D warnings` add `#[allow(clippy::assign_op_pattern)]`

error: could not compile `xauusd-engine` (bin "xauusd-engine") due to 20 previous errors
warning: build failed, waiting for other jobs to finish...
error: could not compile `xauusd-engine` (bin "xauusd-engine" test) due to 21 previous errors

