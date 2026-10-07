//! Durable execution ledger: the idempotency backbone of Node 4.
//!
//! `intent_id` is the system-wide idempotency key (Node 3, Node 4, MT5 bridge,
//! EA comment, reconciliation, audit). The ledger is an append-only JSONL file
//! (`EXECUTION_LEDGER_FILE`, default `data/execution_ledger.jsonl`):
//!
//! * every accepted intent is **durably persisted (append + flush + fsync)
//!   before any broker write**;
//! * a repeated `intent_id` replays the recorded result and never places a
//!   second order;
//! * broker commands, outcomes, reconciliation results, and report delivery
//!   are appended to the same file;
//! * **no secrets ever reach the ledger** — intents and reports carry no
//!   tokens by contract.
//!
//! If durable persistence is unavailable while a real execution venue is
//! selected, execution fails closed (see `intent.rs`).

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::types::{TradeIntent, now_ms};

/// Result of claiming an intent id in the ledger.
pub enum Claim {
    /// First time we see this id — the intent has been durably recorded.
    New,
    /// The id was already claimed; the recorded state is returned so the
    /// caller replays it instead of executing again.
    Duplicate(LedgerRecord),
}

/// Durable persistence failed. Node 4 must refuse to trade on this error.
#[derive(Debug)]
pub struct LedgerError(pub String);

impl std::fmt::Display for LedgerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "execution ledger unavailable: {}", self.0)
    }
}

impl std::error::Error for LedgerError {}

/// The recorded, replayable state of one intent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerRecord {
    pub intent_id: String,
    /// `received` | `accepted` | `filled` | `partial` | `rejected` |
    /// `unknown` | `cancelled` | `closed` | `dry_run`
    pub status: String,
    pub venue: String,
    pub symbol: String,
    pub side: String,
    pub execution_id: Option<String>,
    pub filled_price: Option<f64>,
    pub quantity: Option<f64>,
    pub quantity_unit: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub intent: TradeIntent,
    pub created_at: i64,
    pub updated_at: i64,
}

/// One JSONL audit line. The shape is stable; readers may ignore unknown
/// fields (forward compatibility).
#[derive(Debug, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum LedgerEvent<'a> {
    IntentReceived {
        intent_id: &'a str,
        schema_version: u32,
        strategy: &'a str,
        symbol: &'a str,
        side: &'a str,
        order_type: &'a str,
        reference_price: f64,
        stop_loss: f64,
        take_profit: f64,
        risk_reward: f64,
        level_name: &'a str,
        created_at: i64,
        expires_at: i64,
    },
    IntentAccepted {
        intent_id: &'a str,
        venue: &'a str,
    },
    IntentRejected {
        intent_id: &'a str,
        code: &'a str,
        message: &'a str,
    },
    DryRun {
        intent_id: &'a str,
        note: &'a str,
    },
    BrokerCommand {
        intent_id: &'a str,
        venue: &'a str,
        command: &'a str,
        volume: Option<f64>,
        sl: Option<f64>,
        tp: Option<f64>,
    },
    BrokerOutcome {
        intent_id: &'a str,
        venue: &'a str,
        symbol: &'a str,
        side: &'a str,
        status: &'a str,
        execution_id: Option<&'a str>,
        filled_price: Option<f64>,
        quantity: Option<f64>,
        quantity_unit: Option<&'a str>,
        retcode: Option<i64>,
        reconciled: bool,
    },
    Reconciled {
        intent_id: &'a str,
        venue: &'a str,
        status: &'a str,
        execution_id: Option<&'a str>,
    },
    ReportSent {
        intent_id: &'a str,
        status: &'a str,
    },
    ReportAck {
        intent_id: &'a str,
        accepted: bool,
    },
}

impl<'a> LedgerEvent<'a> {
    fn line(&self) -> String {
        let mut value = serde_json::to_value(self).expect("ledger event serializes");
        value["ts"] = serde_json::json!(now_ms());
        let mut line = value.to_string();
        line.push('\n');
        line
    }
}

struct LedgerInner {
    file: File,
    index: HashMap<String, LedgerRecord>,
}

/// Append-only, durable idempotency + audit ledger.
///
/// All public methods serialize on a single internal mutex; appends are
/// followed by a flush **and** an fsync before the method returns, which is
/// what makes a recorded intent safe to treat as "already ours".
pub struct ExecutionLedger {
    path: PathBuf,
    inner: Mutex<LedgerInner>,
}

