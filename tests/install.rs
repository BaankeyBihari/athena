mod common;

use athena::install::{self, Action, Dest, Facts, Nesting, Removal};
use common::{TempDir, git, init_repo, write};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn tree(root: &Path, files: &[(&str, &str)]) -> PathBuf {
    for (n, c) in files {
        write(&root.join(n), c);
    }
    root.to_path_buf()
}

// @spec INST-HASH-001
#[test]
fn hash_covers_paths_bytes_mode_and_links_but_not_empty_dirs() {
    let tmp = TempDir::new();
    let t = tmp.path();
    let a = tree(&t.join("a"), &[("x.md", "1"), ("sub/y.md", "2")]);
    let b = tree(&t.join("b"), &[("sub/y.md", "2"), ("x.md", "1")]);
    let h = install::hash_dir(&a).unwrap();
    assert!(h.starts_with("sha256:") && h.len() == 7 + 64, "{h}");
    assert_eq!(h, install::hash_dir(&b).unwrap(), "same content, same hash");

    fs::create_dir_all(b.join("empty/dir")).unwrap();
    assert_eq!(
        h,
        install::hash_dir(&b).unwrap(),
        "empty directories ignored"
    );

    let c = tree(&t.join("c"), &[("x.md", "1"), ("sub/z.md", "2")]);
    assert_ne!(h, install::hash_dir(&c).unwrap(), "path matters");
    let d = tree(&t.join("d"), &[("x.md", "1"), ("sub/y.md", "3")]);
    assert_ne!(h, install::hash_dir(&d).unwrap(), "bytes matter");

    let e = tree(&t.join("e"), &[("x.md", "1"), ("sub/y.md", "2")]);
    fs::set_permissions(e.join("x.md"), fs::Permissions::from_mode(0o755)).unwrap();
    assert_ne!(h, install::hash_dir(&e).unwrap(), "executable bit matters");

    let l1 = tree(&t.join("l1"), &[("x.md", "1")]);
    std::os::unix::fs::symlink("x.md", l1.join("link")).unwrap();
    let l2 = tree(&t.join("l2"), &[("x.md", "1")]);
    std::os::unix::fs::symlink("other.md", l2.join("link")).unwrap();
    assert_ne!(
        install::hash_dir(&l1).unwrap(),
        install::hash_dir(&l2).unwrap(),
        "symlink target matters"
    );
    let l3 = tree(&t.join("l3"), &[("x.md", "1"), ("link", "x.md")]);
    assert_ne!(
        install::hash_dir(&l1).unwrap(),
        install::hash_dir(&l3).unwrap(),
        "kind matters"
    );
}

// @spec INST-HASH-002
#[test]
fn copied_tree_hashes_equal_to_source() {
    let tmp = TempDir::new();
    let src = tree(
        &tmp.path().join("src"),
        &[("a", "1"), ("d/b", "2"), (".git/HEAD", "ref")],
    );
    std::os::unix::fs::symlink("a", src.join("ln")).unwrap();
    let dst = tmp.path().join("dst");
    install::copy_tree(&src, &dst).unwrap();
    assert!(!dst.join(".git").exists());
    fs::remove_dir_all(src.join(".git")).unwrap();
    assert_eq!(
        install::hash_dir(&src).unwrap(),
        install::hash_dir(&dst).unwrap()
    );
}

// @spec INST-OBS-001
#[test]
fn staging_dirs_are_unique_siblings_of_the_destination() {
    let dest = Path::new("/x/skills/tool");
    let a = install::staging_dir(dest);
    let b = install::staging_dir(dest);
    assert_eq!(a.parent(), dest.parent());
    assert!(
        a.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(".athena-staging-")
    );
    assert_ne!(a, b);
}

// @spec INST-DIRTY-001, INST-DIRTY-002, INST-DIRTY-003, INST-DIRTY-004
#[test]
fn dirty_rule() {
    let present = Dest::Present {
        hash: "sha256:cur".into(),
    };
    // DIRTY-001
    assert!(install::is_clean(&Dest::Absent, None, None, false));
    assert!(install::is_clean(&Dest::Absent, None, None, true));
    // DIRTY-002: recorded hash decides, git is not consulted.
    assert!(install::is_clean(
        &present,
        Some("sha256:cur"),
        Some(false),
        false
    ));
    assert!(!install::is_clean(
        &present,
        Some("sha256:old"),
        Some(true),
        false
    ));
    // DIRTY-003: no recorded hash falls back to git.
    assert!(install::is_clean(&present, None, Some(true), false));
    assert!(!install::is_clean(&present, None, Some(false), false));
    assert!(!install::is_clean(&present, None, None, false));
    // DIRTY-004: a new entry over an existing directory is dirty even if git could recover it.
    assert!(!install::is_clean(&present, None, Some(true), true));
    // DIRTY-001: a directory holding no files has nothing to lose.
    let empty = Dest::Present {
        hash: install::empty_hash(),
    };
    assert!(install::is_clean(&empty, Some("sha256:old"), None, true));
}

