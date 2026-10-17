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

// @spec SRC-001, SRC-002, SRC-003, SRC-004, SRC-005, SRC-FRAG-001, SRC-FRAG-002, SRC-FRAG-003, SRC-FRAG-004, SRC-FRAG-005, SRC-FRAG-006, SRC-NAME-001, SRC-NAME-002, SRC-NAME-003
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
                Some((r, s)) => (r, Some(s)),
                None => (f, None),
            };
            let sub = sub.map(|s| s.trim_start_matches('/')).map(|s| {
                if s.is_empty() {
                    Err(format!("empty subpath in source `{input}`"))
                } else if s.split('/').any(|c| c == "..") {
                    Err(format!("subpath `{s}` must not contain `..`"))
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

fn last(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
