# athena

A Rust CLI that manages AI agent **skills** and **plugins** like pinned dependencies. Declare what you want in a TOML file; athena fetches each Git source once into a shared cache, locks it to an exact commit, and copies its files into place. It never overwrites files you have edited unless you ask it to.

## Install

Requires `git` on `PATH`. The project uses [mise](https://mise.jdx.dev) to pin the Rust toolchain.

```sh
mise install          # installs the pinned Rust toolchain
mise run build        # cargo build
cargo install --path .
```

## Quick start

```sh
# Add a skill from GitHub (locked to the current HEAD commit)
athena add github.com:obra/superpowers

# Add a plugin from a subfolder of a monorepo, following the `main` branch
athena add 'https://github.com/modelcontextprotocol/servers.git#main:/src/postgres' --plugin --name mcp-postgres

# Add a local directory
athena add ./my-plugins/linter --plugin

# See what is installed
athena list
athena show '*linter' --json

# On another machine or a fresh checkout: install everything at its locked commit
athena sync

# Move entries to the latest commit of what they follow
athena update obra/superpowers
athena update --all
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

## Development

```sh
mise run test    # cargo test
mise run lint    # cargo clippy --all-targets -- -D warnings
mise run fmt     # cargo fmt
```

Design and requirements live in [`docs/`](docs/high-level-design.md). The project follows linked-intent development: each behavior has an EARS spec under `docs/intent/<component>/`, and tests and code carry `@spec` tags pointing at those IDs.
