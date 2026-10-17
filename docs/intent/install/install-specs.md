# Installation Specs

Design: `install-design.md`

## Content Hash

- [x] **INST-HASH-001**: The system shall compute a directory's content hash as `sha256:<hex>` over its files and symlinks in sorted relative-path order, covering each one's relative path, kind, executable bit (files), and bytes (files) or target (symlinks), and ignoring empty directories.
- [x] **INST-HASH-002**: The system shall use the same content hash function for destinations and staging directories.

## Observer

- [x] **INST-OBS-001**: When an entry is processed by `add`, `update`, or `sync`, the system shall export its content into a new staging directory `<destination parent>/.athena-staging-<random>` and compute the staging directory's content hash as the target hash.
- [x] **INST-OBS-002**: When exporting a local entry, the system shall copy the local source path (or its subpath, when set) into the staging directory.
- [x] **INST-OBS-003**: If a local entry's source path or subpath does not exist or is not a directory, then the system shall mark that entry failed with an error naming the path.

## Dirty Rule

- [x] **INST-DIRTY-001**: When an entry's destination does not exist, or is a directory containing no files or symlinks, the system shall treat it as clean.
- [x] **INST-DIRTY-002**: When an entry has a `hash` value, the system shall treat its existing destination as clean if and only if the destination's content hash equals that value.
- [x] **INST-DIRTY-003**: When an entry already in the config has no `hash` value, the system shall treat its existing destination as clean if and only if the destination lies inside a git work tree and `git status --porcelain --ignored --untracked-files=all -- <destination>` prints nothing, so untracked and git-ignored files count as modifications.
- [x] **INST-DIRTY-004**: When `add` installs a new entry and its destination already exists, the system shall treat the destination as dirty regardless of git status, unless its content hash equals the target hash (INST-PLAN-001).

## Planner

- [x] **INST-PLAN-001**: When a destination's content hash equals the target hash, the system shall leave the destination untouched, report the entry as up to date, and record the target hash in `hash` if it differs.
- [x] **INST-PLAN-002**: When a destination is clean (INST-DIRTY-001 to INST-DIRTY-003) and its content hash differs from the target hash, the system shall install the staged content and record the target hash in `hash`.
- [x] **INST-PLAN-003**: When a destination is dirty and `--force` is given, the system shall install the staged content and record the target hash in `hash`.
- [x] **INST-PLAN-004**: When a destination is dirty and `--force` is not given, the system shall skip the entry, leave the destination untouched, and report "local modifications in `<path>`".
- [x] **INST-PLAN-005**: If a destination exists and is not a directory, then the system shall refuse the entry, with or without `--force`.
- [x] **INST-PLAN-006**: When a command processes several entries, the system shall plan each independently, so a skipped, refused, or failed entry does not prevent the others.
- [x] **INST-PLAN-007**: The system shall compute plans from observed facts without filesystem or git access.

## Gobbling

- [x] **INST-GOB-001**: When `add` introduces an entry whose resolved path is an ancestor of one or more existing entries' resolved paths in the same scope, the system shall require gobbling those child entries before installing.
- [x] **INST-GOB-002**: If any child destination to be gobbled is dirty, or the parent destination holds files outside the child destinations, and `--force` is not given, then the system shall refuse the `add` without prompting and without changes.
- [x] **INST-GOB-003**: When gobbling is required and stdin is a terminal, the system shall prompt `Gobble <children> into <parent>? [y/N]` and refuse the `add` without changes unless the answer is `y` or `yes`.
- [x] **INST-GOB-004**: If gobbling is required and stdin is not a terminal, then the system shall refuse the `add` without changes, with a message to remove the child entries first.
- [x] **INST-GOB-005**: When the user confirms gobbling, the system shall delete each child destination, remove each child entry from the config, and install the parent entry.
- [x] **INST-GOB-006**: If `add` introduces an entry whose resolved path lies inside an existing entry's resolved path in the same scope, then the system shall refuse the `add` without changes.
- [x] **INST-GOB-007**: When a child entry to be gobbled has `pinned = true`, the system shall gobble it under the same rules as an unpinned child.

## Removal

- [x] **INST-RM-001**: When `remove` targets an entry that is not involved in a state error (CFG-STATE-006) and whose destination is clean, the system shall delete the destination and remove the entry from the config.
- [x] **INST-RM-002**: When `remove` targets an entry whose destination does not exist, the system shall remove the entry from the config.
- [x] **INST-RM-003**: When `remove` targets an entry that is not involved in a state error and whose destination is dirty and `--force` is not given, the system shall refuse without changes; with `--force` it shall delete the destination and remove the entry.

## Applier

- [x] **INST-APPLY-001**: When installing an entry, the system shall create missing parent directories of the destination, remove the existing destination, and rename the entry's staging directory to the destination path.
- [x] **INST-APPLY-002**: If installing an entry fails before the rename completes, then the system shall remove the staging directory and leave the destination as it was.
- [x] **INST-APPLY-003**: Before a command exits, the system shall remove every staging directory it created that was not renamed into a destination.
- [x] **INST-APPLY-004**: When a command's filesystem actions finish, the system shall write the config changes for the actions that succeeded in a single config write, and none for actions that were skipped, refused, or failed; commit locks (CFG-WRITE-004) are included in that write regardless of install outcome.
