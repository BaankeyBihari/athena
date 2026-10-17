# Security Policy

## Supported versions

athena is pre-1.0. Only the latest release on `main` receives security fixes.

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub: go to the repository's **Security** tab and choose **Report a vulnerability** (<https://github.com/BaankeyBihari/athena/security/advisories/new>). Do not open a public issue.

Include:

- the athena version or commit,
- the operating system,
- the steps or the `athena.toml` that reproduce the problem,
- what an attacker gains.

You can expect an acknowledgement within 7 days. Once a fix is ready, we publish an advisory and credit you unless you prefer otherwise.

## Security model

Understanding what athena does and does not protect against helps you use it safely and decide what counts as a vulnerability.

### What athena does

- Runs the `git` executable to fetch the sources named in your config. Git uses your normal credentials (SSH agent, credential helpers), and athena never stores or logs credentials.
- Copies files from its cache (`~/.cache/athena/remotes/`) or from local directories into destination paths, and deletes destination directories on `remove`, `update`, `sync`, and gobbling.
- **Never executes** fetched files. It only copies them.

### Built-in protections

- athena refuses any destination equal to `/`, your home directory, or the directory containing the config file, any ancestor of those, and any path inside its cache.
- athena will not overwrite or delete a destination whose contents differ from what it last installed, or that holds untracked or git-ignored files, unless you pass `--force`.
- athena updates config files atomically, so a crash cannot leave a truncated config.
- A local `athena.toml` cannot install into a path owned by the global config.

### What athena does not protect against

- **Untrusted config files.** An `athena.toml` decides which repositories are fetched and where their files go. Within the limits above, a local config may name destinations outside its project (for example `path = "../../somewhere"`). Treat an `athena.toml` from a repository you do not trust like a script, and review it before running `athena sync` or `athena update`.
- **Malicious skills or plugins.** athena installs what you ask for; it does not review it. Skills and plugins are instructions and code that AI agents act on. Only add sources you trust, and use full commit SHAs (`#<sha>`) when you need to know exactly what you install.
- **Compromised remotes.** A locked commit SHA protects `sync` from upstream changes, but `update` deliberately moves to the latest commit of a branch.
- **Local attackers** with write access to your home directory, cache, or config files.
- **`--force`**, which discards local modifications by design.

Reports showing that one of the built-in protections can be bypassed are in scope.
