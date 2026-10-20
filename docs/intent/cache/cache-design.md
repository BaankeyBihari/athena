---
parent: high-level-design
prefix: CACHE
---

# Remote Cache

## Context and Design Philosophy

Every remote source is fetched once into a shared cache, and installs copy out of it. The cache is disposable: anything in it can be rebuilt from the network, so this component may delete and recreate cache directories freely, and never holds user data. All git access in `athena` goes through this component, which runs the `git` executable via `std::process::Command`.

## Layout

`$XDG_CACHE_HOME/athena/remotes/<key>/`, falling back to `~/.cache/athena/remotes/<key>/`. `<key>` is the first 16 hex characters of the SHA-256 of the normalized URL. Each directory is a non-bare git repository with remote `origin` set to the URL.

## Operations

### Resolve a ref

`resolve(url, ref) -> sha`. Runs `git fetch --depth 1 origin <ref>` followed by `git rev-parse FETCH_HEAD^{commit}`. This handles `HEAD`, branches, and tags (peeled to their commit). A full 40-hex SHA is not fetched by name: it goes through `ensure` (below), which has a full-fetch fallback for servers that refuse fetching an unadvertised SHA, and resolves to itself. An abbreviated SHA cannot be fetched by name; the error names the ref and asks for a full SHA or a branch or tag.

### Ensure a commit is present

`ensure(url, sha)`:

1. If `git cat-file -e <sha>^{commit}` succeeds, return without network access.
2. Otherwise run `git fetch --depth 1 origin <sha>`.
3. If that fails (the server refuses fetching an unadvertised SHA), run a full fetch of all branches, unshallowing the repository if needed.
4. If the commit is still missing, fail with "commit `<sha>` not found in `<url>`".

### List folders at a commit

`list_dirs(url, sha) -> Vec<String>`. Runs `git ls-tree -r -t <sha>` in the cache repository and returns the relative path of every tree entry (folder), sorted. The commit must be present (`ensure`); no checkout and no network access are needed. Used to expand subpath patterns.

### List files at a commit

`list_files(url, sha) -> Vec<String>`. Runs `git ls-tree -r <sha>` in the cache repository and returns the relative path of every regular file (blob mode `100644` or `100755`), sorted. Symlinks and submodules are excluded. The commit must be present (`ensure`); no checkout and no network access are needed. Used to expand subpath patterns for rules and agents.

### Export a tree

`export(url, sha, subpath, staging_dir)`. Runs `git checkout --detach --force <sha>` in the cache repository, then copies `<repo>/<subpath>` (or the whole worktree) into `staging_dir`, excluding `.git`. Symlinks are copied as symlinks. A subpath that does not exist at that commit, or that names a file rather than a directory, is an error.

`export_file(url, sha, subpath, staging_path)` does the same checkout, then copies the single file `<repo>/<subpath>` to `staging_path`, keeping its executable bit. A subpath that does not exist at that commit, or that does not name a regular file, is an error.

## Failure Handling

- **`git` not on `PATH`:** fatal error naming the requirement.
- **Cache directory exists but is not a git repository, or has a different `origin`:** the directory is deleted and recreated.
- **Network or auth failure:** the `git` stderr is included in the error. `git` inherits the user's environment, so credential helpers and SSH agents work as they do on the command line.

## Decisions & Alternatives

| Decision | Chosen | Alternatives Considered | Rationale |
|---|---|---|---|
| Ref resolution | `git fetch --depth 1 origin <ref>` + `rev-parse FETCH_HEAD` | `git ls-remote` then fetch | One command covers branches, tags, `HEAD`, and SHAs, and leaves the commit in the cache already. |
| Shallow strategy | `--depth 1` per commit, full fetch as fallback | `--shallow-since` | A locked commit is a single commit; a date window cannot guarantee it is included. |
| Export mechanism | Detached checkout + copy, excluding `.git` | `git archive` piped to a tar extractor; `git worktree` per commit | No extra crate, and it needs no per-commit worktree bookkeeping. Exports run one at a time, so the shared checkout is safe. |
| Cache key | Truncated SHA-256 of the normalized URL | Full hash; sanitized URL as directory name | 64 bits avoids collisions at any realistic cache size and keeps paths short; URLs contain characters unsafe in file names. |

## Open Questions & Future Decisions

### Deferred
1. Two concurrent `athena` processes sharing one cache directory can race on checkout. Add a file lock per cache directory if concurrent use appears.
2. Cache garbage collection (`athena cache clean`). Users can delete `~/.cache/athena` safely.

## References

- `docs/high-level-design.md`
