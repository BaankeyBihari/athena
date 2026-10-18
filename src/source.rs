use crate::config::{expand_home, normalize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum SourceKind {
    Remote { url: String },
    Local { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedSource {
    pub kind: SourceKind,
    pub reference: Option<String>,
    pub subpath: Option<String>,
    pub default_name: String,
}

const FORMS: &str = "accepted sources: `<host>:<org>/<repo>`, `git@<host>:<path>`, `https://…`, `http://…`, `ssh://…`, `git://…`, or a local path starting with `/`, `./`, `../`, `~/`, or `file://`";

// @spec SRC-001, SRC-002, SRC-003, SRC-004, SRC-005, SRC-FRAG-001, SRC-FRAG-002, SRC-FRAG-003, SRC-FRAG-004, SRC-FRAG-005, SRC-FRAG-006, SRC-FRAG-007, SRC-NAME-001, SRC-NAME-002, SRC-NAME-003, SRC-PAT-001
pub fn parse(input: &str, cwd: &Path, home: &Path) -> Result<ParsedSource, String> {
    let (base, fragment) = match input.split_once('#') {
        Some((b, f)) => (b, Some(f)),
        None => (input, None),
    };
    let kind = classify(base, cwd, home)
        .ok_or_else(|| format!("unrecognized source `{input}`; {FORMS}"))?;

    let (reference, subpath) = match fragment {
        None => (None, None),
        Some(f) => {
            let (r, sub) = match f.split_once(':') {
                _ if f.starts_with('/') => ("", Some(f)),
                Some((r, s)) => (r, Some(s)),
                None => (f, None),
            };
            let sub = sub.map(|s| s.trim_start_matches('/')).map(|s| {
                if s.is_empty() {
                    Err(format!("empty subpath in source `{input}`"))
                } else if s.split('/').any(|c| c == "..") {
                    Err(format!("subpath `{s}` must not contain `..`"))
                } else if is_pattern(s) {
                    alternatives(s).map(|_| s.trim_end_matches('/').to_string())
                } else {
                    Ok(s.trim_end_matches('/').to_string())
                }
            });
            (
                Some(r).filter(|r| !r.is_empty()).map(str::to_string),
                sub.transpose()?,
            )
        }
    };
    if matches!(kind, SourceKind::Local { .. }) && reference.is_some() {
        return Err(format!(
            "local source `{input}` accepts only a subpath fragment (`#:<subpath>`), not a ref"
        ));
    }

    let default_name = match (&subpath, &kind) {
        (Some(s), _) => last(s).to_string(),
        (None, SourceKind::Local { path }) => path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        (None, SourceKind::Remote { url }) => {
            let comps: Vec<&str> = url_path(url).split('/').filter(|c| !c.is_empty()).collect();
            let tail = &comps[comps.len().saturating_sub(2)..];
            tail.join("/").trim_end_matches(".git").to_string()
        }
    };
    Ok(ParsedSource {
        kind,
        reference,
        subpath,
        default_name,
    })
}

fn classify(base: &str, cwd: &Path, home: &Path) -> Option<SourceKind> {
    if let Some(p) = base.strip_prefix("file://") {
        return Some(SourceKind::Local {
            path: normalize(Path::new(p)),
        });
    }
    if ["/", "./", "../", "~/"].iter().any(|p| base.starts_with(p)) {
        return Some(SourceKind::Local {
            path: normalize(&cwd.join(expand_home(base, home))),
        });
    }
    if ["ssh://", "https://", "http://", "git://"]
        .iter()
        .any(|p| base.starts_with(p))
    {
        return Some(SourceKind::Remote {
            url: base.trim_end_matches('/').to_string(),
        });
    }
    let (host, path) = base.split_once(':')?;
    if host.contains('@') && !path.is_empty() {
        return Some(SourceKind::Remote {
            url: base.to_string(),
        });
    }
    if host.contains('.') && !host.contains('/') && !path.is_empty() {
        let path = path.trim_start_matches('/');
        let suffix = if path.ends_with(".git") { "" } else { ".git" };
        return Some(SourceKind::Remote {
            url: format!("https://{host}/{path}{suffix}"),
        });
    }
    None
}

/// The repository path portion of a remote URL (after the host).
fn url_path(url: &str) -> &str {
    match url.split_once("://") {
        Some((_, rest)) => rest.split_once('/').map_or("", |(_, p)| p),
        None => url.split_once(':').map_or(url, |(_, p)| p),
    }
}

pub fn last(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub fn is_pattern(subpath: &str) -> bool {
    subpath.contains(['*', '?', '{', '}'])
}

/// Expands the braces of a pattern into one brace-free pattern per combination.
// @spec SRC-PAT-004, SRC-PAT-005
pub fn alternatives(pattern: &str) -> Result<Vec<String>, String> {
    let bad = |why: &str| format!("invalid pattern `{pattern}`: {why}");
    let mut out = vec![String::new()];
    let mut rest = pattern;
    while let Some(open) = rest.find(['{', '}']) {
        if rest[open..].starts_with('}') {
            return Err(bad("`}` without `{`"));
        }
        let close = rest[open..]
            .find('}')
            .map(|c| open + c)
            .ok_or_else(|| bad("unclosed `{`"))?;
        let inner = &rest[open + 1..close];
        if inner.contains('{') {
            return Err(bad("braces cannot nest"));
        }
        let alts: Vec<&str> = inner.split(',').collect();
        if alts.len() < 2 || alts.iter().any(|a| a.is_empty() || a.contains('/')) {
            return Err(bad(
                "braces need two or more non-empty alternatives without `/`",
            ));
        }
        let prefix = &rest[..open];
        out = out
            .iter()
            .flat_map(|o| alts.iter().map(move |a| format!("{o}{prefix}{a}")))
            .collect();
        rest = &rest[close + 1..];
    }
    Ok(out.into_iter().map(|o| o + rest).collect())
}

/// Whether a brace-free pattern matches a folder path, one component at a time.
// @spec SRC-PAT-002, SRC-PAT-003
pub fn matches(pattern: &str, dir: &str) -> bool {
    let (pats, dirs): (Vec<&str>, Vec<&str>) =
        (pattern.split('/').collect(), dir.split('/').collect());
    pats.len() == dirs.len()
        && pats.iter().zip(&dirs).all(|(p, d)| {
            let hidden = d.starts_with('.') && !p.starts_with('.');
            !(hidden && p.contains(['*', '?'])) && wildmatch::WildMatch::new(p).matches(d)
        })
}
