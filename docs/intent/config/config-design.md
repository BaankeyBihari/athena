---
parent: high-level-design
prefix: CFG
---

# Configuration

## Context and Design Philosophy

The TOML files are the declared state of every install. This component locates, loads, validates, and rewrites them. Validation is strict and happens on load, so every later component can assume a coherent config: an invalid config is a fatal error naming the offending keys, never a partial load.

## Locations

- **Local:** `./athena.toml` in the current working directory. No upward search.
- **Global:** `$XDG_CONFIG_HOME/athena/config.toml`, falling back to `~/.config/athena/config.toml`. The XDG variables are honored on every platform, including macOS.

## Schema

```toml
[core]                                   # local only, optional
default_skill_path = "./.agents/skills"  # default when absent
default_plugin_path = "./.agents/plugins"
default_rule_path = "./.claude/rules"
default_agent_path = "./.claude/agents"

[skills."<name>"]       # or [plugins."<name>"], [rules."<name>"], [agents."<name>"]
type = "remote"         # required: "remote" | "local"
source = "<url or path>"  # required
commit = "<40-hex SHA or ref>"  # required for remote, forbidden for local
ref = "main"            # optional, remote only: what `update` follows (default HEAD)
subpath = "src/x"       # optional; required for remote rules and agents
path = "./custom/dir"   # optional locally, required globally
pinned = true           # optional, default false: `update --all` syncs but does not refresh it
hash = "sha256:<hex>"   # written only by athena
```

Unknown keys are errors. A name may appear in only one of `[skills]`, `[plugins]`, `[rules]`, and `[agents]` within a scope.

**Entry shape.** Skills and plugins install a directory. Rules and agents install one file: their `subpath` names a file (never a pattern, which is expanded on `add`), and their destination is that file. A remote rule or agent without `subpath` is a load error. A local one may omit `subpath` when `source` itself is a file. Names of rules and agents keep the file extension (`style.md`), because the destination file name is the last component of the name and Claude Code loads only `.md` files.

`commit` holding a 40-hex string is a locked commit. Any other value (`HEAD`, a branch) is an unresolved ref: `sync` resolves it to a SHA, writes the SHA to `commit`, and writes the original value to `ref` when `ref` is absent, so `update` keeps following it. The lock is written even when the entry's install is then skipped or fails, because resolving the ref is independent of the destination.

`pinned` belongs to one entry, and so to one install location. A local entry and a global entry are pinned independently. `pinned` affects only `update --all`, which syncs a pinned entry to its locked commit instead of refreshing it; `sync` and `update <name>` ignore it.

## Path Resolution

Every entry resolves to one absolute destination path:

- Local entry with `path`: relative to the directory holding `athena.toml`.
- Local entry without `path`: `<default path of its kind>/<last component of name>`, where the default path is `default_skill_path`, `default_plugin_path`, `default_rule_path`, or `default_agent_path`.
- Global entry: `path` must be absolute or start with `~`; relative paths are errors.
- `~` expands to the home directory everywhere.
- Paths are normalized lexically (`.` removed, `..` collapsed). They are not canonicalized through the filesystem, because destinations may not exist yet.

A resolved path is invalid if it equals `/`, the home directory, or the directory holding the config file, if it is an ancestor of any of these, or if it lies inside the `athena` cache directory. Deleting such a path would destroy user data.

## Config State Check

Every command checks the config state of its scope before executing. Two classes of problem exist:

**Load errors** make a config unusable, and every command fails on them: TOML syntax errors, schema violations (unknown keys, missing or forbidden fields, a name under both tables), a global entry without `path`, and an invalid resolved path (above). They are fixed by hand.

**State errors** are conflicts between entries that are each valid on their own:

1. **Collision (same path).** Two entries resolving to the same path while differing in `source`, `commit`, or `subpath`.
2. **Nesting.** One entry's path is an ancestor of another's within the same config. A nested pair cannot be installed coherently: the parent's content hash would include the child's files. `add` is the one place nesting is resolved, through gobbling (owned by `install`).
3. **Local vs global conflict.** A local entry whose resolved path equals, contains, or is contained by a global entry's resolved path. A global path is owned by the global config, and the local config can never write to it.

