use athena::source::{ParsedSource, SourceKind, alternatives, is_pattern, matches, parse};
use std::path::{Path, PathBuf};

fn ok(input: &str) -> ParsedSource {
    parse(input, Path::new("/work/proj"), Path::new("/home/u")).unwrap()
}

fn err(input: &str) -> String {
    parse(input, Path::new("/work/proj"), Path::new("/home/u")).unwrap_err()
}

fn url(input: &str) -> String {
    match ok(input).kind {
        SourceKind::Remote { url } => url,
        other => panic!("expected remote, got {other:?}"),
    }
}

fn local(input: &str) -> PathBuf {
    match ok(input).kind {
        SourceKind::Local { path } => path,
        other => panic!("expected local, got {other:?}"),
    }
}

// @spec SRC-001
#[test]
fn host_shorthand_becomes_https_with_git_suffix() {
    assert_eq!(
        url("github.com:org/repo"),
        "https://github.com/org/repo.git"
    );
    assert_eq!(
        url("gitlab.example.com:group/repo.git"),
        "https://gitlab.example.com/group/repo.git"
    );
}

// @spec SRC-002
#[test]
fn scp_style_ssh_is_kept_unchanged() {
    assert_eq!(
        url("git@github.com:org/repo.git"),
        "git@github.com:org/repo.git"
    );
    assert_eq!(url("deploy@host.internal:a/b"), "deploy@host.internal:a/b");
}

// @spec SRC-003
#[test]
fn scheme_urls_lose_only_trailing_slashes() {
    assert_eq!(
        url("https://github.com/org/repo/"),
        "https://github.com/org/repo"
    );
    assert_eq!(url("http://host.example/x//"), "http://host.example/x");
    assert_eq!(
        url("ssh://git@host.example/x.git"),
        "ssh://git@host.example/x.git"
    );
    assert_eq!(url("git://host.example/x"), "git://host.example/x");
    assert_eq!(
        url("https://dev.azure.com/o/p/_git/r"),
        "https://dev.azure.com/o/p/_git/r"
    );
}

// @spec SRC-004
#[test]
fn local_forms_become_absolute_paths() {
    assert_eq!(local("/abs/linter"), PathBuf::from("/abs/linter"));
    assert_eq!(
        local("./plugins/linter"),
        PathBuf::from("/work/proj/plugins/linter")
    );
    assert_eq!(
        local("../shared/linter"),
        PathBuf::from("/work/shared/linter")
    );
    assert_eq!(local("~/my/linter"), PathBuf::from("/home/u/my/linter"));
    assert_eq!(local("file:///opt/linter"), PathBuf::from("/opt/linter"));
}

// @spec SRC-005
#[test]
fn unrecognized_forms_list_accepted_forms() {
    for bad in ["just-words", "localhost:repo", ""] {
        let e = err(bad);
        assert!(
            e.contains("https://") && e.contains("git@") && e.contains("./"),
            "{bad:?}: {e}"
        );
    }
}

// @spec SRC-FRAG-001, SRC-FRAG-002
#[test]
fn fragment_splits_into_ref_and_subpath() {
    let p = ok("https://github.com/org/repo.git#branch/name:/sub/path");
    assert_eq!(
        p.kind,
        SourceKind::Remote {
            url: "https://github.com/org/repo.git".into()
        }
    );
    assert_eq!(p.reference.as_deref(), Some("branch/name"));
    assert_eq!(p.subpath.as_deref(), Some("sub/path"));

    let p = ok("github.com:org/repo#v1.0");
    assert_eq!(p.reference.as_deref(), Some("v1.0"));
    assert_eq!(p.subpath, None);
}

// @spec SRC-FRAG-003
#[test]
fn empty_ref_means_no_ref() {
    let p = ok("https://github.com/org/repo.git#:/src/postgres");
    assert_eq!(p.reference, None);
    assert_eq!(p.subpath.as_deref(), Some("src/postgres"));
}

// @spec SRC-FRAG-004
#[test]
fn subpath_loses_leading_slashes() {
    assert_eq!(
        ok("https://h.example/o/r.git#main:///a/b")
            .subpath
            .as_deref(),
        Some("a/b")
    );
    assert_eq!(
        ok("https://h.example/o/r.git#main:a/b").subpath.as_deref(),
        Some("a/b")
    );
}

// @spec SRC-FRAG-005
#[test]
fn empty_or_escaping_subpath_is_rejected() {
    assert!(err("https://h.example/o/r.git#main:/").contains("subpath"));
    assert!(err("https://h.example/o/r.git#main:a/../b").contains("a/../b"));
    assert!(err("https://h.example/o/r.git#:../up").contains("../up"));
}

