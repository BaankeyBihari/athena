---
parent: high-level-design
prefix: CMD
---

# Commands

## Context and Design Philosophy

The command layer is the user-facing surface: argument rules, orchestration of `source`, `config`, `cache`, and `install`, and output. It owns no safety rules of its own. Output serves two readers: people get tables and colored warnings, and agents get strict JSON on stdout with every diagnostic on stderr.

## Scope Selection

`--global` (short `-g`) selects the global config on every command; without it, commands use `./athena.toml`. Every command except `add` fails with "no athena.toml in `<cwd>`" when the selected config is missing.

The three install commands differ in what they act on:

- **`add`** creates a new entry and installs it.
- **`sync`** restores every existing entry to its locked commit, without moving any commit.
- **`update`** refreshes entries to the latest commit of their `ref` and then installs them as `sync` would.

Each one runs the config state check for its scope first and exits before any fetch or filesystem change if a state error exists (see `config` § Config State Check). With `-g`, only the global config is checked. Without it, the local config is checked together with its conflicts against global entries. `remove`, `pin`, and `unpin` are never blocked, so state errors can always be resolved.

## Commands

### `add <source> [--name <n>] [--path <p>] [--plugin] [--pin] [--no-skill-check] [--global/-g] [--force]`

`add` turns one source into one or more **new entries** and installs them as a single all-or-nothing batch.

1. `--global` without `--path` is an error, raised before any network access.
2. Parse `<source>` (`source`). The entries are skills unless `--plugin` is given.
3. Run the config state check for the scope. For remote sources, resolve the fragment ref (default `HEAD`) to a SHA and ensure it is cached (`cache`).
4. **Select subpaths.** A plain subpath (or none) selects itself. A pattern subpath is expanded against the folders at the commit (`cache` `list_dirs`) or, for local sources, the real folders under the source directory (symlinks to folders are not followed). Every brace alternative must match at least one folder, and the pattern as a whole must match at least one; otherwise the add fails naming what matched nothing.
5. **Name and place each entry.** With a plain subpath, the name is `--name` or the default name, and the path is `--path` or the default path. With a pattern, `--name` is an error (several entries cannot share one name); each entry is named after its folder, and `--path`, when given, is the parent folder that every entry is placed in (`<path>/<name>`). Two matches with the same name fail the add. A match whose name is already in the config is skipped and reported as `already added` when that entry has the same source, subpath, and path, so re-running a pattern picks up only new folders; any other name clash fails the add. `--pin` pins every new entry of the batch.
6. **Check the batch.** Run the config state check with every new entry included. Nesting between one plain new entry and existing entries goes to gobbling (`install`); a pattern batch that nests with existing entries, or within itself, fails instead, suggesting the entry be added on its own.
7. **Stage and verify every entry** before changing any destination: export into staging (`install`), then, for skills without `--no-skill-check`, require a `SKILL.md` at the root of the staged folder (see Skill Layout Check). Then plan each entry's install (`install`). If any entry would be skipped or refused, discard all staging and fail.
8. Install every entry, then write all new entries and their hashes in one config write. A refused add leaves the config unchanged. If installing fails part-way (a filesystem error after every check passed), the entries already installed are still written to the config so it matches the disk, the rest are discarded, and the command reports the failure and exits 1.

Each entry records `type`, `source` (normalized), `commit` (resolved SHA, remote only), `ref` (only when the fragment named a branch or tag), `subpath` (the concrete matched folder, never the pattern), `path` (only when `--path` is given; for a pattern, `<path>/<name>`), and `pinned = true` (only when `--pin` is given).

#### Skill Layout Check

A skill's staged folder must contain a file named exactly `SKILL.md` (case-sensitive; a symlink to a file counts) at its root. When it does not, `add` fails without installing anything, and the error names the entry and offers the two ways forward:

- **A pattern**, when the staged folder holds `SKILL.md` files deeper down. If those files all sit at `<dir>/<x>/SKILL.md` for one `<dir>`, the error suggests the exact source to use, built from what the user typed before `#` and their ref, for example `github.com:obra/superpowers#dev:/skills/*` for `github.com:obra/superpowers#dev`.
- **`--no-skill-check`**, to install the folder as it is (for agents with a different layout).

When the `SKILL.md` files sit at different depths, no single pattern fits, and the error lists where they are instead. In a pattern batch every matched skill must pass; when some fail, the error names them and suggests braces (`{a,b}`) to select only the folders that are skills.

