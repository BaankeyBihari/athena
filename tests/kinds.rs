mod common;

use athena::config::{self, Env, Scope};
use athena::{install, source};
use common::{Sandbox, staging_leftovers, write};
use std::fs;
use std::path::{Path, PathBuf};

fn env() -> Env {
    Env {
        cwd: PathBuf::from("/p/home/proj"),
        home: PathBuf::from("/p/home"),
        config_home: PathBuf::from("/p/home/.config"),
        cache_home: PathBuf::from("/p/home/.cache"),
    }
}

fn local(text: &str) -> Result<config::Config, String> {
    config::parse(
        text,
        Scope::Local,
        Path::new("/p/home/proj/athena.toml"),
        &env(),
    )
}

/// Exit code 1 from athena's own check, not from clap rejecting a flag that does not exist yet.
macro_rules! refused {
    ($r:expr $(, $($arg:tt)+)?) => {{
        assert_eq!($r.code, 1 $(, $($arg)+)?);
        assert!(
            !$r.stderr.contains("unexpected argument"),
            "rejected as an unknown flag: {}",
            $r.stderr
        );
    }};
}

const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// A remote `org/kit` holding rules, agents, and files that are neither.
fn kit(sb: &Sandbox) -> String {
    sb.repo(
        "org/kit",
        &[
            ("rules/style.md", "style\n"),
            ("rules/testing.md", "testing\n"),
            ("rules/notes.txt", "notes\n"),
            ("rules/SHOUT.MD", "shout\n"),
            ("agents/reviewer.md", "reviewer\n"),
            ("agents/planner.md", "planner\n"),
        ],
    )
}

fn field(doc: &toml_edit::DocumentMut, table: &str, name: &str, key: &str) -> Option<String> {
    let item = doc.get(table)?.get(name)?.get(key)?;
    Some(item.as_str()?.to_string())
}

fn no_network_used(sb: &Sandbox) -> bool {
    !sb.cache_home.join("athena").exists()
}

// ---------- config ----------

// @spec CFG-SCHEMA-001
#[test]
fn rule_and_agent_tables_are_accepted() {
    let cfg = local(&format!(
        r#"
[rules."style.md"]
type = "remote"
source = "https://example.test/org/kit.git"
commit = "{SHA}"
subpath = "rules/style.md"

[agents."reviewer.md"]
type = "local"
source = "/src/kit"
subpath = "agents/reviewer.md"
"#
    ))
    .unwrap();
    assert_eq!(cfg.get("style.md").unwrap().kind.label(), "rule");
    assert_eq!(cfg.get("reviewer.md").unwrap().kind.label(), "agent");
}

// @spec CFG-SCHEMA-003
#[test]
fn a_name_may_not_appear_under_two_kind_tables() {
    for (a, b) in [
        ("rules", "agents"),
        ("skills", "rules"),
        ("plugins", "agents"),
    ] {
        let err = local(&format!(
            "[{a}.\"x.md\"]\ntype = \"local\"\nsource = \"/s\"\nsubpath = \"x.md\"\n\n[{b}.\"x.md\"]\ntype = \"local\"\nsource = \"/s\"\nsubpath = \"x.md\"\n"
        ))
        .unwrap_err();
        assert!(err.contains("x.md"), "{a}/{b}: {err}");
    }
}

// @spec CFG-SCHEMA-007, CFG-PATH-002
#[test]
fn rules_and_agents_default_under_dot_claude() {
    let cfg = local(
        r#"
[rules."style.md"]
type = "local"
source = "/s"
subpath = "style.md"

[agents."reviewer.md"]
type = "local"
source = "/s"
subpath = "reviewer.md"
"#,
    )
    .unwrap();
    assert_eq!(
        cfg.get("style.md").unwrap().resolved,
        PathBuf::from("/p/home/proj/.claude/rules/style.md")
    );
    assert_eq!(
        cfg.get("reviewer.md").unwrap().resolved,
        PathBuf::from("/p/home/proj/.claude/agents/reviewer.md")
    );
}

