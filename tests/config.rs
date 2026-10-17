mod common;

use athena::config::{self, Config, Editor, Entry, Env, Kind, Scope, SourceType, StateError};
use common::TempDir;
use std::fs;
use std::path::{Path, PathBuf};

const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn env() -> Env {
    Env {
        cwd: PathBuf::from("/p/home/proj"),
        home: PathBuf::from("/p/home"),
        config_home: PathBuf::from("/p/home/.config"),
        cache_home: PathBuf::from("/p/home/.cache"),
    }
}

fn local(text: &str) -> Result<Config, String> {
    config::parse(
        text,
        Scope::Local,
        Path::new("/p/home/proj/athena.toml"),
        &env(),
    )
}

fn global(text: &str) -> Result<Config, String> {
    config::parse(
        text,
        Scope::Global,
        Path::new("/p/home/.config/athena/config.toml"),
        &env(),
    )
}

fn resolved(cfg: &Config, name: &str) -> PathBuf {
    cfg.get(name).unwrap().resolved.clone()
}

// @spec CFG-LOC-001, CFG-LOC-002
#[test]
fn config_file_locations() {
    assert_eq!(
        env().local_file(),
        PathBuf::from("/p/home/proj/athena.toml")
    );
    assert_eq!(
        env().global_file(),
        PathBuf::from("/p/home/.config/athena/config.toml")
    );
    assert_eq!(env().cache_dir(), PathBuf::from("/p/home/.cache/athena"));
}

// @spec CFG-SCHEMA-001
#[test]
fn accepts_all_schema_keys() {
    let cfg = local(&format!(
        r#"
[core]
default_skill_path = "./.agents/skills"
default_plugin_path = "./.agents/plugins"

[skills."obra/superpowers"]
source = "https://github.com/obra/superpowers.git"
commit = "{SHA_A}"
ref = "main"
type = "remote"
pinned = true
hash = "sha256:00"

[plugins."mcp-postgres"]
source = "https://github.com/modelcontextprotocol/servers.git"
commit = "HEAD"
type = "remote"
subpath = "src/postgres"
path = "./.custom/bin/postgres-plugin"

[plugins."local-linter"]
source = "/Users/dev/my-local-plugins/linter"
type = "local"
path = "./.custom/bin/linter"
"#
    ))
    .unwrap();
    let sp = cfg.get("obra/superpowers").unwrap();
    assert_eq!(sp.kind, Kind::Skill);
    assert_eq!(sp.source_type, SourceType::Remote);
    assert_eq!(sp.commit.as_deref(), Some(SHA_A));
    assert_eq!(sp.git_ref.as_deref(), Some("main"));
    assert!(sp.pinned);
    assert_eq!(sp.hash.as_deref(), Some("sha256:00"));
    let pg = cfg.get("mcp-postgres").unwrap();
    assert_eq!(pg.kind, Kind::Plugin);
    assert_eq!(pg.subpath.as_deref(), Some("src/postgres"));
    assert_eq!(pg.path.as_deref(), Some("./.custom/bin/postgres-plugin"));
    assert_eq!(
        cfg.get("local-linter").unwrap().source_type,
        SourceType::Local
    );
    assert_eq!(cfg.entries.len(), 3);
}

// @spec CFG-SCHEMA-002
#[test]
fn unknown_keys_are_load_errors() {
    let e = local("[skills.x]\ntype = \"local\"\nsource = \"/s\"\ncolour = 1\n").unwrap_err();
    assert!(e.contains("colour") && e.contains("skills"), "{e}");
    let e = local("[core]\ndefault_agent_path = \"x\"\n").unwrap_err();
    assert!(
        e.contains("default_agent_path") && e.contains("core"),
        "{e}"
    );
    let e = global("[themes.x]\n").unwrap_err();
    assert!(e.contains("themes"), "{e}");
}

// @spec CFG-SCHEMA-003
#[test]
fn name_in_both_tables_is_a_load_error() {
    let e = local("[skills.dup]\ntype = \"local\"\nsource = \"/s\"\n[plugins.dup]\ntype = \"local\"\nsource = \"/t\"\n").unwrap_err();
    assert!(e.contains("dup"), "{e}");
}

