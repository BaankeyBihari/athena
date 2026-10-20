# Changelog

Notes for earlier versions are on the [releases page](https://github.com/BaankeyBihari/athena/releases).

## [0.4.0](https://github.com/BaankeyBihari/athena/compare/v0.3.1...v0.4.0) - 2026-10-20

### Added

- *(add)* install rules and agents as single files with --kind

### Other

- ignore memoir session files

## [0.3.1](https://github.com/BaankeyBihari/athena/compare/v0.3.0...v0.3.1) - 2026-10-18

### Other

- automate releases with release-plz ([#8](https://github.com/BaankeyBihari/athena/pull/8))

## 0.3.0

### Breaking

- **`add --path` is now the folder to install into.** `athena add <source> --path ~/.claude/skills` installs to `~/.claude/skills/<name>`, using the last part of the entry's name, for a single folder and for a pattern alike. Before, a single-folder source was installed *as* `~/.claude/skills`, which athena refused when that folder already held other skills.

  If you passed the full destination before, pass its parent now:

  ```sh
  # 0.2.0
  athena add -g 'github.com:BaankeyBihari/memoir-skill#:/memoir' --path ~/.claude/skills/memoir
  # 0.3.0
  athena add -g 'github.com:BaankeyBihari/memoir-skill#:/memoir' --path '~/.claude/skills'
  ```

  Use `--name` to pick a different folder name. Entries already in `athena.toml` or the global config keep their recorded paths; only new `add` commands are affected.

### Added

- `add` fails before any network access when the entry's name cannot name a folder (its last part is empty, `.`, or `..`, for example `--name ..`), and when `--path` is empty.
- `athena add --help` describes `--name` and `--path`.

### Install

`brew install BaankeyBihari/tap/athena`, `cargo install athena-ai-manager`, or download a binary from the release and verify it with `slsa-verifier` (see the README).