// @spec SRC-FRAG-006
#[test]
fn local_sources_reject_refs_but_accept_subpaths() {
    assert!(err("/plugins/linter#main").contains("local"));
    let p = ok("/plugins/repo#:linter");
    assert_eq!(p.subpath.as_deref(), Some("linter"));
    assert_eq!(p.reference, None);
}

// @spec SRC-NAME-001
#[test]
fn remote_with_subpath_is_named_after_last_subpath_component() {
    assert_eq!(
        ok("https://github.com/modelcontextprotocol/servers.git#:src/postgres").default_name,
        "postgres"
    );
}

// @spec SRC-NAME-002
#[test]
fn remote_without_subpath_is_named_owner_slash_repo() {
    assert_eq!(
        ok("https://github.com/obra/superpowers.git").default_name,
        "obra/superpowers"
    );
    assert_eq!(
        ok("git@github.com:obra/superpowers.git").default_name,
        "obra/superpowers"
    );
    assert_eq!(
        ok("github.com:obra/superpowers").default_name,
        "obra/superpowers"
    );
    assert_eq!(ok("https://host.example/single").default_name, "single");
}

// @spec SRC-NAME-003
#[test]
fn local_is_named_after_last_component() {
    assert_eq!(
        ok("/Users/dev/my-local-plugins/linter").default_name,
        "linter"
    );
    assert_eq!(ok("/Users/dev/repo#:a/b").default_name, "b");
}

// @spec SRC-FRAG-007
#[test]
fn leading_slash_fragment_is_a_subpath_without_ref() {
    let p = ok("github.com:jszmajda/lid#/plugins/arrow-maintenance/skills");
    assert_eq!(p.reference, None);
    assert_eq!(
        p.subpath.as_deref(),
        Some("plugins/arrow-maintenance/skills")
    );
    let p = ok("github.com:jszmajda/lid#main/plugins/x");
    assert_eq!(
        p.reference.as_deref(),
        Some("main/plugins/x"),
        "a ref may contain `/`"
    );
    assert_eq!(p.subpath, None);
}

// @spec SRC-PAT-001
#[test]
fn pattern_subpaths_are_detected_and_kept() {
    assert!(is_pattern("skills/*"));
    assert!(is_pattern("skills/?x"));
    assert!(is_pattern("skills/{a,b}"));
    assert!(!is_pattern("skills/a"));
    let p = ok("github.com:jszmajda/lid#main:/plugins/*/skills/{a,b}");
    assert_eq!(p.reference.as_deref(), Some("main"));
    assert_eq!(p.subpath.as_deref(), Some("plugins/*/skills/{a,b}"));
}

// @spec SRC-PAT-002
#[test]
fn patterns_match_component_by_component() {
    assert!(matches("skills/*", "skills/brainstorming"));
    assert!(matches("skills/*", "skills/x"));
    assert!(!matches("skills/*", "skills"));
    assert!(!matches("skills/*", "skills/a/b"), "`*` never crosses `/`");
    assert!(matches("plugins/*/skills/*", "plugins/lid/skills/coach"));
    assert!(matches("skills/?b", "skills/ab"));
    assert!(!matches("skills/?b", "skills/abb"));
    assert!(matches("skills/map-*", "skills/map-codebase"));
    assert!(!matches("skills/a", "skills/ab"));
}

// @spec SRC-PAT-003
#[test]
fn wildcards_skip_hidden_folders() {
    assert!(!matches("skills/*", "skills/.github"));
    assert!(!matches("skills/?github", "skills/.github"));
    assert!(matches("skills/.git*", "skills/.github"));
}

// @spec SRC-PAT-004
#[test]
fn braces_expand_in_written_order() {
    assert_eq!(
        alternatives("a/{x,y}/{1,2}").unwrap(),
        vec!["a/x/1", "a/x/2", "a/y/1", "a/y/2"]
    );
    assert_eq!(
        alternatives("skills/{map-codebase,arrow-maintenance}").unwrap(),
        vec!["skills/map-codebase", "skills/arrow-maintenance"]
    );
    assert_eq!(alternatives("skills/*").unwrap(), vec!["skills/*"]);
}

// @spec SRC-PAT-005
#[test]
fn malformed_braces_are_rejected() {
    for bad in [
        "a/{x",
        "a/x}",
        "a/{x}",
        "a/{x,}",
        "a/{,x}",
        "a/{x,{y,z}}",
        "a/{x/y,z}",
    ] {
        let e = err(&format!("https://h.example/o/r.git#:/{bad}"));
        assert!(e.contains(bad), "{bad}: {e}");
        assert!(alternatives(bad).is_err(), "{bad}");
    }
}