// @spec CFG-SCHEMA-004
#[test]
fn missing_or_bad_type_or_source_is_a_load_error() {
    assert!(
        local("[skills.a]\nsource = \"/s\"\n")
            .unwrap_err()
            .contains("a")
    );
    assert!(
        local("[skills.b]\ntype = \"local\"\n")
            .unwrap_err()
            .contains("b")
    );
    assert!(
        local("[skills.c]\ntype = \"svn\"\nsource = \"/s\"\n")
            .unwrap_err()
            .contains("c")
    );
}

// @spec CFG-SCHEMA-005
#[test]
fn commit_rules_by_type() {
    assert!(
        local("[skills.r]\ntype = \"remote\"\nsource = \"https://h.example/o/r.git\"\n")
            .unwrap_err()
            .contains("r")
    );
    assert!(
        local(&format!(
            "[skills.l]\ntype = \"local\"\nsource = \"/s\"\ncommit = \"{SHA_A}\"\n"
        ))
        .unwrap_err()
        .contains("l")
    );
    assert!(
        local("[skills.m]\ntype = \"local\"\nsource = \"/s\"\nref = \"main\"\n")
            .unwrap_err()
            .contains("m")
    );
}

// @spec CFG-SCHEMA-006
#[test]
fn core_is_local_only() {
    let e = global("[core]\ndefault_skill_path = \"~/s\"\n").unwrap_err();
    assert!(e.contains("[core]"), "{e}");
}

// @spec CFG-SCHEMA-007
#[test]
fn default_paths_apply_when_core_is_missing_or_partial() {
    let cfg = local("").unwrap();
    assert_eq!(cfg.skill_dir, PathBuf::from("/p/home/proj/.agents/skills"));
    assert_eq!(
        cfg.plugin_dir,
        PathBuf::from("/p/home/proj/.agents/plugins")
    );
    let cfg = local("[core]\ndefault_skill_path = \"./ai/skills\"\n").unwrap();
    assert_eq!(cfg.skill_dir, PathBuf::from("/p/home/proj/ai/skills"));
    assert_eq!(
        cfg.plugin_dir,
        PathBuf::from("/p/home/proj/.agents/plugins")
    );
}

// @spec CFG-SCHEMA-008
#[test]
fn locked_commit_is_forty_hex() {
    assert!(config::is_locked(SHA_A));
    assert!(config::is_locked(
        "0123456789abcdef0123456789ABCDEF01234567"
    ));
    assert!(!config::is_locked("HEAD"));
    assert!(!config::is_locked("main"));
    assert!(!config::is_locked(&SHA_A[..39]));
    assert!(!config::is_locked(&format!("{SHA_A}a")));
    assert!(!config::is_locked(
        "gggggggggggggggggggggggggggggggggggggggg"
    ));
}

// @spec CFG-SCHEMA-009
#[test]
fn pinned_defaults_to_false() {
    let cfg = local("[skills.a]\ntype = \"local\"\nsource = \"/s\"\n").unwrap();
    assert!(!cfg.get("a").unwrap().pinned);
}

// @spec CFG-PATH-001
#[test]
fn local_path_is_relative_to_config_dir() {
    let cfg =
        local("[skills.a]\ntype = \"local\"\nsource = \"/s\"\npath = \"./custom/a\"\n").unwrap();
    assert_eq!(resolved(&cfg, "a"), PathBuf::from("/p/home/proj/custom/a"));
}

// @spec CFG-PATH-002
#[test]
fn local_default_path_uses_last_name_component() {
    let cfg = local(&format!(
        "[core]\ndefault_plugin_path = \"./ai/plugins\"\n[skills.\"obra/superpowers\"]\ntype = \"remote\"\nsource = \"https://h.example/obra/superpowers.git\"\ncommit = \"{SHA_A}\"\n[plugins.lint]\ntype = \"local\"\nsource = \"/s\"\n"
    ))
    .unwrap();
    assert_eq!(
        resolved(&cfg, "obra/superpowers"),
        PathBuf::from("/p/home/proj/.agents/skills/superpowers")
    );
    assert_eq!(
        resolved(&cfg, "lint"),
        PathBuf::from("/p/home/proj/ai/plugins/lint")
    );
}

