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

### `add <source> [--name <n>] [--path <p>] [--plugin] [--pin] [--global/-g] [--force]`

1. `--global` without `--path` is an error, raised before any network access.
2. Parse `<source>` (`source`). The entry is a skill unless `--plugin` is given.
3. The name is `--name` or the parsed default name. A name already present in the scope is an error pointing at `update`.
4. For remote sources, resolve the fragment ref (default `HEAD`) to a SHA (`cache`).
5. Build the entry: `type`, `source` (normalized), `commit` (resolved SHA), `ref` (only when the fragment named a branch or tag), `subpath`, `path` (only when `--path` is given), and `pinned = true` (only when `--pin` is given).
6. Run the config state check with the new entry included. Nesting between the new entry and existing entries is not a state error here; `install` resolves it by gobbling or refusal. Then install.
7. Write the entry and its `hash` to the config only after the install succeeds. A refused or failed add leaves the config unchanged.

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
- Warnings and errors go to stderr, prefixed `WARN` or `ERROR`. Color is used only when stderr is a terminal and `NO_COLOR` is unset.
- Exit code 0 on success, 1 when any error, refusal, skip, or failed entry occurred. `show` with no matches exits 1; with `--json` it still prints `[]`.

## Decisions & Alternatives

| Decision | Chosen | Alternatives Considered | Rationale |
|---|---|---|---|
| Choosing skill vs plugin on `add` | `--plugin` flag, skill by default | Detect from repo contents (`SKILL.md`, `.claude-plugin/`); a required `--kind` | Detection depends on agent-specific layouts that vary between tools. Skills are the common case. |
| `--force` on `add`, `remove`, `update` | Present | Only on `sync` | The refuse-over-proceed tenet makes every destructive command refuse on a dirty destination; each needs the same override. |
| `list` reads config only | No destination checks | Include status per entry | Keeps `list` instant and offline; `show` carries the status. |
| Glob crate | `wildmatch` | `globset` | Names are matched one pattern at a time; `globset` sets add nothing here. |
| Exit codes | 0 or 1 | Distinct codes per failure class | Agents read the JSON and stderr for detail; no consumer needs finer codes yet. |

## Open Questions & Future Decisions

None.

## References

- `docs/high-level-design.md`
