use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use toml_edit::{DocumentMut, Item, Table, TableLike};

/// Process-level locations every config operation resolves against.
#[derive(Debug, Clone)]
pub struct Env {
    pub cwd: PathBuf,
    pub home: PathBuf,
    pub config_home: PathBuf,
    pub cache_home: PathBuf,
}

impl Env {
    // @spec CFG-LOC-002
    pub fn from_process() -> Result<Env, String> {
        let cwd = std::env::current_dir()
            .map_err(|e| format!("cannot read the current directory: {e}"))?;
        let home = std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from)
            .ok_or("HOME is not set")?;
        let xdg = |var: &str, fallback: &str| {
            std::env::var_os(var)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(fallback))
        };
        Ok(Env {
            config_home: xdg("XDG_CONFIG_HOME", ".config"),
            cache_home: xdg("XDG_CACHE_HOME", ".cache"),
            cwd,
            home,
        })
    }

    // @spec CFG-LOC-001
    pub fn local_file(&self) -> PathBuf {
        self.cwd.join("athena.toml")
    }

    pub fn global_file(&self) -> PathBuf {
        self.config_home.join("athena/config.toml")
    }

    /// `<cache_home>/athena`
    pub fn cache_dir(&self) -> PathBuf {
        self.cache_home.join("athena")
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Scope {
    Local,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Skill,
    Plugin,
    Rule,
    Agent,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Skill, Kind::Plugin, Kind::Rule, Kind::Agent];

    pub fn table(self) -> &'static str {
        match self {
            Kind::Skill => "skills",
            Kind::Plugin => "plugins",
            Kind::Rule => "rules",
            Kind::Agent => "agents",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Skill => "skill",
            Kind::Plugin => "plugin",
            Kind::Rule => "rule",
            Kind::Agent => "agent",
        }
    }

    /// Rules and agents install one `.md` file; skills and plugins install a folder.
    pub fn is_file(self) -> bool {
        matches!(self, Kind::Rule | Kind::Agent)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SourceType {
    Remote,
    Local,
}

impl SourceType {
    pub fn label(self) -> &'static str {
        match self {
            SourceType::Remote => "remote",
            SourceType::Local => "local",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub name: String,
    pub kind: Kind,
    pub source_type: SourceType,
    pub source: String,
    pub commit: Option<String>,
    pub git_ref: Option<String>,
    pub subpath: Option<String>,
    pub path: Option<String>,
    pub pinned: bool,
    pub hash: Option<String>,
    pub resolved: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub scope: Scope,
    pub file: PathBuf,
    pub skill_dir: PathBuf,
    pub plugin_dir: PathBuf,
    pub rule_dir: PathBuf,
    pub agent_dir: PathBuf,
    pub entries: Vec<Entry>,
}

impl Config {
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name)
    }

    fn dir(&self) -> &Path {
        self.file.parent().unwrap_or(Path::new("/"))
    }

    /// Resolves and validates the destination for an entry of this config.
    // @spec CFG-PATH-001, CFG-PATH-002, CFG-PATH-003, CFG-PATH-004, CFG-PATH-005
    pub fn resolve_path(
        &self,
        kind: Kind,
        name: &str,
        path: Option<&str>,
        env: &Env,
    ) -> Result<PathBuf, String> {
        let resolved = match (self.scope, path) {
            (Scope::Global, None) => return Err(format!("global entry `{name}` needs a `path`")),
            (Scope::Global, Some(p)) if !(p.starts_with('/') || p.starts_with('~')) => {
                return Err(format!(
                    "global entry `{name}` has relative path `{p}`; use an absolute or `~/` path"
                ));
            }
            (_, Some(p)) => normalize(&self.dir().join(expand_home(p, &env.home))),
            (Scope::Local, None) => {
                let base = match kind {
                    Kind::Skill => &self.skill_dir,
                    Kind::Plugin => &self.plugin_dir,
                    Kind::Rule => &self.rule_dir,
                    Kind::Agent => &self.agent_dir,
                };
                base.join(name.rsplit('/').next().unwrap_or(name))
            }
        };
        let protected = [Path::new("/"), env.home.as_path(), self.dir()];
        if protected.iter().any(|p| p.starts_with(&resolved))
            || resolved.starts_with(env.cache_dir())
        {
            return Err(format!(
                "entry `{name}` resolves to `{}`, which athena must never manage (root, home, config directory, or cache)",
                resolved.display()
            ));
        }
        Ok(resolved)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StateError {
    Collision {
        a: String,
        b: String,
        path: PathBuf,
        default_path: bool,
    },
    Nesting {
        parent: String,
        child: String,
    },
    Conflict {
        local: String,
        local_path: PathBuf,
        global: String,
        global_path: PathBuf,
    },
}

impl StateError {
    /// Whether the named entry of the given scope takes part in this error.
    pub fn involves(&self, name: &str, scope: Scope) -> bool {
        match self {
            StateError::Collision { a, b, .. } => a == name || b == name,
            StateError::Nesting { parent, child } => parent == name || child == name,
            StateError::Conflict { local, global, .. } => match scope {
                Scope::Local => local == name,
                Scope::Global => global == name,
            },
        }
    }
}

// @spec CFG-CONF-003
impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateError::Collision {
                a,
                b,
                path,
                default_path,
            } => {
                write!(
                    f,
                    "entries `{a}` and `{b}` both install to `{}` with different versions",
                    path.display()
                )?;
                if *default_path {
                    write!(
                        f,
                        "; give one a different location with `--name` or `--path`"
                    )?;
                }
                Ok(())
            }
            StateError::Nesting { parent, child } => {
                write!(
                    f,
                    "entry `{child}` is installed inside entry `{parent}`; remove one of them"
                )
            }
            StateError::Conflict {
                local,
                local_path,
                global,
                global_path,
            } => write!(
                f,
                "local entry `{local}` (`{}`) overlaps global entry `{global}` (`{}`); remove one of them by hand or with `athena remove {local}` or `athena remove -g {global}`",
                local_path.display(),
                global_path.display()
            ),
        }
    }
}

