mod common;

use common::{Sandbox, staging_leftovers, write};
use std::fs;

fn remote_entry(name: &str, url: &str, commit: &str, extra: &str) -> String {
    format!(
        "[skills.\"{name}\"]\ntype = \"remote\"\nsource = \"{url}\"\ncommit = \"{commit}\"\n{extra}\n"
    )
}

fn local_entry(table: &str, name: &str, source: &str, extra: &str) -> String {
    format!("[{table}.\"{name}\"]\ntype = \"local\"\nsource = \"{source}\"\n{extra}\n")
}

fn field(doc: &toml_edit::DocumentMut, table: &str, name: &str, key: &str) -> Option<String> {
    let item = doc.get(table)?.get(name)?.get(key)?;
    Some(match item.as_value()? {
        toml_edit::Value::String(s) => s.value().clone(),
        v => v.to_string().trim().to_string(),
    })
}

fn json(out: &str) -> Vec<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(out)
        .unwrap_or_else(|e| panic!("bad JSON {e}: {out:?}"))
        .as_array()
        .expect("array")
        .clone()
}

fn names(out: &str) -> Vec<String> {
    let mut v: Vec<String> = json(out)
        .iter()
        .map(|o| o["name"].as_str().unwrap().to_string())
        .collect();
    v.sort();
    v
}

// ---------- scope and locations ----------

// @spec CFG-LOC-001, CMD-SCOPE-002
#[test]
fn local_config_is_only_read_from_the_working_directory() {
    let sb = Sandbox::new();
    sb.write_local("");
    let sub = sb.project.join("sub");
    fs::create_dir_all(&sub).unwrap();
    let mut cmd = sb.command(&["list"]);
    cmd.current_dir(&sub);
    let r = Sandbox::run_cmd(cmd);
    assert_eq!(r.code, 1);
    assert!(
        r.stderr.contains("athena.toml") && r.stderr.contains("sub"),
        "{}",
        r.stderr
    );
    for args in [
        &["sync"][..],
        &["remove", "x"],
        &["update", "-a"],
        &["pin", "x"],
        &["show", "*"],
    ] {
        let mut cmd = sb.command(args);
        cmd.current_dir(&sub);
        let r = Sandbox::run_cmd(cmd);
        assert_eq!(r.code, 1, "{args:?}");
        assert!(r.stderr.contains("athena.toml"), "{args:?}: {}", r.stderr);
    }
}

// @spec CFG-LOC-002
#[test]
fn global_config_follows_xdg_then_home() {
    let sb = Sandbox::new();
    sb.write_global(&local_entry("skills", "from-xdg", "/s", "path = \"~/g/x\""));
    write(
        &sb.home.join(".config/athena/config.toml"),
        &local_entry("skills", "from-home", "/s", "path = \"~/g/y\""),
    );
    let r = sb.athena(&["list", "-g", "--json"]);
    assert_eq!(names(&r.stdout), vec!["from-xdg"], "{}", r.stderr);
    let mut cmd = sb.command(&["list", "-g", "--json"]);
    cmd.env_remove("XDG_CONFIG_HOME");
    let r = Sandbox::run_cmd(cmd);
    assert_eq!(names(&r.stdout), vec!["from-home"], "{}", r.stderr);
}

// @spec CMD-SCOPE-001
#[test]
fn global_flag_selects_the_global_config() {
    let sb = Sandbox::new();
    sb.write_local(&local_entry("skills", "mine", "/s", ""));
    sb.write_global(&local_entry(
        "skills",
        "shared",
        "/s",
        "path = \"~/g/shared\"",
    ));
    assert_eq!(names(&sb.athena(&["list", "--json"]).stdout), vec!["mine"]);
    assert_eq!(
        names(&sb.athena(&["list", "-g", "--json"]).stdout),
        vec!["shared"]
    );
    assert_eq!(
        names(&sb.athena(&["list", "--global", "--json"]).stdout),
        vec!["shared"]
    );
}

// ---------- add ----------

// @spec CMD-ADD-001
#[test]
fn global_add_requires_path_before_any_network() {
    let sb = Sandbox::new();
    sb.repo("org/a", &[("SKILL.md", "v1")]);
    let r = sb.athena(&["add", &sb.url("org/a"), "-g"]);
    assert_eq!(r.code, 1);
    assert!(r.stderr.contains("--path"), "{}", r.stderr);
    assert!(!sb.cache_home.join("athena").exists());
    assert!(!sb.global_file().exists());
}

