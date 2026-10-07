# Fixture provenance

Based on official Homebrew 7.0.8 JSON captured on macOS on 2026-10-06:

| File | Source command |
| --- | --- |
| installed.json | `brew info --json=v2 --installed` |
| outdated.json | `brew outdated --json=v2` |
| services.json | `brew services list --json` |
| service-info.json | `brew services info atuin --json` |
| info-formula.json | `brew info --json=v2 --formula jq` |
| info-cask.json | `brew info --json=v2 --cask firefox` |

Public fixtures are intentionally minimized and sanitized. The installed list retains
one representative formula rather than the original machine inventory. Personal
user/home names are replaced with `example`, process IDs use a placeholder, and
installation timestamps and request/dependency metadata are removed.

The real outdated response was empty. Populated outdated responses and alternate
`cask.installed` shapes in the parser tests are explicitly synthetic examples.
Tests never require Homebrew or modify the local installation.
