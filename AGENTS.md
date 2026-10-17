# Project Instructions

## LID

- Mode: Full
- Version: 1.3.0

This project uses linked-intent development: HLD (`docs/high-level-design.md`) → LLDs (`docs/intent/<segment>/`) → EARS specs → tests → code. Tests and code carry `@spec <ID>` annotations. Stop for review at each phase boundary.

## Tooling

`mise` maintains the project. `mise.toml` pins the Rust toolchain. Run work through its tasks: `mise run build`, `mise run test`, `mise run lint`, `mise run fmt`.
