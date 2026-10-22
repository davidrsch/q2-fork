# Investigation notes: bd-cig41dcs

## Code anchors (as of main @ 8ddbf8b17)

- `crates/quarto-core/src/engine/ts_engine.rs:1357`: `watchdog` helper (an mpsc `recv_timeout`)
- `crates/quarto-core/src/engine/ts_engine.rs:1462`: `test_race_free_instance_exclusive`
- `crates/quarto-core/src/engine/ts_engine.rs:494`: `ensure_launched` holds the `instance` mutex across `launch_engine`
- `crates/quarto-core/src/engine/ts_engine.rs:337-347`: documented double-send of LoadEngine under concurrency
- `crates/quarto-core/src/engine/ts_process.rs:756`: `DISCOVERY_WINDOW = 10s` (LaunchEngine window)
- `crates/quarto-core/src/engine/ts_process.rs:1176`: id allocated (`fetch_add`) *before* `write.send` pushes to `sent`
- `crates/quarto-core/src/engine/ts_process.rs:1475`: reader drops unknown ids silently
- `crates/quarto-core/src/engine/ts_process.rs:1906`: `sent_messages()` strips ids

## Timeline arithmetic for the observed 15.010 s

The watcher misses the delivery. T_a holds the instance mutex and waits
10 s for LaunchEngine, then gets `Err(Timeout)`. T_b takes the mutex and
sends a second LaunchEngine, which waits another 10 s. The total is ≥ 20 s,
more than the 15 s watchdog, so the test panics at 15.0 s.

## Local stress run

See `stress.txt` (if present) for the result of looping the test locally.
