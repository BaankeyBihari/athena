use crate::cache::Cache;
use crate::config::{
    self, Config, Editor, Entry, Env, Kind, Scope, SourceType, StateError, expand_home, is_locked,
    normalize,
};
use crate::install::{self, Action, Dest, Facts, Nesting, Removal};
use crate::source::{self, SourceKind};
use clap::{Parser, Subcommand};
use colored::Colorize;
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use wildmatch::WildMatch;

/// Output that never panics: a closed stdout (for example `| head`) must not abort a command midway.
// @spec CMD-OUT-005
macro_rules! say {
    ($($t:tt)*) => {{ let _ = writeln!(std::io::stdout(), $($t)*); }};
}
macro_rules! say_inline {
    ($($t:tt)*) => {{ let _ = write!(std::io::stdout(), $($t)*); }};
}
macro_rules! shout {
    ($($t:tt)*) => {{ let _ = writeln!(std::io::stderr(), $($t)*); }};
}

#[derive(Parser)]
#[command(
    name = "athena",
    version,
    about = "Install and pin AI skills and plugins from git repositories"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Add a new entry and install it
    Add {
        source: String,
        /// Entry name; its last component names the installed folder
        #[arg(long)]
        name: Option<String>,
        /// Folder to install into; each entry goes in <PATH>/<name>
        #[arg(long)]
        path: Option<String>,
        /// Record the entry under [plugins] instead of [skills]
        #[arg(long)]
        plugin: bool,
        /// Exclude the entry from `update --all` refreshes
        #[arg(long)]
        pin: bool,
        /// Install skills even without a SKILL.md at their root
        #[arg(long)]
        no_skill_check: bool,
        #[arg(short = 'g', long)]
        global: bool,
        #[arg(long)]
        force: bool,
    },
    /// Delete an entry's files and remove it from the config
    Remove {
        name: String,
        #[arg(short = 'g', long)]
        global: bool,
        #[arg(long)]
        force: bool,
    },
    /// Refresh entries to the latest commit of their ref and install them
    Update {
        name: Option<String>,
        #[arg(short = 'a', long)]
        all: bool,
        #[arg(short = 'g', long)]
        global: bool,
        #[arg(long)]
        force: bool,
    },
    /// Exclude an entry from `update --all` refreshes
    Pin {
        name: String,
        #[arg(short = 'g', long)]
        global: bool,
    },
    /// Include an entry in `update --all` refreshes again
    Unpin {
        name: String,
        #[arg(short = 'g', long)]
        global: bool,
    },
    /// Restore every entry to its locked commit
    Sync {
        #[arg(short = 'g', long)]
        global: bool,
        #[arg(long)]
        force: bool,
    },
    /// Summarize all entries
    List {
        #[arg(short = 'g', long)]
        global: bool,
        #[arg(long)]
        json: bool,
    },
    /// Show entries whose names match a glob pattern
    Show {
        pattern: String,
        #[arg(short = 'g', long)]
        global: bool,
        #[arg(long)]
        json: bool,
    },
}

/// Parses process arguments, runs the command, and returns the exit code.
// @spec CMD-OUT-002, CMD-OUT-003
pub fn main() -> i32 {
    colored::control::set_override(
        std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
    );
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            let _ = e.print();
            return if e.use_stderr() { 1 } else { 0 };
        }
    };
    match run(cli.command) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(msg) => {
            shout!("{} {msg}", "ERROR".red());
            1
        }
    }
}

fn warn(msg: impl std::fmt::Display) {
    shout!("{} {msg}", "WARN".yellow());
}

fn run(cmd: Cmd) -> Result<bool, String> {
    match cmd {
        Cmd::Add {
            source,
            name,
            path,
            plugin,
            pin,
            no_skill_check,
            global,
            force,
        } => Ctx::new(global)?.add(
            &source,
            name,
            path,
            if plugin { Kind::Plugin } else { Kind::Skill },
            pin,
            force,
            !no_skill_check,
        ),
        Cmd::Remove {
            name,
            global,
            force,
        } => Ctx::new(global)?.remove(&name, force),
        Cmd::Update {
            name,
            all,
            global,
            force,
        } => Ctx::new(global)?.update(name.as_deref(), all, force),
        Cmd::Pin { name, global } => Ctx::new(global)?.pin(&name, true),
        Cmd::Unpin { name, global } => Ctx::new(global)?.pin(&name, false),
        Cmd::Sync { global, force } => Ctx::new(global)?.sync(force),
        Cmd::List { global, json } => Ctx::new(global)?.list(json),
        Cmd::Show {
            pattern,
            global,
            json,
        } => Ctx::new(global)?.show(&pattern, json),
    }
}

