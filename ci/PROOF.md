# CI proof — run 40335672305
commit: eb2266501e4756e3af855c96b6f5c280dedaf9a4
generated: 2026-10-26T14:01:10Z

## Unit tests
(missing)

## Live calendar feed test
(missing)

## Runtime smoke test (C1 v2 contract matrix)
(missing)

## Clippy
    Checking xauusd-engine v0.4.0 (/home/runner/work/Bot/Bot)
error: methods `len` and `is_empty` are never used
  --> src/tick_volume.rs:62:12
   |
23 | impl TickVolumeStore {
   | -------------------- methods in this implementation
...
62 |     pub fn len(&self) -> usize {
   |            ^^^
...
66 |     pub fn is_empty(&self) -> bool {
   |            ^^^^^^^^
   |
   = note: `-D dead-code` implied by `-D warnings`
   = help: to override `-D warnings` add `#[expect(dead_code)]` or `#[allow(dead_code)]`

error: this `if` statement can be collapsed
  --> src/session.rs:76:9
   |
76 | /         if let Some(candidate) = anchor_ms(date, New_York) {
77 | |             if candidate <= now_ms {
78 | |                 open_ms = Some(candidate);
79 | |                 break;
80 | |             }
81 | |         }
   | |_________^
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#collapsible_if
   = note: `-D clippy::collapsible-if` implied by `-D warnings`
   = help: to override `-D warnings` add `#[allow(clippy::collapsible_if)]`
help: collapse nested if block
   |
76 ~         if let Some(candidate) = anchor_ms(date, New_York)
77 ~             && candidate <= now_ms {
78 |                 open_ms = Some(candidate);
79 |                 break;
80 ~             }
   |

error: could not compile `xauusd-engine` (bin "xauusd-engine") due to 2 previous errors
warning: build failed, waiting for other jobs to finish...
error: this operation has no effect
   --> src/sifting_ws.rs:208:21
    |
208 |     const T0: i64 = 1_700_000_100_000 - 1_700_000_100_000 % (15 * 60_000);
    |                     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ help: consider reducing it to: `1_700_000_100_000`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#identity_op
    = note: `-D clippy::identity-op` implied by `-D warnings`
    = help: to override `-D warnings` add `#[allow(clippy::identity_op)]`

error: this operation has no effect
   --> src/types.rs:452:19
    |
452 |             time: 1_700_000_100_000 - 1_700_000_100_000 % 900_000,
    |                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ help: consider reducing it to: `1_700_000_100_000`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#identity_op

error: could not compile `xauusd-engine` (bin "xauusd-engine" test) due to 3 previous errors

