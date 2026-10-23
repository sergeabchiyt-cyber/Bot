# CI proof — run 40006920764
commit: a4e2e052b4a740b3903f5df3710787e804cf4c5f
generated: 2026-10-23T18:53:34Z

## Unit tests
(missing)

## Live calendar feed test
(missing)

## Runtime smoke test
(missing)

## Clippy
    Checking xauusd-engine v0.4.0 (/home/runner/work/Bot/Bot)
error: this `let...else` may be rewritten with the `?` operator
   --> src/demo_engine.rs:358:9
    |
358 | /         let Some(position) = self.position.as_ref() else {
359 | |             return None;
360 | |         };
    | |__________^ help: replace it with: `let position = self.position.as_ref()?;`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#question_mark
    = note: `-D clippy::question-mark` implied by `-D warnings`
    = help: to override `-D warnings` add `#[allow(clippy::question_mark)]`

error: could not compile `xauusd-engine` (bin "xauusd-engine") due to 1 previous error
warning: build failed, waiting for other jobs to finish...
error: could not compile `xauusd-engine` (bin "xauusd-engine" test) due to 1 previous error