impl ExecutionLedger {
    /// Open (creating the parent directory and file if needed) and load the
    /// existing index from disk. A corrupt line is counted and skipped — it
    /// must not crash startup, but a corrupt file is logged by the caller.
    pub fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;

        let contents = std::fs::read_to_string(&path).unwrap_or_default();
        let mut index = HashMap::new();
        let mut corrupt = 0usize;
        for line in contents.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match Self::record_from_line(line) {
                Some(record) => {
                    // Merge in audit order: the first line with the full
                    // intent body wins the identity fields; the last write
                    // wins the status and outcome fields.
                    match index.get_mut(&record.intent_id) {
                        Some(existing) => {
                            if existing.symbol.is_empty() {
                                existing.symbol = record.symbol.clone();
                            }
                            if existing.side.is_empty() {
                                existing.side = record.side.clone();
                            }
                            if !record.venue.is_empty() {
                                existing.venue = record.venue.clone();
                            }
                            existing.status = record.status;
                            existing.updated_at = record.updated_at.max(existing.updated_at);
                            if existing.execution_id.is_none() {
                                existing.execution_id = record.execution_id.clone();
                            }
                            if existing.filled_price.is_none() {
                                existing.filled_price = record.filled_price;
                            }
                            if existing.quantity.is_none() {
                                existing.quantity = record.quantity;
                            }
                            if existing.quantity_unit.is_none() {
                                existing.quantity_unit = record.quantity_unit.clone();
                            }
                            if existing.error_code.is_none() {
                                existing.error_code = record.error_code.clone();
                            }
                            if existing.error_message.is_none() {
                                existing.error_message = record.error_message.clone();
                            }
                        }
                        None => {
                            index.insert(record.intent_id.clone(), record);
                        }
                    }
                }
                None => corrupt += 1,
            }
        }
        if corrupt > 0 {
            tracing::warn!(
                "execution ledger {}: {} corrupt line(s) skipped",
                path.display(),
                corrupt
            );
        }
        Ok(Self {
            path,
            inner: Mutex::new(LedgerInner { file, index }),
        })
    }

        /// Rebuild a `LedgerRecord` from one audit line.
    ///
    /// `intent_received` lines carry the full intent body. Later lines
    /// (`broker_outcome`, `reconciled`, …) only update the record claimed by
    /// the id; when one is seen before its receive line (a partially written
    /// file), a stub record is created so the outcome is not lost.
    fn record_from_line(line: &str) -> Option<LedgerRecord> {
        let value: serde_json::Value = serde_json::from_str(line).ok()?;
        let event = value.get("event")?.as_str()?;
        let intent_id = value.get("intent_id")?.as_str()?;
        let ts = value.get("ts").and_then(|v| v.as_i64()).unwrap_or(0);
        let get_str = |key: &str| value.get(key).and_then(|v| v.as_str()).unwrap_or_default();
        let get_f64 = |key: &str| value.get(key).and_then(|v| v.as_f64());
        let get_i64 = |key: &str| value.get(key).and_then(|v| v.as_i64());

        match event {
            "intent_received" => {
                let intent: TradeIntent = serde_json::from_value(serde_json::json!({
                    "schema_version": value.get("schema_version"),
                    "intent_id": intent_id,
                    "strategy": value.get("strategy"),
                    "symbol": value.get("symbol"),
                    "side": value.get("side"),
                    "order_type": value.get("order_type"),
                    "reference_price": value.get("reference_price"),
                    "stop_loss": value.get("stop_loss"),
                    "take_profit": value.get("take_profit"),
                    "risk_reward": value.get("risk_reward"),
                    "level_name": value.get("level_name"),
                    "source_candle_time": value.get("source_candle_time"),
                    "created_at": value.get("created_at"),
                    "expires_at": value.get("expires_at"),
                }))
                .ok()?;
                Some(LedgerRecord {
                    intent_id: intent_id.to_string(),
                    status: "received".into(),
                    venue: String::new(),
                    symbol: intent.symbol.clone(),
                    side: intent.side.clone(),
                    execution_id: None,
                    filled_price: None,
                    quantity: None,
                    quantity_unit: None,
                    error_code: None,
                    error_message: None,
                    intent,
                    created_at: ts,
                    updated_at: ts,
                })
            }
            other => {
                let status = match other {
                    "intent_accepted" => "accepted".to_string(),
                    "dry_run" => "dry_run".to_string(),
                    "intent_rejected" => "rejected".to_string(),
                    "broker_outcome" => get_str("status"),
                    "reconciled" => get_str("status"),
                    _ => return None,
                };
                if status.is_empty() {
                    return None;
                }
                Some(LedgerRecord {
                    intent_id: intent_id.to_string(),
                    status,
                    venue: get_str("venue"),
                    symbol: get_str("symbol"),
                    side: get_str("side"),
                    execution_id: value
                        .get("execution_id")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    filled_price: get_f64("filled_price"),
                    quantity: get_f64("quantity"),
                    quantity_unit: value
                        .get("quantity_unit")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    error_code: value
                        .get("code")
                        .or_else(|| value.get("error_code"))
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    error_message: value
                        .get("message")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    intent: stub_intent(intent_id, get_str("symbol"), get_str("side"), get_i64("expires_at")),
                    created_at: ts,
                    updated_at: ts,
                })
            }
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Number of distinct intent ids recorded.
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, intent_id: &str) -> Option<LedgerRecord> {
        self.inner.lock().unwrap().index.get(intent_id).cloned()
    }

    /// Durable append of one audit event. Returns `Err` when the write or
    /// the fsync failed — callers must treat that as fail-closed.
    pub async fn append(&self, event: &LedgerEvent<'_>) -> Result<(), LedgerError> {
        self.append_impl(event)
    }

    fn append_impl(&self, event: &LedgerEvent<'_>) -> Result<(), LedgerError> {
        let line = event.line();
        let mut guard = self.inner.lock().unwrap();
        guard
            .file
            .write_all(line.as_bytes())
            .and_then(|_| guard.file.flush())
            .and_then(|_| guard.file.sync_all())
            .map_err(|err| LedgerError(format!("{}: {err}", self.path.display())))
    }

    /// Claim an intent id: durably record it as received, or report the
    /// existing record when the id was already claimed.
    ///
    /// The append happens **before** the in-memory index is updated, so a
    /// crash between the two cannot create a gap: the next startup reloads
    /// the id from disk.
    pub async fn claim(&self, intent: &TradeIntent) -> Result<Claim, LedgerError> {
        self.claim_impl(intent)
    }

    fn claim_impl(&self, intent: &TradeIntent) -> Result<Claim, LedgerError> {
        let event = LedgerEvent::IntentReceived {
            intent_id: &intent.intent_id,
            schema_version: intent.schema_version,
            strategy: &intent.strategy,
            symbol: &intent.symbol,
            side: &intent.side,
            order_type: &intent.order_type,
            reference_price: intent.reference_price,
            stop_loss: intent.stop_loss,
            take_profit: intent.take_profit,
            risk_reward: intent.risk_reward,
            level_name: &intent.level_name,
            created_at: intent.created_at,
            expires_at: intent.expires_at,
        };
        let line = event.line();

        let mut guard = self.inner.lock().unwrap();
        if let Some(existing) = guard.index.get(&intent.intent_id) {
            // Replay path: the id is already ours. Re-appending the receive
            // line keeps the audit trail honest about the retry.
            drop(guard);
            let _ = self.append_impl(&event);
            return Ok(Claim::Duplicate(existing.clone()));
        }

        guard
            .file
            .write_all(line.as_bytes())
            .and_then(|_| guard.file.flush())
            .and_then(|_| guard.file.sync_all())
            .map_err(|err| LedgerError(format!("{}: {err}", self.path.display())))?;

        let record = LedgerRecord {
            intent_id: intent.intent_id.clone(),
            status: "received".into(),
            venue: String::new(),
            symbol: intent.symbol.clone(),
            side: intent.side.clone(),
            execution_id: None,
            filled_price: None,
            quantity: None,
            quantity_unit: None,
            error_code: None,
            error_message: None,
            intent: intent.clone(),
            created_at: now_ms(),
            updated_at: now_ms(),
        };
        guard.index.insert(intent.intent_id.clone(), record);
        Ok(Claim::New)
    }

    /// Update the recorded status of an intent (and append an audit line).
    ///
    /// `audit` is the line describing the transition; the index fields are
    /// merged on top.
    pub async fn set_status(
        &self,
        intent_id: &str,
        status: &str,
        audit: &LedgerEvent<'_>,
        fields: impl FnOnce(&mut LedgerRecord),
    ) -> Result<(), LedgerError> {
        self.set_status_impl(intent_id, status, audit, fields)
    }

    fn set_status_impl(
        &self,
        intent_id: &str,
        status: &str,
        audit: &LedgerEvent<'_>,
        fields: impl FnOnce(&mut LedgerRecord),
    ) -> Result<(), LedgerError> {
        self.append_impl(audit)?;
        let mut guard = self.inner.lock().unwrap();
        let Some(record) = guard.index.get_mut(intent_id) else {
            // A status change without a claim is a bug, but not fatal:
            // keep the audit line, skip the index.
            return Ok(());
        };
        record.status = status.to_string();
        record.updated_at = now_ms();
        fields(record);
        Ok(())
    }
}