/// Result of processing one entry in `sync`, `update`, or `add`.
enum Outcome {
    /// Installed; carries the hash to record.
    Installed(String),
    /// Destination already matches; carries the hash to record when it changed.
    UpToDate(Option<String>),
    AlreadyAt(String),
    Note(&'static str),
    Skipped(String),
    Failed(String),
}

impl Outcome {
    fn ok(&self) -> bool {
        !matches!(self, Outcome::Skipped(_) | Outcome::Failed(_))
    }

    fn text(&self) -> String {
        match self {
            Outcome::Installed(_) => "installed".into(),
            Outcome::UpToDate(_) => "up to date".into(),
            Outcome::AlreadyAt(sha) => format!("already at `{sha}`"),
            Outcome::Note(n) => n.to_string(),
            Outcome::Skipped(r) => format!("skipped: {r}"),
            Outcome::Failed(r) => format!("failed: {r}"),
        }
    }

    fn recorded_hash(&self) -> Option<&String> {
        match self {
            Outcome::Installed(h) => Some(h),
            Outcome::UpToDate(h) => h.as_ref(),
            _ => None,
        }
    }
}

/// A staged entry with its planned install.
struct Prepared {
    staging: PathBuf,
    target_hash: String,
    action: Action,
}

/// Pending config edits, written once after the filesystem work.
type Changes = Vec<(Kind, String, &'static str, String)>;

struct Ctx {
    env: Env,
    scope: Scope,
    cache: Cache,
}

impl Ctx {
    // @spec CMD-SCOPE-001
    fn new(global: bool) -> Result<Ctx, String> {
        let env = Env::from_process()?;
        let cache = Cache::new(&env.cache_dir());
        Ok(Ctx {
            scope: if global { Scope::Global } else { Scope::Local },
            env,
            cache,
        })
    }

    fn file(&self) -> PathBuf {
        match self.scope {
            Scope::Local => self.env.local_file(),
            Scope::Global => self.env.global_file(),
        }
    }

    // @spec CMD-SCOPE-002
    fn load(&self, required: bool) -> Result<Config, String> {
        let file = self.file();
        if required && !file.exists() {
            return Err(match self.scope {
                Scope::Local => format!("no athena.toml in `{}`", self.env.cwd.display()),
                Scope::Global => format!("no global config at `{}`", file.display()),
            });
        }
        config::load(self.scope, &file, &self.env)
    }

    /// The global config, read only for local-scope commands.
    // @spec CFG-CONF-006
    fn global_for_local(&self) -> Result<Option<Config>, String> {
        let file = self.env.global_file();
        if self.scope == Scope::Global || !file.exists() {
            return Ok(None);
        }
        config::load(Scope::Global, &file, &self.env).map(Some)
    }

    fn state(&self, cfg: &Config) -> Result<Vec<StateError>, String> {
        Ok(config::state_errors(cfg, self.global_for_local()?.as_ref()))
    }

    // @spec CFG-STATE-003
    fn block_on(errors: &[StateError]) -> Result<(), String> {
        if errors.is_empty() {
            return Ok(());
        }
        let list: Vec<String> = errors.iter().map(|e| format!("  - {e}")).collect();
        Err(format!(
            "the config is inconsistent; resolve these first:\n{}",
            list.join("\n")
        ))
    }

    // @spec CMD-RM-001
    fn not_found(&self, name: &str) -> String {
        let mut msg = format!("no entry `{name}` in `{}`", self.file().display());
        let in_global = self.scope == Scope::Local
            && config::load(Scope::Global, &self.env.global_file(), &self.env)
                .is_ok_and(|g| g.get(name).is_some());
        if in_global {
            msg.push_str("; it exists in the global config, so pass `-g`");
        }
        msg
    }

    fn entry<'c>(&self, cfg: &'c Config, name: &str) -> Result<&'c Entry, String> {
        cfg.get(name).ok_or_else(|| self.not_found(name))
    }

    fn save(&self, changes: &Changes) -> Result<(), String> {
        if changes.is_empty() {
            return Ok(());
        }
        let mut ed = Editor::open(&self.file())?;
        for (kind, name, key, value) in changes {
            ed.set(*kind, name, key, value.as_str());
        }
        ed.save()
    }

    fn local_source(&self, entry: &Entry) -> PathBuf {
        let dir = self
            .file()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        normalize(
            &dir.join(expand_home(&entry.source, &self.env.home))
                .join(entry.subpath.as_deref().unwrap_or("")),
        )
    }

