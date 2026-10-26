# Handoff packs — source that left Node 1 at the C1 v2 cutover

Node 1 is now a **read-only market-data + calendar feed**. Everything that used
to compute something *from* the market data lived in `src/` and was moved out
here at the cutover:

| Pack | Files | Where it belongs now |
|---|---|---|
| `node3-vp/` | `volume_profile.rs`, `histogram.rs`, `session.rs`, `swing.rs`, `timeframe.rs`, `weekly.rs` | **Node 3** (`node3-strategy/src/vp/`) — Node 3 owns all volume-profile maths |
| `node3-ai/` | `econ_monitor.rs`, `ai_cache.rs` | **Node 3**, optional — nothing on the Node 3 branch consumes `audio_chunk`/`learn` today, so these were triaged out of Node 1 rather than kept alive |

## These files are not Node 1

* They are **not** declared in `src/main.rs` and are not part of the
  `xauusd-engine` crate. `cargo build` ignores this directory.
* `ci/check_node1_boundary.py` scans the *service* surface (`src/`, `ci/`,
  `Cargo.toml`, `Dockerfile`, `.github/`, `README.md`) and deliberately skips
  `handoff/`: the whole point is that these modules are Node 3 source that
  happens to be stored here until Agent 3 has ported them.
* Nothing in Node 1 may import them. If one reappears under `src/`, the
  boundary guard fails.

## Porting rules for Agent 3

1. Copy the file into `node3-strategy/src/vp/` **byte-for-byte** and verify with
   the manifest below before editing. The recorded FRVP fixtures
   (`ci/live/vp-{PW,PS,CW}.json`) are the parity baseline: recomputing PW/PS/CW
   from Node 1 candles must land within one histogram row.
2. `types::VpCandle` was replaced on Node 1 by the C1 v2 `Candle`; the port
   needs a 3-field adapter (`time`, OHLC, `volume`) plus the new `interval`
   label. `ProfileMeta` moved with `volume_profile.rs` (it is part of the
   profile audit, not of the feed).
3. `volume_profile/session.rs` contains the old 18:00 New York window maths.
   Node 3 should keep its own DST-aware clock (decision D3 in the brief); do
   not re-derive it from Node 1's `/status`.
4. `econ_monitor.rs` needs `McpClient` (browser MCP) and `FeedStatus`; both are
   Node 1 modules that stayed behind. If Node 3 adopts the pack, it needs its
   own minimal copies — or the audio relay can be dropped entirely, which is
   what Node 1 did.

## Manifest

`handoff/MANIFEST.sha256` pins every file above. Verify before porting:

```bash
cd handoff && sha256sum -c MANIFEST.sha256
```

`ci/check_node1_boundary.py` also verifies the manifest, so a hand-edited pack
cannot drift from what Node 3 is asked to port.
