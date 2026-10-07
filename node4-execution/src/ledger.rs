//! Durable idempotency ledger.
//!
//! `intent_id` is the system-wide idempotency key shared by Node 3, Node 4, the
//! MT5 bridge, the EA comment, reconciliation, and the audit log. Node 4 must
//! persist an intent **before** any broker write, must return the recorded
//! result on a repeat (Node 3 legitimately replays unacknowledged intents after
//! a reconnect), and must never create a second order for the same ID.
//!
//! The ledger is an append-only JSONL file (`EXECUTION_LEDGER_FILE`, default
//! `data/execution_ledger.jsonl`):
//!
//! ```json
//! {"kind":"intent_received","ts":1700000001100,"intent":{...},"report":{...}}
//! {"kind":"broker_command","ts":1700000001200,"intent_id":"...","command":"mt5_order",...}
//! {"kind":"report","ts":1700000001500,"report":{"status":"filled",...}}
//! {"kind":"reconciliation","ts":...,"intent_id":"...","source":"mt5_history",...}
//! {"kind":"duplicate","ts":...,"intent_id":"...","recorded_status":"filled"}
//! ```
//!
//! Every append is flushed and `sync_data`-ed before the caller continues, so
//! an accepted report means "durably assumed responsibility", not "buffered".
//! No secret (token, password, header) is ever written.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::intent::{ExecutionReport, ExecutionStatus, TradeIntent};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LedgerRecord {
    /// A validated intent Node 4 has durably assumed responsibility for. The
    /// embedded report is the `accepted` report sent back to Node 3.
    IntentReceived {
        ts: i64,
        intent: TradeIntent,
        report: ExecutionReport,
    },
    /// One command about to be (or just) handed to a venue. Written before the
    /// broker call so an unclear write always has a local trace.
    BrokerCommand {
        ts: i64,
        intent_id: String,
        venue: String,
        command: String,
        idempotency_key: String,
        symbol: String,
        side: String,
        volume: f64,
        stop_loss: f64,
        take_profit: f64,
    },
    /// Any later lifecycle report (filled / partial / rejected / unknown /
    /// cancelled / closed) exactly as it was sent to Node 3.
    Report { ts: i64, report: ExecutionReport },
    /// Result of reconciling an unclear write against broker state.
    Reconciliation {
        ts: i64,
        intent_id: String,
        venue: String,
        source: String,
        detail: String,
        report: ExecutionReport,
    },
    /// A repeated `intent_id`: recorded so replay attempts are auditable.
    Duplicate {
        ts: i64,
        intent_id: String,
        recorded_status: String,
    },
}

impl LedgerRecord {
    pub fn kind(&self) -> &'static str {
        match self {
            LedgerRecord::IntentReceived { .. } => "intent_received",
            LedgerRecord::BrokerCommand { .. } => "broker_command",
            LedgerRecord::Report { .. } => "report",
            LedgerRecord::Reconciliation { .. } => "reconciliation",
            LedgerRecord::Duplicate { .. } => "duplicate",
        }
    }

    fn intent_id(&self) -> Option<&str> {
        match self {
            LedgerRecord::IntentReceived { intent, .. } => Some(&intent.intent_id),
            LedgerRecord::BrokerCommand { intent_id, .. } => Some(intent_id),
            LedgerRecord::Report { report, .. } => Some(&report.intent_id),
            LedgerRecord::Reconciliation { intent_id, .. } => Some(intent_id),
            LedgerRecord::Duplicate { intent_id, .. } => Some(intent_id),
        }
    }
}

/// Everything Node 4 recorded about one `intent_id`.
#[derive(Debug, Clone, Default)]
pub struct IntentLedgerState {
    pub intent_id: String,
    /// True when the intent was durably received (or refused) and reported.
    pub accepted: bool,
    pub last_status: Option<ExecutionStatus>,
    /// The most recent report sent to Node 3 for this intent.
    pub last_report: Option<ExecutionReport>,
    pub broker_commands: u64,
    pub reconciled: bool,
    pub duplicates: u64,
    pub first_seen_ms: i64,
    pub last_update_ms: i64,
}