    /// Exports the entry's content into a staging directory beside its destination; returns it and its hash.
    // @spec INST-OBS-001, INST-OBS-002, INST-OBS-003
    fn stage(&self, entry: &Entry, sha: Option<&str>) -> Result<(PathBuf, String), String> {
        let parent = entry.resolved.parent().unwrap_or(Path::new("/"));
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create `{}`: {e}", parent.display()))?;
        let staging = install::staging_dir(&entry.resolved);
        let exported = match (entry.source_type, sha) {
            (SourceType::Remote, Some(sha)) => {
                self.cache
                    .export(&entry.source, sha, entry.subpath.as_deref(), &staging)
            }
            _ => {
                let src = self.local_source(entry);
                if src.is_dir() {
                    install::copy_tree(&src, &staging)
                } else {
                    Err(format!(
                        "local source `{}` does not exist or is not a directory",
                        src.display()
                    ))
                }
            }
        };
        match exported.and_then(|_| install::hash_dir(&staging)) {
            Ok(hash) => Ok((staging, hash)),
            Err(e) => {
                let _ = fs::remove_dir_all(&staging);
                Err(e)
            }
        }
    }

    /// `git status` is consulted only for a present destination with no recorded hash.
    fn git_clean(entry: &Entry, dest: &Dest) -> Option<bool> {
        match (dest, &entry.hash) {
            (Dest::Present { .. }, None) => install::git_clean(&entry.resolved),
            _ => None,
        }
    }

    fn clean(entry: &Entry, dest: &Dest) -> bool {
        install::is_clean(
            dest,
            entry.hash.as_deref(),
            Self::git_clean(entry, dest),
            false,
        )
    }

    /// Stages one entry at `sha` and plans its install; `dest` overrides the observed destination.
    fn prepare(
        &self,
        entry: &Entry,
        sha: Option<&str>,
        force: bool,
        new_entry: bool,
        dest: Option<Dest>,
    ) -> Result<Prepared, String> {
        let (staging, target_hash) = self.stage(entry, sha)?;
        let dest = match dest.map_or_else(|| install::observe_dest(&entry.resolved), Ok) {
            Ok(d) => d,
            Err(e) => {
                let _ = fs::remove_dir_all(&staging);
                return Err(e);
            }
        };
        let facts = Facts {
            target_hash: target_hash.clone(),
            git_clean: Self::git_clean(entry, &dest),
            dest,
            recorded_hash: entry.hash.clone(),
            new_entry,
        };
        let action = install::plan(&facts, force, &entry.resolved);
        Ok(Prepared {
            staging,
            target_hash,
            action,
        })
    }

    /// Carries out a prepared plan, consuming its staging directory.
    // @spec INST-APPLY-003
    fn finish(entry: &Entry, p: Prepared) -> Outcome {
        let discard = || {
            let _ = fs::remove_dir_all(&p.staging);
        };
        match p.action {
            Action::UpToDate { record_hash } => {
                discard();
                Outcome::UpToDate(record_hash.then_some(p.target_hash))
            }
            Action::Install => match install::install_staged(&p.staging, &entry.resolved) {
                Ok(()) => Outcome::Installed(p.target_hash),
                Err(e) => Outcome::Failed(e),
            },
            Action::Skip(r) => {
                discard();
                Outcome::Skipped(r)
            }
            Action::Refuse(r) => {
                discard();
                Outcome::Failed(r)
            }
        }
    }

    /// Stages, plans, and applies one entry's install at `sha`.
    // @spec INST-PLAN-006
    fn apply(&self, entry: &Entry, sha: Option<&str>, force: bool, new_entry: bool) -> Outcome {
        match self.prepare(entry, sha, force, new_entry, None) {
            Ok(p) => Self::finish(entry, p),
            Err(e) => Outcome::Failed(e),
        }
    }

    fn record(entry: &Entry, outcome: &Outcome, changes: &mut Changes) {
        if let Some(h) = outcome.recorded_hash() {
            changes.push((entry.kind, entry.name.clone(), "hash", h.clone()));
        }
    }

    /// Restores one entry to its locked commit, locking an unresolved `commit` first.
    // @spec CMD-SYNC-001, CFG-WRITE-004, INST-APPLY-004
    fn sync_entry(&self, entry: &Entry, force: bool, changes: &mut Changes) -> Outcome {
        let mut sha = None;
        if entry.source_type == SourceType::Remote {
            let commit = entry.commit.clone().unwrap_or_default();
            let locked = if is_locked(&commit) {
                commit
            } else {
                match self.cache.resolve(&entry.source, &commit) {
                    Ok(s) => {
                        changes.push((entry.kind, entry.name.clone(), "commit", s.clone()));
                        if entry.git_ref.is_none() {
                            changes.push((entry.kind, entry.name.clone(), "ref", commit));
                        }
                        s
                    }
                    Err(e) => return Outcome::Failed(e),
                }
            };
            if let Err(e) = self.cache.ensure(&entry.source, &locked) {
                return Outcome::Failed(e);
            }
            sha = Some(locked);
        }
        let outcome = self.apply(entry, sha.as_deref(), force, false);
        Self::record(entry, &outcome, changes);
        outcome
    }