// @spec CFG-SCHEMA-008
pub fn is_locked(commit: &str) -> bool {
    commit.len() == 40 && commit.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Lexical normalization: removes `.`, collapses `..`, never touches the filesystem.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() && !out.has_root() {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    out
}

pub fn expand_home(path: &str, home: &Path) -> PathBuf {
    match path.strip_prefix('~') {
        Some("") => home.to_path_buf(),
        Some(rest) if rest.starts_with('/') => home.join(rest.trim_start_matches('/')),
        _ => PathBuf::from(path),
    }
}

/// Reads and parses a config file. A missing file yields an empty config.
pub fn load(scope: Scope, file: &Path, env: &Env) -> Result<Config, String> {
    let text = match fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("cannot read `{}`: {e}", file.display())),
    };
    parse(&text, scope, file, env)
}

const ENTRY_KEYS: [&str; 8] = [
    "type", "source", "commit", "ref", "subpath", "path", "pinned", "hash",
];

// @spec CFG-SCHEMA-001, CFG-SCHEMA-002, CFG-SCHEMA-003, CFG-SCHEMA-004, CFG-SCHEMA-005, CFG-SCHEMA-006, CFG-SCHEMA-007, CFG-SCHEMA-009, CFG-SCHEMA-010
pub fn parse(text: &str, scope: Scope, file: &Path, env: &Env) -> Result<Config, String> {
    let doc: DocumentMut = text
        .parse()
        .map_err(|e| format!("invalid TOML in `{}`: {e}", file.display()))?;
    let dir = file.parent().unwrap_or(Path::new("/"));
    let mut core = [
        String::from("./.agents/skills"),
        String::from("./.agents/plugins"),
        String::from("./.claude/rules"),
        String::from("./.claude/agents"),
    ];

    for (key, item) in doc.iter() {
        match key {
            "skills" | "plugins" | "rules" | "agents" => {}
            "core" if scope == Scope::Global => {
                return Err(format!(
                    "`[core]` is local-only; remove it from `{}`",
                    file.display()
                ));
            }
            "core" => {
                let table = item.as_table_like().ok_or("`core` must be a table")?;
                for (k, v) in table.iter() {
                    let slot = match k {
                        "default_skill_path" => &mut core[0],
                        "default_plugin_path" => &mut core[1],
                        "default_rule_path" => &mut core[2],
                        "default_agent_path" => &mut core[3],
                        _ => {
                            return Err(format!(
                                "unknown key `{k}` in [core] of `{}`",
                                file.display()
                            ));
                        }
                    };
                    *slot = v
                        .as_str()
                        .ok_or_else(|| format!("[core] `{k}` must be a string"))?
                        .to_string();
                }
            }
            _ => return Err(format!("unknown table `{key}` in `{}`", file.display())),
        }
    }
    let base = |p: &str| normalize(&dir.join(expand_home(p, &env.home)));
    let mut config = Config {
        scope,
        file: file.to_path_buf(),
        skill_dir: base(&core[0]),
        plugin_dir: base(&core[1]),
        rule_dir: base(&core[2]),
        agent_dir: base(&core[3]),
        entries: Vec::new(),
    };

    for kind in Kind::ALL {
        let Some(item) = doc.get(kind.table()) else {
            continue;
        };
        let table = item
            .as_table_like()
            .ok_or_else(|| format!("`{}` must be a table", kind.table()))?;
        for (name, item) in table.iter() {
            let t = item
                .as_table_like()
                .ok_or_else(|| format!("`{}.{name}` must be a table", kind.table()))?;
            if config.get(name).is_some() {
                return Err(format!(
                    "`{name}` appears under more than one of [skills], [plugins], [rules], and [agents] in `{}`",
                    file.display()
                ));
            }
            let entry = parse_entry(&config, kind, name, t, env)?;
            config.entries.push(entry);
        }
    }
    Ok(config)
}

