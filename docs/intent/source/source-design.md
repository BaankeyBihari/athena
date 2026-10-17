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

`<base>#<ref>[:<subpath>]` or `<base>#:<subpath>`.

- The base is everything before the first `#`.
- Inside the fragment, the first `:` separates ref from subpath. Git forbids `:` in ref names, so the split is unambiguous.
- An empty ref means no ref (the caller uses `HEAD`).
- The subpath has leading `/` removed. Empty subpaths, absolute paths remaining after stripping, and any `..` component are errors.
- Local sources accept a subpath fragment (`/dir#:sub`) but not a ref; a ref on a local source is an error.

## Default Name

- Remote with subpath: the last component of the subpath (`servers.git#:src/postgres` gives `postgres`).
- Remote without subpath: the last two path components with `.git` removed (`github.com/obra/superpowers.git` gives `obra/superpowers`).
- Local: the last component of the path (or of the subpath when present).

## Decisions & Alternatives

| Decision | Chosen | Alternatives Considered | Rationale |
|---|---|---|---|
| SSH and HTTPS for the same repo | Kept as distinct URLs | Canonicalize both to one form | The two forms authenticate differently; rewriting SSH to HTTPS breaks private repos that rely on SSH keys. The cost is a possible duplicate cache directory. |
| `.git` suffix on explicit HTTPS URLs | Left as given | Always append `.git` | Some hosts (Azure DevOps) reject the suffix. Only the shorthand form, where `athena` builds the URL, gets `.git`. |
| URL parsing | Hand-written string splitting | `url` or `regex` crate | The grammar is a handful of prefix and separator checks; scp-style SSH URLs are not valid `url` crate URLs anyway. |
| Abbreviated SHA in fragment | Accepted as a ref string; resolution is the cache's job | Reject here | Source parsing cannot tell an abbreviated SHA from a branch named `abc1234`. |

## Open Questions & Future Decisions

### Deferred
1. Bare `org/repo` shorthand (no host) defaulting to GitHub. Not in scope until requested.

## References

- `docs/high-level-design.md`