The check runs only on `add`: `sync` and `update` act on entries that were already accepted, and plugins are never checked.

### `remove <name> [--global/-g] [--force]`

Deletes the destination and the entry (`install` removal). An unknown name is an error.

### `update (<name> | --all/-a) [--global/-g] [--force]`

Exactly one of `<name>` and `--all` is required. Without `--global` the command acts on the local config; with it, on the global config.

Per remote entry: resolve the entry's `ref`, else its `commit` when that is an unresolved ref (which is then also written to `ref`), else `HEAD`; install it, and set `commit` to the new SHA only once the install succeeds. A refused install leaves `commit` unchanged. If the SHA is unchanged, the command reports "already at `<sha>`". Local entries report that local sources have nothing to update and that `sync` re-copies them.

`--all` processes every entry in the chosen scope. Unpinned entries are refreshed and installed as above. Entries with `pinned = true` are not refreshed: they are synced to their locked commit exactly as `sync` would, and reported with a `pinned:` prefix (`pinned: up to date`, `pinned: installed`). `update <name>` on a pinned entry refreshes it: the pin guards only the `--all` selector.

### `pin <name> [--global/-g]` and `unpin <name> [--global/-g]`

Set `pinned = true` on the entry, or remove the `pinned` key. Config-only: no fetch and no filesystem change, so neither is blocked by a local/global conflict. Pinning an already pinned entry, or unpinning an unpinned one, succeeds and reports no change. An unknown name is an error.

### `sync [--global/-g] [--force]`

For each entry: resolve an unresolved `commit` to a SHA and write it back, ensure the commit is cached, then plan and apply (`install`). Prints one line per entry: `installed`, `up to date`, `skipped: <reason>`, or `failed: <reason>`.

### `list [--global/-g] [--json]`

Table columns: name, kind (`skill`/`plugin`), type, commit (first 12 characters), path, source. Reads only the config: no network, no git, and no destination hashing.

### `show <pattern> [--global/-g] [--json]`

Matches entry names against a glob (`*`, `?`) using `wildmatch`. For each match, prints every config field plus the resolved absolute path and a destination status: `present`, `missing`, or `modified` (hash differs from the recorded `hash`).

## Output Contract

- `--json` writes exactly one JSON array to stdout, even when empty. Objects use the config field names plus `name`, `kind`, `resolved_path`, and (for `show`) `status`. Absent optional fields are `null`.
- A failure to write output, such as stdout closed early by `| head`, is ignored, so a command always finishes its filesystem and config work once it has started.
- Warnings and errors go to stderr, prefixed `WARN` or `ERROR`. Color is used only when stderr is a terminal and `NO_COLOR` is unset.
- Exit code 0 on success, 1 when any error, refusal, skip, or failed entry occurred. `show` with no matches exits 1; with `--json` it still prints `[]`.

## Decisions & Alternatives

| Decision | Chosen | Alternatives Considered | Rationale |
|---|---|---|---|
| Choosing skill vs plugin on `add` | `--plugin` flag, skill by default | Detect from repo contents (`SKILL.md`, `.claude-plugin/`); a required `--kind` | Detection depends on agent-specific layouts that vary between tools. Skills are the common case. |
| Pattern expansion | Expanded on `add` into concrete entries, installed all-or-nothing | Install the matches that work and report the rest | A partial batch leaves the user to work out which skills of a collection arrived; all-or-nothing keeps one `add` one outcome. |
| Missing `SKILL.md` | Fail, suggest a pattern or `--no-skill-check` | Warn and install | Installing a folder no agent loads is the more expensive mistake; the suggestion makes the fix one copy-paste. |
| Skill check escape hatch | Dedicated `--no-skill-check` | Reuse `--force` | `--force` means "discard local modifications"; folding an unrelated check into it would make one flag do two dangerous things. |
| `--force` on `add`, `remove`, `update` | Present | Only on `sync` | The refuse-over-proceed tenet makes every destructive command refuse on a dirty destination; each needs the same override. |
| `list` reads config only | No destination checks | Include status per entry | Keeps `list` instant and offline; `show` carries the status. |
| Glob crate | `wildmatch` | `globset` | Names are matched one pattern at a time; `globset` sets add nothing here. |
| Exit codes | 0 or 1 | Distinct codes per failure class | Agents read the JSON and stderr for detail; no consumer needs finer codes yet. |

## Open Questions & Future Decisions

None.

## References

- `docs/high-level-design.md`
