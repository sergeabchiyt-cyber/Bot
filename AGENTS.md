# Agent assignment index

The master plan is [`PLAN.md`](PLAN.md). It contains the audit of all four
branches, the formalised strategy, the target architecture, the versioned
contracts, the gated migration order and the full Node 4 brief (section 5).

Because the four nodes live on **disjoint-history branches** (D2 = B), the work
is split into one Arena session per branch. Paste the matching brief into each
session:

| Session base branch | Brief | Language | Mission |
|---|---|---|---|
| `Node1` | [`AGENT-NODE1.md`](AGENT-NODE1.md) | Rust | market data only + calendar (C1 v2 producer) |
| `Node2` | [`AGENT-NODE2.md`](AGENT-NODE2.md) | vanilla HTML/CSS/JS + lightweight-charts | pure frontend; wire Node 3's new endpoint; better design |
| `Node3` | [`AGENT-NODE3.md`](AGENT-NODE3.md) | Rust | FRVP/VP calculations, drawings, break/retest, ATR SL/TP/RR, intents |
| `arena/c3e4eae3-bot` ← `Node-4` | `PLAN.md` §5 Agent 4 (**being executed here**) | Rust | demo execution + risk management |

Shared, byte-identical on all four branches: [`contracts/`](contracts/README.md)
(C1 v2, C2 v1, C3 v1.1, C4 v1 + historical Node 1 audit docs). Run
`python3 ci/contract_sync.py --write` after any contract edit and copy the
whole folder (including `MANIFEST.sha256`) to every branch in the same wave.

Decisions are settled (D1–D9, see `PLAN.md` §4.1). Only the non-blocking D5b
default remains: `PS_MODE=week_open` (say "roll PS daily" to flip it).

Node 4 work done in this session and its verification status:
[`PLAN.md` §10](PLAN.md).
