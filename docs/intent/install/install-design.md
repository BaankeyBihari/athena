---
parent: high-level-design
prefix: INST
---

# Installation

## Context and Design Philosophy

This component turns declared entries into files on disk without destroying user work. It has three parts: an **observer** that gathers facts about destinations, a pure **planner** that decides what to do from those facts, and an **applier** that carries out the plan. Every safety rule lives in the planner, so each one can be tested on plain data without touching the filesystem.

## Content Hash

`sha256:<hex>` over a directory tree. Paths are walked in sorted order, and for each file or symlink the hash covers its relative path, its kind, the executable bit (for files), and its bytes (for a symlink, its target). Empty directories do not contribute. The same function hashes destinations and staged exports, so equal content always gives an equal hash.

## Observer Facts

For each entry, the observer produces:

- `target_hash`: the hash of the entry's content at its locked commit, computed by exporting to a staging directory `<dest parent>/.athena-staging-<random>` (local sources: copied from the source path). The applier installs from this same staging directory, so each entry is exported once per command.
- `dest`: `Absent`, `NotADirectory`, or `Present { hash }`.
- `git_clean`: for a present destination inside a git work tree, whether `git status --porcelain --ignored --untracked-files=all -- <dest>` is empty (every file tracked and unmodified; ignored files count as modifications because git cannot recover them). `None` outside a work tree.
- `recorded_hash`: the entry's `hash` field.

## Dirty Rule

A present destination is **clean** when either:

1. its hash equals `recorded_hash`, or
2. `recorded_hash` is absent and `git_clean` is true, so every file can be recovered from git.

Otherwise it is **dirty**. An absent destination, or a directory holding no files, is clean: there is nothing to lose. For a new entry being added, rule 2 does not apply: an existing destination that no entry manages is dirty even when git could recover it, unless it already holds exactly the target content. A destination that exists but is not a directory is always refused, even with `--force`.

The git check covers entries with no recorded hash (hand-written entries, configs from before a first install). It is not consulted when a recorded hash exists, because untracked files freshly written by `athena` would otherwise read as dirty.

## Planner

`plan(entries, facts, flags) -> Plan`. Per entry:

| Condition | Action |
|---|---|
| `dest` hash equals `target_hash` | `UpToDate`; also `RecordHash` if `recorded_hash` differs |
| clean | `Install` then `RecordHash` |
| dirty, `--force` | `Install` then `RecordHash` |
| dirty, no `--force` | `Skip` with reason "local modifications in `<path>`" |
| `NotADirectory` | `Refuse` |

Entries are planned independently: a skipped entry does not stop the others.

### Gobbling

Gobbling applies only when `add` introduces an entry whose path is an ancestor of one or more existing entries' paths.

1. The planner emits `ConfirmGobble { parent, children }`.
2. If any child destination is dirty, or the parent destination holds files outside the child destinations, and `--force` is absent, the whole add is refused before any prompt.
3. The applier asks the user to confirm (`Gobble <children> into <parent>? [y/N]`). If stdin is not a terminal, the add is refused with a message to remove the children first.
4. On confirmation, the plan proceeds with `Delete` for each child destination, `RemoveEntry` for each child, then `Install` of the parent.

A pinned child is gobbled like any other; `pinned` guards only `update --all`.

An `add` whose new path lies inside an existing entry's path is refused: a package cannot be installed inside another package.

### Removal

`remove` plans `Delete` of the destination (refused when dirty without `--force`; skipped when absent) followed by `RemoveEntry`.

## Applier

- **Install:** remove the existing destination and rename the observer's staging directory into place. Staging sits beside the destination so the rename stays on one filesystem. On any failure, the staging directory is removed and the destination is left as it was before the rename step.
- **Staging cleanup:** staging directories not consumed by an `Install` (up-to-date, skipped, refused, or failed entries) are removed before the command exits.
- **Delete:** removes the destination directory tree.
- **Config writes** (`RecordHash`, `RemoveEntry`, commit updates) are collected and written in one `config` write after the filesystem actions, covering only the actions that succeeded.
- Missing parent directories of a destination are created.

**Crash recovery invariant.** Filesystem actions precede the config write, so a crash can leave a destination installed with its `hash` unrecorded. The next `sync` finds the destination hash equal to `target_hash`, plans `UpToDate` with `RecordHash`, and converges without user action.

## Decisions & Alternatives

| Decision | Chosen | Alternatives Considered | Rationale |
|---|---|---|---|
| "Up to date" test | Destination hash equals the hash of a fresh export at the locked commit | Compare recorded hash to destination | The recorded hash shows the destination is unmodified, not that it holds the pinned commit; a hand-edited `commit` would never reinstall. The export is local, so no network is needed. |
| Git status role | Fallback when no hash is recorded | Always consulted; never consulted | Always: freshly installed, uncommitted files read as dirty. Never: hand-written entries over committed files could not be synced without `--force`. |
| `add` over an unmanaged existing directory | Refused unless `--force` or identical content | Overwrite when git can recover it | No entry owns that directory, so nothing says the user meant to replace it; recoverability through git does not make the overwrite intended. |
| New entry inside an existing entry | Refused | Prompt and drop the new entry | Absorbing the new child into an existing parent means doing nothing, which is a confusing result for an approved prompt. |
| Non-interactive gobble | Refused | Treat `--force` as approval | `--force` means "overwrite modifications"; overloading it to also mean "delete other packages" widens what one flag destroys. |
| Hash crate | `sha2` | Hand-rolled hash; `blake3` | `std` has no cryptographic hash; `sha2` is small and pure Rust, and the cache uses it for keys too. |

## Open Questions & Future Decisions

### Deferred
1. Destinations of entries deleted from the TOML by hand are left on disk. There is no record of past entries; orphan cleanup would need one.
2. `--dry-run` printing the plan without applying it. The plan/apply split makes it cheap when wanted.

## References

- `docs/high-level-design.md`