// @spec CMD-ADD-002, CMD-ADD-003, CMD-ADD-005, CMD-ADD-006, CFG-WRITE-003
#[test]
fn add_records_a_remote_entry() {
    let sb = Sandbox::new();
    let sha = sb.repo("org/a", &[("SKILL.md", "v1")]);
    let r = sb.athena(&["add", &sb.url("org/a")]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let text = sb.read_local();
    assert!(!text.contains("[core]"), "{text}");
    let doc = sb.local_doc();
    assert_eq!(
        field(&doc, "skills", "org/a", "type").as_deref(),
        Some("remote")
    );
    assert_eq!(
        field(&doc, "skills", "org/a", "source"),
        Some(sb.url("org/a"))
    );
    assert_eq!(field(&doc, "skills", "org/a", "commit"), Some(sha));
    for absent in ["ref", "subpath", "path", "pinned"] {
        assert_eq!(
            field(&doc, "skills", "org/a", absent),
            None,
            "{absent} should be absent"
        );
    }
    assert!(
        field(&doc, "skills", "org/a", "hash")
            .unwrap()
            .starts_with("sha256:")
    );
    assert_eq!(
        fs::read_to_string(sb.skill_dir("a").join("SKILL.md")).unwrap(),
        "v1"
    );
}

// @spec CMD-ADD-002, CMD-ADD-003, CMD-ADD-006
#[test]
fn add_with_fragment_plugin_name_path_and_pin() {
    let sb = Sandbox::new();
    sb.repo(
        "org/servers",
        &[("src/postgres/plugin.json", "{}"), ("README.md", "r")],
    );
    let dev = {
        let r = sb.repo_dir("org/servers");
        common::git(&r, &["checkout", "-q", "-b", "dev"]);
        let s = sb.push(
            "org/servers",
            &[("src/postgres/plugin.json", "{\"dev\":1}")],
        );
        common::git(&r, &["checkout", "-q", "main"]);
        s
    };
    let src = format!("{}#dev:/src/postgres", sb.url("org/servers"));
    let r = sb.athena(&[
        "add",
        &src,
        "--plugin",
        "--name",
        "mcp-postgres",
        "--path",
        "./.custom/pg",
        "--pin",
    ]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let doc = sb.local_doc();
    assert_eq!(field(&doc, "plugins", "mcp-postgres", "commit"), Some(dev));
    assert_eq!(
        field(&doc, "plugins", "mcp-postgres", "ref").as_deref(),
        Some("dev")
    );
    assert_eq!(
        field(&doc, "plugins", "mcp-postgres", "subpath").as_deref(),
        Some("src/postgres")
    );
    assert_eq!(
        field(&doc, "plugins", "mcp-postgres", "path").as_deref(),
        Some("./.custom/pg")
    );
    assert_eq!(
        field(&doc, "plugins", "mcp-postgres", "pinned").as_deref(),
        Some("true")
    );
    assert!(doc.get("skills").is_none());
    assert_eq!(
        fs::read_to_string(sb.project.join(".custom/pg/plugin.json")).unwrap(),
        "{\"dev\":1}"
    );
    assert!(!sb.project.join(".custom/pg/README.md").exists());
}

// @spec CMD-ADD-006
#[test]
fn add_with_sha_fragment_records_no_ref() {
    let sb = Sandbox::new();
    let first = sb.repo("org/a", &[("SKILL.md", "v1")]);
    sb.push("org/a", &[("SKILL.md", "v2")]);
    let r = sb.athena(&["add", &format!("{}#{first}", sb.url("org/a"))]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let doc = sb.local_doc();
    assert_eq!(field(&doc, "skills", "org/a", "commit"), Some(first));
    assert_eq!(field(&doc, "skills", "org/a", "ref"), None);
    assert_eq!(
        fs::read_to_string(sb.skill_dir("a").join("SKILL.md")).unwrap(),
        "v1"
    );
}

// @spec CMD-ADD-002, INST-OBS-002
#[test]
fn add_local_source() {
    let sb = Sandbox::new();
    let src = sb.tmp.path().join("my-plugins");
    write(&src.join("linter/run.sh"), "echo");
    let r = sb.athena(&["add", &format!("{}#:linter", src.display()), "--plugin"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let doc = sb.local_doc();
    assert_eq!(
        field(&doc, "plugins", "linter", "type").as_deref(),
        Some("local")
    );
    assert_eq!(field(&doc, "plugins", "linter", "commit"), None);
    assert!(sb.project.join(".agents/plugins/linter/run.sh").exists());
}

// @spec CFG-WRITE-003
#[test]
fn global_add_creates_global_config_and_dirs() {
    let sb = Sandbox::new();
    sb.repo("org/a", &[("SKILL.md", "v1")]);
    let r = sb.athena(&["add", &sb.url("org/a"), "-g", "--path", "~/agent-skills/a"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        field(&sb.global_doc(), "skills", "org/a", "path").as_deref(),
        Some("~/agent-skills/a")
    );
    assert!(sb.home.join("agent-skills/a/SKILL.md").exists());
    assert!(!sb.local_file().exists());
}

// @spec CMD-ADD-004
#[test]
fn add_rejects_existing_names() {
    let sb = Sandbox::new();
    sb.repo("org/a", &[("SKILL.md", "v1")]);
    assert_eq!(sb.athena(&["add", &sb.url("org/a")]).code, 0);
    let r = sb.athena(&["add", &sb.url("org/a"), "--path", "./other"]);
    assert_eq!(r.code, 1);
    assert!(r.stderr.contains("update"), "{}", r.stderr);
}

// @spec CMD-ADD-007, INST-DIRTY-004
#[test]
fn add_over_unmanaged_directory_is_refused_and_config_untouched() {
    let sb = Sandbox::new();
    sb.repo("org/a", &[("SKILL.md", "v1")]);
    write(&sb.skill_dir("a").join("mine.md"), "user file");
    let r = sb.athena(&["add", &sb.url("org/a")]);
    assert_eq!(r.code, 1);
    assert!(
        !sb.local_file().exists(),
        "config must not be created by a refused add"
    );
    assert_eq!(
        fs::read_to_string(sb.skill_dir("a").join("mine.md")).unwrap(),
        "user file"
    );
    assert!(staging_leftovers(&sb.project.join(".agents/skills")).is_empty());

    let r = sb.athena(&["add", &sb.url("org/a"), "--force"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(!sb.skill_dir("a").join("mine.md").exists());
}

// @spec INST-DIRTY-004
#[test]
fn add_over_git_tracked_unmanaged_directory_is_still_refused() {
    let sb = Sandbox::new();
    sb.repo("org/a", &[("SKILL.md", "v1")]);
    common::init_repo(&sb.project, &[(".agents/skills/a/mine.md", "committed")]);
    let r = sb.athena(&["add", &sb.url("org/a")]);
    assert_eq!(r.code, 1, "{}", r.stdout);
    assert!(sb.skill_dir("a").join("mine.md").exists());
}

// @spec INST-PLAN-001
#[test]
fn add_adopts_identical_existing_content() {
    let sb = Sandbox::new();
    sb.repo("org/a", &[("SKILL.md", "v1")]);
    write(&sb.skill_dir("a").join("SKILL.md"), "v1");
    let r = sb.athena(&["add", &sb.url("org/a")]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(field(&sb.local_doc(), "skills", "org/a", "hash").is_some());
}

// ---------- gobbling ----------

fn gobble_setup(sb: &Sandbox) {
    let src = sb.tmp.path().join("srcs");
    write(&src.join("one/a.md"), "1");
    write(&src.join("two/b.md"), "2");
    write(&src.join("parent/p.md"), "p");
    for d in ["one", "two", "parent"] {
        write(&src.join(d).join("SKILL.md"), d);
    }
    let one = src.join("one").display().to_string();
    let two = src.join("two").display().to_string();
    assert_eq!(sb.athena(&["add", &one, "--path", "./tools/one"]).code, 0);
    assert_eq!(
        sb.athena(&["add", &two, "--path", "./tools/two", "--pin"])
            .code,
        0
    );
}

// @spec INST-GOB-004
#[test]
fn gobbling_without_a_terminal_is_refused() {
    let sb = Sandbox::new();
    gobble_setup(&sb);
    let before = sb.read_local();
    let parent = sb.tmp.path().join("srcs/parent").display().to_string();
    let r = sb.athena(&["add", &parent, "--path", "./tools"]);
    assert_eq!(r.code, 1);
    assert!(r.stderr.contains("remove"), "{}", r.stderr);
    assert_eq!(sb.read_local(), before);
    assert!(sb.project.join("tools/one/a.md").exists());
}

// @spec INST-GOB-002
#[test]
fn gobbling_a_dirty_child_is_refused_without_prompt() {
    let sb = Sandbox::new();
    gobble_setup(&sb);
    write(&sb.project.join("tools/one/a.md"), "edited");
    let before = sb.read_local();
    let parent = sb.tmp.path().join("srcs/parent").display().to_string();
    let r = sb.athena_tty(&["add", &parent, "--path", "./tools"], "y");
    assert_eq!(r.code, 1);
    assert!(!r.stdout.contains("[y/N]"), "must not prompt: {}", r.stdout);
    assert_eq!(sb.read_local(), before);
    assert_eq!(
        fs::read_to_string(sb.project.join("tools/one/a.md")).unwrap(),
        "edited"
    );
}

// @spec INST-GOB-002
#[test]
fn gobbling_is_refused_when_parent_holds_other_files() {
    let sb = Sandbox::new();
    gobble_setup(&sb);
    write(&sb.project.join("tools/user-notes.md"), "mine");
    let before = sb.read_local();
    let parent = sb.tmp.path().join("srcs/parent").display().to_string();
    let r = sb.athena_tty(&["add", &parent, "--path", "./tools"], "y");
    assert_eq!(r.code, 1);
    assert!(!r.stdout.contains("[y/N]"), "must not prompt: {}", r.stdout);
    assert_eq!(sb.read_local(), before);
    assert!(sb.project.join("tools/user-notes.md").exists());
    assert!(sb.project.join("tools/one/a.md").exists());
}

// @spec INST-GOB-003
#[test]
fn declining_the_gobble_prompt_changes_nothing() {
    let sb = Sandbox::new();
    gobble_setup(&sb);
    let before = sb.read_local();
    let parent = sb.tmp.path().join("srcs/parent").display().to_string();
    let r = sb.athena_tty(&["add", &parent, "--path", "./tools"], "n");
    assert_eq!(r.code, 1);
    assert!(
        r.stdout.contains("Gobble") && r.stdout.contains("[y/N]"),
        "{}",
        r.stdout
    );
    assert_eq!(sb.read_local(), before);
}

// @spec INST-GOB-003, INST-GOB-005, INST-GOB-007
#[test]
fn confirmed_gobble_replaces_children_including_pinned() {
    let sb = Sandbox::new();
    gobble_setup(&sb);
    let parent = sb.tmp.path().join("srcs/parent").display().to_string();
    let r = sb.athena_tty(&["add", &parent, "--path", "./tools"], "y");
    assert_eq!(r.code, 0, "{}", r.stdout);
    let doc = sb.local_doc();
    assert!(field(&doc, "skills", "one", "type").is_none());
    assert!(
        field(&doc, "skills", "two", "type").is_none(),
        "pinned child is gobbled too"
    );
    assert!(field(&doc, "skills", "parent", "type").is_some());
    assert!(sb.project.join("tools/p.md").exists());
    assert!(!sb.project.join("tools/one").exists());
}

// @spec INST-GOB-006
#[test]
fn add_inside_an_existing_entry_is_refused() {
    let sb = Sandbox::new();
    gobble_setup(&sb);
    let before = sb.read_local();
    let parent = sb.tmp.path().join("srcs/parent").display().to_string();
    let r = sb.athena(&["add", &parent, "--path", "./tools/one/inner"]);
    assert_eq!(r.code, 1);
    assert!(r.stderr.contains("one"), "{}", r.stderr);
    assert_eq!(sb.read_local(), before);
}

// ---------- remove ----------

// @spec INST-RM-001, INST-RM-002
#[test]
fn remove_deletes_destination_and_entry() {
    let sb = Sandbox::new();
    sb.repo("org/a", &[("SKILL.md", "v1")]);
    sb.repo("org/b", &[("SKILL.md", "v1")]);
    assert_eq!(sb.athena(&["add", &sb.url("org/a")]).code, 0);
    assert_eq!(sb.athena(&["add", &sb.url("org/b")]).code, 0);
    let r = sb.athena(&["remove", "org/a"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(!sb.skill_dir("a").exists());
    assert!(field(&sb.local_doc(), "skills", "org/a", "type").is_none());
    fs::remove_dir_all(sb.skill_dir("b")).unwrap();
    assert_eq!(sb.athena(&["remove", "org/b"]).code, 0);
    assert!(field(&sb.local_doc(), "skills", "org/b", "type").is_none());
}

// @spec INST-RM-003
#[test]
fn remove_refuses_modified_destination_without_force() {
    let sb = Sandbox::new();
    sb.repo("org/a", &[("SKILL.md", "v1")]);
    assert_eq!(sb.athena(&["add", &sb.url("org/a")]).code, 0);
    write(&sb.skill_dir("a").join("SKILL.md"), "edited");
    let r = sb.athena(&["remove", "org/a"]);
    assert_eq!(r.code, 1);
    assert!(sb.skill_dir("a").exists());
    assert!(field(&sb.local_doc(), "skills", "org/a", "type").is_some());
    assert_eq!(sb.athena(&["remove", "org/a", "--force"]).code, 0);
    assert!(!sb.skill_dir("a").exists());
}

// @spec CMD-RM-001
#[test]
fn unknown_names_error_and_hint_at_global() {
    let sb = Sandbox::new();
    sb.write_local("");
    sb.write_global(&local_entry(
        "skills",
        "shared",
        "/s",
        "path = \"~/g/shared\"",
    ));
    for args in [
        &["remove", "shared"][..],
        &["update", "shared"],
        &["pin", "shared"],
        &["unpin", "shared"],
    ] {
        let r = sb.athena(args);
        assert_eq!(r.code, 1, "{args:?}");
        assert!(
            r.stderr.contains("shared") && r.stderr.contains("-g"),
            "{args:?}: {}",
            r.stderr
        );
    }
    let r = sb.athena(&["remove", "nobody"]);
    assert_eq!(r.code, 1);
    assert!(
        r.stderr.contains("nobody") && !r.stderr.contains("-g"),
        "{}",
        r.stderr
    );
}

// ---------- update ----------

// @spec CMD-UPD-001
#[test]
fn update_needs_exactly_one_selector() {
    let sb = Sandbox::new();
    sb.write_local("");
    assert_eq!(sb.athena(&["update"]).code, 1);
    assert_eq!(sb.athena(&["update", "x", "--all"]).code, 1);
}

// @spec CMD-UPD-002
#[test]
fn update_moves_to_the_latest_commit_of_ref() {
    let sb = Sandbox::new();
    sb.repo("org/a", &[("SKILL.md", "v1")]);
    assert_eq!(sb.athena(&["add", &sb.url("org/a")]).code, 0);
    let second = sb.push("org/a", &[("SKILL.md", "v2")]);
    let r = sb.athena(&["update", "org/a"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        field(&sb.local_doc(), "skills", "org/a", "commit"),
        Some(second)
    );
    assert_eq!(
        fs::read_to_string(sb.skill_dir("a").join("SKILL.md")).unwrap(),
        "v2"
    );
}

// @spec CMD-UPD-002
#[test]
fn update_follows_ref_then_unresolved_commit_then_head() {
    let sb = Sandbox::new();
    let main1 = sb.repo("org/a", &[("SKILL.md", "main")]);
    let r = sb.repo_dir("org/a");
    common::git(&r, &["checkout", "-q", "-b", "dev"]);
    let dev = sb.push("org/a", &[("SKILL.md", "dev")]);
    common::git(&r, &["checkout", "-q", "main"]);
    sb.write_local(
        &(remote_entry(
            "by-ref",
            &sb.url("org/a"),
            &main1,
            "ref = \"dev\"\npath = \"./x/by-ref\"",
        ) + &remote_entry(
            "by-commit",
            &sb.url("org/a"),
            "dev",
            "path = \"./x/by-commit\"",
        ) + &remote_entry("by-head", &sb.url("org/a"), &dev, "path = \"./x/by-head\"")),
    );
    for n in ["by-ref", "by-commit", "by-head"] {
        let r = sb.athena(&["update", n]);
        assert_eq!(r.code, 0, "{n}: {}", r.stderr);
    }
    let doc = sb.local_doc();
    assert_eq!(field(&doc, "skills", "by-ref", "commit"), Some(dev.clone()));
    assert_eq!(
        field(&doc, "skills", "by-commit", "commit"),
        Some(dev.clone())
    );
    assert_eq!(
        field(&doc, "skills", "by-commit", "ref").as_deref(),
        Some("dev")
    );
    assert_eq!(field(&doc, "skills", "by-head", "commit"), Some(main1));
}

// @spec CMD-UPD-003
#[test]
fn update_at_latest_reports_already_at() {
    let sb = Sandbox::new();
    let sha = sb.repo("org/a", &[("SKILL.md", "v1")]);
    assert_eq!(sb.athena(&["add", &sb.url("org/a")]).code, 0);
    let before = sb.read_local();
    let r = sb.athena(&["update", "org/a"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(
        r.stdout.contains(&format!("already at `{sha}`")),
        "{}",
        r.stdout
    );
    assert_eq!(sb.read_local(), before);
}

// @spec CMD-UPD-004
#[test]
fn update_of_local_source_points_at_sync() {
    let sb = Sandbox::new();
    let src = sb.tmp.path().join("lint");
    write(&src.join("a"), "1");
    sb.write_local(&local_entry(
        "skills",
        "lint",
        &src.display().to_string(),
        "",
    ));
    let r = sb.athena(&["update", "lint"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stdout.contains("sync"), "{}", r.stdout);
    assert!(!sb.skill_dir("lint").exists());
}

// @spec CMD-UPD-005, CFG-WRITE-004
#[test]
fn update_all_refreshes_unpinned_and_syncs_pinned() {
    let sb = Sandbox::new();
    let a1 = sb.repo("org/a", &[("SKILL.md", "a1")]);
    let b1 = sb.repo("org/b", &[("SKILL.md", "b1")]);
    sb.repo("org/c", &[("SKILL.md", "c1")]);
    assert_eq!(sb.athena(&["add", &sb.url("org/a")]).code, 0);
    assert_eq!(sb.athena(&["add", &sb.url("org/b"), "--pin"]).code, 0);
    let a2 = sb.push("org/a", &[("SKILL.md", "a2")]);
    sb.push("org/b", &[("SKILL.md", "b2")]);
    // A pinned hand-written entry with an unresolved commit is locked, not refreshed.
    let c_head = common::git(&sb.repo_dir("org/c"), &["rev-parse", "HEAD"]);
    let mut text = sb.read_local();
    text.push_str(&remote_entry(
        "org/c",
        &sb.url("org/c"),
        "main",
        "pinned = true",
    ));
    sb.write_local(&text);
    fs::remove_dir_all(sb.skill_dir("b")).unwrap();

    let r = sb.athena(&["update", "-a"]);
    assert_eq!(r.code, 0, "{}\n{}", r.stdout, r.stderr);
    let doc = sb.local_doc();
    assert_eq!(field(&doc, "skills", "org/a", "commit"), Some(a2));
    assert_eq!(
        field(&doc, "skills", "org/b", "commit"),
        Some(b1),
        "pinned entry keeps its commit"
    );
    assert_eq!(field(&doc, "skills", "org/c", "commit"), Some(c_head));
    assert_eq!(
        field(&doc, "skills", "org/c", "ref").as_deref(),
        Some("main")
    );
    assert_eq!(
        fs::read_to_string(sb.skill_dir("b").join("SKILL.md")).unwrap(),
        "b1",
        "pinned entry is synced"
    );
    assert!(
        r.stdout
            .lines()
            .any(|l| l.contains("org/b") && l.contains("pinned:")),
        "{}",
        r.stdout
    );
    let _ = a1;
}

// @spec CMD-UPD-006
#[test]
fn update_by_name_refreshes_pinned_entries() {
    let sb = Sandbox::new();
    sb.repo("org/a", &[("SKILL.md", "v1")]);
    assert_eq!(sb.athena(&["add", &sb.url("org/a"), "--pin"]).code, 0);
    let second = sb.push("org/a", &[("SKILL.md", "v2")]);
    assert_eq!(sb.athena(&["update", "org/a"]).code, 0);
    assert_eq!(
        field(&sb.local_doc(), "skills", "org/a", "commit"),
        Some(second)
    );
    assert_eq!(
        field(&sb.local_doc(), "skills", "org/a", "pinned").as_deref(),
        Some("true")
    );
}

// @spec CMD-UPD-002
#[test]
fn refused_update_keeps_commit() {
    let sb = Sandbox::new();
    let first = sb.repo("org/a", &[("SKILL.md", "v1")]);
    assert_eq!(sb.athena(&["add", &sb.url("org/a")]).code, 0);
    sb.push("org/a", &[("SKILL.md", "v2")]);
    write(&sb.skill_dir("a").join("SKILL.md"), "edited");
    let r = sb.athena(&["update", "org/a"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        field(&sb.local_doc(), "skills", "org/a", "commit"),
        Some(first)
    );
    assert_eq!(
        fs::read_to_string(sb.skill_dir("a").join("SKILL.md")).unwrap(),
        "edited"
    );
}

// ---------- pin ----------

// @spec CMD-PIN-001, CMD-PIN-002, CMD-PIN-004, CFG-STATE-004
#[test]
fn pin_and_unpin_edit_only_the_config() {
    let sb = Sandbox::new();
    sb.write_local(&remote_entry(
        "org/a",
        &sb.url("org/a"),
        "0123456789abcdef0123456789abcdef01234567",
        "# keep",
    ));
    let r = sb.athena(&["pin", "org/a"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        field(&sb.local_doc(), "skills", "org/a", "pinned").as_deref(),
        Some("true")
    );
    let r = sb.athena(&["pin", "org/a"]);
    assert_eq!(r.code, 0);
    assert!(r.stdout.contains("no change"), "{}", r.stdout);
    assert_eq!(sb.athena(&["unpin", "org/a"]).code, 0);
    assert_eq!(field(&sb.local_doc(), "skills", "org/a", "pinned"), None);
    assert!(sb.read_local().contains("# keep"));
    let r = sb.athena(&["unpin", "org/a"]);
    assert_eq!(r.code, 0);
    assert!(r.stdout.contains("no change"), "{}", r.stdout);
    assert!(!sb.cache_home.join("athena").exists(), "no fetch");
    assert!(
        !sb.project.join(".agents").exists(),
        "no destination change"
    );
}

// ---------- sync ----------

// @spec CMD-SYNC-001, CMD-SYNC-002, CFG-WRITE-004, CMD-OUT-003
#[test]
fn sync_locks_installs_and_is_idempotent() {
    let sb = Sandbox::new();
    let sha = sb.repo("org/a", &[("SKILL.md", "v1")]);
    sb.write_local(&remote_entry("org/a", &sb.url("org/a"), "main", ""));
    let r = sb.athena(&["sync"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(
        r.stdout
            .lines()
            .any(|l| l.contains("org/a") && l.contains("installed")),
        "{}",
        r.stdout
    );
    let doc = sb.local_doc();
    assert_eq!(field(&doc, "skills", "org/a", "commit"), Some(sha));
    assert_eq!(
        field(&doc, "skills", "org/a", "ref").as_deref(),
        Some("main")
    );
    assert!(field(&doc, "skills", "org/a", "hash").is_some());
    let r = sb.athena(&["sync"]);
    assert_eq!(r.code, 0);
    assert!(
        r.stdout
            .lines()
            .any(|l| l.contains("org/a") && l.contains("up to date")),
        "{}",
        r.stdout
    );
}

// @spec CMD-SYNC-001
#[test]
fn sync_restores_locked_commit_without_moving_it() {
    let sb = Sandbox::new();
    let first = sb.repo("org/a", &[("SKILL.md", "v1")]);
    assert_eq!(sb.athena(&["add", &sb.url("org/a")]).code, 0);
    sb.push("org/a", &[("SKILL.md", "v2")]);
    fs::remove_dir_all(sb.skill_dir("a")).unwrap();
    assert_eq!(sb.athena(&["sync"]).code, 0);
    assert_eq!(
        fs::read_to_string(sb.skill_dir("a").join("SKILL.md")).unwrap(),
        "v1"
    );
    assert_eq!(
        field(&sb.local_doc(), "skills", "org/a", "commit"),
        Some(first)
    );
}

// @spec CACHE-ENS-001
#[test]
fn sync_works_offline_once_cached() {
    let sb = Sandbox::new();
    sb.repo("org/a", &[("SKILL.md", "v1")]);
    assert_eq!(sb.athena(&["add", &sb.url("org/a")]).code, 0);
    fs::remove_dir_all(sb.repos.join("org")).unwrap();
    fs::remove_dir_all(sb.skill_dir("a")).unwrap();
    let r = sb.athena(&["sync"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(sb.skill_dir("a").join("SKILL.md").exists());
}

// @spec INST-PLAN-004, INST-PLAN-006, INST-APPLY-003, INST-APPLY-004, CMD-OUT-003
#[test]
fn sync_skips_modified_entries_and_continues() {
    let sb = Sandbox::new();
    sb.repo("org/a", &[("SKILL.md", "a")]);
    sb.repo("org/b", &[("SKILL.md", "b")]);
    assert_eq!(sb.athena(&["add", &sb.url("org/a")]).code, 0);
    assert_eq!(sb.athena(&["add", &sb.url("org/b")]).code, 0);
    let hash_a = field(&sb.local_doc(), "skills", "org/a", "hash");
    write(&sb.skill_dir("a").join("SKILL.md"), "edited");
    fs::remove_dir_all(sb.skill_dir("b")).unwrap();
    let r = sb.athena(&["sync"]);
    assert_eq!(r.code, 1);
    assert!(
        r.stdout
            .lines()
            .any(|l| l.contains("org/a") && l.contains("skipped: local modifications in")),
        "{}",
        r.stdout
    );
    assert!(
        r.stdout
            .lines()
            .any(|l| l.contains("org/b") && l.contains("installed")),
        "{}",
        r.stdout
    );
    assert_eq!(
        fs::read_to_string(sb.skill_dir("a").join("SKILL.md")).unwrap(),
        "edited"
    );
    assert_eq!(field(&sb.local_doc(), "skills", "org/a", "hash"), hash_a);
    assert!(staging_leftovers(&sb.project.join(".agents/skills")).is_empty());
}

// @spec INST-PLAN-003
#[test]
fn sync_force_overwrites_modifications() {
    let sb = Sandbox::new();
    sb.repo("org/a", &[("SKILL.md", "a")]);
    assert_eq!(sb.athena(&["add", &sb.url("org/a")]).code, 0);
    write(&sb.skill_dir("a").join("SKILL.md"), "edited");
    assert_eq!(sb.athena(&["sync", "--force"]).code, 0);
    assert_eq!(
        fs::read_to_string(sb.skill_dir("a").join("SKILL.md")).unwrap(),
        "a"
    );
}

// @spec INST-DIRTY-003
#[test]
fn sync_without_hash_trusts_committed_git_content() {
    let sb = Sandbox::new();
    let sha = sb.repo("org/a", &[("SKILL.md", "new")]);
    common::init_repo(&sb.project, &[(".agents/skills/a/SKILL.md", "old")]);
    sb.write_local(&remote_entry("org/a", &sb.url("org/a"), &sha, ""));
    let r = sb.athena(&["sync"]);
    assert_eq!(r.code, 0, "{}\n{}", r.stdout, r.stderr);
    assert_eq!(
        fs::read_to_string(sb.skill_dir("a").join("SKILL.md")).unwrap(),
        "new"
    );
}

// @spec CFG-WRITE-004, INST-APPLY-004
#[test]
fn sync_locks_commit_even_when_install_is_skipped() {
    let sb = Sandbox::new();
    let sha = sb.repo("org/a", &[("SKILL.md", "v1")]);
    write(&sb.skill_dir("a").join("SKILL.md"), "untracked user file");
    sb.write_local(&remote_entry("org/a", &sb.url("org/a"), "main", ""));
    let r = sb.athena(&["sync"]);
    assert_eq!(r.code, 1);
    let doc = sb.local_doc();
    assert_eq!(field(&doc, "skills", "org/a", "commit"), Some(sha));
    assert_eq!(
        field(&doc, "skills", "org/a", "ref").as_deref(),
        Some("main")
    );
    assert_eq!(field(&doc, "skills", "org/a", "hash"), None);
}

// @spec INST-OBS-002, INST-OBS-003, INST-PLAN-005
#[test]
fn sync_local_sources() {
    let sb = Sandbox::new();
    let src = sb.tmp.path().join("repo");
    write(&src.join("tools/lint/run.sh"), "v1");
    write(&sb.project.join(".agents/skills/file-not-dir"), "x");
    sb.write_local(
        &(local_entry(
            "skills",
            "lint",
            &src.display().to_string(),
            "subpath = \"tools/lint\"",
        ) + &local_entry("skills", "missing", "/definitely/not/here", "")
            + &local_entry("skills", "file-not-dir", &src.display().to_string(), "")),
    );
    let r = sb.athena(&["sync"]);
    assert_eq!(r.code, 1);
    assert_eq!(
        fs::read_to_string(sb.skill_dir("lint").join("run.sh")).unwrap(),
        "v1"
    );
    assert!(
        r.stdout.lines().any(|l| l.contains("missing")
            && l.contains("failed:")
            && l.contains("/definitely/not/here")),
        "{}",
        r.stdout
    );
    assert!(
        r.stdout
            .lines()
            .any(|l| l.contains("file-not-dir") && !l.contains("installed")),
        "{}",
        r.stdout
    );
    write(&src.join("tools/lint/run.sh"), "v2");
    sb.athena(&["sync"]);
    assert_eq!(
        fs::read_to_string(sb.skill_dir("lint").join("run.sh")).unwrap(),
        "v2",
        "local sources re-copy on sync"
    );
}

// @spec CACHE-ERR-001
#[test]
fn missing_git_is_reported() {
    let sb = Sandbox::new();
    sb.write_local(&remote_entry("org/a", &sb.url("org/a"), "main", ""));
    let mut cmd = sb.command(&["sync"]);
    cmd.env("PATH", "");
    let r = Sandbox::run_cmd(cmd);
    assert_eq!(r.code, 1);
    assert!(
        r.stdout.contains("git") || r.stderr.contains("git"),
        "{}{}",
        r.stdout,
        r.stderr
    );
    assert!(format!("{}{}", r.stdout, r.stderr).contains("required"));
}

// ---------- config state ----------

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

// @spec CFG-STATE-003
#[test]
fn state_errors_block_add_sync_and_update_before_any_change() {
    let sb = Sandbox::new();
    sb.repo("org/a", &[("SKILL.md", "v1")]);
    sb.write_local(
        &(remote_entry("one", &sb.url("org/a"), SHA, "path = \"./same\"")
            + &remote_entry("two", &sb.url("org/b"), SHA, "path = \"./same\"")),
    );
    for args in [
        &["sync"][..],
        &["update", "-a"],
        &["update", "one"],
        &["add", &sb.url("org/a"), "--name", "three"],
    ] {
        let r = sb.athena(args);
        assert_eq!(r.code, 1, "{args:?}");
        assert!(
            r.stderr.contains("one") && r.stderr.contains("two") && r.stderr.contains("same"),
            "{args:?}: {}",
            r.stderr
        );
    }
    assert!(!sb.cache_home.join("athena").exists(), "no fetch");
    assert!(!sb.project.join("same").exists());
    assert!(!sb.read_local().contains("three"));
}

// @spec CFG-STATE-005
#[test]
fn state_errors_warn_on_list_and_show() {
    let sb = Sandbox::new();
    sb.write_local(
        &(remote_entry("one", &sb.url("org/a"), SHA, "path = \"./same\"")
            + &remote_entry("two", &sb.url("org/b"), SHA, "path = \"./same\"")),
    );
    for args in [&["list"][..], &["show", "*"]] {
        let r = sb.athena(args);
        assert_eq!(r.code, 0, "{args:?}: {}", r.stderr);
        assert_eq!(
            r.stderr.matches("WARN").count(),
            1,
            "{args:?}: {}",
            r.stderr
        );
    }
}

fn conflict_setup(sb: &Sandbox) {
    let src = sb.tmp.path().join("g");
    write(&src.join("g.md"), "global");
    sb.write_global(&local_entry(
        "skills",
        "theirs",
        &src.display().to_string(),
        "path = \"~/project/.agents/skills/shared\"",
    ));
    let r = sb.athena(&["sync", "-g"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    sb.write_local(&local_entry(
        "skills",
        "mine",
        &src.display().to_string(),
        "path = \"./.agents/skills/shared\"",
    ));
}

// @spec CFG-CONF-001, CFG-CONF-003
#[test]
fn local_conflict_with_global_blocks_with_fix_instructions() {
    let sb = Sandbox::new();
    conflict_setup(&sb);
    let r = sb.athena(&["sync"]);
    assert_eq!(r.code, 1);
    for needle in [
        "mine",
        "theirs",
        ".agents/skills/shared",
        "athena remove mine",
        "athena remove -g theirs",
    ] {
        assert!(
            r.stderr.contains(needle),
            "missing {needle:?}: {}",
            r.stderr
        );
    }
}

// @spec CFG-CONF-002
#[test]
fn local_add_onto_a_global_path_is_blocked() {
    let sb = Sandbox::new();
    conflict_setup(&sb);
    sb.write_local("");
    sb.repo("org/shared", &[("SKILL.md", "v1")]);
    let r = sb.athena(&["add", &sb.url("org/shared")]);
    assert_eq!(r.code, 1);
    assert!(r.stderr.contains("theirs"), "{}", r.stderr);
    assert_eq!(sb.read_local(), "");
    assert_eq!(
        fs::read_to_string(sb.skill_dir("shared").join("g.md")).unwrap(),
        "global"
    );
}

// @spec CFG-STATE-001, CFG-CONF-006
#[test]
fn global_commands_ignore_the_local_config() {
    let sb = Sandbox::new();
    conflict_setup(&sb);
    sb.write_local("this is [not toml");
    let r = sb.athena(&["sync", "-g"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(sb.athena(&["list", "-g"]).code, 0);
}

// @spec CFG-STATE-004, CFG-STATE-006
#[test]
fn remove_resolves_conflicts_without_touching_files() {
    let sb = Sandbox::new();
    conflict_setup(&sb);
    assert_eq!(sb.athena(&["pin", "mine"]).code, 0);
    let r = sb.athena(&["remove", "mine"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(field(&sb.local_doc(), "skills", "mine", "type").is_none());
    assert_eq!(
        fs::read_to_string(sb.skill_dir("shared").join("g.md")).unwrap(),
        "global",
        "global-owned files untouched"
    );
    assert_eq!(sb.athena(&["sync"]).code, 0);
}

// @spec CFG-PATH-005
#[test]
fn load_errors_fail_every_command() {
    let sb = Sandbox::new();
    sb.write_local(&local_entry("skills", "bad", "/s", "path = \".\""));
    for args in [
        &["list"][..],
        &["show", "*"],
        &["remove", "bad"],
        &["pin", "bad"],
        &["sync"],
    ] {
        let r = sb.athena(args);
        assert_eq!(r.code, 1, "{args:?}");
        assert!(r.stderr.contains("bad"), "{args:?}: {}", r.stderr);
    }
}

// ---------- list / show / output ----------

fn listing_setup(sb: &Sandbox) {
    let src = sb.tmp.path().join("lint");
    write(&src.join("a"), "1");
    sb.write_local(
        &(remote_entry(
            "obra/superpowers",
            "https://example.test/obra/superpowers.git",
            SHA,
            "",
        ) + &local_entry(
            "plugins",
            "local-linter",
            &src.display().to_string(),
            "path = \"./.custom/linter\"",
        ) + &local_entry("skills", "remote-linter", &src.display().to_string(), "")
            + &local_entry("skills", "other", &src.display().to_string(), "")),
    );
}

// @spec CMD-LIST-001
#[test]
fn list_prints_a_table() {
    let sb = Sandbox::new();
    listing_setup(&sb);
    let r = sb.athena(&["list"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    for h in ["name", "kind", "type", "commit", "path", "source"] {
        assert!(
            r.stdout.to_lowercase().contains(h),
            "header {h}: {}",
            r.stdout
        );
    }
    assert!(
        r.stdout.contains(&SHA[..12]) && !r.stdout.contains(&SHA[..13]),
        "{}",
        r.stdout
    );
    assert!(r.stdout.contains("plugin") && r.stdout.contains("skill"));
}

// @spec CMD-LIST-002
#[test]
fn list_runs_without_git() {
    let sb = Sandbox::new();
    listing_setup(&sb);
    let mut cmd = sb.command(&["list", "--json"]);
    cmd.env("PATH", "");
    let r = Sandbox::run_cmd(cmd);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(json(&r.stdout).len(), 4);
}

// @spec CMD-OUT-001
#[test]
fn list_json_is_a_strict_array() {
    let sb = Sandbox::new();
    listing_setup(&sb);
    let r = sb.athena(&["list", "--json"]);
    let rows = json(&r.stdout);
    let sp = rows
        .iter()
        .find(|o| o["name"] == "obra/superpowers")
        .unwrap();
    assert_eq!(sp["kind"], "skill");
    assert_eq!(sp["type"], "remote");
    assert_eq!(sp["commit"], SHA);
    assert!(
        sp["subpath"].is_null()
            && sp["path"].is_null()
            && sp["ref"].is_null()
            && sp["hash"].is_null()
    );
    assert_eq!(
        sp["resolved_path"],
        sb.skill_dir("superpowers").display().to_string()
    );
    sb.write_local("");
    assert_eq!(sb.athena(&["list", "--json"]).stdout.trim(), "[]");
}

// @spec CMD-SHOW-001, CMD-SHOW-002
#[test]
fn show_matches_globs_and_reports_status() {
    let sb = Sandbox::new();
    listing_setup(&sb);
    assert_eq!(
        sb.athena(&["sync"]).code,
        1,
        "remote entry cannot fetch; locals install"
    );
    write(&sb.skill_dir("other").join("a"), "edited");
    let r = sb.athena(&["show", "*linter", "--json"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(names(&r.stdout), vec!["local-linter", "remote-linter"]);
    let rows = json(&r.stdout);
    let ll = rows.iter().find(|o| o["name"] == "local-linter").unwrap();
    assert_eq!(ll["status"], "present");
    assert_eq!(
        ll["resolved_path"],
        sb.project.join(".custom/linter").display().to_string()
    );
    assert_eq!(ll["path"], "./.custom/linter");
    let rows = json(&sb.athena(&["show", "?ther", "--json"]).stdout);
    assert_eq!(rows[0]["status"], "modified");
    let rows = json(&sb.athena(&["show", "obra/*", "--json"]).stdout);
    assert_eq!(rows[0]["status"], "missing");
    let r = sb.athena(&["show", "other"]);
    assert!(
        r.stdout.contains("modified")
            && r.stdout
                .contains(&sb.skill_dir("other").display().to_string()),
        "{}",
        r.stdout
    );
}

// @spec CMD-OUT-004
#[test]
fn show_without_matches_exits_one() {
    let sb = Sandbox::new();
    listing_setup(&sb);
    let r = sb.athena(&["show", "zzz*", "--json"]);
    assert_eq!(r.code, 1);
    assert_eq!(r.stdout.trim(), "[]");
    assert_eq!(sb.athena(&["show", "zzz*"]).code, 1);
}

// @spec CMD-OUT-002
#[test]
fn diagnostics_go_to_stderr_uncolored_when_piped() {
    let sb = Sandbox::new();
    let mut cmd = sb.command(&["list"]);
    cmd.env_remove("NO_COLOR");
    let r = Sandbox::run_cmd(cmd);
    assert_eq!(r.code, 1);
    assert!(r.stderr.starts_with("ERROR"), "{}", r.stderr);
    assert!(!r.stderr.contains('\u{1b}'), "{:?}", r.stderr);
    assert!(r.stdout.is_empty());
}

// ---------- patterns and the skill layout check ----------

/// A fake remote shaped like a skill collection.
fn collection(sb: &Sandbox) -> String {
    sb.repo(
        "org/pack",
        &[
            ("README.md", "r"),
            ("skills/alpha/SKILL.md", "a"),
            ("skills/beta/SKILL.md", "b"),
            ("skills/beta/refs/notes.md", "n"),
            ("skills/.github/x.yml", "hidden"),
        ],
    )
}

// @spec CMD-ADD-009, SRC-NAME-004, CMD-ADD-006
#[test]
fn pattern_add_creates_one_entry_per_folder() {
    let sb = Sandbox::new();
    let sha = collection(&sb);
    let r = sb.athena(&["add", &format!("{}#:/skills/*", sb.url("org/pack"))]);
    assert_eq!(r.code, 0, "{}\n{}", r.stdout, r.stderr);
    let doc = sb.local_doc();
    for (name, sub) in [("alpha", "skills/alpha"), ("beta", "skills/beta")] {
        assert_eq!(field(&doc, "skills", name, "subpath").as_deref(), Some(sub));
        assert_eq!(field(&doc, "skills", name, "commit"), Some(sha.clone()));
        assert_eq!(field(&doc, "skills", name, "path"), None);
        assert!(sb.skill_dir(name).join("SKILL.md").exists());
    }
    assert!(
        field(&doc, "skills", ".github", "type").is_none(),
        "hidden folders are not matched"
    );
    assert!(sb.skill_dir("beta").join("refs/notes.md").exists());
    assert!(
        !sb.read_local().contains('*'),
        "the pattern is never stored"
    );
}

// @spec CMD-ADD-009
#[test]
fn pattern_add_from_a_local_source_skips_symlinked_folders() {
    let sb = Sandbox::new();
    let src = sb.tmp.path().join("local-pack");
    write(&src.join("skills/alpha/SKILL.md"), "a");
    write(&sb.tmp.path().join("elsewhere/SKILL.md"), "e");
    std::os::unix::fs::symlink(sb.tmp.path().join("elsewhere"), src.join("skills/linked")).unwrap();
    let r = sb.athena(&["add", &format!("{}#:/skills/*", src.display())]);
    assert_eq!(r.code, 0, "{}\n{}", r.stdout, r.stderr);
    let doc = sb.local_doc();
    assert_eq!(
        field(&doc, "skills", "alpha", "type").as_deref(),
        Some("local")
    );
    assert!(field(&doc, "skills", "linked", "type").is_none());
}

// @spec CMD-ADD-010
#[test]
fn brace_alternatives_that_match_nothing_fail_the_add() {
    let sb = Sandbox::new();
    collection(&sb);
    let r = sb.athena(&[
        "add",
        &format!("{}#:/skills/{{alpha,alpah}}", sb.url("org/pack")),
    ]);
    assert_eq!(r.code, 1);
    assert!(
        r.stderr.contains("skills/alpah") && !r.stderr.contains("skills/alpha "),
        "{}",
        r.stderr
    );
    assert!(!sb.local_file().exists());
    assert!(!sb.skill_dir("alpha").exists());
    let r = sb.athena(&["add", &format!("{}#:/nothing/*", sb.url("org/pack"))]);
    assert_eq!(r.code, 1);
    assert!(r.stderr.contains("nothing/*"), "{}", r.stderr);
}

// @spec CMD-ADD-011
#[test]
fn name_with_a_pattern_is_refused_before_any_fetch() {
    let sb = Sandbox::new();
    collection(&sb);
    let r = sb.athena(&[
        "add",
        &format!("{}#:/skills/*", sb.url("org/pack")),
        "--name",
        "x",
    ]);
    assert_eq!(r.code, 1);
    assert!(r.stderr.contains("--name"), "{}", r.stderr);
    assert!(!sb.cache_home.join("athena").exists());
}

// @spec CMD-ADD-012, CMD-ADD-017
#[test]
fn pattern_with_path_and_pin_places_and_pins_every_entry() {
    let sb = Sandbox::new();
    collection(&sb);
    let r = sb.athena(&[
        "add",
        &format!("{}#:/skills/*", sb.url("org/pack")),
        "--path",
        "./tools",
        "--pin",
    ]);
    assert_eq!(r.code, 0, "{}\n{}", r.stdout, r.stderr);
    let doc = sb.local_doc();
    for name in ["alpha", "beta"] {
        assert_eq!(
            field(&doc, "skills", name, "path"),
            Some(format!("./tools/{name}"))
        );
        assert_eq!(
            field(&doc, "skills", name, "pinned").as_deref(),
            Some("true")
        );
        assert!(
            sb.project
                .join("tools")
                .join(name)
                .join("SKILL.md")
                .exists()
        );
    }
}

// @spec CMD-ADD-013
#[test]
fn name_clashes_fail_the_whole_batch() {
    let sb = Sandbox::new();
    sb.repo(
        "org/two",
        &[
            ("x/same/SKILL.md", "x"),
            ("y/same/SKILL.md", "y"),
            ("y/other/SKILL.md", "o"),
        ],
    );
    let r = sb.athena(&["add", &format!("{}#:/{{x,y}}/*", sb.url("org/two"))]);
    assert_eq!(r.code, 1);
    assert!(r.stderr.contains("same"), "{}", r.stderr);
    assert!(!sb.local_file().exists());

    let src = sb.tmp.path().join("mine");
    write(&src.join("SKILL.md"), "m");
    assert_eq!(
        sb.athena(&[
            "add",
            &src.display().to_string(),
            "--name",
            "other",
            "--path",
            "./elsewhere"
        ])
        .code,
        0
    );
    let before = sb.read_local();
    let r = sb.athena(&["add", &format!("{}#:/y/*", sb.url("org/two"))]);
    assert_eq!(r.code, 1);
    assert!(r.stderr.contains("other"), "{}", r.stderr);
    assert_eq!(sb.read_local(), before);
    assert!(!sb.skill_dir("same").exists());
}

// @spec CMD-ADD-016
#[test]
fn rerunning_a_pattern_adds_only_new_folders() {
    let sb = Sandbox::new();
    collection(&sb);
    let src = format!("{}#:/skills/*", sb.url("org/pack"));
    assert_eq!(sb.athena(&["add", &src]).code, 0);
    let alpha_commit = field(&sb.local_doc(), "skills", "alpha", "commit");
    sb.push("org/pack", &[("skills/gamma/SKILL.md", "g")]);
    let r = sb.athena(&["add", &src]);
    assert_eq!(r.code, 0, "{}\n{}", r.stdout, r.stderr);
    assert!(
        r.stdout
            .lines()
            .any(|l| l.contains("alpha") && l.contains("already added")),
        "{}",
        r.stdout
    );
    let doc = sb.local_doc();
    assert!(field(&doc, "skills", "gamma", "type").is_some());
    assert_eq!(
        field(&doc, "skills", "alpha", "commit"),
        alpha_commit,
        "existing entries are untouched"
    );
}

// @spec CMD-ADD-014, CFG-CONF-002
#[test]
fn pattern_batches_never_gobble() {
    let sb = Sandbox::new();
    collection(&sb);
    let src = sb.tmp.path().join("tools-src");
    write(&src.join("SKILL.md"), "t");
    assert_eq!(
        sb.athena(&["add", &src.display().to_string(), "--path", "./tools"])
            .code,
        0
    );
    let before = sb.read_local();
    let r = sb.athena_tty(
        &[
            "add",
            &format!("{}#:/skills/*", sb.url("org/pack")),
            "--path",
            "./tools",
        ],
        "y",
    );
    assert_eq!(r.code, 1);
    assert!(!r.stdout.contains("[y/N]"), "must not prompt: {}", r.stdout);
    assert!(r.stdout.contains("on its own"), "{}", r.stdout);
    assert_eq!(sb.read_local(), before);
}

// @spec CMD-ADD-015
#[test]
fn one_blocked_entry_stops_the_whole_batch() {
    let sb = Sandbox::new();
    collection(&sb);
    write(&sb.skill_dir("beta").join("mine.md"), "user file");
    let r = sb.athena(&["add", &format!("{}#:/skills/*", sb.url("org/pack"))]);
    assert_eq!(r.code, 1);
    assert!(r.stderr.contains("beta"), "{}", r.stderr);
    assert!(!sb.skill_dir("alpha").exists(), "nothing installed");
    assert!(!sb.local_file().exists());
    assert!(staging_leftovers(&sb.project.join(".agents/skills")).is_empty());
}

// @spec CMD-ADD-015
#[test]
fn gobbled_children_survive_a_failed_parent_export() {
    let sb = Sandbox::new();
    gobble_setup(&sb);
    let parent = sb.tmp.path().join("srcs/parent");
    fs::remove_file(parent.join("SKILL.md")).unwrap();
    let r = sb.athena_tty(
        &["add", &parent.display().to_string(), "--path", "./tools"],
        "y",
    );
    assert_eq!(r.code, 1);
    assert!(
        sb.project.join("tools/one/a.md").exists(),
        "children are deleted only after the parent passes every check"
    );
    assert!(sb.project.join("tools/two/b.md").exists());
}

// @spec CMD-ADD-007
// macOS only: Linux has no unprivileged immutable flag to make the install fail.
#[cfg(target_os = "macos")]
#[test]
fn a_failure_mid_install_records_what_was_installed() {
    let sb = Sandbox::new();
    collection(&sb);
    write(&sb.skill_dir("beta").join("mine.md"), "locked");
    let locked = sb.skill_dir("beta");
    let chflags = |flag: &str| {
        assert!(
            std::process::Command::new("chflags")
                .args([flag, &locked.display().to_string()])
                .status()
                .unwrap()
                .success()
        )
    };
    chflags("uchg");
    let r = sb.athena(&[
        "add",
        &format!("{}#:/skills/*", sb.url("org/pack")),
        "--force",
    ]);
    chflags("nouchg");
    assert_eq!(r.code, 1, "{}\n{}", r.stdout, r.stderr);
    let doc = sb.local_doc();
    assert!(
        field(&doc, "skills", "alpha", "hash").is_some(),
        "installed entry is recorded"
    );
    assert!(
        field(&doc, "skills", "beta", "type").is_none(),
        "failed entry is not recorded"
    );
    assert!(sb.skill_dir("alpha").join("SKILL.md").exists());
}

// @spec CMD-SKILL-001, CMD-SKILL-004
#[test]
fn skills_without_skill_md_are_refused() {
    let sb = Sandbox::new();
    sb.repo(
        "org/mixed",
        &[
            ("skills/alpha/SKILL.md", "a"),
            ("skills/shared/util.md", "u"),
            ("skills/lower/skill.md", "l"),
        ],
    );
    let r = sb.athena(&["add", &format!("{}#:/skills/*", sb.url("org/mixed"))]);
    assert_eq!(r.code, 1);
    assert!(
        r.stderr.contains("shared") && r.stderr.contains("lower") && !r.stderr.contains("`alpha`"),
        "{}",
        r.stderr
    );
    assert!(
        r.stderr.contains("--no-skill-check") && r.stderr.contains('{'),
        "{}",
        r.stderr
    );
    assert!(!sb.local_file().exists());
    assert!(!sb.skill_dir("alpha").exists());

    let r = sb.athena(&[
        "add",
        &format!("{}#:/skills/*", sb.url("org/mixed")),
        "--no-skill-check",
    ]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(sb.skill_dir("shared").join("util.md").exists());
}

// @spec CMD-SKILL-001
#[test]
fn a_symlinked_skill_md_counts() {
    let sb = Sandbox::new();
    let src = sb.tmp.path().join("linked");
    write(&src.join("docs/skill.txt"), "s");
    std::os::unix::fs::symlink("docs/skill.txt", src.join("SKILL.md")).unwrap();
    let r = sb.athena(&["add", &src.display().to_string()]);
    assert_eq!(r.code, 0, "{}", r.stderr);
}

// @spec CMD-SKILL-002
#[test]
fn collections_get_an_exact_pattern_suggestion() {
    let sb = Sandbox::new();
    collection(&sb);
    let base = sb.url("org/pack");
    let r = sb.athena(&["add", &base]);
    assert_eq!(r.code, 1);
    assert!(
        r.stderr.contains(&format!("{base}#:/skills/*")),
        "{}",
        r.stderr
    );
    let r = sb.athena(&["add", &format!("{base}#main")]);
    assert!(
        r.stderr.contains(&format!("{base}#main:/skills/*")),
        "{}",
        r.stderr
    );

    sb.repo(
        "org/nested",
        &[
            ("pack/skills/x/SKILL.md", "x"),
            ("pack/skills/y/SKILL.md", "y"),
        ],
    );
    let nested = sb.url("org/nested");
    let r = sb.athena(&["add", &format!("{nested}#:/pack")]);
    assert_eq!(r.code, 1);
    assert!(
        r.stderr.contains(&format!("{nested}#:/pack/skills/*")),
        "{}",
        r.stderr
    );
}

// @spec CMD-SKILL-003
#[test]
fn mixed_depths_list_where_skill_md_files_are() {
    let sb = Sandbox::new();
    sb.repo("org/ragged", &[("a/SKILL.md", "a"), ("b/c/SKILL.md", "c")]);
    let r = sb.athena(&["add", &sb.url("org/ragged")]);
    assert_eq!(r.code, 1);
    assert!(
        r.stderr.contains("a/SKILL.md") && r.stderr.contains("b/c/SKILL.md"),
        "{}",
        r.stderr
    );
}

// @spec CMD-SKILL-005
#[test]
fn plugins_sync_and_update_skip_the_skill_check() {
    let sb = Sandbox::new();
    sb.repo("org/plain", &[("tool.md", "t")]);
    let r = sb.athena(&["add", &sb.url("org/plain"), "--plugin"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let sha = sb.repo("org/bare", &[("notes.md", "n")]);
    let mut text = sb.read_local();
    text.push_str(&remote_entry("org/bare", &sb.url("org/bare"), &sha, ""));
    sb.write_local(&text);
    assert_eq!(sb.athena(&["sync"]).code, 0);
    sb.push("org/bare", &[("notes.md", "n2")]);
    assert_eq!(sb.athena(&["update", "org/bare"]).code, 0);
}

// @spec CMD-OUT-005
#[test]
fn a_closed_stdout_does_not_abort_an_add() {
    let sb = Sandbox::new();
    collection(&sb);
    let mut cmd = sb.command(&["add", &format!("{}#:/skills/*", sb.url("org/pack"))]);
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    drop(child.stderr.take());
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(0));
    let doc = sb.local_doc();
    assert!(
        field(&doc, "skills", "alpha", "hash").is_some()
            && field(&doc, "skills", "beta", "hash").is_some()
    );
}