// @spec CFG-SCHEMA-010
#[test]
fn core_overrides_the_rule_and_agent_default_paths() {
    let cfg = local(
        r#"
[core]
default_rule_path = "./cfg/rules"
default_agent_path = "./cfg/agents"

[rules."style.md"]
type = "local"
source = "/s"
subpath = "style.md"

[agents."reviewer.md"]
type = "local"
source = "/s"
subpath = "reviewer.md"
"#,
    )
    .unwrap();
    assert_eq!(
        cfg.get("style.md").unwrap().resolved,
        PathBuf::from("/p/home/proj/cfg/rules/style.md")
    );
    assert_eq!(
        cfg.get("reviewer.md").unwrap().resolved,
        PathBuf::from("/p/home/proj/cfg/agents/reviewer.md")
    );
}

// @spec CFG-SCHEMA-011
#[test]
fn a_remote_rule_or_agent_needs_a_subpath() {
    for table in ["rules", "agents"] {
        let err = local(&format!(
            "[{table}.\"x.md\"]\ntype = \"remote\"\nsource = \"https://example.test/o/r.git\"\ncommit = \"{SHA}\"\n"
        ))
        .unwrap_err();
        assert!(err.contains("x.md"), "{table}: {err}");
    }
    // A local entry may omit it: the source can be the file itself.
    local("[rules.\"x.md\"]\ntype = \"local\"\nsource = \"/s/x.md\"\n").unwrap();
}

// ---------- add: kinds and placement ----------