The scope sets which state errors are checked:

- **With `-g`/`--global`:** collisions and nesting among global entries only. The local config is not read.
- **Without `-g`:** collisions and nesting among local entries, plus local vs global conflicts, using the global config when it exists.

For `add`, the check includes the entry being added. How each command treats state errors:

- **`add`, `sync`, `update`:** blocked. The command exits before any fetch or filesystem change and lists every state error with the entries and paths involved. For a local vs global conflict, the message asks the user to remove either the local or the global entry, by hand or with `athena remove <name>` / `athena remove -g <name>`.
- **`remove`, `pin`, `unpin`:** proceed, because `remove` is how state errors are resolved. `remove` of an entry involved in a state error removes only its config entry and leaves the filesystem untouched: the files at that path may belong to the other entry in the conflict.
- **`list`, `show`:** proceed, printing one `WARN` per state error.

The block covers the whole command, not only the entries involved, so declared state never advances while the config is inconsistent.

## Writing

All writes go through `toml_edit`, preserving comments, ordering, and formatting. The write operations are: insert an entry, remove an entry, set `commit` (with `ref` when locking an unresolved ref), set `hash`, and set or clear `pinned`. `add` creates a missing local `athena.toml` (without a `[core]` section) or a missing global config file and its directory. Writes go to a temporary file in the same directory followed by a rename, so a crash never leaves a truncated config.

## Decisions & Alternatives

| Decision | Chosen | Alternatives Considered | Rationale |
|---|---|---|---|
| Default install directory name | Last component of the entry name | Full name as nested path (`obra/superpowers`) | Agent skill directories are flat; `.agents/skills/superpowers` is what agents load. Two names sharing a last component collide and are caught by the collision rule. |
| Names across kind tables | One name per scope across `[skills]`, `[plugins]`, `[rules]`, and `[agents]` | Names unique per table | `remove`, `show`, `pin`, `unpin`, and `update` take a bare name; per-table names would need a `--kind` on each of them. A clash is rare because agents are named by role and rules by topic, and `--name` resolves it. |
| Default rule and agent paths | `./.claude/rules`, `./.claude/agents` | `./.agents/rules` and `./.agents/agents`, to match the skill and plugin defaults | Claude Code reads rules and agents only from `.claude/`; an `.agents/` default would install files no agent loads. |
| Resolving XDG directories | Read `XDG_*` variables, fall back to `~/.config` and `~/.cache` | `directories` crate | On macOS the crate returns `~/Library/...`, contradicting the required `~/.config` and `~/.cache` locations. `std::env::home_dir` covers the rest. |
| Nested entries | State error blocking `add`, `sync`, `update` | Allow and install in order | The parent's hash would always include the child's files, so the parent would always look modified. |
| `ref` field | Optional field recording what `update` follows | Store only `commit` | Without it, a package added from `#dev` would silently switch to the default branch on `update`. |
| Same path, identical entries | Allowed | Always an error | Identical duplicates install the same files, so nothing conflicts. |
| Local entry overlapping a global path | Local `add`, `sync`, `update` are blocked until the user removes one side | Local wins with a `WARN`; global wins silently | Letting local write to a globally managed path would make the two configs fight over the same files, each seeing the other's install as a modification. Requiring the user to pick a side keeps every path with exactly one owner. |
| Global commands and local configs | Global commands ignore local configs | Global commands also check the current directory's `athena.toml` | Global installs must not depend on which directory the command runs from; the block lives on the local side, which is the side that would write into another owner's path. |
| Load errors vs state errors | Load errors fail every command; state errors block only `add`, `sync`, `update` | Every problem fatal on load | A fatal state error would also block `remove`, the command that resolves it. Load errors stay fatal because an unparseable file or an invalid path cannot be acted on safely. |
| `remove` of an entry in a state error | Removes the config entry only | Delete the destination as usual | The destination may hold files owned by the other entry in the conflict, such as a global install under a local path. |
| Atomic write | Temp file + rename | Write in place | A crash mid-write would otherwise corrupt the user's declared state. |

## Open Questions & Future Decisions

### Deferred
1. Upward search for `athena.toml` from subdirectories, like `git`. Commands act on the active directory only.

## References

- `docs/high-level-design.md`