// @spec CFG-PATH-003
#[test]
fn global_entries_need_absolute_or_home_path() {
    assert!(
        global("[skills.a]\ntype = \"local\"\nsource = \"/s\"\n")
            .unwrap_err()
            .contains("a")
    );
    assert!(
        global("[skills.b]\ntype = \"local\"\nsource = \"/s\"\npath = \"rel/b\"\n")
            .unwrap_err()
            .contains("b")
    );
    let cfg = global("[skills.c]\ntype = \"local\"\nsource = \"/s\"\npath = \"~/skills/c\"\n[skills.d]\ntype = \"local\"\nsource = \"/s\"\npath = \"/opt/d\"\n").unwrap();
    assert_eq!(resolved(&cfg, "c"), PathBuf::from("/p/home/skills/c"));
    assert_eq!(resolved(&cfg, "d"), PathBuf::from("/opt/d"));
}

// @spec CFG-PATH-004
#[test]
fn paths_expand_home_and_normalize_lexically() {
    assert_eq!(
        config::normalize(Path::new("/a/./b/../c")),
        PathBuf::from("/a/c")
    );
    assert_eq!(
        config::normalize(Path::new("/a/b/../../..")),
        PathBuf::from("/")
    );
    assert_eq!(
        config::expand_home("~/x", Path::new("/h")),
        PathBuf::from("/h/x")
    );
    assert_eq!(
        config::expand_home("~", Path::new("/h")),
        PathBuf::from("/h")
    );
    assert_eq!(
        config::expand_home("/abs/~x", Path::new("/h")),
        PathBuf::from("/abs/~x")
    );
    let cfg =
        global("[skills.a]\ntype = \"local\"\nsource = \"/s\"\npath = \"~/x/./y/../z\"\n").unwrap();
    assert_eq!(resolved(&cfg, "a"), PathBuf::from("/p/home/x/z"));
}

// @spec CFG-PATH-005
#[test]
fn dangerous_paths_are_load_errors() {
    for p in [
        "/",
        "~",
        "/p",
        "~/.cache/athena/remotes/x",
        "~/.cache/athena",
    ] {
        let e = global(&format!(
            "[skills.bad]\ntype = \"local\"\nsource = \"/s\"\npath = \"{p}\"\n"
        ))
        .unwrap_err();
        assert!(e.contains("bad"), "{p}: {e}");
    }
    assert!(
        global("[skills.cfgdir]\ntype = \"local\"\nsource = \"/s\"\npath = \"~/.config/athena\"\n")
            .is_err()
    );
    for p in [".", "..", "./"] {
        let e = local(&format!(
            "[skills.bad]\ntype = \"local\"\nsource = \"/s\"\npath = \"{p}\"\n"
        ))
        .unwrap_err();
        assert!(e.contains("bad"), "{p}: {e}");
    }
}

fn entry_toml(name: &str, source: &str, commit: &str, path: Option<&str>) -> String {
    let mut s = format!(
        "[skills.\"{name}\"]\ntype = \"remote\"\nsource = \"{source}\"\ncommit = \"{commit}\"\n"
    );
    if let Some(p) = path {
        s.push_str(&format!("path = \"{p}\"\n"));
    }
    s
}

// @spec CFG-VAL-001
#[test]
fn same_path_with_different_versions_is_a_state_error() {
    let cfg = local(
        &(entry_toml("a", "https://h.example/o/a.git", SHA_A, Some("./x"))
            + &entry_toml("b", "https://h.example/o/a.git", SHA_B, Some("./x"))),
    )
    .unwrap();
    let errs = config::state_errors(&cfg, None);
    assert_eq!(errs.len(), 1);
    let msg = errs[0].to_string();
    assert!(
        msg.contains('a') && msg.contains('b') && msg.contains("/p/home/proj/x"),
        "{msg}"
    );
    assert!(
        !msg.contains("--name"),
        "explicit paths need no naming hint: {msg}"
    );

    let cfg = local(
        &(entry_toml("one/tool", "https://h.example/one/tool.git", SHA_A, None)
            + &entry_toml("two/tool", "https://h.example/two/tool.git", SHA_A, None)),
    )
    .unwrap();
    let errs = config::state_errors(&cfg, None);
    assert!(
        matches!(
            &errs[..],
            [StateError::Collision {
                default_path: true,
                ..
            }]
        ),
        "{errs:?}"
    );
    let msg = errs[0].to_string();
    assert!(msg.contains("--name") && msg.contains("--path"), "{msg}");
}

