# Configuration Specs

Design: `config-design.md`

## Locations

- [x] **CFG-LOC-001**: The system shall read the local config from `athena.toml` in the current working directory only, without searching parent directories.
- [x] **CFG-LOC-002**: The system shall read the global config from `$XDG_CONFIG_HOME/athena/config.toml` when `XDG_CONFIG_HOME` is set, and from `~/.config/athena/config.toml` otherwise, on every operating system including macOS.

## Schema

- [x] **CFG-SCHEMA-001**: The system shall accept entries as tables `[skills."<name>"]` and `[plugins."<name>"]` with the keys `type`, `source`, `commit`, `ref`, `subpath`, `path`, `pinned`, and `hash`.
- [x] **CFG-SCHEMA-002**: If a config file (local or global) contains a key the schema does not define, then the system shall fail on load with an error naming the key and its table.
- [x] **CFG-SCHEMA-003**: If one name appears under both `[skills]` and `[plugins]` in the same config file, then the system shall fail on load with an error naming it.
- [x] **CFG-SCHEMA-004**: If an entry lacks `type` or `source`, or has a `type` other than `remote` or `local`, then the system shall fail on load with an error naming the entry.
- [x] **CFG-SCHEMA-005**: If an entry with `type = "remote"` lacks `commit`, or an entry with `type = "local"` has `commit` or `ref`, then the system shall fail on load with an error naming the entry.
- [x] **CFG-SCHEMA-006**: If the global config contains a `[core]` table, then the system shall fail on load with an error stating that `[core]` is local-only.
- [x] **CFG-SCHEMA-007**: When the local config has no `[core]` table or omits a default path key, the system shall use `./.agents/skills` as `default_skill_path` and `./.agents/plugins` as `default_plugin_path`.
- [x] **CFG-SCHEMA-008**: When an entry's `commit` is a 40-character hexadecimal string, the system shall treat it as a locked commit; any other `commit` value shall be treated as an unresolved ref.
- [x] **CFG-SCHEMA-009**: When an entry has no `pinned` key, the system shall treat the entry as not pinned.

## Path Resolution

- [x] **CFG-PATH-001**: When a local entry has `path`, the system shall resolve a relative `path` against the directory containing `athena.toml`.
- [x] **CFG-PATH-002**: When a local entry has no `path`, the system shall resolve its destination to `<default_skill_path>/<last component of name>` for skills and `<default_plugin_path>/<last component of name>` for plugins, relative to the directory containing `athena.toml`.
- [x] **CFG-PATH-003**: If a global entry has no `path`, or a `path` that is neither absolute nor `~`-prefixed, then the system shall fail on load with an error naming the entry.
- [x] **CFG-PATH-004**: When resolving any entry path (local or global), the system shall expand a leading `~` to the home directory and normalize the path lexically (removing `.` and collapsing `..`) without consulting the filesystem.
- [x] **CFG-PATH-005**: If an entry's resolved path equals `/`, the home directory, or the directory containing its config file, is an ancestor of any of these, or lies inside the `athena` cache directory, then the system shall fail on load with an error naming the entry and path, for every command.

## Config State Check

- [x] **CFG-VAL-001**: When two entries in one config file resolve to the same path and differ in `source`, `commit`, or `subpath`, the system shall report a state error naming both entries and the path, and suggesting `--name` or `--path` when either entry's path is a default path (CFG-PATH-002).
- [x] **CFG-VAL-002**: When two entries in one config file resolve to the same path and have equal `source`, `commit`, and `subpath`, the system shall report no state error for that pair.
- [x] **CFG-VAL-003**: When one entry's resolved path is an ancestor of another entry's resolved path in the same config file, the system shall report a state error naming both entries.
- [x] **CFG-STATE-001**: When any command runs with `-g`/`--global`, the system shall check collisions (CFG-VAL-001) and nesting (CFG-VAL-003) among global entries only.
- [x] **CFG-STATE-002**: When any command runs without `-g`/`--global`, the system shall check collisions and nesting among local entries, and local vs global conflicts (CFG-CONF-001).
- [x] **CFG-STATE-003**: If any state error exists when `add`, `sync`, or `update` runs (local or global scope), then the system shall exit with an error before any fetch or filesystem change, listing every state error with the entries and paths involved.
- [x] **CFG-STATE-004**: When `remove`, `pin`, or `unpin` runs while state errors exist (local or global scope), the system shall proceed without blocking.
- [x] **CFG-STATE-005**: When `list` or `show` runs while state errors exist (local or global scope), the system shall print one `WARN` per state error and proceed.
- [x] **CFG-STATE-006**: When `remove` targets an entry involved in a state error, the system shall remove only its config entry and leave its destination untouched.

## Local vs Global Conflicts

- [x] **CFG-CONF-001**: When a command runs without `-g`/`--global` and the global config exists, the system shall load the global config and report a state error for every conflict, a conflict being a local entry whose resolved path equals, contains, or is contained by a global entry's resolved path.
- [x] **CFG-CONF-002**: When `add` runs (local or global scope), the system shall include every entry being added in the state check, except that nesting between new and existing entries is not reported as a state error: a single-entry add resolves it by gobbling (INST-GOB-001 to INST-GOB-006), and a pattern batch fails (CMD-ADD-014).
- [x] **CFG-CONF-003**: When a local vs global conflict blocks a command (CFG-STATE-003), the system shall name the local entry and path and the global entry and path, and instruct the user to remove either one, by hand or with `athena remove <name>` or `athena remove -g <name>`.
- [x] **CFG-CONF-006**: When a command runs with `-g`/`--global`, the system shall not read the local config.

## Writing

- [x] **CFG-WRITE-001**: When the system writes a config file (local or global), it shall preserve the comments, key order, and formatting of every part of the file it does not change.
- [x] **CFG-WRITE-002**: When the system writes a config file, it shall write to a temporary file in the same directory and rename it over the original.
- [x] **CFG-WRITE-003**: When `add` targets a config file that does not exist, the system shall create it (a local file without a `[core]` table; a global file together with its parent directories).
- [x] **CFG-WRITE-004**: When `sync` (or `update --all` for a pinned entry) locks an entry whose `commit` is an unresolved ref, the system shall write the resolved SHA to `commit` and, if the entry has no `ref`, write the original `commit` value to `ref`, whether or not the entry's install later succeeds.
