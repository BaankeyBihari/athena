<div align="center">

# athena

**A package manager for AI agent skills and plugins.**

Pin them to exact commits, install them anywhere, and never lose a local edit.

[![CI](https://github.com/BaankeyBihari/athena/actions/workflows/ci.yml/badge.svg)](https://github.com/BaankeyBihari/athena/actions/workflows/ci.yml)
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

**With Homebrew** (macOS on Apple Silicon, Linux x86_64)

```sh
brew install BaankeyBihari/tap/athena
```

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
$ athena add 'github.com:jszmajda/lid#main:/plugins/*/skills/{linked-intent-dev,map-codebase}'
added `map-codebase` at `/private/tmp/my-project/.agents/skills/map-codebase`
added `linked-intent-dev` at `/private/tmp/my-project/.agents/skills/linked-intent-dev`

$ athena list
+-------------------+-------+--------+--------------+----------------------------------------------------------+-------------------------------------+
| Name              | Kind  | Type   | Commit       | Path                                                     | Source                              |
+====================================================================================================================================================+
| map-codebase      | skill | remote | 9679360f382b | /private/tmp/my-project/.agents/skills/map-codebase      | https://github.com/jszmajda/lid.git |
|-------------------+-------+--------+--------------+----------------------------------------------------------+-------------------------------------|
| linked-intent-dev | skill | remote | 9679360f382b | /private/tmp/my-project/.agents/skills/linked-intent-dev | https://github.com/jszmajda/lid.git |
+-------------------+-------+--------+--------------+----------------------------------------------------------+-------------------------------------+

$ echo '# my tweak' >> .agents/skills/map-codebase/SKILL.md
$ athena sync
map-codebase: skipped: local modifications in `/private/tmp/my-project/.agents/skills/map-codebase`
linked-intent-dev: up to date
```

One `add` with a pattern installed two skills from different folders of the repository. `athena add` wrote them to `athena.toml`; commit it, and anyone who runs `athena sync` gets the same files:

```toml
[skills.map-codebase]
type = "remote"
source = "https://github.com/jszmajda/lid.git"
commit = "9679360f382b5c265da073c4e06badd3afcca7d0"
ref = "main"
subpath = "plugins/arrow-maintenance/skills/map-codebase"
hash = "sha256:0982b15ec0d9a2fd6210f78ec4e72670f9cae0ca172169aabc630cb96fc678ae"

[skills.linked-intent-dev]
type = "remote"
source = "https://github.com/jszmajda/lid.git"
commit = "9679360f382b5c265da073c4e06badd3afcca7d0"
ref = "main"
subpath = "plugins/linked-intent-dev/skills/linked-intent-dev"
hash = "sha256:af6a79de370e429d7884ed22a20d376425a32db1fa64ce810040560d10fcb14c"
```

Mistakes fail before anything is installed, and say how to fix them:

```console
$ athena add github.com:obra/superpowers
ERROR no `SKILL.md` at the root of `obra/superpowers`; it looks like a collection of skills, so add them with `github.com:obra/superpowers#:/skills/*`; pass `--no-skill-check` to install as they are

$ athena add 'github.com:jszmajda/lid#main:/plugins/*/skills/{linked-intent-dev,map-codeabse}'
ERROR no folder matches `plugins/*/skills/map-codeabse`
```

More things to try:

```sh
# Every skill in a collection; re-run it later to pick up new ones
athena add 'github.com:obra/superpowers#:/skills/*'

# A plugin from one folder of a monorepo, following the `main` branch
athena add 'https://github.com/modelcontextprotocol/servers.git#main:/src/postgres' --plugin --name mcp-postgres

# A local directory
athena add ./my-plugins/linter --plugin

# One skill for every project, into Claude Code's skills folder (~/.claude/skills/memoir)
athena add -g 'github.com:BaankeyBihari/memoir-skill#:/memoir' --path '~/.claude/skills'

# Move entries to the latest commit of what they follow
athena update map-codebase
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
| Global (`-g` / `--global`) | `$XDG_CONFIG_HOME/athena/config.toml` (default `~/.config/athena/config.toml`) | `<path>/<name>`, under the folder given to `add --path`, which is required |

Remote repositories are cached in `$XDG_CACHE_HOME/athena/remotes/` (default `~/.cache/athena/remotes/`). The cache is disposable; delete it at any time.

## Commands

| Command | What it does |
|---|---|
| `add <source> [--name N] [--path P] [--plugin] [--pin] [--no-skill-check] [-g] [--force]` | Creates new entries, installs them, and records them, all or nothing. A pattern source creates one entry per matched folder. Skills must have a `SKILL.md` at their root unless `--no-skill-check` is given. `--path P` is the folder to install into: each entry goes in `P/<name>`, using the last part of the name (set it with `--name`). `--global` requires `--path`. |
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
| `<source>#<ref>:/sub/path`, `<source>#:/sub/path`, or `<source>#/sub/path` | Install only that subfolder (the last two use the default branch) |
| `<source>#<ref>:/skills/*` | One entry per matching folder. `*` and `?` match within one path component and skip hidden folders; `{a,b}` picks alternatives. |

## Configuration

```toml
[core]                                   # local config only; these are the defaults
default_skill_path = "./.agents/skills"
default_plugin_path = "./.agents/plugins"

[skills.brainstorming]
type = "remote"
source = "https://github.com/obra/superpowers.git"
subpath = "skills/brainstorming"
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
- **Skills are checked.** `add` refuses a skill folder without a `SKILL.md` at its root, so a wrong repository or subpath fails instead of installing files no agent loads. For a collection, it suggests the exact pattern to use.
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
