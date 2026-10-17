# Software Requirements Specification (SRS): Skill & Plugin Manager

## 1. Project Overview

Build a Rust-based Command Line Interface (CLI) tool acting as a package manager for AI "skills" and "plugins". The tool manages the installation, versioning, and synchronization of these assets from Git repositories into designated directories.

## 2. Core Architecture & Scope

The manager operates across three strictly defined spatial scopes:

### 2.1 Global Cache (The Registry)
* **Location:** `~/.cache/<tool_name>/remotes/` (XDG Base Directory conventions).
* **Behavior:** All remote repositories are shallow-cloned (`git clone --depth 1` or `--shallow-since`) into this centralized cache first, keyed by a hash of the source URL. The CLI copies files from here to the final destinations to avoid nested `.git` directory conflicts and redundant network calls.

### 2.2 Global Scope (System-Wide Installs)
* **Config Location:** `~/.config/<tool_name>/config.toml`.
* **Behavior:** Global installations are pinned to a single specific commit hash.
* **Install Destination:** No default paths exist. Every entry *must* explicitly define its destination `path`.

### 2.3 Local Scope (Project-Specific Installs)
* **Config Location:** `./<tool_name>.toml` (Located in the active directory).
* **Behavior:** Local installations are isolated to the active folder/project.
* **Install Destination:** Defaults to project-local directories defined in a `[core]` section, unless overridden by a package-specific `path`.

## 3. Data Model (TOML Specification)

### Local Configuration Example (`./<tool_name>.toml`)
```toml
[core]
default_skill_path = "./.agents/skills"
default_plugin_path = "./.agents/plugins"

[skills."obra/superpowers"]
source = "https://github.com/obra/superpowers.git" 
commit = "a1b2c3d4e5f6g7h8i9j0"                    
type = "remote"
# Uses default_skill_path

[plugins."mcp-postgres"]
source = "https://github.com/modelcontextprotocol/servers.git"
commit = "HEAD"
type = "remote"
subpath = "src/postgres"                          # Extracts only this subfolder from the monorepo
path = "./.custom/bin/postgres-plugin"            # Overrides default path

[plugins."local-linter"]
source = "/Users/dev/my-local-plugins/linter"     
type = "local"
path = "./.custom/bin/linter"                     
```

## 4. Command Line Interface (CLI)

* **`add <source> [--name <custom_name>] [--path <custom_install_path>] [--global]`**
  * Parses the `<source>` URL. Extracts commit/branch fragments (e.g., `#main`) and subpaths, populating the TOML accordingly.
  * If `--global` is provided, `--path` is **mandatory**.
* **`remove <name> [--global]`**
  * Deletes the package from the installation path and removes the entry from the `.toml`.
* **`update <name> [--global]`**
  * Fetches the latest commits into the global cache, updates the installation to the latest HEAD, and modifies the `.toml`.
* **`sync [--global] [--force]`**
  * Reads the `.toml` file and ensures all packages match their target commit IDs by copying from the cache to the destination.
  * **Safety:** Must check for a dirty working tree in the destination (if it is a git repo) or uncommitted modified files. Skips sync and warns if local changes exist, unless `--force` is used.
* **`list [--global] [--json]`**
  * Outputs a tabulated summary. If `--json` is passed, outputs a strict JSON array for agent consumption.
* **`show <name_or_pattern> [--global] [--json]`**
  * Queries packages matching the pattern (supports glob/wildcards like `*linter`). Outputs detailed metadata. Outputs JSON if `--json` is passed.

## 5. Conflict Resolution & Path Management

### 5.1 Version Pinning & Path Collisions
* A single destination `path` can only host one version of a package.
* If the TOML configuration contains two different keys that resolve to the exact same destination `path` but demand different commit hashes, the CLI must throw a fatal validation error on load.

### 5.2 Nested Paths (Parent/Child Gobbling)
* If a new entry specifies a `path` that is a parent directory of an existing child installation (or vice versa), the CLI must detect the overlap.
* **Interactive Gobbling:** The CLI will prompt the user. If approved, the parent installation will "gobble up" (replace/absorb) the child.
* **Safety Constraint:** Before gobbling, the CLI must check the child directory for user modifications. If dirty, the gobble is blocked to prevent data loss unless `--force` is applied.

### 5.3 Global vs. Local Precedence
* If a local `.toml` specifies a path that overlaps with a globally managed path, the local configuration takes precedence for execution, but the CLI should emit a `WARN` indicating a global path is being overridden.

## 6. Git Integration & URL Normalization

* **Execution:** Must use `std::process::Command` to invoke the native `git` CLI. **Do NOT use the `git2` crate** to avoid C-binding compile failures.
* **URL Normalization:** The CLI must normalize shorthand inputs into valid Git URLs before caching:
  * `github.com:org/repo` -> `https://github.com/org/repo.git`
  * Support standard SSH (`git@github.com:org/repo.git`) and HTTPS formats.
* **Fragment Parsing:** If a user passes a source with fragments (e.g., `https://github.com/org/repo.git#branch/name:/sub/path`), the CLI must strip the fragments, use the base URL for the `source`, populate the `commit` field with the branch/commit, and populate the `subpath` field with the subdirectory.

## 7. Recommended Rust Crates
* `clap`, `serde`, `toml`, `serde_json`.
* `comfy-table` (for standard output), `colored`.
* `directories` (for resolving XDG Cache and Config paths).
* `wildmatch` or `globset` (for `show` wildcards).
* `regex` or `url` (for parsing and normalizing Git URLs and fragments).