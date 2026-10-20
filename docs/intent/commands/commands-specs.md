# Commands Specs

Design: `commands-design.md`

## Scope

- [x] **CMD-SCOPE-001**: When any command is given `--global` or `-g`, the system shall act on the global config; otherwise it shall act on the local config.
- [x] **CMD-SCOPE-002**: If the selected config file does not exist when a command other than `add` runs, then the system shall exit with an error naming the missing file.

## add

- [x] **CMD-ADD-001**: If `add` is given `--global` without `--path`, then the system shall exit with an error before any network access.
- [x] **CMD-ADD-002**: When `add` runs, the system shall record the entry under `[skills]` when no kind is given, and under `[skills]`, `[plugins]`, `[rules]`, or `[agents]` when `--kind` is `skill`, `plugin`, `rule`, or `agent` respectively.
- [x] **CMD-ADD-003**: When `add` runs, the system shall name the entry with `--name` if given and with the source's default name otherwise.
- [x] **CMD-ADD-004**: If the name chosen by `add` already exists in the selected config, then the system shall exit with an error suggesting `update`.
- [x] **CMD-ADD-005**: When `add` runs with a remote source, the system shall resolve the fragment ref (or `HEAD` when none) to a commit SHA and record it in `commit`.
- [x] **CMD-ADD-006**: When `add` records an entry, the system shall write `type`, the normalized `source`, `commit` (remote only), `ref` only when the fragment ref is not a 40-character hexadecimal string, `subpath` only when set (the concrete matched folder when the source used a pattern, never the pattern), `path` only when `--path` is given (the `--path` value as written, with trailing `/` characters removed, followed by `/<leaf>` as in CMD-ADD-012), and `pinned = true` only when `--pin` is given.
- [x] **CMD-ADD-007**: When `add` installs its new entries, the system shall write them and their hashes to the config in one write after the installs; if an install fails after every check passed, the system shall still record the entries installed before the failure, discard the rest, report the failure, and exit with code 1.
- [x] **CMD-ADD-009**: When `add` is given a source whose subpath is a pattern (SRC-PAT-001), the system shall expand it against the folders at the resolved commit (remote sources) or the real folders under the source directory, not following symlinks (local sources), and create one new entry per matched folder, in sorted order.
- [x] **CMD-ADD-010**: If an `add` pattern matches no folder, or any of its brace alternatives (SRC-PAT-004) matches no folder, then the system shall fail naming the alternatives that matched nothing, with no change to the config or any destination.
- [x] **CMD-ADD-011**: If `add` is given `--name` together with a pattern source, then the system shall fail before any network access.
- [x] **CMD-ADD-012**: When `add` is given `--path`, for a plain or a pattern source, the system shall place each new entry at `<path>/<leaf>`, where `<leaf>` is the last `/`-separated component of the entry's name and trailing `/` characters of `<path>` are ignored.
- [x] **CMD-ADD-013**: If two entries of one `add` batch share a name, or a new entry's name already exists in the selected config with a different source, subpath, or path, then the system shall fail naming them, with no change to the config or any destination.
- [x] **CMD-ADD-014**: If an `add` pattern batch has an entry whose path contains or lies inside an existing entry's path, or another entry of the batch, then the system shall fail without prompting, suggesting that the entry be added on its own.
- [x] **CMD-ADD-015**: When `add` runs (single entry or pattern batch), the system shall stage, check, and plan every new entry before changing any destination (including deleting gobbled children), and if any entry fails, would be skipped, or would be refused, discard all staging and change nothing.
- [x] **CMD-ADD-016**: When an `add` pattern matches a folder whose entry name already exists in the selected config with the same source, subpath, and path, the system shall skip that match and report it as `already added`.
- [x] **CMD-ADD-017**: When `add` is given `--pin` with a pattern source, the system shall set `pinned = true` on every new entry of the batch.
- [x] **CMD-ADD-018**: If the name `add` chooses for a plain (non-pattern) source (CMD-ADD-003) has a last `/`-separated component that is empty, `.`, or `..`, then the system shall exit with an error naming the name before any network access.
- [x] **CMD-ADD-019**: If `add` is given an empty `--path`, then the system shall exit with an error before any network access.
- [x] **CMD-ADD-020**: If `add` is given a `--kind` value other than `skill`, `plugin`, `rule`, or `agent`, then the system shall exit with an error listing the accepted values, before any network access.
- [x] **CMD-ADD-021**: When `add` is given `--plugin` without `--kind`, the system shall behave as for `--kind plugin` and print a `WARN` stating that `--plugin` is deprecated in favor of `--kind plugin`.
- [x] **CMD-ADD-022**: If `add` is given both `--plugin` and `--kind`, then the system shall exit with an error before any network access.
- [x] **CMD-ADD-023**: If `add` for a rule or an agent is given a remote source without a subpath, a local source that is a folder without a subpath, or a local source that is a file with a subpath, then the system shall exit with an error before any network access.
- [x] **CMD-ADD-024**: When `add` runs for a rule or an agent, the system shall select files instead of folders: a plain subpath shall name a regular file at the resolved commit (remote) or under the source (local), and a pattern subpath (SRC-PAT-001) shall be expanded against the regular files at the commit (CACHE-LIST-002) or the real files under the source directory, not following symlinks, creating one new entry per matched file, in sorted order.
- [x] **CMD-ADD-025**: When `add` names a rule or an agent entry, the system shall use the file name including its extension as the default name (SRC-NAME-005), and shall place the entry at `<path>/<leaf>` or the kind's default path as it does for folders.