    /// Refreshes one entry to the latest commit of what it follows and installs it.
    // @spec CMD-UPD-002, CMD-UPD-003, CMD-UPD-004
    fn update_entry(&self, entry: &Entry, force: bool, changes: &mut Changes) -> Outcome {
        if entry.source_type == SourceType::Local {
            return Outcome::Note(
                "local sources have nothing to update; `athena sync` re-copies them",
            );
        }
        let unresolved = entry.commit.clone().filter(|c| !is_locked(c));
        let follow = entry
            .git_ref
            .clone()
            .or(unresolved.clone())
            .unwrap_or_else(|| "HEAD".into());
        let sha = match self.cache.resolve(&entry.source, &follow) {
            Ok(s) => s,
            Err(e) => return Outcome::Failed(e),
        };
        if entry.commit.as_deref() == Some(sha.as_str()) {
            return Outcome::AlreadyAt(sha);
        }
        if let Err(e) = self.cache.ensure(&entry.source, &sha) {
            return Outcome::Failed(e);
        }
        let outcome = self.apply(entry, Some(&sha), force, false);
        if outcome.ok() {
            changes.push((entry.kind, entry.name.clone(), "commit", sha));
            if entry.git_ref.is_none() && unresolved.is_some() {
                changes.push((entry.kind, entry.name.clone(), "ref", follow));
            }
            Self::record(entry, &outcome, changes);
        }
        outcome
    }

    // @spec CMD-SYNC-002
    fn sync(&self, force: bool) -> Result<bool, String> {
        let cfg = self.load(true)?;
        Self::block_on(&self.state(&cfg)?)?;
        let mut changes = Changes::new();
        let mut ok = true;
        for entry in &cfg.entries {
            let outcome = self.sync_entry(entry, force, &mut changes);
            say!("{}: {}", entry.name, outcome.text());
            ok &= outcome.ok();
        }
        self.save(&changes)?;
        Ok(ok)
    }

    // @spec CMD-UPD-001, CMD-UPD-005, CMD-UPD-006
    fn update(&self, name: Option<&str>, all: bool, force: bool) -> Result<bool, String> {
        if name.is_some() == all {
            return Err("give either an entry name or `--all`, not both".into());
        }
        let cfg = self.load(true)?;
        Self::block_on(&self.state(&cfg)?)?;
        let entries = match name {
            Some(n) => vec![self.entry(&cfg, n)?],
            None => cfg.entries.iter().collect(),
        };
        let mut changes = Changes::new();
        let mut ok = true;
        for entry in entries {
            let (prefix, outcome) = if all && entry.pinned {
                ("pinned: ", self.sync_entry(entry, force, &mut changes))
            } else {
                ("", self.update_entry(entry, force, &mut changes))
            };
            say!("{}: {prefix}{}", entry.name, outcome.text());
            ok &= outcome.ok();
        }
        self.save(&changes)?;
        Ok(ok)
    }