fn facts(dest: Dest, recorded: Option<&str>, git_clean: Option<bool>) -> Facts {
    Facts {
        target_hash: "sha256:target".into(),
        dest,
        recorded_hash: recorded.map(str::to_string),
        git_clean,
        new_entry: false,
    }
}

const P: &str = "/nonexistent/skills/tool";

// @spec INST-PLAN-001, INST-PLAN-007
#[test]
fn matching_content_is_up_to_date() {
    let f = facts(
        Dest::Present {
            hash: "sha256:target".into(),
        },
        Some("sha256:target"),
        None,
    );
    assert_eq!(
        install::plan(&f, false, Path::new(P)),
        Action::UpToDate { record_hash: false }
    );
    let f = facts(
        Dest::Present {
            hash: "sha256:target".into(),
        },
        None,
        None,
    );
    assert_eq!(
        install::plan(&f, false, Path::new(P)),
        Action::UpToDate { record_hash: true }
    );
    let mut f = facts(
        Dest::Present {
            hash: "sha256:target".into(),
        },
        None,
        None,
    );
    f.new_entry = true;
    assert_eq!(
        install::plan(&f, false, Path::new(P)),
        Action::UpToDate { record_hash: true },
        "identical content is adoptable even for add"
    );
}

// @spec INST-PLAN-002
#[test]
fn clean_destination_is_installed() {
    assert_eq!(
        install::plan(&facts(Dest::Absent, None, None), false, Path::new(P)),
        Action::Install
    );
    let f = facts(
        Dest::Present {
            hash: "sha256:old".into(),
        },
        Some("sha256:old"),
        None,
    );
    assert_eq!(install::plan(&f, false, Path::new(P)), Action::Install);
}

// @spec INST-PLAN-003, INST-PLAN-004
#[test]
fn dirty_destination_needs_force() {
    let f = facts(
        Dest::Present {
            hash: "sha256:edited".into(),
        },
        Some("sha256:old"),
        None,
    );
    assert_eq!(install::plan(&f, true, Path::new(P)), Action::Install);
    assert_eq!(
        install::plan(&f, false, Path::new(P)),
        Action::Skip(format!("local modifications in `{P}`"))
    );
}

// @spec INST-PLAN-005
#[test]
fn non_directory_destination_is_always_refused() {
    let f = facts(Dest::NotADirectory, None, None);
    assert!(matches!(
        install::plan(&f, false, Path::new(P)),
        Action::Refuse(_)
    ));
    assert!(matches!(
        install::plan(&f, true, Path::new(P)),
        Action::Refuse(_)
    ));
}

fn existing(items: &[(&str, &str, bool)]) -> Vec<(String, PathBuf, bool)> {
    items
        .iter()
        .map(|(n, p, c)| (n.to_string(), PathBuf::from(p), *c))
        .collect()
}

// @spec INST-GOB-001, INST-GOB-006
#[test]
fn nesting_on_add() {
    let ex = existing(&[
        ("a", "/p/tools/a", true),
        ("b", "/p/tools/b", true),
        ("c", "/p/other", true),
    ]);
    assert_eq!(
        install::plan_nesting(Path::new("/p/tools"), &ex, false),
        Nesting::Gobble(vec!["a".into(), "b".into()])
    );
    assert_eq!(
        install::plan_nesting(Path::new("/p/tools/a/inner"), &ex, false),
        Nesting::InsideExisting("a".into())
    );
    assert_eq!(
        install::plan_nesting(Path::new("/p/tool"), &ex, false),
        Nesting::None
    );
    assert_eq!(
        install::plan_nesting(Path::new("/p/other2"), &ex, false),
        Nesting::None
    );
}

// @spec INST-GOB-002
#[test]
fn dirty_child_blocks_gobbling_without_force() {
    let ex = existing(&[("a", "/p/tools/a", true), ("b", "/p/tools/b", false)]);
    assert_eq!(
        install::plan_nesting(Path::new("/p/tools"), &ex, false),
        Nesting::DirtyChild("b".into())
    );
    assert_eq!(
        install::plan_nesting(Path::new("/p/tools"), &ex, true),
        Nesting::Gobble(vec!["a".into(), "b".into()])
    );
}