// @spec CFG-VAL-002
#[test]
fn identical_duplicates_are_not_state_errors() {
    let cfg = local(
        &(entry_toml("a", "https://h.example/o/a.git", SHA_A, Some("./x"))
            + &entry_toml("b", "https://h.example/o/a.git", SHA_A, Some("./x"))),
    )
    .unwrap();
    assert!(config::state_errors(&cfg, None).is_empty());
}

// @spec CFG-VAL-003
#[test]
fn nested_entries_are_state_errors() {
    let cfg = local(
        &(entry_toml(
            "parent",
            "https://h.example/o/p.git",
            SHA_A,
            Some("./tools"),
        ) + &entry_toml(
            "child",
            "https://h.example/o/c.git",
            SHA_A,
            Some("./tools/c"),
        )),
    )
    .unwrap();
    let errs = config::state_errors(&cfg, None);
    assert_eq!(
        errs,
        vec![StateError::Nesting {
            parent: "parent".into(),
            child: "child".into()
        }]
    );
    let msg = errs[0].to_string();
    assert!(msg.contains("parent") && msg.contains("child"), "{msg}");
    // Sibling prefixes are not nesting.
    let cfg = local(
        &(entry_toml("a", "https://h.example/o/p.git", SHA_A, Some("./tool"))
            + &entry_toml("b", "https://h.example/o/c.git", SHA_A, Some("./tools"))),
    )
    .unwrap();
    assert!(config::state_errors(&cfg, None).is_empty());
}

// @spec CFG-STATE-001, CFG-STATE-002, CFG-CONF-001
#[test]
fn scope_decides_which_state_errors_are_checked() {
    let g = global(
        &(entry_toml(
            "g-skill",
            "https://h.example/o/g.git",
            SHA_A,
            Some("~/project-shared/skills/x"),
        ) + &entry_toml(
            "g-other",
            "https://h.example/o/o.git",
            SHA_A,
            Some("~/elsewhere"),
        )),
    )
    .unwrap();
    assert!(
        config::state_errors(&g, None).is_empty(),
        "global alone is consistent"
    );

    let env2 = Env {
        cwd: PathBuf::from("/p/home/project-shared"),
        ..env()
    };
    let l = config::parse(
        &(entry_toml(
            "equal",
            "https://h.example/o/l.git",
            SHA_A,
            Some("./skills/x"),
        ) + &entry_toml(
            "contains",
            "https://h.example/o/m.git",
            SHA_A,
            Some("./skills2"),
        )),
        Scope::Local,
        Path::new("/p/home/project-shared/athena.toml"),
        &env2,
    )
    .unwrap();
    let errs = config::state_errors(&l, Some(&g));
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(
        matches!(&errs[0], StateError::Conflict { local, global, .. } if local == "equal" && global == "g-skill")
    );

    let l2 = config::parse(
        &(entry_toml(
            "parent",
            "https://h.example/o/l.git",
            SHA_A,
            Some("./skills"),
        ) + &entry_toml(
            "child",
            "https://h.example/o/m.git",
            SHA_A,
            Some("./skills/x/inner"),
        )),
        Scope::Local,
        Path::new("/p/home/project-shared/athena.toml"),
        &env2,
    )
    .unwrap();
    let errs = config::state_errors(&l2, Some(&g));
    let conflicts = errs
        .iter()
        .filter(|e| matches!(e, StateError::Conflict { .. }))
        .count();
    assert_eq!(
        conflicts, 2,
        "contains and contained-by both conflict: {errs:?}"
    );
    assert!(
        errs.iter().any(|e| matches!(e, StateError::Nesting { .. })),
        "local nesting still checked: {errs:?}"
    );
}

