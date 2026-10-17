# Commands Specs

Design: `commands-design.md`

## Scope

- [x] **CMD-SCOPE-001**: When any command is given `--global` or `-g`, the system shall act on the global config; otherwise it shall act on the local config.
- [x] **CMD-SCOPE-002**: If the selected config file does not exist when a command other than `add` runs, then the system shall exit with an error naming the missing file.

## add

- [x] **CMD-ADD-001**: If `add` is given `--global` without `--path`, then the system shall exit with an error before any network access.
- [x] **CMD-ADD-002**: When `add` runs, the system shall record the entry under `[plugins]` if `--plugin` is given and under `[skills]` otherwise.
- [x] **CMD-ADD-003**: When `add` runs, the system shall name the entry with `--name` if given and with the source's default name otherwise.
- [x] **CMD-ADD-004**: If the name chosen by `add` already exists in the selected config, then the system shall exit with an error suggesting `update`.
- [x] **CMD-ADD-005**: When `add` runs with a remote source, the system shall resolve the fragment ref (or `HEAD` when none) to a commit SHA and record it in `commit`.
- [x] **CMD-ADD-006**: When `add` records an entry, the system shall write `type`, the normalized `source`, `commit` (remote only), `ref` only when the fragment ref is not a 40-character hexadecimal string, `subpath` only when set, `path` only when `--path` is given, and `pinned = true` only when `--pin` is given.
- [x] **CMD-ADD-007**: When `add` runs, the system shall write the new entry and its `hash` to the config only after its install succeeds, leaving the config unchanged when the install is refused or fails.

## remove

- [x] **CMD-RM-001**: If `remove`, `update <name>`, `pin`, or `unpin` names an entry absent from the selected config, then the system shall exit with an error naming it, and, when run without `-g` and the name exists in the global config, suggest `-g`.

## update

- [x] **CMD-UPD-001**: If `update` is given both a name and `--all`/`-a`, or neither, then the system shall exit with an error.
- [x] **CMD-UPD-002**: When `update` refreshes a remote entry, the system shall resolve its `ref` when set, else its `commit` when that is an unresolved ref, else `HEAD`; install the result; and set `commit` to the new SHA only after the install succeeds, also writing the followed value to `ref` when it came from an unresolved `commit`.
- [x] **CMD-UPD-003**: When `update` processes a remote entry whose resolved SHA equals its `commit`, the system shall report "already at `<sha>`" and change nothing.
- [x] **CMD-UPD-004**: When `update` processes a local entry, the system shall report that local sources have nothing to update and that `sync` re-copies them, and change nothing.
- [x] **CMD-UPD-005**: When `update --all` runs, the system shall refresh and install every entry in the selected config that is not pinned, and sync every entry with `pinned = true` to its locked commit without refreshing it, prefixing that entry's result line with `pinned:`.
- [x] **CMD-UPD-006**: When `update <name>` names an entry with `pinned = true`, the system shall refresh and install it like an unpinned entry.

## pin and unpin

- [x] **CMD-PIN-001**: When `pin <name>` runs, the system shall set `pinned = true` on the entry; when `unpin <name>` runs, the system shall remove the entry's `pinned` key.
- [x] **CMD-PIN-002**: When `pin` targets an already pinned entry, or `unpin` an unpinned one, the system shall exit successfully and report no change.
- [x] **CMD-PIN-004**: When `pin` or `unpin` runs, the system shall change only the config file, with no fetch and no change to any destination.

## sync

- [x] **CMD-SYNC-001**: When `sync` runs, the system shall, for each entry in the selected config, lock an unresolved `commit` (CFG-WRITE-004) and ensure the commit is cached when the entry is remote, and then plan and apply its install.
- [x] **CMD-SYNC-002**: When `sync` finishes an entry, the system shall print one line for it reading `installed`, `up to date`, `skipped: <reason>`, or `failed: <reason>`.

## list and show

- [x] **CMD-LIST-001**: When `list` runs without `--json`, the system shall print a table with the columns name, kind (`skill` or `plugin`), type, commit (first 12 characters), path, and source.
- [x] **CMD-LIST-002**: When `list` runs, the system shall read only the config files, with no network access, no git invocation, and no destination hashing.
- [x] **CMD-SHOW-001**: When `show <pattern>` runs, the system shall select entries whose names match the glob pattern, where `*` matches any run of characters and `?` matches one character.
- [x] **CMD-SHOW-002**: When `show` prints a matching entry, the system shall include every config field, the resolved absolute path, and a status: `missing` when the destination does not exist, `modified` when the entry has a `hash` that differs from the destination's content hash, and `present` otherwise.

## Output

- [x] **CMD-OUT-001**: When `list` or `show` runs with `--json`, the system shall write exactly one JSON array to stdout and nothing else, each object holding the config fields plus `name`, `kind`, and `resolved_path` (and `status` for `show`), with absent optional fields as `null`.
- [x] **CMD-OUT-002**: The system shall write warnings and errors to stderr prefixed `WARN` or `ERROR`, colored only when stderr is a terminal and `NO_COLOR` is unset.
- [x] **CMD-OUT-003**: When a command finishes, the system shall exit with code 1 if any error, refusal, skip, or failed entry occurred, and with code 0 otherwise.
- [x] **CMD-OUT-004**: When `show` matches no entries, the system shall exit with code 1, printing `[]` to stdout when `--json` is given.
