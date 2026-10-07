# Validation — roadmap completion

Local validation on macOS / Apple Silicon with stable Rust:

- `cargo fmt --check`: passed.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `cargo test --locked`: 32 tests passed.
- `cargo build --release --locked`: passed.
- `python3 scripts/smoke.py`: passed. The PTY test exercises automatic details,
  mouse workspace selection, action buttons, cancelling a confirmation, empty
  search, help, running cancellation, persisted history/catalogue, automatic
  background refresh, stack confirmation, offline restart, exit and restoration
  of terminal mode and mouse capture.
- `python3 scripts/theme_smoke.py`: tests automatic light/dark detection through
  OSC 10/11 replies, CLI/config precedence, environment/default fallback and terminal
  restoration in a controlling PTY. Palette tests require text/status contrast
  of at least 4.5:1 on background, panel and selected surfaces.
- No real packages or services were modified by the test suite.
- The README preview is generated from fictional data by `examples/preview.rs`.

Roadmap coverage:

- Cancellation tests terminate a fake command and its descendant; cancelled plans
  never run later steps and produce one terminal result.
- The PTY test observes the configured 30-second background timer, then restarts
  with a failing backend and reads the saved catalogue and history.
- Storage tests cover state round-trips, expired details, corrupt JSON,
  backend isolation, identifier validation and interrupted previous sessions.
- Workflow tests cover PHP, Node and PostgreSQL stop/unlink/link/start ordering,
  missing stack installs, service actions, validation and stopping on failure.
- Application tests cover refresh deferral, persisted failure results, cache reuse
  and rendering/reviewing long confirmation plans.
- Version switches are verified with argument plans and fake processes; no real
  Homebrew packages, services or database directories were changed.

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