// @spec CFG-CONF-003
#[test]
fn conflict_message_names_both_sides_and_the_fix() {
    let e = StateError::Conflict {
        local: "mine".into(),
        local_path: PathBuf::from("/p/x"),
        global: "theirs".into(),
        global_path: PathBuf::from("/p/x"),
    };
    let msg = e.to_string();
    for needle in [
        "mine",
        "theirs",
        "/p/x",
        "athena remove mine",
        "athena remove -g theirs",
    ] {
        assert!(msg.contains(needle), "missing {needle:?} in {msg}");
    }
    assert!(e.involves("mine", Scope::Local) && e.involves("theirs", Scope::Global));
    assert!(!e.involves("mine", Scope::Global));
}

fn sample_entry() -> Entry {
    Entry {
        name: "obra/superpowers".into(),
        kind: Kind::Skill,
        source_type: SourceType::Remote,
        source: "https://github.com/obra/superpowers.git".into(),
        commit: Some(SHA_A.into()),
        git_ref: Some("main".into()),
        subpath: None,
        path: None,
        pinned: false,
        hash: Some("sha256:ab".into()),
        resolved: PathBuf::new(),
    }
}

// @spec CFG-WRITE-001
#[test]
fn edits_preserve_comments_and_formatting() {
    let tmp = TempDir::new();
    let file = tmp.path().join("athena.toml");
    let original = "# my skills\n[core]\ndefault_skill_path = \"./s\"   # trailing note\n\n[skills.keep]\ntype = \"local\"\nsource = \"/keep\"  # keep me\n\n[skills.gone]\ntype = \"local\"\nsource = \"/gone\"\n";
    fs::write(&file, original).unwrap();
    let mut ed = Editor::open(&file).unwrap();
    ed.remove(Kind::Skill, "gone");
    ed.set(Kind::Skill, "keep", "hash", "sha256:01");
    ed.insert(&sample_entry());
    ed.save().unwrap();
    let text = fs::read_to_string(&file).unwrap();
    assert!(
        text.starts_with("# my skills\n[core]\ndefault_skill_path = \"./s\"   # trailing note\n"),
        "{text}"
    );
    assert!(text.contains("source = \"/keep\"  # keep me"), "{text}");
    assert!(!text.contains("gone"), "{text}");
    let cfg = config::parse(
        &text,
        Scope::Local,
        &file,
        &Env {
            cwd: tmp.path().into(),
            home: "/h".into(),
            config_home: "/h/c".into(),
            cache_home: "/h/k".into(),
        },
    )
    .unwrap();
    assert_eq!(cfg.get("keep").unwrap().hash.as_deref(), Some("sha256:01"));
    let added = cfg.get("obra/superpowers").unwrap();
    assert_eq!(added.git_ref.as_deref(), Some("main"));
    assert_eq!(added.path, None);

    let mut ed = Editor::open(&file).unwrap();
    ed.set(Kind::Skill, "keep", "pinned", true);
    ed.save().unwrap();
    assert!(fs::read_to_string(&file).unwrap().contains("pinned = true"));
    let mut ed = Editor::open(&file).unwrap();
    ed.unset(Kind::Skill, "keep", "pinned");
    ed.save().unwrap();
    assert!(!fs::read_to_string(&file).unwrap().contains("pinned"));
}

// @spec CFG-WRITE-002
#[test]
fn writes_replace_the_file_by_rename() {
    use std::os::unix::fs::MetadataExt;
    let tmp = TempDir::new();
    let file = tmp.path().join("athena.toml");
    fs::write(&file, "[skills.a]\ntype = \"local\"\nsource = \"/a\"\n").unwrap();
    let before = fs::metadata(&file).unwrap().ino();
    let mut ed = Editor::open(&file).unwrap();
    ed.set(Kind::Skill, "a", "hash", "sha256:02");
    ed.save().unwrap();
    assert_ne!(
        fs::metadata(&file).unwrap().ino(),
        before,
        "file must be replaced, not rewritten in place"
    );
    let names: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(
        names,
        vec![std::ffi::OsString::from("athena.toml")],
        "no temp files left behind"
    );
}

// @spec CFG-WRITE-003
#[test]
fn saving_a_missing_file_creates_it_and_its_directories() {
    let tmp = TempDir::new();
    let file = tmp.path().join("deep/athena/config.toml");
    let mut ed = Editor::open(&file).unwrap();
    ed.insert(&sample_entry());
    ed.save().unwrap();
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("[skills.\"obra/superpowers\"]"), "{text}");
    assert!(!text.contains("[core]"), "{text}");
}