    // @spec CMD-ADD-001, CMD-ADD-002, CMD-ADD-003, CMD-ADD-004, CMD-ADD-005, CMD-ADD-006, CMD-ADD-007, CMD-ADD-011, CMD-ADD-012, CMD-ADD-013, CMD-ADD-015, CMD-ADD-016, CMD-ADD-017, CMD-ADD-018, CMD-ADD-019, CFG-CONF-002, INST-GOB-003, INST-GOB-004, INST-GOB-005
    #[allow(clippy::too_many_arguments)]
    fn add(
        &self,
        input: &str,
        name: Option<String>,
        path: Option<String>,
        kind: Kind,
        pin: bool,
        force: bool,
        skill_check: bool,
    ) -> Result<bool, String> {
        if self.scope == Scope::Global && path.is_none() {
            return Err("`--path` is required with `--global`".into());
        }
        if path.as_deref() == Some("") {
            return Err("`--path` must not be empty".into());
        }
        let parsed = source::parse(input, &self.env.cwd, &self.env.home)?;
        let pattern = parsed.subpath.clone().filter(|s| source::is_pattern(s));
        if pattern.is_some() && name.is_some() {
            return Err(
                "`--name` cannot be used with a pattern; each matched folder is named after itself"
                    .into(),
            );
        }
        let cfg = self.load(false)?;
        let single_name = name.unwrap_or(parsed.default_name.clone());
        if pattern.is_none() {
            if single_name.is_empty() {
                return Err(format!(
                    "cannot derive a name from `{input}`; pass `--name`"
                ));
            }
            if matches!(source::last(&single_name), "" | "." | "..") {
                return Err(format!(
                    "name `{single_name}` cannot name a folder; pass a `--name` ending in a folder name"
                ));
            }
            if cfg.get(&single_name).is_some() {
                return Err(format!(
                    "`{single_name}` already exists in `{}`; use `athena update {single_name}`",
                    self.file().display()
                ));
            }
        }
        Self::block_on(&self.state(&cfg)?)?;

        let (source_type, source) = match &parsed.kind {
            SourceKind::Remote { url } => (SourceType::Remote, url.clone()),
            SourceKind::Local { path } => (SourceType::Local, path.display().to_string()),
        };
        let sha = match source_type {
            SourceType::Remote => Some(
                self.cache
                    .resolve(&source, parsed.reference.as_deref().unwrap_or("HEAD"))?,
            ),
            SourceType::Local => None,
        };
        let subpaths = match &pattern {
            None => vec![parsed.subpath.clone()],
            Some(p) => self
                .expand(p, source_type, &source, sha.as_deref())?
                .into_iter()
                .map(Some)
                .collect(),
        };

        let mut entries: Vec<Entry> = Vec::new();
        for subpath in subpaths {
            // @spec SRC-NAME-004
            let name = match (&pattern, &subpath) {
                (Some(_), Some(sub)) => source::last(sub).to_string(),
                _ => single_name.clone(),
            };
            let path = path
                .as_ref()
                .map(|p| format!("{}/{}", p.trim_end_matches('/'), source::last(&name)));
            let entry = Entry {
                resolved: cfg.resolve_path(kind, &name, path.as_deref(), &self.env)?,
                name,
                kind,
                source_type,
                source: source.clone(),
                commit: sha.clone(),
                git_ref: parsed.reference.clone().filter(|r| !is_locked(r)),
                subpath,
                path,
                pinned: pin,
                hash: None,
            };
            if let Some(existing) = cfg.get(&entry.name) {
                let same = (&existing.source, &existing.subpath, &existing.resolved)
                    == (&entry.source, &entry.subpath, &entry.resolved);
                if !same {
                    return Err(format!(
                        "`{}` already exists in `{}` with a different source, subpath, or path",
                        entry.name,
                        self.file().display()
                    ));
                }
                say!("{}: already added", entry.name);
                continue;
            }
            if entries.iter().any(|e| e.name == entry.name) {
                return Err(format!(
                    "two matched folders are both named `{}`; narrow the pattern",
                    entry.name
                ));
            }
            entries.push(entry);
        }
        if entries.is_empty() {
            return Ok(true);
        }

        let mut with_new = cfg.clone();
        with_new.entries.extend(entries.iter().cloned());
        let errors: Vec<StateError> = self
            .state(&with_new)?
            .into_iter()
            .filter(|e| {
                !(matches!(e, StateError::Nesting { .. })
                    && entries.iter().any(|n| e.involves(&n.name, self.scope)))
            })
            .collect();
        Self::block_on(&errors)?;
        let children = match pattern {
            None => self.plan_gobble(&cfg, &entries[0], force)?,
            Some(_) => {
                Self::refuse_nesting(&with_new.entries, &entries)?;
                vec![]
            }
        };

        // Stage, check, and plan every entry before any destination changes.
        let mut prepared: Vec<Prepared> = Vec::new();
        let discard = |prepared: &[Prepared]| {
            for p in prepared {
                let _ = fs::remove_dir_all(&p.staging);
            }
        };
        for entry in &entries {
            let dest = (!children.is_empty()).then_some(Dest::Absent);
            match self.prepare(entry, sha.as_deref(), force, true, dest) {
                Ok(p) => prepared.push(p),
                Err(e) => {
                    discard(&prepared);
                    return Err(format!("cannot add `{}`: {e}", entry.name));
                }
            }
        }
        if kind == Kind::Skill
            && skill_check
            && let Err(e) = skill_layout(input, &parsed, pattern.is_some(), &entries, &prepared)
        {
            discard(&prepared);
            return Err(e);
        }
        if let Some((entry, reason)) =
            entries
                .iter()
                .zip(&prepared)
                .find_map(|(e, p)| match &p.action {
                    Action::Skip(r) | Action::Refuse(r) => Some((e, r.clone())),
                    _ => None,
                })
        {
            discard(&prepared);
            return Err(format!("cannot add `{}`: {reason}", entry.name));
        }

        for child in &children {
            if child.resolved.exists() {
                fs::remove_dir_all(&child.resolved)
                    .map_err(|e| format!("cannot remove `{}`: {e}", child.resolved.display()))?;
            }
        }
        let mut ed = Editor::open(&self.file())?;
        for child in &children {
            ed.remove(child.kind, &child.name);
        }
        let mut failure = None;
        let mut pending = prepared.into_iter();
        for mut entry in entries {
            let Some(p) = pending.next() else { break };
            match Self::finish(&entry, p) {
                Outcome::Installed(h) | Outcome::UpToDate(Some(h)) => {
                    entry.hash = Some(h);
                    ed.insert(&entry);
                    say!("added `{}` at `{}`", entry.name, entry.resolved.display());
                }
                other => {
                    failure = Some(format!("cannot add `{}`: {}", entry.name, other.text()));
                    break;
                }
            }
        }
        discard(&pending.collect::<Vec<_>>());
        ed.save()?;
        failure.map_or(Ok(true), Err)
    }

