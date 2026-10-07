# Contributing

Bug reports, UX feedback, documentation and pull requests are welcome.

## Getting started

1. Fork the repository and create a branch.
2. Use a current stable Rust toolchain and Python 3.
3. Run the checks listed in the README before opening a pull request.
4. Describe the problem, the user-visible behavior and how you verified the change.

For UI changes, include a screenshot using fictional or sanitized data. Please keep
keyboard navigation, mouse hit testing and small-terminal behavior in sync.

## Design constraints

- Treat the Homebrew CLI as the backend. Do not import Ruby internals.
- Prefer structured JSON to parsing human-readable output.
- Never concatenate user input into shell commands.
- Keep process details out of the renderer and preserve the backend abstraction.
- Keep input responsive while commands run and keep queues/caches bounded.
- Add focused regression tests for changes in behavior.
- Test mutating operations with a fake backend, not a contributor's real packages.

## Privacy

Do not commit credentials, environment files, command logs, personal home paths or
machine inventory. Scrub fixtures and screenshots before sharing them. Include the
Homebrew version and fixture provenance without private host/user details.

Use private vulnerability reporting for security-sensitive issues; see SECURITY.md.