fn parse_entry(
    config: &Config,
    kind: Kind,
    name: &str,
    t: &dyn TableLike,
    env: &Env,
) -> Result<Entry, String> {
    let at = format!("[{}.\"{name}\"]", kind.table());
    if let Some((k, _)) = t.iter().find(|(k, _)| !ENTRY_KEYS.contains(k)) {
        return Err(format!("unknown key `{k}` in {at}"));
    }
    let string = |key: &str| -> Result<Option<String>, String> {
        t.get(key)
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| format!("`{key}` in {at} must be a string"))
            })
            .transpose()
    };
    let source_type = match string("type")?.as_deref() {
        Some("remote") => SourceType::Remote,
        Some("local") => SourceType::Local,
        Some(other) => {
            return Err(format!(
                "entry `{name}`: `type` must be \"remote\" or \"local\", not \"{other}\""
            ));
        }
        None => return Err(format!("entry `{name}` is missing `type`")),
    };
    let source = string("source")?.ok_or_else(|| format!("entry `{name}` is missing `source`"))?;
    let commit = string("commit")?;
    let git_ref = string("ref")?;
    match source_type {
        SourceType::Remote if commit.is_none() => {
            return Err(format!("remote entry `{name}` is missing `commit`"));
        }
        SourceType::Local if commit.is_some() || git_ref.is_some() => {
            return Err(format!(
                "local entry `{name}` must not have `commit` or `ref`"
            ));
        }
        _ => {}
    }
    let pinned = match t.get("pinned") {
        None => false,
        Some(v) => v
            .as_bool()
            .ok_or_else(|| format!("`pinned` in {at} must be true or false"))?,
    };
    let subpath = string("subpath")?;
    if kind.is_file() && source_type == SourceType::Remote && subpath.is_none() {
        return Err(format!(
            "remote {} `{name}` needs a `subpath` naming its file",
            kind.label()
        ));
    }
    let path = string("path")?;
    let resolved = config.resolve_path(kind, name, path.as_deref(), env)?;
    Ok(Entry {
        name: name.to_string(),
        kind,
        source_type,
        source,
        commit,
        git_ref,
        subpath,
        path,
        pinned,
        hash: string("hash")?,
        resolved,
    })
}

