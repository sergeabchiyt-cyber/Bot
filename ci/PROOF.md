# CI proof — run 40390167732
commit: e1f4ed96d66fd51e6cf6cb3a12fc0b8c8db6e76c
generated: 2026-10-26T19:49:54Z

## Unit tests
(missing)

## Live calendar feed test
(missing)

## Runtime smoke test (C1 v2 contract matrix)
(missing)

## Clippy
    Checking xauusd-engine v0.4.0 (/home/runner/work/Bot/Bot)
error: unused variable: `err`
   --> src/sifting_rest.rs:294:17
    |
294 |             Err(err) if attempt < MAX_RETRIES => {
    |                 ^^^ help: if this is intentional, prefix it with an underscore: `_err`
    |
    = note: `-D unused-variables` implied by `-D warnings`
    = help: to override `-D warnings` add `#[allow(unused_variables)]`

error: associated function `new` is never used
   --> src/calendar.rs:316:12
    |
315 | impl CalendarSource {
    | ------------------- associated function in this implementation
316 |     pub fn new(primary: Option<String>) -> Self {
    |            ^^^
    |
    = note: `-D dead-code` implied by `-D warnings`
    = help: to override `-D warnings` add `#[expect(dead_code)]` or `#[allow(dead_code)]`

error: could not compile `xauusd-engine` (bin "xauusd-engine") due to 2 previous errors
warning: build failed, waiting for other jobs to finish...
error: this assertion has a constant value
   --> src/ws_server.rs:967:9
    |
967 | /         assert!(
968 | |             CAPACITY_M1 >= 43_200,
969 | |             "full 1m retention must remain intact"
970 | |         );
    | |_________^
    |
    = help: consider moving this into a const block: `const { assert!(..) }`
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#assertions_on_constants
    = note: `-D clippy::assertions-on-constants` implied by `-D warnings`
    = help: to override `-D warnings` add `#[allow(clippy::assertions_on_constants)]`

error: this assertion has a constant value
   --> src/ws_server.rs:971:9
    |
971 | /         assert!(
972 | |             BROADCAST_CAPACITY >= 4_096,
973 | |             "broadcast buffer must stay at least 4096"
974 | |         );
    | |_________^
    |
    = help: consider moving this into a const block: `const { assert!(..) }`
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#assertions_on_constants

error: could not compile `xauusd-engine` (bin "xauusd-engine" test) due to 3 previous errors