    /// Folders a pattern selects at the commit (remote) or under the source directory (local).
    // @spec CMD-ADD-009, CMD-ADD-010
    fn expand(
        &self,
        pattern: &str,
        source_type: SourceType,
        source: &str,
        sha: Option<&str>,
    ) -> Result<Vec<String>, String> {
        let alts = source::alternatives(pattern)?;
        let depth = pattern.split('/').count();
        let dirs = match (source_type, sha) {
            (SourceType::Remote, Some(sha)) => self.cache.list_dirs(source, sha)?,
            _ => local_dirs(Path::new(source), depth)?,
        };
        let unmatched: Vec<String> = alts
            .iter()
            .filter(|a| !dirs.iter().any(|d| source::matches(a, d)))
            .map(|a| format!("`{a}`"))
            .collect();
        if !unmatched.is_empty() {
            return Err(format!("no folder matches {}", unmatched.join(", ")));
        }
        let mut selected: Vec<String> = dirs
            .into_iter()
            .filter(|d| alts.iter().any(|a| source::matches(a, d)))
            .collect();
        selected.dedup();
        Ok(selected)
    }

    /// A pattern batch never gobbles: any nesting with an existing or sibling entry fails the add.
    // @spec CMD-ADD-014
    fn refuse_nesting(all: &[Entry], new: &[Entry]) -> Result<(), String> {
        for n in new {
            if let Some(o) = all.iter().find(|o| {
                o.name != n.name
                    && (o.resolved.starts_with(&n.resolved) || n.resolved.starts_with(&o.resolved))
            }) {
                return Err(format!(
                    "`{}` would be nested with entry `{}`; add it on its own to gobble or resolve the overlap",
                    n.name, o.name
                ));
            }
        }
        Ok(())
    }