// @spec CFG-VAL-001, CFG-VAL-002, CFG-VAL-003, CFG-STATE-001, CFG-STATE-002, CFG-CONF-001
pub fn state_errors(config: &Config, global: Option<&Config>) -> Vec<StateError> {
    let mut errors = Vec::new();
    for (i, a) in config.entries.iter().enumerate() {
        for b in &config.entries[i + 1..] {
            if a.resolved == b.resolved {
                if (&a.source, &a.commit, &a.subpath) != (&b.source, &b.commit, &b.subpath) {
                    errors.push(StateError::Collision {
                        a: a.name.clone(),
                        b: b.name.clone(),
                        path: a.resolved.clone(),
                        default_path: a.path.is_none() || b.path.is_none(),
                    });
                }
            } else if b.resolved.starts_with(&a.resolved) {
                errors.push(StateError::Nesting {
                    parent: a.name.clone(),
                    child: b.name.clone(),
                });
            } else if a.resolved.starts_with(&b.resolved) {
                errors.push(StateError::Nesting {
                    parent: b.name.clone(),
                    child: a.name.clone(),
                });
            }
        }
    }
    if let (Scope::Local, Some(global)) = (config.scope, global) {
        for l in &config.entries {
            for g in &global.entries {
                if l.resolved.starts_with(&g.resolved) || g.resolved.starts_with(&l.resolved) {
                    errors.push(StateError::Conflict {
                        local: l.name.clone(),
                        local_path: l.resolved.clone(),
                        global: g.name.clone(),
                        global_path: g.resolved.clone(),
                    });
                }
            }
        }
    }
    errors
}

/// Format-preserving writer for one config file.
pub struct Editor {
    file: PathBuf,
    doc: DocumentMut,
}

impl Editor {
    pub fn open(file: &Path) -> Result<Editor, String> {
        let text = match fs::read_to_string(file) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(format!("cannot read `{}`: {e}", file.display())),
        };
        let doc = text
            .parse()
            .map_err(|e| format!("invalid TOML in `{}`: {e}", file.display()))?;
        Ok(Editor {
            file: file.to_path_buf(),
            doc,
        })
    }

    fn entry(&mut self, kind: Kind, name: &str) -> Option<&mut dyn TableLike> {
        self.doc
            .get_mut(kind.table())?
            .get_mut(name)?
            .as_table_like_mut()
    }

    pub fn insert(&mut self, entry: &Entry) {
        let tables = self.doc.entry(entry.kind.table()).or_insert_with(|| {
            let mut t = Table::new();
            t.set_implicit(true);
            Item::Table(t)
        });
        let mut t = Table::new();
        t["type"] = toml_edit::value(entry.source_type.label());
        t["source"] = toml_edit::value(&entry.source);
        let optional = [
            ("commit", &entry.commit),
            ("ref", &entry.git_ref),
            ("subpath", &entry.subpath),
            ("path", &entry.path),
        ];
        for (k, v) in optional {
            if let Some(v) = v {
                t[k] = toml_edit::value(v);
            }
        }
        if entry.pinned {
            t["pinned"] = toml_edit::value(true);
        }
        if let Some(h) = &entry.hash {
            t["hash"] = toml_edit::value(h);
        }
        tables[&entry.name] = Item::Table(t);
    }

    pub fn remove(&mut self, kind: Kind, name: &str) {
        if let Some(t) = self
            .doc
            .get_mut(kind.table())
            .and_then(Item::as_table_like_mut)
        {
            t.remove(name);
        }
    }

    pub fn set(&mut self, kind: Kind, name: &str, key: &str, value: impl Into<toml_edit::Value>) {
        if let Some(t) = self.entry(kind, name) {
            t.insert(key, Item::Value(value.into()));
        }
    }

    pub fn unset(&mut self, kind: Kind, name: &str, key: &str) {
        if let Some(t) = self.entry(kind, name) {
            t.remove(key);
        }
    }

    // @spec CFG-WRITE-001, CFG-WRITE-002, CFG-WRITE-003
    pub fn save(&self) -> Result<(), String> {
        let dir = self.file.parent().unwrap_or(Path::new("/"));
        fs::create_dir_all(dir).map_err(|e| format!("cannot create `{}`: {e}", dir.display()))?;
        let tmp = dir.join(format!(".athena-config-{}.tmp", std::process::id()));
        fs::write(&tmp, self.doc.to_string())
            .and_then(|_| fs::rename(&tmp, &self.file))
            .map_err(|e| {
                let _ = fs::remove_file(&tmp);
                format!("cannot write `{}`: {e}", self.file.display())
            })
    }
}