/// Minimal placeholder for records reloaded from audit lines that did not
/// carry the full intent body. Replay only needs the id, status, symbol and
/// side, which live on the record itself.
fn stub_intent(intent_id: &str, symbol: &str, side: &str, expires_at: Option<i64>) -> TradeIntent {
    TradeIntent {
        schema_version: 1,
        intent_id: intent_id.to_string(),
        strategy: String::new(),
        symbol: symbol.to_string(),
        side: side.to_string(),
        order_type: "market".to_string(),
        reference_price: 0.0,
        stop_loss: 0.0,
        take_profit: 0.0,
        risk_reward: 0.0,
        level_name: String::new(),
        source_candle_time: 0,
        created_at: 0,
        expires_at: expires_at.unwrap_or(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("node4-ledger-{}-{}.jsonl", std::process::id(), name));
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
            reference_price: 2650.0,
            stop_loss: 2647.5,
            take_profit: 2656.0,
            risk_reward: 2.4,
            level_name: "PW PoC".into(),
            source_candle_time: 0,
            created_at: 1,
            expires_at: 120_000,
        }
    }

    #[tokio::test]
    async fn claim_is_unique_and_duplicates_replay_the_recorded_state() {
        let path = temp_path("claim");
        let ledger = ExecutionLedger::open(&path).unwrap();
        let i = intent("n3-intent-1");

        assert!(matches!(ledger.claim(&i).await.unwrap(), Claim::New));
        let again = intent("n3-intent-1");
        match ledger.claim(&again).await.unwrap() {
            Claim::Duplicate(record) => {
                assert_eq!(record.intent_id, "n3-intent-1");
                assert_eq!(record.symbol, "XAUUSD");
                assert_eq!(record.side, "buy");
            }
            Claim::New => panic!("a second claim for the same id must replay"),
        }
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn ledger_survives_restart_and_replays_status() {
        let path = temp_path("restart");
        {
            let ledger = ExecutionLedger::open(&path).unwrap();
            let i = intent("n3-intent-2");
            assert!(matches!(ledger.claim(&i).await.unwrap(), Claim::New));
            ledger
                .append(&LedgerEvent::IntentAccepted {
                    intent_id: "n3-intent-2",
                    venue: "deriv_mt5_demo",
                })
                .await
                .unwrap();
            ledger
                .set_status(
                    "n3-intent-2",
                    "filled",
                    &LedgerEvent::BrokerOutcome {
                        intent_id: "n3-intent-2",
                        venue: "deriv_mt5_demo",
                        symbol: "XAUUSD",
                        side: "buy",
                        status: "filled",
                        execution_id: Some("mt5-deal-456"),
                        filled_price: Some(2650.28),
                        quantity: Some(0.01),
                        quantity_unit: Some("lots"),
                        retcode: Some(10009),
                        reconciled: false,
                    },
                    |record| {
                        record.execution_id = Some("mt5-deal-456".into());
                        record.filled_price = Some(2650.28);
                        record.quantity = Some(0.01);
                        record.quantity_unit = Some("lots".into());
                    },
                )
                .unwrap();
        }

        // Simulate a Node 4 restart: the id must still be ours, with status.
        let ledger = ExecutionLedger::open(&path).unwrap();
        assert_eq!(ledger.len(), 1);
        let record = ledger.get("n3-intent-2").expect("record must survive restart");
        assert_eq!(record.status, "filled");
        assert_eq!(record.execution_id.as_deref(), Some("mt5-deal-456"));
        assert_eq!(record.filled_price, Some(2650.28));
        assert_eq!(record.intent.reference_price, 2650.0);

        // A replayed intent id still returns the duplicate.
        assert!(matches!(
            ledger.claim(&intent("n3-intent-2")).await.unwrap(),
            Claim::Duplicate(_)
        ));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn ledger_lines_never_carry_secret_material() {
        let path = temp_path("nosecrets");
        let ledger = ExecutionLedger::open(&path).unwrap();
        let i = intent("n3-intent-3");
        assert!(matches!(ledger.claim(&i).await.unwrap(), Claim::New));
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("\"intent_id\":\"n3-intent-3\""));
        assert!(!contents.contains("NODE4_SHARED_TOKEN"));
        assert!(!contents.contains("MT5_BRIDGE_TOKEN"));
        let _ = std::fs::remove_file(&path);
    }
}
