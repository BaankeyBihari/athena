mod common;

use athena::cache::Cache;
use common::{TempDir, commit, init_repo};
use std::fs;
use std::os::unix::fs::PermissionsExt;

struct Fixture {
    tmp: TempDir,
    cache: Cache,
    url: String,
    sha: String,
}

fn fixture() -> Fixture {
    let tmp = TempDir::new();
    let repo = tmp.path().join("remote");
    init_repo(
        &repo,
        &[
            ("rules/b.md", "b\n"),
            ("rules/a.md", "a\n"),
            ("rules/deep/c.md", "c\n"),
            ("run.sh", "#!/bin/sh\n"),
        ],
    );
    fs::set_permissions(repo.join("run.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    std::os::unix::fs::symlink("a.md", repo.join("rules/link.md")).unwrap();
    let sha = commit(&repo, &[]);
    let cache = Cache::new(&tmp.path().join("cache/athena"));
    let url = repo.to_string_lossy().into_owned();
    cache.ensure(&url, &sha).unwrap();
    Fixture {
        tmp,
        cache,
        url,
        sha,
    }
}

// @spec CACHE-LIST-002
#[test]
fn list_files_returns_sorted_regular_files_only() {
    let f = fixture();
    let files = f.cache.list_files(&f.url, &f.sha).unwrap();
    assert_eq!(
        files,
        ["rules/a.md", "rules/b.md", "rules/deep/c.md", "run.sh"],
        "no folders, no symlinks"
    );
}

// @spec CACHE-EXP-003
#[test]
fn export_file_copies_one_file_and_keeps_the_executable_bit() {
    let f = fixture();
    let out = f.tmp.path().join("staged-a");
    f.cache
        .export_file(&f.url, &f.sha, "rules/a.md", &out)
        .unwrap();
    assert!(out.is_file());
    assert_eq!(fs::read_to_string(&out).unwrap(), "a\n");

    let exe = f.tmp.path().join("staged-run");
    f.cache.export_file(&f.url, &f.sha, "run.sh", &exe).unwrap();
    assert_eq!(
        fs::metadata(&exe).unwrap().permissions().mode() & 0o111,
        0o111
    );
}

// @spec CACHE-EXP-004
#[test]
fn export_file_refuses_anything_but_a_regular_file() {
    let f = fixture();
    for sub in ["rules/missing.md", "rules", "rules/link.md"] {
        let out = f.tmp.path().join("staged");
        let err = f.cache.export_file(&f.url, &f.sha, sub, &out).unwrap_err();
        assert!(err.contains(sub) && err.contains(&f.sha), "{sub}: {err}");
        assert!(!out.exists(), "{sub}");
    }
}