// @spec CMD-ADD-002, CMD-ADD-024, CMD-ADD-025, CMD-SKILL-005, INST-OBS-004, INST-APPLY-001
#[test]
fn add_rule_installs_one_file_under_the_rules_table() {
    let sb = Sandbox::new();
    let sha = kit(&sb);
    let src = format!("{}#:rules/style.md", sb.url("org/kit"));
    let r = sb.athena(&["add", &src, "--kind", "rule"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(!r.stderr.contains("SKILL.md"), "{}", r.stderr);

    let dest = sb.project.join(".claude/rules/style.md");
    assert!(dest.is_file(), "a file, not a folder");
    assert_eq!(fs::read_to_string(&dest).unwrap(), "style\n");
    assert!(staging_leftovers(&sb.project.join(".claude/rules")).is_empty());

    let doc = sb.local_doc();
    assert_eq!(
        field(&doc, "rules", "style.md", "type").as_deref(),
        Some("remote")
    );
    assert_eq!(
        field(&doc, "rules", "style.md", "commit").as_deref(),
        Some(sha.as_str())
    );
    assert_eq!(
        field(&doc, "rules", "style.md", "subpath").as_deref(),
        Some("rules/style.md")
    );
    assert!(field(&doc, "rules", "style.md", "hash").is_some());
}

// @spec CMD-ADD-002, CMD-ADD-025
#[test]
fn add_agent_uses_the_agents_table_and_default_path() {
    let sb = Sandbox::new();
    kit(&sb);
    let src = format!("{}#:agents/reviewer.md", sb.url("org/kit"));
    let r = sb.athena(&["add", &src, "--kind", "agent"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(sb.project.join(".claude/agents/reviewer.md").is_file());
    assert!(field(&sb.local_doc(), "agents", "reviewer.md", "type").is_some());
}

// @spec CMD-ADD-025, CMD-ADD-012
#[test]
fn global_rule_goes_to_path_slash_file_name() {
    let sb = Sandbox::new();
    kit(&sb);
    let src = format!("{}#:rules/style.md", sb.url("org/kit"));
    let r = sb.athena(&[
        "add",
        "-g",
        &src,
        "--kind",
        "rule",
        "--path",
        "~/.claude/rules",
    ]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        fs::read_to_string(sb.home.join(".claude/rules/style.md")).unwrap(),
        "style\n"
    );
    assert_eq!(
        field(&sb.global_doc(), "rules", "style.md", "path").as_deref(),
        Some("~/.claude/rules/style.md")
    );
}

// @spec CMD-ADD-024
#[test]
fn a_pattern_selects_files_and_creates_one_entry_each() {
    let sb = Sandbox::new();
    kit(&sb);
    let src = format!("{}#:agents/*.md", sb.url("org/kit"));
    let r = sb.athena(&["add", &src, "--kind", "agent"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    for n in ["planner.md", "reviewer.md"] {
        assert!(sb.project.join(".claude/agents").join(n).is_file(), "{n}");
        assert_eq!(
            field(&sb.local_doc(), "agents", n, "subpath"),
            Some(format!("agents/{n}"))
        );
    }
}

// ---------- add: kind flag ----------

// @spec CMD-ADD-020
#[test]
fn unknown_kind_lists_the_accepted_values_before_any_network() {
    let sb = Sandbox::new();
    let r = sb.athena(&["add", &sb.url("org/kit"), "--kind", "bogus"]);
    refused!(r);
    for k in ["skill", "plugin", "rule", "agent"] {
        assert!(r.stderr.contains(k), "{k}: {}", r.stderr);
    }
    assert!(no_network_used(&sb));
    assert!(!sb.local_file().exists());
}

// @spec CMD-ADD-021
#[test]
fn plugin_flag_still_works_and_warns() {
    let sb = Sandbox::new();
    write(&sb.tmp.path().join("src/lint/bin"), "x");
    let src = sb.tmp.path().join("src/lint");
    let r = sb.athena(&["add", src.to_str().unwrap(), "--plugin"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(
        r.stderr.contains("WARN") && r.stderr.contains("--kind plugin"),
        "{}",
        r.stderr
    );
    assert!(field(&sb.local_doc(), "plugins", "lint", "type").is_some());
}

// @spec CMD-ADD-022
#[test]
fn plugin_together_with_kind_is_an_error_before_any_network() {
    let sb = Sandbox::new();
    let r = sb.athena(&["add", &sb.url("org/kit"), "--plugin", "--kind", "plugin"]);
    refused!(r);
    assert!(no_network_used(&sb));
    assert!(!sb.local_file().exists());
}

// @spec CMD-ADD-023
#[test]
fn rules_and_agents_need_a_source_that_names_a_file() {
    let sb = Sandbox::new();
    let dir = sb.tmp.path().join("src/kit");
    write(&dir.join("rules/style.md"), "s");
    let file = dir.join("rules/style.md");

    // remote without subpath
    let r = sb.athena(&["add", &sb.url("org/kit"), "--kind", "rule"]);
    refused!(r);
    assert!(no_network_used(&sb));
    // local folder without subpath
    let r = sb.athena(&["add", dir.to_str().unwrap(), "--kind", "agent"]);
    refused!(r, "{}", r.stderr);
    // local file together with a subpath
    let with_sub = format!("{}#:x.md", file.display());
    let r = sb.athena(&["add", &with_sub, "--kind", "rule"]);
    refused!(r, "{}", r.stderr);
    assert!(!sb.local_file().exists());
    assert!(!sb.project.join(".claude").exists());
}

// @spec CMD-ADD-023, SRC-NAME-005, SRC-FRAG-008, INST-OBS-002
#[test]
fn a_local_file_source_is_a_rule_named_after_the_file() {
    let sb = Sandbox::new();
    let file = sb.tmp.path().join("src/style.md");
    write(&file, "local style\n");
    let r = sb.athena(&["add", file.to_str().unwrap(), "--kind", "rule"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        fs::read_to_string(sb.project.join(".claude/rules/style.md")).unwrap(),
        "local style\n"
    );
    assert_eq!(
        field(&sb.local_doc(), "rules", "style.md", "type").as_deref(),
        Some("local")
    );
}

// ---------- add: file kind check ----------

// @spec CMD-FILE-001
#[test]
fn a_rule_that_is_not_dot_md_is_refused_and_nothing_changes() {
    let sb = Sandbox::new();
    kit(&sb);
    for file in ["rules/notes.txt", "rules/SHOUT.MD"] {
        let src = format!("{}#:{file}", sb.url("org/kit"));
        let r = sb.athena(&["add", &src, "--kind", "rule"]);
        refused!(r, "{file}");
        assert!(
            r.stderr.contains(file.rsplit('/').next().unwrap()),
            "{}",
            r.stderr
        );
        assert!(!sb.local_file().exists());
        assert!(!sb.project.join(".claude/rules").exists());
    }
}

// @spec CMD-FILE-001, CMD-FILE-002, CMD-ADD-015
#[test]
fn a_pattern_batch_names_every_offender_and_suggests_narrowing() {
    let sb = Sandbox::new();
    kit(&sb);
    let src = format!("{}#:rules/*", sb.url("org/kit"));
    let r = sb.athena(&["add", &src, "--kind", "rule"]);
    refused!(r);
    assert!(
        r.stderr.contains("notes.txt") && r.stderr.contains("SHOUT.MD"),
        "{}",
        r.stderr
    );
    assert!(r.stderr.contains("rules/*.md"), "{}", r.stderr);
    assert!(!sb.project.join(".claude/rules").exists(), "all or nothing");
    assert!(!sb.local_file().exists());
}

// @spec CMD-FILE-003
#[test]
fn no_skill_check_does_not_skip_the_file_check() {
    let sb = Sandbox::new();
    kit(&sb);
    let src = format!("{}#:rules/notes.txt", sb.url("org/kit"));
    let r = sb.athena(&["add", &src, "--kind", "rule", "--no-skill-check"]);
    refused!(r);
    assert!(!sb.project.join(".claude/rules").exists());
}

// @spec CMD-FILE-004
#[test]
fn a_symlinked_source_file_is_refused() {
    let sb = Sandbox::new();
    let real = sb.tmp.path().join("src/real.md");
    write(&real, "real\n");
    let link = sb.tmp.path().join("src/link.md");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let r = sb.athena(&["add", link.to_str().unwrap(), "--kind", "rule"]);
    refused!(r);
    assert!(r.stderr.contains("link.md"), "{}", r.stderr);
    assert!(!sb.project.join(".claude/rules").exists());
    assert!(!sb.local_file().exists());
}

// @spec CMD-FILE-005
#[test]
fn a_rule_name_without_dot_md_is_refused_with_a_suggestion_before_any_network() {
    let sb = Sandbox::new();
    let src = format!("{}#:agents/reviewer.md", sb.url("org/kit"));
    let r = sb.athena(&["add", &src, "--kind", "agent", "--name", "reviewer"]);
    refused!(r);
    assert!(r.stderr.contains("reviewer.md"), "{}", r.stderr);
    assert!(no_network_used(&sb));
    assert!(!sb.local_file().exists());
}

// ---------- install ----------

// @spec INST-HASH-003, INST-PLAN-001, INST-PLAN-004
#[test]
fn a_file_installs_clean_and_its_edits_are_detected() {
    let sb = Sandbox::new();
    kit(&sb);
    let src = format!("{}#:rules/style.md", sb.url("org/kit"));
    assert_eq!(sb.athena(&["add", &src, "--kind", "rule"]).code, 0);

    // The staged file and its installed copy must hash equal: nothing to reinstall.
    let r = sb.athena(&["sync"]);
    assert_eq!(r.code, 0, "{}{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("up to date"), "{}", r.stdout);

    let dest = sb.project.join(".claude/rules/style.md");
    fs::write(&dest, "edited\n").unwrap();
    let r = sb.athena(&["sync"]);
    refused!(r);
    assert!(
        r.stdout.contains("local modifications"),
        "{}{}",
        r.stdout,
        r.stderr
    );
    assert_eq!(fs::read_to_string(&dest).unwrap(), "edited\n");

    assert_eq!(sb.athena(&["sync", "--force"]).code, 0);
    assert_eq!(fs::read_to_string(&dest).unwrap(), "style\n");
}

// @spec INST-PLAN-005
#[test]
fn a_directory_where_a_rule_file_belongs_is_refused_even_with_force() {
    let sb = Sandbox::new();
    kit(&sb);
    write(&sb.project.join(".claude/rules/style.md/inner.txt"), "x");
    let src = format!("{}#:rules/style.md", sb.url("org/kit"));
    for extra in [&[][..], &["--force"]] {
        let mut args = vec!["add", &src, "--kind", "rule"];
        args.extend(extra);
        let r = sb.athena(&args);
        refused!(r, "{extra:?}");
        assert!(
            sb.project
                .join(".claude/rules/style.md/inner.txt")
                .is_file()
        );
    }
    assert!(!sb.local_file().exists());
}

// @spec INST-OBS-005
#[test]
fn a_local_rule_whose_source_is_a_folder_fails_on_sync() {
    let sb = Sandbox::new();
    let dir = sb.tmp.path().join("src/a-folder");
    write(&dir.join("x.md"), "x");
    sb.write_local(&format!(
        "[rules.\"x.md\"]\ntype = \"local\"\nsource = \"{}\"\n",
        dir.display()
    ));
    let r = sb.athena(&["sync"]);
    refused!(r);
    assert!(
        r.stdout.contains("failed") && r.stdout.contains("a-folder"),
        "{}{}",
        r.stdout,
        r.stderr
    );
    assert!(!sb.project.join(".claude/rules/x.md").exists());
}

// @spec INST-OBS-006
#[test]
fn a_staging_file_for_a_rule_never_ends_in_dot_md() {
    let name = install::staging_dir(Path::new("/p/.claude/rules/style.md"));
    let name = name.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        name.starts_with(".athena-staging-") && !name.ends_with(".md"),
        "{name}"
    );
}

// ---------- list ----------

// @spec CMD-LIST-001, CMD-OUT-001
#[test]
fn list_reports_the_rule_and_agent_kinds() {
    let sb = Sandbox::new();
    kit(&sb);
    let rule = format!("{}#:rules/style.md", sb.url("org/kit"));
    let agent = format!("{}#:agents/reviewer.md", sb.url("org/kit"));
    assert_eq!(sb.athena(&["add", &rule, "--kind", "rule"]).code, 0);
    assert_eq!(sb.athena(&["add", &agent, "--kind", "agent"]).code, 0);
    let r = sb.athena(&["list", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&r.stdout).unwrap();
    let kinds: Vec<(String, String)> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            (
                o["name"].as_str().unwrap().to_string(),
                o["kind"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(
        kinds.contains(&("style.md".into(), "rule".into())),
        "{kinds:?}"
    );
    assert!(
        kinds.contains(&("reviewer.md".into(), "agent".into())),
        "{kinds:?}"
    );
    let table = sb.athena(&["list"]).stdout;
    assert!(table.contains("rule") && table.contains("agent"), "{table}");
}

// ---------- source ----------

// @spec SRC-FRAG-008, SRC-NAME-005
#[test]
fn a_file_source_keeps_its_extension_in_the_default_name() {
    let (cwd, home) = (Path::new("/p/home/proj"), Path::new("/p/home"));
    let p = source::parse("./rules/style.md", cwd, home).unwrap();
    assert_eq!(p.default_name, "style.md");
    let p = source::parse("https://example.test/o/r.git#:rules/style.md", cwd, home).unwrap();
    assert_eq!(p.default_name, "style.md");
    assert!(source::matches("rules/*.md", "rules/style.md"));
    assert!(!source::matches("rules/*.md", "rules/.hidden.md"));
}
