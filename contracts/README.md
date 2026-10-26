# Shared service contracts (Phase 0)

The XAUUSD system is four independently deployable services on four
disjoint-history branches (`Node1`, `Node2`, `Node3`, `Node-4`). This folder is
the **only coupling** between them. It must be committed **byte-identically to
all four branches**, and `ci/contract_sync.py` fails CI when a copy drifts.

## Version rules

* Every payload carries an explicit `contract_version` / `schema_version`.
* **Additive optional fields** may keep a version. Any changed meaning, required
  field, status semantics, authentication flow or idempotency rule requires a
  new version.
* Unknown versions **fail closed** — never guess, never best-effort parse.
* Producers must tolerate consumers that ignore unknown fields; consumers must
  not invent defaults for missing required fields.

## The four contracts

| ID | Producer → consumer | Files | Status |
|---|---|---|---|
| **C1 v2** | Node 1 → Node 3 / Node 2 / operators | `NODE1_FEED_CONTRACT_v2.md`, `c1-market-feed.schema.json` | signed for this refactor; Node 1 implements |
| **C2 v1** | Node 3 → Node 2 / operators | `c2-node3-public.schema.json` | Node 3 implements; Node 2 consumes |
| **C3 v1.1** | Node 3 → Node 4 (private `/execution`) | `c3-intent.schema.json` | v1 unchanged, additive fields documented |
| **C4 v1** | Node 4 → Node 2 / operators | `c4-diagnostics.schema.json` | `market_feed`/`strategy` removed, `risk` added |

Historical material kept for the porting work:
`NODE1_FEED_CONTRACT_v1_DRAFT.md` (the unsigned draft the Node 4 local-strategy
mode targeted) and `NODE1_BOUNDARY_AUDIT.md` (read-only inventory of the old
Node 1 tree). **Neither is normative.**

## Ownership map

```
SiftingIO / ForexFactory
        │
        ▼
   NODE 1  market data + calendar  ──C1 v2──►  NODE 3  VP + break/retest
                                                   │  C2 public (levels/drawings/
                                                   │  scanning/signals)
                                                   │  C3 private (/execution)
                                       ┌───────────┴───────────┐
                                       ▼                       ▼
                                  NODE 2 dashboard       NODE 4 execution + risk
                                  (frontend only)        ──C4 public──► NODE 2
```

* Node 1: feed adapters, candles, tick volume, calendar, feed health. **No VP,
  no strategy, no execution.**
* Node 2: static HTML/CSS/JS + lightweight-charts. **No server, no proxy, no
  secrets, no writes.**
* Node 3: FRVP PW/PS/CW, drawing payload, break/retest scanner, ATR SL/TP/RR
  projection, intents. **No broker adapter, no sizing, no account state.**
* Node 4: intent validation, risk policy, sizing, durable ledger, demo/MT5
  execution, kill switch, reports. **No market data, no VP, no strategy.**

## Decisions baked in

* D3 — week/session anchor is DST-aware `America/New_York` 18:00.
* D4 — CW first published after Monday's 18:00 close and refreshed daily.
* D5 — PS = last closed 18:00→18:00 session (Thu 18:00 → Fri 18:00 at week
  open), PoC only. D5b: frozen for the week by default (`PS_MODE=rolling`
  switches it).
* D6 — the 10,500 volume-spike filter is off; break + retest confirm.
* D9 — 1 pip = $0.01, so SL 200–300 pips = $2–3, TP 600–800 pips = $6–8,
  RR 2–3.

## Regenerating the manifest

```bash
python3 ci/contract_sync.py --write   # after any contract edit
python3 ci/contract_sync.py           # CI verification
```
