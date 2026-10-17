<div align="center">

# athena

**A package manager for AI agent skills and plugins.**

Pin them to exact commits, install them anywhere, and never lose a local edit.

[![crates.io](https://img.shields.io/crates/v/athena-ai-manager.svg)](https://crates.io/crates/athena-ai-manager)
[![downloads](https://img.shields.io/crates/d/athena-ai-manager.svg)](https://crates.io/crates/athena-ai-manager)
[![license](https://img.shields.io/crates/l/athena-ai-manager.svg)](LICENSE)
[![SLSA 3](https://slsa.dev/images/gh-badge-level3.svg)](https://slsa.dev)
[![release](https://github.com/BaankeyBihari/athena/actions/workflows/generator-generic-ossf-slsa3-publish.yml/badge.svg)](https://github.com/BaankeyBihari/athena/actions/workflows/generator-generic-ossf-slsa3-publish.yml)

</div>

---

## Contents

- [Why athena](#why-athena)
- [Install](#install)
- [Quick start](#quick-start)
- [Scopes](#scopes)
- [Commands](#commands)
- [Configuration](#configuration)
- [Safety rules](#safety-rules)
- [Contributing](#contributing)

## Why athena

AI skills and plugins live in Git repositories, and most people copy them into their agent directories by hand. That leaves no record of where a skill came from or which version it is, and updating means overwriting whatever you changed.

athena treats them like dependencies:

- **Declared.** A small `athena.toml` lists every skill and plugin, per project or globally.
- **Reproducible.** Each entry is locked to a commit SHA. `athena sync` on a fresh checkout installs exactly the same files.
- **Safe.** athena records a hash of what it installed and refuses to overwrite anything you have modified, unless you pass `--force`.
- **Efficient.** Each repository is fetched once into a shared cache; installs are plain file copies with no nested `.git` directories.
- **Scriptable.** `--json` output and strict exit codes for agents and CI.

## Install

athena needs `git` on your `PATH`. The crate is named `athena-ai-manager`; the command it installs is `athena`.

**With Cargo**

```sh
cargo install athena-ai-manager
```

**Prebuilt binaries** for Linux (x86_64) and macOS (Apple Silicon) are attached to every [release](https://github.com/BaankeyBihari/athena/releases/latest):

```sh
curl -LO https://github.com/BaankeyBihari/athena/releases/latest/download/athena-aarch64-apple-darwin.tar.gz
tar -xzf athena-aarch64-apple-darwin.tar.gz
sudo mv athena /usr/local/bin/
```

Each release also carries signed [SLSA level 3](https://slsa.dev) provenance. To confirm a binary was built from this repository by its release workflow, check it with [slsa-verifier](https://github.com/slsa-framework/slsa-verifier):

```sh
curl -LO https://github.com/BaankeyBihari/athena/releases/latest/download/multiple.intoto.jsonl
slsa-verifier verify-artifact athena-aarch64-apple-darwin.tar.gz \
  --provenance-path multiple.intoto.jsonl \
  --source-uri github.com/BaankeyBihari/athena
```

**From source** (the project pins its Rust toolchain with [mise](https://mise.jdx.dev)):

```sh
git clone https://github.com/BaankeyBihari/athena.git && cd athena
mise install
cargo install --path .
```

## Quick start

```console
$ athena add github.com:obra/superpowers
added `obra/superpowers` at `/private/tmp/my-project/.agents/skills/superpowers`

$ athena list
+------------------+-------+--------+--------------+----------------------------------------------------+-----------------------------------------+
| Name             | Kind  | Type   | Commit       | Path                                               | Source                                  |
+=================================================================================================================================================+
| obra/superpowers | skill | remote | bb92a7774141 | /private/tmp/my-project/.agents/skills/superpowers | https://github.com/obra/superpowers.git |
+------------------+-------+--------+--------------+----------------------------------------------------+-----------------------------------------+

$ echo '# my tweak' >> .agents/skills/superpowers/README.md
$ athena sync
obra/superpowers: skipped: local modifications in `/private/tmp/my-project/.agents/skills/superpowers`
```

`athena add` wrote this to `athena.toml`. Commit it, and anyone who runs `athena sync` gets the same files:

```toml
[skills."obra/superpowers"]
type = "remote"
source = "https://github.com/obra/superpowers.git"
commit = "bb92a77741419a4ab5f06e711a283343f1ada0c3"
hash = "sha256:47ad77cb7e6d88302451b6822152aff8c1c395755d7f86d4a51724b0cc700c58"
```

More things to try:

```sh
# A plugin from one folder of a monorepo, following the `main` branch
athena add 'https://github.com/modelcontextprotocol/servers.git#main:/src/postgres' --plugin --name mcp-postgres

# A local directory
athena add ./my-plugins/linter --plugin

# Move entries to the latest commit of what they follow
athena update obra/superpowers
athena update --all

# Keep one entry where it is when running `update --all`
athena pin mcp-postgres

# Details, as JSON for agents
athena show '*postgres' --json
```

## Scopes

Every command works on one scope:

| Scope | Config file | Install location |
|---|---|---|
| Local (default) | `./athena.toml` in the current directory | `./.agents/skills/<name>` and `./.agents/plugins/<name>`, or the entry's `path` |
| Global (`-g` / `--global`) | `$XDG_CONFIG_HOME/athena/config.toml` (default `~/.config/athena/config.toml`) | The entry's `path`, which is required |

Remote repositories are cached in `$XDG_CACHE_HOME/athena/remotes/` (default `~/.cache/athena/remotes/`). The cache is disposable; delete it at any time.

## Commands

| Command | What it does |
|---|---|
| `add <source> [--name N] [--path P] [--plugin] [--pin] [-g] [--force]` | Creates a new entry, installs it, and records it. `--global` requires `--path`. |
| `sync [-g] [--force]` | Restores every entry to its locked commit. Never moves a commit. |
| `update (<name> \| -a/--all) [-g] [--force]` | Refreshes entries to the latest commit of their `ref` (default `HEAD`), then installs them. `--all` syncs pinned entries without refreshing them. |
| `pin <name> [-g]` / `unpin <name> [-g]` | Excludes an entry from `update --all` refreshes, or includes it again. |
| `remove <name> [-g] [--force]` | Deletes the entry's files and removes it from the config. |
| `list [-g] [--json]` | Summarizes entries. Reads only the config. |
| `show <glob> [-g] [--json]` | Shows matching entries with their status: `present`, `missing`, or `modified`. |

`--json` prints exactly one JSON array on stdout; warnings and errors go to stderr. The exit code is `0` on success and `1` if anything failed, was refused, or was skipped.

### Source formats

| Input | Meaning |
|---|---|
| `github.com:org/repo` | Shorthand for `https://github.com/org/repo.git` |
| `git@github.com:org/repo.git`, `https://…`, `ssh://…` | Used as given |
| `/abs/path`, `./rel`, `../rel`, `~/path`, `file://…` | Local directory, copied on each `sync` |
| `<source>#<ref>` | Branch, tag, or full commit SHA |
| `<source>#<ref>:/sub/path` or `<source>#:/sub/path` | Install only that subfolder |

## Configuration

```toml
[core]                                   # local config only; these are the defaults
default_skill_path = "./.agents/skills"
default_plugin_path = "./.agents/plugins"

[skills."obra/superpowers"]
type = "remote"
source = "https://github.com/obra/superpowers.git"
commit = "a1b2c3d4e5f6a7b8c9d0a1b2c3d4e5f6a7b8c9d0"   # locked commit
hash = "sha256:…"                                    # written by athena

[plugins."mcp-postgres"]
type = "remote"
source = "https://github.com/modelcontextprotocol/servers.git"
commit = "HEAD"              # an unresolved ref; `sync` locks it to a SHA
ref = "main"                 # what `update` follows
subpath = "src/postgres"
path = "./.custom/bin/postgres-plugin"
pinned = true                # `update --all` leaves this commit alone

[plugins."local-linter"]
type = "local"
source = "/Users/dev/my-local-plugins/linter"
path = "./.custom/bin/linter"
```

athena edits this file in place and keeps your comments and formatting.

## Safety rules

- **Modified files are never overwritten silently.** athena records a content hash of what it installed. If the files on disk no longer match, `sync`, `update`, and `remove` skip or refuse that entry unless you pass `--force`. For an entry without a recorded hash, files that git tracks unmodified count as safe to replace; untracked and git-ignored files do not.
- **One owner per path.** Two entries installing different versions to the same path, or one entry nested inside another, block `add`, `sync`, and `update` until you fix the config. `remove` always works, so you can resolve it.
- **Local never writes over global.** If a local entry overlaps a path managed by the global config, local `add`, `sync`, and `update` stop and tell you to remove one side (`athena remove <name>` or `athena remove -g <name>`).
- **Gobbling.** Adding an entry whose path contains existing entries asks `Gobble … into …? [y/N]` and replaces them on `y`. It is refused without a terminal, or when any of those entries (or the parent folder) holds your own changes.

athena never runs the files it installs, but the agents that load them will. Only add sources you trust, and review an `athena.toml` from an untrusted repository before running `athena sync`. See [SECURITY.md](SECURITY.md) for the full security model and how to report a vulnerability.

## Contributing

Contributions are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) covers setup, the design-to-code workflow, and what a pull request needs.

```sh
mise run test    # cargo test
mise run lint    # cargo clippy --all-targets -- -D warnings
mise run fmt     # cargo fmt
```

The design lives in [`docs/`](docs/high-level-design.md): a high-level design, one design doc per component, and EARS specs that tests and code cite with `@spec` tags.

## License

[MIT](LICENSE)
