//! Deriv MT5 **demo** execution bridge (library target).
//!
//! The binary (`src/main.rs`) is a thin shell around these modules so that the
//! contract tests in `tests/contract.rs` can drive the same code paths the
//! deployed bridge runs.
//!
//! Module map:
//!
//! | module | responsibility |
//! |---|---|
//! | `config` | environment configuration + fail-closed validation |
//! | `proto` | EA ⇄ bridge line protocol (framing, percent codec, parse) |
//! | `ea_link` | loopback TCP server the terminal's EA dials into |
//! | `terminal` | transport trait, typed client, validation/normalization |
//! | `bridge` | order flow, idempotency, demo guard, halt, reconciliation, history |
//! | `node3` | outbound WSS session to Node 3 (snapshots + commands) |
//! | `snapshot` | wire schema shared with Node 3 |
//! | `fake` | in-memory terminal for `MT5_SIM_TERMINAL=1` and tests |

pub mod bridge;
pub mod config;
pub mod ea_link;
pub mod fake;
pub mod node3;
pub mod proto;
pub mod snapshot;
pub mod terminal;
