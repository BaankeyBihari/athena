mod common;

use athena::cache::Cache;
use common::{TempDir, commit, git, init_repo, write};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

struct Fixture {
    tmp: TempDir,
    cache: Cache,
    url: String,
}

/// A remote repo (addressed by its filesystem path) and an empty cache.
fn fixture(files: &[(&str, &str)]) -> (Fixture, String) {
    let tmp = TempDir::new();
    let repo = tmp.path().join("remote");
    let sha = init_repo(&repo, files);
    let cache = Cache::new(&tmp.path().join("cache/athena"));
    let url = repo.to_string_lossy().into_owned();
    (Fixture { tmp, cache, url }, sha)
}

fn remote(f: &Fixture) -> std::path::PathBuf {
    f.tmp.path().join("remote")
}

// @spec CACHE-001
#[test]
fn cache_dir_is_keyed_by_truncated_url_hash() {
    let cache = Cache::new(Path::new("/c/athena"));
    let url = "https://github.com/obra/superpowers.git";
    let key: String = Sha256::digest(url.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()[..16]
        .to_string();
    assert_eq!(cache.dir_for(url), Path::new("/c/athena/remotes").join(key));
    assert_ne!(
        cache.dir_for(url),
        cache.dir_for("git@github.com:obra/superpowers.git")
    );
}

// @spec CACHE-002
#[test]
fn git2_is_not_a_dependency() {
    let manifest = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    assert!(!manifest.contains("git2"));
}

// @spec CACHE-RES-001
#[test]
fn resolves_branches_tags_and_head() {
    let (f, first) = fixture(&[("SKILL.md", "v1")]);
    let r = remote(&f);
    git(&r, &["tag", "-a", "v1", "-m", "release"]);
    git(&r, &["checkout", "-q", "-b", "dev"]);
    let dev = commit(&r, &[("SKILL.md", "dev")]);
    git(&r, &["checkout", "-q", "main"]);
    assert_eq!(f.cache.resolve(&f.url, "main").unwrap(), first);
    assert_eq!(f.cache.resolve(&f.url, "HEAD").unwrap(), first);
    assert_eq!(f.cache.resolve(&f.url, "dev").unwrap(), dev);
    assert_eq!(
        f.cache.resolve(&f.url, "v1").unwrap(),
        first,
        "annotated tags peel to their commit"
    );
    assert!(f.cache.dir_for(&f.url).join(".git").exists());
}

// @spec CACHE-RES-002
#[test]
fn full_sha_resolves_to_itself() {
    let (f, first) = fixture(&[("SKILL.md", "v1")]);
    commit(&remote(&f), &[("SKILL.md", "v2")]);
    assert_eq!(f.cache.resolve(&f.url, &first).unwrap(), first);
}

// @spec CACHE-RES-003
#[test]
fn unresolvable_ref_names_ref_and_url() {
    let (f, _) = fixture(&[("SKILL.md", "v1")]);
    let e = f.cache.resolve(&f.url, "no-such-branch").unwrap_err();
    assert!(e.contains("no-such-branch") && e.contains(&f.url), "{e}");
    let e = f.cache.resolve(&f.url, "abc1234").unwrap_err();
    assert!(e.contains("full SHA"), "{e}");
}

// @spec CACHE-ENS-001
#[test]
fn cached_commit_needs_no_network() {
    let (f, sha) = fixture(&[("SKILL.md", "v1")]);
    f.cache.ensure(&f.url, &sha).unwrap();
    fs::rename(remote(&f), f.tmp.path().join("gone")).unwrap();
    f.cache.ensure(&f.url, &sha).unwrap();
}

// @spec CACHE-ENS-002
#[test]
fn ensures_commits_behind_the_branch_tip() {
    let (f, first) = fixture(&[("SKILL.md", "v1")]);
    let r = remote(&f);
    let second = commit(&r, &[("SKILL.md", "v2")]);
    commit(&r, &[("SKILL.md", "v3")]);
    f.cache.ensure(&f.url, &second).unwrap();
    f.cache.ensure(&f.url, &first).unwrap();
    let dir = f.cache.dir_for(&f.url);
    git(&dir, &["cat-file", "-e", &format!("{first}^{{commit}}")]);
}

// @spec CACHE-ENS-003
#[test]
fn missing_commit_error() {
    let (f, _) = fixture(&[("SKILL.md", "v1")]);
    let fake = "0123456789abcdef0123456789abcdef01234567";
    let e = f.cache.ensure(&f.url, fake).unwrap_err();
    assert!(
        e.contains(&format!("commit `{fake}` not found in `{}`", f.url)),
        "{e}"
    );
}

// @spec CACHE-EXP-001
#[test]
fn export_copies_subpath_without_git_dir() {
    let (f, _) = fixture(&[
        ("src/pg/plugin.json", "{}"),
        ("src/pg/lib/a.txt", "a"),
        ("README.md", "r"),
    ]);
    let r = remote(&f);
    std::os::unix::fs::symlink("plugin.json", r.join("src/pg/link.json")).unwrap();
    let sha = commit(&r, &[]);
    f.cache.ensure(&f.url, &sha).unwrap();

    let staging = f.tmp.path().join("out/pg");
    f.cache
        .export(&f.url, &sha, Some("src/pg"), &staging)
        .unwrap();
    assert_eq!(fs::read_to_string(staging.join("lib/a.txt")).unwrap(), "a");
    assert!(!staging.join("README.md").exists());
    assert_eq!(
        fs::read_link(staging.join("link.json")).unwrap(),
        Path::new("plugin.json")
    );

    let whole = f.tmp.path().join("out/whole");
    f.cache.export(&f.url, &sha, None, &whole).unwrap();
    assert!(whole.join("README.md").exists());
    assert!(!whole.join(".git").exists());
}

// @spec CACHE-EXP-001
#[test]
fn export_uses_the_requested_commit() {
    let (f, first) = fixture(&[("SKILL.md", "v1")]);
    let second = commit(&remote(&f), &[("SKILL.md", "v2")]);
    f.cache.ensure(&f.url, &second).unwrap();
    f.cache.ensure(&f.url, &first).unwrap();
    let out = f.tmp.path().join("out");
    f.cache
        .export(&f.url, &first, None, &out.join("a"))
        .unwrap();
    f.cache
        .export(&f.url, &second, None, &out.join("b"))
        .unwrap();
    assert_eq!(fs::read_to_string(out.join("a/SKILL.md")).unwrap(), "v1");
    assert_eq!(fs::read_to_string(out.join("b/SKILL.md")).unwrap(), "v2");
}

// @spec CACHE-EXP-002
#[test]
fn export_rejects_missing_or_file_subpaths() {
    let (f, sha) = fixture(&[("file.txt", "x")]);
    f.cache.ensure(&f.url, &sha).unwrap();
    let out = f.tmp.path().join("out");
    let e = f
        .cache
        .export(&f.url, &sha, Some("nope"), &out.join("a"))
        .unwrap_err();
    assert!(e.contains("nope") && e.contains(&sha), "{e}");
    let e = f
        .cache
        .export(&f.url, &sha, Some("file.txt"), &out.join("b"))
        .unwrap_err();
    assert!(e.contains("file.txt"), "{e}");
}

// @spec CACHE-ERR-002
#[test]
fn broken_or_foreign_cache_dirs_are_recreated() {
    let (f, sha) = fixture(&[("SKILL.md", "v1")]);
    let dir = f.cache.dir_for(&f.url);
    write(&dir.join("junk"), "not a repo");
    f.cache.ensure(&f.url, &sha).unwrap();
    assert!(!dir.join("junk").exists());

    let other = f.tmp.path().join("other");
    init_repo(&other, &[("x", "y")]);
    fs::remove_dir_all(&dir).unwrap();
    init_repo(&dir, &[("stale", "s")]);
    git(&dir, &["remote", "add", "origin", &other.to_string_lossy()]);
    f.cache.ensure(&f.url, &sha).unwrap();
    assert_eq!(git(&dir, &["config", "remote.origin.url"]), f.url);
}

// @spec CACHE-LIST-001
#[test]
fn lists_every_folder_at_a_commit_offline() {
    let (f, _) = fixture(&[
        ("skills/b/SKILL.md", "b"),
        ("skills/a/SKILL.md", "a"),
        ("skills/a/refs/x.md", "x"),
        ("README.md", "r"),
    ]);
    let r = remote(&f);
    let first = git(&r, &["rev-parse", "HEAD"]);
    commit(&r, &[("later/SKILL.md", "l")]);
    f.cache.ensure(&f.url, &first).unwrap();
    fs::rename(remote(&f), f.tmp.path().join("gone")).unwrap();
    let dirs = f.cache.list_dirs(&f.url, &first).unwrap();
    assert_eq!(
        dirs,
        vec!["skills", "skills/a", "skills/a/refs", "skills/b"]
    );
}
