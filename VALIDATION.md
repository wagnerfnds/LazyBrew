# Validation — v0.2.0

Local validation on macOS / Apple Silicon with stable Rust:

- `cargo fmt --check`: passed.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `cargo test --locked`: 19 tests passed.
- `cargo build --release --locked`: passed.
- `python3 scripts/smoke.py`: passed. The PTY test exercises automatic details,
  mouse workspace selection, action buttons, cancelling a confirmation, empty
  search, help, exit and restoration of terminal mode and mouse capture.
- No real packages or services were modified by the test suite.
- The README preview is generated from fictional data by `examples/preview.rs`.

Privacy review before the initial public commit:

- Reviewed the exact files staged for publication; excluded build output and logs.
- Minimized captured fixtures, removed installation timestamps and machine inventory,
  and replaced user/home names and process IDs with placeholders.
- Checked for personal home paths and credentials in publishable text.
- Gitleaks 8.30.1: no leaks found in the staged source/documentation snapshot.
- Commit identity uses the public GitHub handle and GitHub's noreply address.

Performance design: blocking input reads on a dedicated thread wake an async event
loop directly; there is no periodic keyboard poll or idle redraw. Selection previews
are immediate, remote details are debounced for 120 ms, obsolete requests are cancelled,
and the detail cache is limited to 64 entries. These are implementation properties,
not claims of a statistically measured latency or memory budget.

CI repeats checks, tests, the release build and the PTY smoke test on macOS and Linux.
