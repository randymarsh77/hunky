# Coverage Plan (toward 100%)

## Current baseline (from `cargo llvm-cov --summary-only`)

- Total line coverage: **17.44%**
- Highest uncovered modules: `app.rs`, `ui.rs`, `watcher.rs`, `syntax.rs`, `main.rs`

## Tooling integration

- Local development:
  - Install once: `cargo install cargo-llvm-cov --locked`
  - Run summary: `cargo cov`
  - Generate LCOV: `cargo cov-lcov`
  - Generate HTML report: `cargo cov-html`
- CI:
  - Run `cargo llvm-cov --locked --lcov --output-path lcov.info --summary-only`
  - Upload `lcov.info` as a workflow artifact for inspection

## Execution plan

- [x] Add coverage tooling and commands for local development
- [x] Add CI coverage job that always produces a coverage report artifact
- [x] Add focused unit tests for pure logic modules (`diff`, `syntax`, `watcher`)
- [x] Add TUI integration test via ASCII buffer capture (`ratatui::TestBackend`)
- [x] Expand `app.rs` behavior tests (mode transitions, navigation, staging toggles)
- [x] Expand `ui.rs` rendering assertions across modes/layout widths/help states
- [x] Add integration tests for watcher event loop + snapshot dispatch behavior
- [x] Add `main.rs` smoke/integration tests for CLI argument behavior
- [ ] Introduce phased CI enforcement (`--fail-under-lines`) and raise threshold to 100%

## Notes on 100% goal

Reaching strict 100% will require comprehensive behavioral tests for async/event-loop-driven code paths in `app.rs` and richer TUI-state permutations in `ui.rs`. The current changes establish the tooling and test harness needed to iteratively raise coverage in small, safe increments.
