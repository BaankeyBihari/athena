# Contributing to athena

Thanks for helping. This guide covers setup, how changes are made, and what a pull request needs.

## Setup

You need [mise](https://mise.jdx.dev) and `git`.

```sh
git clone git@github.com:BaankeyBihari/athena.git
cd athena
mise install        # installs the pinned Rust toolchain (see mise.toml)
mise run test
```

Run all project work through the mise tasks:

| Task | Runs |
|---|---|
| `mise run build` | `cargo build` |
| `mise run test` | `cargo test` |
| `mise run lint` | `cargo clippy --all-targets -- -D warnings` |
| `mise run fmt` | `cargo fmt` |

## How changes are made

athena uses linked-intent development (LID). Every behavior traces from design to code:

1. **High-level design**: `docs/high-level-design.md`. Architecture, goals, tenets.
2. **Component designs**: `docs/intent/<component>/<component>-design.md` for `source`, `config`, `cache`, `install`, and `commands`.
3. **EARS specs**: `docs/intent/<component>/<component>-specs.md`. One line per behavior with a stable ID such as `CFG-STATE-003`.
4. **Tests**: under `tests/`, tagged `// @spec <ID>`.
5. **Code**: under `src/`, tagged `// @spec <ID>` at the entry point of the behavior.

When you change behavior, update the levels above it first: adjust the design doc, add or edit the spec, write a failing test, then change the code. Bug fixes follow the same path: find where the behavior diverged from the spec, and fix the spec, the test, or the code accordingly.

Spec rules:

- Never reuse or renumber an ID. Edit a spec's text in place; delete specs that are no longer wanted.
- Each spec must read correctly on its own, naming its scope (local or global, which command) instead of relying on its section heading.
- Mark a spec `[x]` once code and a test for it exist, `[ ]` while it is still to do.

Before opening a pull request, check the traceability:

```sh
# every spec ID cited in tests or code must exist
grep -ohE '\*\*[A-Z]+(-[A-Z]+)?-[0-9]{3}\*\*' docs/intent/*/*-specs.md | tr -d '*' | sort -u > /tmp/specs
grep -h '@spec' src/*.rs tests/*.rs | grep -oE '[A-Z]+(-[A-Z]+)?-[0-9]{3}' | sort -u | comm -13 /tmp/specs -
```

The command prints nothing when every citation is valid.

## Code style

- Keep the diff small and in the style of the surrounding code. No speculative features or abstractions.
- All git access goes through the `git` executable in `src/cache.rs`; do not add the `git2` crate.
- Safety rules (dirty destinations, collisions, gobbling) belong in the pure planner functions in `src/install.rs` and `src/config.rs`, so they can be tested without touching disk.
- Prefer the standard library and existing dependencies over new crates.
- A deliberate shortcut with a known limit gets a `// ponytail:` comment naming the limit and the upgrade path.

## Tests

- `tests/source.rs`, `tests/config.rs`, `tests/cache.rs`, and `tests/install.rs` test the library modules directly.
- `tests/commands.rs` runs the real binary in a sandbox (`tests/common/mod.rs`) with its own `HOME`, XDG directories, and git config. Remote URLs `https://example.test/<org>/<repo>.git` are rewritten to local repositories, so no test touches the network or your real config.
- The gobble-prompt tests drive a pseudo-terminal with macOS `script` syntax; they need adjusting to run on Linux.

## Pull requests

1. Branch from `main`.
2. Use [Conventional Commits](https://www.conventionalcommits.org) for messages, for example `feat: add athena cache clean` or `fix: keep ref when locking on sync`.
3. Make sure `mise run fmt`, `mise run lint`, and `mise run test` all pass.
4. In the description, list the spec IDs you added or changed and explain any design-doc change.

Record design decisions that choose between alternatives in the relevant design doc's Decisions & Alternatives table.

## Reporting bugs and security issues

Open a GitHub issue for bugs, including the command, the relevant part of `athena.toml`, and the output. Report security vulnerabilities privately as described in [SECURITY.md](SECURITY.md), not in a public issue.