// @spec INST-GOB-003
#[test]
fn only_y_or_yes_confirms() {
    for yes in ["y", "Y", "yes", "YES", " yes\n"] {
        assert!(install::confirmed(yes), "{yes:?}");
    }
    for no in ["", "n", "no", "yep", "\n"] {
        assert!(!install::confirmed(no), "{no:?}");
    }
}

// @spec INST-RM-001, INST-RM-002, INST-RM-003, CFG-STATE-006
#[test]
fn removal_plans() {
    let present = Dest::Present { hash: "h".into() };
    assert_eq!(
        install::plan_remove(&present, true, false, false),
        Removal::DeleteAndEntry
    );
    assert_eq!(
        install::plan_remove(&Dest::Absent, true, false, false),
        Removal::EntryOnly
    );
    assert!(matches!(
        install::plan_remove(&present, false, false, false),
        Removal::Refuse(_)
    ));
    assert_eq!(
        install::plan_remove(&present, false, false, true),
        Removal::DeleteAndEntry
    );
    assert_eq!(
        install::plan_remove(&present, false, true, false),
        Removal::EntryOnly
    );
    assert_eq!(
        install::plan_remove(&present, true, true, true),
        Removal::EntryOnly
    );
}

#[test]
fn observes_destinations() {
    let tmp = TempDir::new();
    assert_eq!(
        install::observe_dest(&tmp.path().join("none")).unwrap(),
        Dest::Absent
    );
    write(&tmp.path().join("file"), "x");
    assert_eq!(
        install::observe_dest(&tmp.path().join("file")).unwrap(),
        Dest::NotADirectory
    );
    let d = tree(&tmp.path().join("d"), &[("a", "1")]);
    assert_eq!(
        install::observe_dest(&d).unwrap(),
        Dest::Present {
            hash: install::hash_dir(&d).unwrap()
        }
    );
}

// @spec INST-DIRTY-003
#[test]
fn git_clean_reports_tracked_unmodified_files() {
    let tmp = TempDir::new();
    assert_eq!(install::git_clean(tmp.path()), None, "outside a work tree");
    let repo = tmp.path().join("proj");
    init_repo(&repo, &[("skills/a/SKILL.md", "1"), ("other.txt", "o")]);
    let dest = repo.join("skills/a");
    assert_eq!(install::git_clean(&dest), Some(true));
    write(&repo.join("other.txt"), "changed elsewhere");
    assert_eq!(
        install::git_clean(&dest),
        Some(true),
        "changes outside the destination do not count"
    );
    write(&dest.join("SKILL.md"), "2");
    assert_eq!(install::git_clean(&dest), Some(false));
    git(&repo, &["checkout", "--", "."]);
    write(&dest.join("new.md"), "untracked");
    assert_eq!(install::git_clean(&dest), Some(false));
    fs::remove_file(dest.join("new.md")).unwrap();
    write(&repo.join(".gitignore"), "*.log\n");
    git(&repo, &["add", ".gitignore"]);
    git(
        &repo,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "ignore",
        ],
    );
    write(&dest.join("notes.log"), "ignored but not recoverable");
    assert_eq!(
        install::git_clean(&dest),
        Some(false),
        "ignored files count as modifications"
    );
}

// @spec INST-APPLY-001
#[test]
fn install_replaces_destination_and_creates_parents() {
    let tmp = TempDir::new();
    let dest = tmp.path().join("a/b/tool");
    let staging = install::staging_dir(&dest);
    tree(&staging, &[("new.md", "n")]);
    install::install_staged(&staging, &dest).unwrap();
    assert_eq!(fs::read_to_string(dest.join("new.md")).unwrap(), "n");
    assert!(!staging.exists());

    let staging = install::staging_dir(&dest);
    tree(&staging, &[("newer.md", "m")]);
    install::install_staged(&staging, &dest).unwrap();
    assert!(!dest.join("new.md").exists());
    assert!(dest.join("newer.md").exists());
    assert!(common::staging_leftovers(dest.parent().unwrap()).is_empty());
}

// @spec INST-APPLY-002
#[test]
fn failed_install_leaves_destination_intact() {
    let tmp = TempDir::new();
    let dest = tree(&tmp.path().join("tool"), &[("keep.md", "k")]);
    let missing = install::staging_dir(&dest);
    assert!(install::install_staged(&missing, &dest).is_err());
    assert_eq!(fs::read_to_string(dest.join("keep.md")).unwrap(), "k");
    let names: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["tool".to_string()]);
}