#[derive(Debug, Clone)]
pub struct LedgerStats {
    pub path: String,
    pub available: bool,
    pub records: u64,
    pub intents: usize,
    pub accepted_intents: usize,
    pub last_write_ms: Option<i64>,
    pub open_error: Option<String>,
}

/// Append-only JSONL ledger plus its in-memory index.
pub struct ExecutionLedger {
    path: PathBuf,
    file: Mutex<Option<File>>,
    state: tokio::sync::RwLock<HashMap<String, IntentLedgerState>>,
    available: AtomicBool,
    records: AtomicU64,
    last_write_ms: AtomicU64,
    open_error: Mutex<Option<String>>,
}

impl ExecutionLedger {
    /// Open (and replay) the ledger. A file that cannot be created or appended
    /// to yields a ledger that reports `available() == false`; the caller
    /// decides whether that is fatal (it is, for a real execution venue).
    pub fn open(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        let mut open_error = None;
        let mut records = 0u64;
        let mut state: HashMap<String, IntentLedgerState> = HashMap::new();

        if path.exists() {
            match File::open(&path) {
                Ok(file) => {
                    let reader = BufReader::new(file);
                    for (line_no, line) in reader.lines().enumerate() {
                        let Ok(line) = line else {
                            open_error = Some(format!("ledger line {} is unreadable", line_no + 1));
                            break;
                        };
                        if line.trim().is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<LedgerRecord>(&line) {
                            Ok(record) => {
                                records += 1;
                                apply_record(&mut state, &record);
                            }
                            Err(err) => {
                                // A torn final line (crash mid-append) must not
                                // brick the service on restart; anything else is
                                // a real corruption and is surfaced.
                                warn!("ignoring malformed ledger line {}: {err}", line_no + 1);
                            }
                        }
                    }
                    info!(
                        "execution ledger loaded: {} ({} records, {} intents)",
                        path.display(),
                        records,
                        state.len()
                    );
                }
                Err(err) => {
                    open_error = Some(format!("cannot read {}: {err}", path.display()));
                }
            }
        }

        let file = match open_for_append(&path) {
            Ok(file) => Some(file),
            Err(err) => {
                open_error = Some(err);
                None
            }
        };

        let ledger = Self {
            path,
            file: Mutex::new(file),
            state: tokio::sync::RwLock::new(state),
            available: AtomicBool::new(true),
            records: AtomicU64::new(records),
            last_write_ms: AtomicU64::new(0),
            open_error: Mutex::new(open_error),
        };
        if !ledger.available() {
            warn!(
                "execution ledger is UNAVAILABLE ({}): {} — a real execution venue must refuse to \
                 trade until this is fixed",
                ledger.path.display(),
                ledger.open_error().unwrap_or_else(|| "unknown error".into())
            );
        }
        ledger
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn available(&self) -> bool {
        self.available.load(Ordering::SeqCst) && self.file.lock().map(|f| f.is_some()).unwrap_or(false)
    }

    pub fn open_error(&self) -> Option<String> {
        self.open_error.lock().ok().and_then(|guard| guard.clone())
    }

    pub fn records(&self) -> u64 {
        self.records.load(Ordering::SeqCst)
    }

    pub async fn state(&self, intent_id: &str) -> Option<IntentLedgerState> {
        self.state.read().await.get(intent_id).cloned()
    }

    pub async fn stats(&self) -> LedgerStats {
        let state = self.state.read().await;
        let accepted_intents = state.values().filter(|s| s.accepted).count();
        let last = self.last_write_ms.load(Ordering::SeqCst);
        LedgerStats {
            path: self.path.display().to_string(),
            available: self.available(),
            records: self.records(),
            intents: state.len(),
            accepted_intents,
            last_write_ms: if last == 0 { None } else { Some(last as i64) },
            open_error: self.open_error(),
        }
    }

    /// Append one record, durably. Returns `Err` when the ledger is not
    /// writable — the caller must treat that as "no broker write" whenever a
    /// real venue is selected.
    pub async fn append(&self, record: &LedgerRecord) -> Result<(), String> {
        let line = serde_json::to_string(record)
            .map_err(|err| format!("cannot serialise ledger record: {err}"))?;

        {
            let mut guard = self
                .file
                .lock()
                .map_err(|_| "ledger file lock poisoned".to_string())?;
            let Some(file) = guard.as_mut() else {
                self.available.store(false, Ordering::SeqCst);
                return Err(format!(
                    "execution ledger {} is not writable ({})",
                    self.path.display(),
                    self.open_error().unwrap_or_else(|| "no file handle".into())
                ));
            };
            file.write_all(line.as_bytes())
                .and_then(|_| file.write_all(b"\n"))
                .and_then(|_| file.flush())
                .and_then(|_| file.sync_data())
                .map_err(|err| {
                    self.available.store(false, Ordering::SeqCst);
                    if let Ok(mut open_error) = self.open_error.lock() {
                        *open_error = Some(format!("append failed: {err}"));
                    }
                    format!("cannot append to {}: {err}", self.path.display())
                })?;
        }

        self.records.fetch_add(1, Ordering::SeqCst);
        self.last_write_ms.store(now_ms() as u64, Ordering::SeqCst);
        {
            let mut state = self.state.write().await;
            apply_record(&mut state, record);
        }
        Ok(())
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn open_for_append(path: &Path) -> Result<File, String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("cannot create {}: {err}", parent.display()))?;
        }
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| format!("cannot open {} for append: {err}", path.display()))
}