    /// Checks nesting between a new entry and existing ones; returns the children to gobble once confirmed.
    // @spec INST-GOB-001, INST-GOB-002, INST-GOB-006, INST-GOB-007
    fn plan_gobble<'c>(
        &self,
        cfg: &'c Config,
        entry: &Entry,
        force: bool,
    ) -> Result<Vec<&'c Entry>, String> {
        let new_path = &entry.resolved;
        let is_child = |e: &Entry| e.resolved.starts_with(new_path) && e.resolved != *new_path;
        let mut existing = Vec::new();
        for e in &cfg.entries {
            let clean = !is_child(e) || Self::clean(e, &install::observe_dest(&e.resolved)?);
            existing.push((e.name.clone(), e.resolved.clone(), clean));
        }
        let names = match install::plan_nesting(new_path, &existing, force) {
            Nesting::None => return Ok(vec![]),
            Nesting::InsideExisting(n) => {
                return Err(format!(
                    "`{}` lies inside entry `{n}`; a package cannot be installed inside another package",
                    new_path.display()
                ));
            }
            Nesting::DirtyChild(n) => {
                return Err(format!(
                    "entry `{n}` has local modifications that gobbling would destroy; use `--force` to discard them"
                ));
            }
            Nesting::Gobble(names) => names,
        };
        let children: Vec<&Entry> = cfg
            .entries
            .iter()
            .filter(|e| names.contains(&e.name))
            .collect();
        let child_paths: Vec<PathBuf> = children.iter().map(|c| c.resolved.clone()).collect();
        if !force && install::has_files_outside(new_path, &child_paths)? {
            return Err(format!(
                "local modifications in `{}` outside the entries it would gobble",
                new_path.display()
            ));
        }
        let list = names
            .iter()
            .map(|n| format!("`{n}`"))
            .collect::<Vec<_>>()
            .join(", ");
        if !std::io::stdin().is_terminal() {
            return Err(format!(
                "adding `{}` would gobble {list}; remove them first with `athena remove <name>`",
                entry.name
            ));
        }
        say_inline!("Gobble {list} into `{}`? [y/N] ", entry.name);
        let _ = std::io::stdout().flush();
        let mut answer = String::new();
        let _ = std::io::stdin().read_line(&mut answer);
        if !install::confirmed(&answer) {
            return Err("gobble declined; nothing changed".into());
        }
        Ok(children)
    }

    // @spec CFG-STATE-004, CFG-STATE-006
    fn remove(&self, name: &str, force: bool) -> Result<bool, String> {
        let cfg = self.load(true)?;
        let entry = self.entry(&cfg, name)?;
        let in_error = self
            .state(&cfg)?
            .iter()
            .any(|e| e.involves(name, self.scope));
        let dest = install::observe_dest(&entry.resolved)?;
        match install::plan_remove(&dest, Self::clean(entry, &dest), in_error, force) {
            Removal::Refuse(r) => return Err(format!("cannot remove `{name}`: {r}")),
            Removal::DeleteAndEntry => fs::remove_dir_all(&entry.resolved)
                .map_err(|e| format!("cannot delete `{}`: {e}", entry.resolved.display()))?,
            Removal::EntryOnly => {}
        }
        let mut ed = Editor::open(&self.file())?;
        ed.remove(entry.kind, name);
        ed.save()?;
        if in_error {
            say!(
                "removed `{name}` from the config; files at `{}` left untouched",
                entry.resolved.display()
            );
        } else {
            say!("removed `{name}`");
        }
        Ok(true)
    }

    // @spec CMD-PIN-001, CMD-PIN-002, CMD-PIN-004
    fn pin(&self, name: &str, on: bool) -> Result<bool, String> {
        let cfg = self.load(true)?;
        let entry = self.entry(&cfg, name)?;
        let word = if on { "pinned" } else { "unpinned" };
        if entry.pinned == on {
            say!("`{name}` is already {word}; no change");
            return Ok(true);
        }
        let mut ed = Editor::open(&self.file())?;
        if on {
            ed.set(entry.kind, name, "pinned", true);
        } else {
            ed.unset(entry.kind, name, "pinned");
        }
        ed.save()?;
        say!("`{name}` {word}");
        Ok(true)
    }

    // @spec CFG-STATE-005
    fn load_warning(&self) -> Result<Config, String> {
        let cfg = self.load(true)?;
        for e in self.state(&cfg)? {
            warn(e);
        }
        Ok(cfg)
    }

    // @spec CMD-LIST-001, CMD-LIST-002, CMD-OUT-001
    fn list(&self, json: bool) -> Result<bool, String> {
        let cfg = self.load_warning()?;
        if json {
            let rows: Vec<_> = cfg.entries.iter().map(|e| row(e, None)).collect();
            say!("{}", serde_json::Value::Array(rows));
            return Ok(true);
        }
        let mut table = comfy_table::Table::new();
        table.set_header(["Name", "Kind", "Type", "Commit", "Path", "Source"]);
        for e in &cfg.entries {
            let commit: String = e
                .commit
                .as_deref()
                .unwrap_or("-")
                .chars()
                .take(12)
                .collect();
            table.add_row([
                e.name.as_str(),
                e.kind.label(),
                e.source_type.label(),
                &commit,
                &e.resolved.display().to_string(),
                &e.source,
            ]);
        }
        say!("{table}");
        Ok(true)
    }

    // @spec CMD-SHOW-001, CMD-SHOW-002, CMD-OUT-004
    fn show(&self, pattern: &str, json: bool) -> Result<bool, String> {
        let cfg = self.load_warning()?;
        let glob = WildMatch::new(pattern);
        let matches: Vec<&Entry> = cfg
            .entries
            .iter()
            .filter(|e| glob.matches(&e.name))
            .collect();
        if json {
            let rows: Vec<_> = matches.iter().map(|e| row(e, Some(status(e)))).collect();
            say!("{}", serde_json::Value::Array(rows));
        } else {
            for e in &matches {
                let opt = |v: &Option<String>| v.clone().unwrap_or_else(|| "-".into());
                say!("{}", e.name.bold());
                for (k, v) in [
                    ("kind", e.kind.label().to_string()),
                    ("type", e.source_type.label().to_string()),
                    ("source", e.source.clone()),
                    ("commit", opt(&e.commit)),
                    ("ref", opt(&e.git_ref)),
                    ("subpath", opt(&e.subpath)),
                    ("path", opt(&e.path)),
                    ("pinned", e.pinned.to_string()),
                    ("hash", opt(&e.hash)),
                    ("resolved path", e.resolved.display().to_string()),
                    ("status", status(e).to_string()),
                ] {
                    say!("  {k}: {v}");
                }
            }
        }
        if matches.is_empty() {
            shout!("{} no entries match `{pattern}`", "ERROR".red());
        }
        Ok(!matches.is_empty())
    }
}

