---
parent: high-level-design
prefix: SRC
---

# Source Parsing

## Context and Design Philosophy

Users hand `athena add` a source in whatever form they copied it: a GitHub shorthand, an SSH URL, an HTTPS URL, or a local directory, optionally followed by a fragment naming a ref and a subdirectory. This component turns that string into a `ParsedSource`, the single normalized form every other component consumes. It is pure string handling: no network, no git, and filesystem access only to make a local path absolute.

Normalization is deliberately conservative. It rewrites only forms that are unambiguous (the `host:org/repo` shorthand) and otherwise keeps the user's URL, because rewriting a URL can change which credentials and which server git uses.

## Output Shape

```rust
pub struct ParsedSource {
    pub kind: SourceKind,          // Remote { url } | Local { path }
    pub reference: Option<String>, // branch, tag, or full commit SHA from the fragment
    pub subpath: Option<String>,   // relative, no leading '/', no '..'
    pub default_name: String,
}
```

## Accepted Forms

| Input | Kind | Normalized |
|---|---|---|
| `github.com:org/repo` | Remote | `https://github.com/org/repo.git` |
| `<host>:<path>`, host contains a `.` and no `@`, no scheme | Remote | `https://<host>/<path>`, `.git` appended if absent |
| `git@host:org/repo.git` | Remote | unchanged |
| `ssh://…`, `https://…`, `http://…`, `git://…` | Remote | trailing `/` stripped, otherwise unchanged |
| `/abs`, `./rel`, `../rel`, `~/…`, `file://…` | Local | absolute path; `~` expanded; `file://` stripped |

Anything else is an error naming the accepted forms.

## Fragment Grammar

`<base>#<ref>[:<subpath>]`, `<base>#:<subpath>`, or `<base>#/<subpath>`.

- The base is everything before the first `#`.
- Inside the fragment, the first `:` separates ref from subpath. Git forbids `:` in ref names, so the split is unambiguous.
- An empty ref means no ref (the caller uses `HEAD`).
- A fragment starting with `/` is a subpath with no ref: `#/plugins/x` means `#:/plugins/x`. Git forbids ref names starting with `/`, so this form is unambiguous. A fragment such as `#main/plugins/x` stays a ref, because `main/plugins/x` is a valid branch name.
- The subpath has leading `/` removed. Empty subpaths, absolute paths remaining after stripping, and any `..` component are errors.
- Local sources accept a subpath fragment (`/dir#:sub`) but not a ref; a ref on a local source is an error.

## Subpath Patterns

A subpath may be a **pattern** that selects several folders. `ParsedSource::subpath` then holds the pattern text unchanged, and `is_pattern(subpath)` reports whether it contains any of the pattern characters below.

| Syntax | Matches |
|---|---|
| `*` | Any run of characters within one path component, including none |
| `?` | Exactly one character within one path component |
| `{a,b,c}` | Exactly one of the comma-separated alternatives |

- Patterns may appear in any component: `plugins/*/skills/*` is valid. A component never matches across `/`.
- `*` and `?` never match a leading `.`, as in shell globs, so `skills/*` does not pick up `skills/.github`. Name the hidden folder explicitly to match it.
- Braces do not nest. An unclosed `{`, a `}` without `{`, braces with fewer than two alternatives, an empty alternative, or an alternative containing `/` is an error naming the pattern.
- `..` is rejected in patterns as in plain subpaths.

Matching is pure. `alternatives(pattern)` expands the braces into one brace-free pattern per combination; `matches(pattern, dir)` tests one brace-free pattern against one folder path, component by component. The caller (`commands`) supplies the list of folders that exist at the commit, so this component still needs no git or filesystem access. A brace alternative that matches no folder is reported by name, so a typo such as `{arrow-maintainance,map-codebase}` fails instead of silently installing less.

## Default Name

- Remote with subpath: the last component of the subpath (`servers.git#:src/postgres` gives `postgres`).
- Remote without subpath: the last two path components with `.git` removed (`github.com/obra/superpowers.git` gives `obra/superpowers`).
- Local: the last component of the path (or of the subpath when present).
- Pattern subpath: each matched folder is named after its own last component (`skills/*` matching `skills/brainstorming` gives `brainstorming`).

## Decisions & Alternatives

| Decision | Chosen | Alternatives Considered | Rationale |
|---|---|---|---|
| SSH and HTTPS for the same repo | Kept as distinct URLs | Canonicalize both to one form | The two forms authenticate differently; rewriting SSH to HTTPS breaks private repos that rely on SSH keys. The cost is a possible duplicate cache directory. |
| `.git` suffix on explicit HTTPS URLs | Left as given | Always append `.git` | Some hosts (Azure DevOps) reject the suffix. Only the shorthand form, where `athena` builds the URL, gets `.git`. |
| URL parsing | Hand-written string splitting | `url` or `regex` crate | The grammar is a handful of prefix and separator checks; scp-style SSH URLs are not valid `url` crate URLs anyway. |
| Ref and subpath separator | `:` required, or a leading `/` for no ref | Split `#main/plugins/x` at the first `/` | Branch names may contain `/`; `#feature/x/skills` would be split in the wrong place. |
| Pattern syntax | `*`, `?`, `{a,b}` within components, no `**` | Full glob with `**`; regex | Folder layouts in skill repositories are shallow and regular; `**` would match nested folders that are themselves inside a matched skill, creating nested entries. |
| Abbreviated SHA in fragment | Accepted as a ref string; resolution is the cache's job | Reject here | Source parsing cannot tell an abbreviated SHA from a branch named `abc1234`. |

## Open Questions & Future Decisions

### Deferred
1. Bare `org/repo` shorthand (no host) defaulting to GitHub. Not in scope until requested.

## References

- `docs/high-level-design.md`