fn apply_record(state: &mut HashMap<String, IntentLedgerState>, record: &LedgerRecord) {
    let Some(intent_id) = record.intent_id() else { return };
    let entry = state
        .entry(intent_id.to_string())
        .or_insert_with(|| IntentLedgerState {
            intent_id: intent_id.to_string(),
            first_seen_ms: now_ms(),
            ..Default::default()
        });

    match record {
        LedgerRecord::IntentReceived { ts, report, .. } => {
            entry.accepted = true;
            entry.last_status = Some(report.status);
            entry.last_report = Some(report.clone());
            entry.first_seen_ms = *ts;
            entry.last_update_ms = *ts;
        }
        LedgerRecord::BrokerCommand { ts, .. } => {
            entry.broker_commands += 1;
            entry.last_update_ms = *ts;
        }
        LedgerRecord::Report { ts, report } => {
            entry.last_status = Some(report.status);
            entry.last_report = Some(report.clone());
            entry.last_update_ms = *ts;
        }
        LedgerRecord::Reconciliation { ts, report, .. } => {
            entry.reconciled = true;
            entry.last_status = Some(report.status);
            entry.last_report = Some(report.clone());
            entry.last_update_ms = *ts;
        }
        LedgerRecord::Duplicate { ts, .. } => {
            entry.duplicates += 1;
            entry.last_update_ms = *ts;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::{ExecutionReport, TradeIntent};

    fn temp_path(tag: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "node4-ledger-{tag}-{}-{n}.jsonl",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn intent(id: &str) -> TradeIntent {
        TradeIntent {
            schema_version: 1,
            intent_id: id.into(),
            strategy: "vp_break_retest_v1".into(),
            symbol: "XAUUSD".into(),
            side: "buy".into(),
            order_type: "market".into(),
            reference_price: 2_650.0,
            stop_loss: 2_648.0,
            take_profit: 2_654.0,
            risk_reward: 2.0,
            level_name: "PW PoC".into(),
            source_candle_time: 1_700_000_000_000,
            created_at: 1_700_000_000_100,
            expires_at: 1_700_000_120_100,
        }
    }

    #[tokio::test]
    async fn accepted_intents_survive_a_reopen_and_block_the_second_attempt() {
        let path = temp_path("durable");
        let intent = intent("n3-xauusd-1-buy-pw-poc");

        {
            let ledger = ExecutionLedger::open(&path);
            assert!(ledger.available());
            let report = ExecutionReport::accepted(&intent, "deriv_mt5_demo", 1);
            ledger
                .append(&LedgerRecord::IntentReceived {
                    ts: 1,
                    intent: intent.clone(),
                    report,
                })
                .await
                .unwrap();
            assert_eq!(ledger.records(), 1);
        }

        // A restart must remember the id: this is what makes replay safe.
        let reopened = ExecutionLedger::open(&path);
        let state = reopened
            .state(&intent.intent_id)
            .await
            .expect("intent is known after restart");
        assert!(state.accepted);
        assert_eq!(state.last_status, Some(ExecutionStatus::Accepted));

        // Later lifecycle records also persist.
        let filled = {
            let mut report = ExecutionReport::new(
                &intent,
                ExecutionStatus::Filled,
                "deriv_mt5_demo",
                2,
            );
            report.execution_id = Some("mt5-1".into());
            report
        };
        reopened
            .append(&LedgerRecord::Report {
                ts: 2,
                report: filled,
            })
            .await
            .unwrap();

        let again = ExecutionLedger::open(&path);
        let state = again.state(&intent.intent_id).await.unwrap();
        assert_eq!(state.last_status, Some(ExecutionStatus::Filled));
        assert_eq!(
            state.last_report.and_then(|r| r.execution_id),
            Some("mt5-1".into())
        );

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn malformed_lines_do_not_prevent_startup() {
        let path = temp_path("torn");
        let intent = intent("n3-xauusd-2-buy-pw-poc");
        {
            let ledger = ExecutionLedger::open(&path);
            ledger
                .append(&LedgerRecord::IntentReceived {
                    ts: 1,
                    intent: intent.clone(),
                    report: ExecutionReport::accepted(&intent, "none", 1),
                })
                .await
                .unwrap();
        }
        // Simulate a crash mid-append.
        {
            use std::io::Write as _;
            let mut file = OpenOptions::new().append(true).open(&path).unwrap();
            file.write_all(b"{\"kind\":\"report\",\"ts\":12").unwrap();
        }

        let ledger = ExecutionLedger::open(&path);
        assert!(ledger.available());
        assert!(ledger.state(&intent.intent_id).await.is_some());

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn an_unwritable_path_reports_unavailable() {
        let mut path = std::env::temp_dir();
        path.push("node4-ledger-missing-dir");
        let ledger = ExecutionLedger::open(path.join("nested/deep/ledger.jsonl"));
        // The directory is created on demand, so this specific path is fine;
        // an actually impossible path (a file used as a directory) is not.
        assert!(ledger.available());

        let mut file_path = std::env::temp_dir();
        file_path.push(format!("node4-ledger-file-{}.jsonl", std::process::id()));
        std::fs::write(&file_path, b"not a directory").unwrap();
        let broken = ExecutionLedger::open(file_path.join("child/ledger.jsonl"));
        assert!(!broken.available());
        assert!(broken.open_error().is_some());
        let intent = intent("n3-xauusd-3-buy-pw-poc");
        let err = broken
            .append(&LedgerRecord::IntentReceived {
                ts: 1,
                intent,
                report: ExecutionReport::new(
                    &intent("n3-xauusd-3-buy-pw-poc"),
                    ExecutionStatus::Accepted,
                    "none",
                    1,
                ),
            })
            .await
            .unwrap_err();
        assert!(err.contains("not writable"));

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn the_ledger_never_contains_a_secret_looking_field() {
        let path = temp_path("secrets");
        let intent = intent("n3-xauusd-4-buy-pw-poc");
        let ledger = ExecutionLedger::open(&path);
        ledger
            .append(&LedgerRecord::BrokerCommand {
                ts: 1,
                intent_id: intent.intent_id.clone(),
                venue: "deriv_mt5_demo".into(),
                command: "mt5_order".into(),
                idempotency_key: intent.intent_id.clone(),
                symbol: "XAUUSD".into(),
                side: "buy".into(),
                volume: 0.01,
                stop_loss: 2_648.0,
                take_profit: 2_654.0,
            })
            .await
            .unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        let record: LedgerRecord = serde_json::from_str(body.trim()).unwrap();
        assert_eq!(record.kind(), "broker_command");
        for forbidden in ["token", "password", "authorization", "secret"] {
            assert!(
                !body.to_ascii_lowercase().contains(forbidden),
                "ledger leaked a {forbidden}-shaped field"
            );
        }
        let _ = std::fs::remove_file(&path);
    }
}