## File kind check

- [x] **CMD-FILE-001**: If any file selected by `add` for a rule or an agent has a name not ending in `.md` (case-sensitive), then the system shall fail naming every such file, with no change to the config or any destination.
- [x] **CMD-FILE-004**: If any file selected by `add` for a rule or an agent is a symlink, then the system shall fail naming every such file, with no change to the config or any destination.
- [x] **CMD-FILE-005**: If `add` for a rule or an agent chooses an entry name whose last `/`-separated component does not end in `.md`, then the system shall fail naming it and suggesting the same name with `.md` appended, before any network access.
- [x] **CMD-FILE-002**: When the file kind check fails for a pattern batch, the system shall suggest narrowing the pattern, for example `rules/*.md`.
- [x] **CMD-FILE-003**: The system shall run the file kind check only in `add`, and shall offer no flag to skip it; `--no-skill-check` shall not affect it.

## Skill layout check

- [x] **CMD-SKILL-001**: When `add` stages a skill entry without `--no-skill-check`, the system shall require a file named exactly `SKILL.md` (case-sensitive; a symlink to a file counts) at the root of the staged folder, and if any skill entry of the batch lacks one, fail naming those entries with no change to the config or any destination.
- [x] **CMD-SKILL-002**: When the skill layout check fails for an entry whose staged folder holds `SKILL.md` files that all sit at `<dir>/<x>/SKILL.md` for a single `<dir>`, the system shall suggest the source as typed before `#`, followed by `#<ref>:/<subpath>/<dir>/*` (the user's ref, empty when none; `<dir>/*` when there was no subpath).
- [x] **CMD-SKILL-003**: When the skill layout check fails and the staged folder holds `SKILL.md` files not covered by CMD-SKILL-002, the system shall list their relative paths.
- [x] **CMD-SKILL-004**: When the skill layout check fails, the system shall state that `--no-skill-check` installs the folders as they are, and, for a pattern batch, suggest braces (`{a,b}`) to select only the folders that are skills.
- [x] **CMD-SKILL-005**: The system shall run the skill layout check only in `add` for skill entries; `add` for plugins, rules, and agents, `sync`, and `update` shall not run it.

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

- [x] **CMD-LIST-001**: When `list` runs without `--json`, the system shall print a table with the columns name, kind (`skill`, `plugin`, `rule`, or `agent`), type, commit (first 12 characters), path, and source.
- [x] **CMD-LIST-002**: When `list` runs, the system shall read only the config files, with no network access, no git invocation, and no destination hashing.
- [x] **CMD-SHOW-001**: When `show <pattern>` runs, the system shall select entries whose names match the glob pattern, where `*` matches any run of characters and `?` matches one character.
- [x] **CMD-SHOW-002**: When `show` prints a matching entry, the system shall include every config field, the resolved absolute path, and a status: `missing` when the destination does not exist, `modified` when the entry has a `hash` that differs from the destination's content hash, and `present` otherwise.

## Output

- [x] **CMD-OUT-001**: When `list` or `show` runs with `--json`, the system shall write exactly one JSON array to stdout and nothing else, each object holding the config fields plus `name`, `kind`, and `resolved_path` (and `status` for `show`), with absent optional fields as `null`.
- [x] **CMD-OUT-002**: The system shall write warnings and errors to stderr prefixed `WARN` or `ERROR`, colored only when stderr is a terminal and `NO_COLOR` is unset.
- [x] **CMD-OUT-003**: When a command finishes, the system shall exit with code 1 if any error, refusal, skip, or failed entry occurred, and with code 0 otherwise.
- [x] **CMD-OUT-004**: When `show` matches no entries, the system shall exit with code 1, printing `[]` to stdout when `--json` is given.
- [x] **CMD-OUT-005**: If writing to stdout or stderr fails during any command (for example, a closed pipe), then the system shall ignore the failure and complete the command, including its config write.
