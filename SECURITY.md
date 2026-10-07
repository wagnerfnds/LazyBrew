# Security

Please report potential vulnerabilities privately through
[GitHub private vulnerability reporting](https://github.com/wagnerfnds/LazyBrew/security/advisories/new).
Do not put credentials or exploit details in a public issue.

Include the affected version, reproduction steps using dummy data and expected impact.
The project is early-stage; there is no guaranteed response SLA.

LazyBrew invokes Homebrew with individual arguments and validated package identifiers.
It does not request sudo, collect telemetry or send local logs to the maintainers.
Homebrew commands can install software and manage user services, so review each
confirmation and the formula's own caveats. Local logs may contain package names,
paths or command errors; inspect them before attaching to an issue.
