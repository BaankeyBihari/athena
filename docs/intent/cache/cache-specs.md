# Remote Cache Specs

Design: `cache-design.md`

## Layout

- [x] **CACHE-001**: The system shall store each remote source in `<cache root>/athena/remotes/<key>/`, where `<cache root>` is `$XDG_CACHE_HOME` when set and `~/.cache` otherwise, and `<key>` is the first 16 hexadecimal characters of the SHA-256 of the normalized URL.
- [x] **CACHE-002**: The system shall perform every git operation by running the `git` executable through `std::process::Command`, and shall not depend on the `git2` crate.

## Resolve

- [x] **CACHE-RES-001**: When resolving a ref that is not a 40-character hexadecimal string, the system shall run `git fetch --depth 1 origin <ref>` in the cache repository and return the commit SHA of `FETCH_HEAD`.
- [x] **CACHE-RES-002**: When resolving a ref that is a 40-character hexadecimal string, the system shall ensure that commit is present (CACHE-ENS-001 to CACHE-ENS-003) and return it unchanged.
- [x] **CACHE-RES-003**: If a ref cannot be resolved, then the system shall fail with an error naming the ref and the URL, suggesting a full SHA, branch, or tag, and including git's stderr.

## Ensure

- [x] **CACHE-ENS-001**: When ensuring a commit that `git cat-file -e <sha>^{commit}` finds in the cache repository, the system shall return without any network access.
- [x] **CACHE-ENS-002**: When ensuring a commit missing from the cache repository, the system shall run `git fetch --depth 1 origin <sha>`, and if that fails, fetch all branches of `origin` with full history.
- [x] **CACHE-ENS-003**: If a commit is still missing after CACHE-ENS-002, then the system shall fail with the error "commit `<sha>` not found in `<url>`".

## Export

- [x] **CACHE-EXP-001**: When exporting a remote entry, the system shall check out the commit detached in the cache repository and copy the subpath (or the whole worktree when no subpath is set) into the staging directory, excluding `.git` and copying symlinks as symlinks.
- [x] **CACHE-EXP-002**: If an export's subpath does not exist at the commit, or names a file rather than a directory, then the system shall fail with an error naming the subpath and commit.
- [x] **CACHE-EXP-003**: When exporting a rule or an agent from a remote entry, the system shall check out the commit detached in the cache repository and copy the single file at the subpath to the staging path, keeping its executable bit.
- [x] **CACHE-EXP-004**: If a file export's subpath does not exist at the commit, or does not name a regular file, then the system shall fail with an error naming the subpath and commit.

## List

- [x] **CACHE-LIST-001**: When listing folders for a commit that is present in the cache repository, the system shall return the relative path of every folder in that commit's tree, sorted, without checking out the commit and without network access.
- [x] **CACHE-LIST-002**: When listing files for a commit that is present in the cache repository, the system shall return the relative path of every regular file (blob mode `100644` or `100755`) in that commit's tree, sorted, excluding symlinks and submodules, without checking out the commit and without network access.

## Failures

- [x] **CACHE-ERR-001**: If the `git` executable is not found on `PATH`, then the system shall fail with an error stating that `git` is required.
- [x] **CACHE-ERR-002**: If a cache directory exists but is not a git repository, or its `origin` URL differs from the entry's normalized URL, then the system shall delete and recreate that cache directory.
