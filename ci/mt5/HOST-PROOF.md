# mt5-host build + test proof

Recorded by `.github/workflows/build.yml` (job `host`) whenever it passes,
so the dependency closure and the test results are readable from a checkout
instead of only from the Actions tab.

- toolchain: `rustc 1.94.0 (4a4ef493e 2026-03-02)`
- cargo: `cargo 1.94.0 (85eff7c80 2026-01-15)`

```
$ cargo tree --depth 1   # must resolve serde_json from crates.io
mt5-host v0.1.0 (/home/runner/work/Bot/Bot/mt5-host)
└── serde_json v1.0.151
```

```
$ cargo test --all-targets -- --nocapture
running 38 tests
test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished
```
