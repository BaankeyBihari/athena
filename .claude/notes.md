## 2026-10-17 - Store install content hash in the TOML entry (`hash` field); no `athena.lock`
WHY: One file instead of two; hand-edited pins still sync cleanly. Use `toml_edit` to preserve user formatting.
ALTERNATIVES: Separate `athena.lock` state file (cargo/npm style); compare destination to cache copy; skip dirty detection for non-git destinations.
DEFERRED: Merge-conflict noise from tool-written hashes in committed TOML; revisit if it hurts.

## 2026-10-17 - `update` gains `--all/-a`; `pinned = true` excludes an entry from `--all`
WHY: Pin guards one install location from bulk updates; explicit `update <name>` still updates it. "Pin" now means this flag only; the `commit` field is the "locked commit".
ALTERNATIVES: Pin blocks every update; pin as a locked commit SHA (original SRS wording).
DEFERRED: Command to set/clear pin; hand-edited field for now.

## 2026-10-17 - Local/global path conflict blocks local add/sync/update (replaces SRS §5.3 local-wins WARN)
WHY: Local config can never write a globally managed path; every path has one owner. User resolves by removing the local or global block (hand edit or `remove` / `remove -g`).
ALTERNATIVES: Local wins with WARN (SRS §5.3); global wins silently.
DEFERRED: none.

## 2026-10-17 - `update --all` syncs pinned entries without refreshing them; local/global conflicts are only overlaps with local entries
WHY: `update` = refresh + sync, so `update --all` covers everything; the pin only withholds the refresh. Conflicts limited to real overlaps so a project holding a global install can still run local commands.
ALTERNATIVES: Skip pinned entries entirely under `--all`; treat any global path inside the current directory as a conflict.
DEFERRED: none.

## 2026-10-17 - Phase 4 edge resolutions
WHY: `sync` locks unresolved refs even when the install is skipped (reproducibility); `update` follows `ref`, else an unresolved `commit`, else `HEAD`; pinned children gobble normally; `add` over an unmanaged existing directory is refused unless `--force` or identical content (strict).
ALTERNATIVES: Lock only on successful install; always `HEAD`; refuse gobbling pinned children; allow `add` over git-recoverable directories.
DEFERRED: none.