fn status(e: &Entry) -> &'static str {
    match install::observe_dest(&e.resolved) {
        Ok(Dest::Absent) => "missing",
        Ok(Dest::Present { hash }) if e.hash.as_ref().is_none_or(|h| *h == hash) => "present",
        _ => "modified",
    }
}

fn row(e: &Entry, status: Option<&str>) -> serde_json::Value {
    let mut v = serde_json::json!({
        "name": e.name,
        "kind": e.kind.label(),
        "type": e.source_type.label(),
        "source": e.source,
        "commit": e.commit,
        "ref": e.git_ref,
        "subpath": e.subpath,
        "path": e.path,
        "pinned": e.pinned,
        "hash": e.hash,
        "resolved_path": e.resolved.display().to_string(),
    });
    if let Some(s) = status {
        v["status"] = s.into();
    }
    v
}

/// Real folders under `root`, as relative paths, at most `depth` components deep. Symlinks are not followed.
fn local_dirs(root: &Path, depth: usize) -> Result<Vec<String>, String> {
    fn walk(root: &Path, rel: &str, depth: usize, out: &mut Vec<String>) -> Result<(), String> {
        let dir = root.join(rel);
        let rd = fs::read_dir(&dir).map_err(|e| format!("cannot read `{}`: {e}", dir.display()))?;
        for entry in rd.flatten() {
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                let name = entry.file_name().to_string_lossy().into_owned();
                let child = if rel.is_empty() {
                    name
                } else {
                    format!("{rel}/{name}")
                };
                if depth > 1 {
                    walk(root, &child, depth - 1, out)?;
                }
                out.push(child);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, "", depth, &mut out)?;
    out.sort();
    Ok(out)
}

/// Whether `dir` holds a file named exactly `SKILL.md` (a symlink to a file counts).
fn has_skill_md(dir: &Path) -> bool {
    fs::read_dir(dir).is_ok_and(|rd| {
        rd.flatten()
            .any(|e| e.file_name() == "SKILL.md" && e.path().is_file())
    })
}

/// Relative paths of every exact-case `SKILL.md` under `dir`.
fn skill_md_files(dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                stack.push(p);
            } else if e.file_name() == "SKILL.md" && p.is_file() {
                found.push(p.strip_prefix(dir).unwrap().to_string_lossy().into_owned());
            }
        }
    }
    found.sort();
    found
}

/// Refuses skills whose staged folder has no `SKILL.md` at its root, explaining how to proceed.
// @spec CMD-SKILL-001, CMD-SKILL-002, CMD-SKILL-003, CMD-SKILL-004, CMD-SKILL-005
fn skill_layout(
    input: &str,
    parsed: &source::ParsedSource,
    pattern: bool,
    entries: &[Entry],
    prepared: &[Prepared],
) -> Result<(), String> {
    let failing: Vec<(&Entry, &Prepared)> = entries
        .iter()
        .zip(prepared)
        .filter(|(_, p)| !has_skill_md(&p.staging))
        .collect();
    if failing.is_empty() {
        return Ok(());
    }
    let names = failing
        .iter()
        .map(|(e, _)| format!("`{}`", e.name))
        .collect::<Vec<_>>()
        .join(", ");
    let mut msg = format!("no `SKILL.md` at the root of {names}");
    if pattern {
        msg.push_str("; use braces such as `{a,b}` in the pattern to select only the folders that are skills");
    } else if let [(_, p)] = failing.as_slice() {
        let found = skill_md_files(&p.staging);
        let parents: Vec<&str> = found
            .iter()
            .filter_map(|f| Path::new(f).parent()?.parent()?.to_str())
            .collect();
        if !found.is_empty()
            && parents.len() == found.len()
            && parents.iter().all(|d| *d == parents[0])
        {
            let base = input.split('#').next().unwrap_or(input);
            let glob = [parsed.subpath.as_deref().unwrap_or(""), parents[0], "*"]
                .iter()
                .filter(|s| !s.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join("/");
            msg.push_str(&format!(
                "; it looks like a collection of skills, so add them with `{base}#{}:/{glob}`",
                parsed.reference.as_deref().unwrap_or("")
            ));
        } else if !found.is_empty() {
            msg.push_str(&format!(
                "; `SKILL.md` files found at: {}",
                found.join(", ")
            ));
        }
    }
    msg.push_str("; pass `--no-skill-check` to install as they are");
    Err(msg)
}
