use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn hex(digest: &[u8]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Every file and symlink under `dir` as (relative path, absolute path), sorted by relative path.
fn walk(dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    fn go(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<(), String> {
        let rd = fs::read_dir(dir).map_err(|e| format!("cannot read `{}`: {e}", dir.display()))?;
        for entry in rd {
            let p = entry.map_err(|e| e.to_string())?.path();
            let meta = fs::symlink_metadata(&p)
                .map_err(|e| format!("cannot read `{}`: {e}", p.display()))?;
            if meta.is_dir() {
                go(root, &p, out)?;
            } else {
                out.push((
                    p.strip_prefix(root).unwrap().to_string_lossy().into_owned(),
                    p,
                ));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    go(dir, dir, &mut out)?;
    out.sort();
    Ok(out)
}

// @spec INST-HASH-001, INST-HASH-002
pub fn hash_dir(dir: &Path) -> Result<String, String> {
    let mut h = Sha256::new();
    for (rel, abs) in walk(dir)? {
        let meta = fs::symlink_metadata(&abs).map_err(|e| e.to_string())?;
        let (kind, body) = if meta.file_type().is_symlink() {
            (
                b'L',
                fs::read_link(&abs)
                    .map_err(|e| e.to_string())?
                    .as_os_str()
                    .as_encoded_bytes()
                    .to_vec(),
            )
        } else {
            let exec = if meta.permissions().mode() & 0o111 != 0 {
                b'X'
            } else {
                b'F'
            };
            (
                exec,
                fs::read(&abs).map_err(|e| format!("cannot read `{}`: {e}", abs.display()))?,
            )
        };
        for part in [rel.as_bytes(), &[kind], &body] {
            h.update((part.len() as u64).to_le_bytes());
            h.update(part);
        }
    }
    Ok(format!("sha256:{}", hex(&h.finalize())))
}

/// The hash of a directory holding no files.
pub fn empty_hash() -> String {
    format!("sha256:{}", hex(&Sha256::new().finalize()))
}

/// Copies a directory tree, skipping `.git` and keeping symlinks as symlinks.
pub fn copy_tree(src: &Path, dst: &Path) -> Result<(), String> {
    let err = |p: &Path, e: std::io::Error| format!("cannot copy `{}`: {e}", p.display());
    fs::create_dir_all(dst).map_err(|e| err(dst, e))?;
    for entry in fs::read_dir(src).map_err(|e| err(src, e))? {
        let entry = entry.map_err(|e| err(src, e))?;
        if entry.file_name() == ".git" {
            continue;
        }
        let (from, to) = (entry.path(), dst.join(entry.file_name()));
        let ft = entry.file_type().map_err(|e| err(&from, e))?;
        if ft.is_symlink() {
            let target = fs::read_link(&from).map_err(|e| err(&from, e))?;
            std::os::unix::fs::symlink(target, &to).map_err(|e| err(&to, e))?;
        } else if ft.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|e| err(&from, e))?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub enum Dest {
    Absent,
    NotADirectory,
    Present { hash: String },
}

#[derive(Debug, Clone)]
pub struct Facts {
    pub target_hash: String,
    pub dest: Dest,
    pub recorded_hash: Option<String>,
    pub git_clean: Option<bool>,
    pub new_entry: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    UpToDate { record_hash: bool },
    Install,
    Skip(String),
    Refuse(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Nesting {
    None,
    InsideExisting(String),
    DirtyChild(String),
    Gobble(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Removal {
    EntryOnly,
    DeleteAndEntry,
    Refuse(String),
}

// @spec INST-DIRTY-001, INST-DIRTY-002, INST-DIRTY-003, INST-DIRTY-004
pub fn is_clean(
    dest: &Dest,
    recorded_hash: Option<&str>,
    git_clean: Option<bool>,
    new_entry: bool,
) -> bool {
    match dest {
        Dest::Absent => true,
        Dest::NotADirectory => false,
        Dest::Present { hash } if *hash == empty_hash() => true,
        Dest::Present { .. } if new_entry => false,
        Dest::Present { hash } => match recorded_hash {
            Some(r) => hash == r,
            None => git_clean == Some(true),
        },
    }
}

// @spec INST-PLAN-001, INST-PLAN-002, INST-PLAN-003, INST-PLAN-004, INST-PLAN-005, INST-PLAN-007
pub fn plan(facts: &Facts, force: bool, path: &Path) -> Action {
    match &facts.dest {
        Dest::NotADirectory => Action::Refuse(format!(
            "`{}` exists and is not a directory",
            path.display()
        )),
        Dest::Present { hash } if *hash == facts.target_hash => Action::UpToDate {
            record_hash: facts.recorded_hash.as_deref() != Some(hash.as_str()),
        },
        dest if force
            || is_clean(
                dest,
                facts.recorded_hash.as_deref(),
                facts.git_clean,
                facts.new_entry,
            ) =>
        {
            Action::Install
        }
        _ => Action::Skip(format!("local modifications in `{}`", path.display())),
    }
}

/// `existing` holds (name, resolved path, destination clean) for every entry already in the scope.
// @spec INST-GOB-001, INST-GOB-002, INST-GOB-006, INST-GOB-007
pub fn plan_nesting(new_path: &Path, existing: &[(String, PathBuf, bool)], force: bool) -> Nesting {
    if let Some((name, _, _)) = existing
        .iter()
        .find(|(_, p, _)| new_path.starts_with(p) && new_path != p)
    {
        return Nesting::InsideExisting(name.clone());
    }
    let children: Vec<_> = existing
        .iter()
        .filter(|(_, p, _)| p.starts_with(new_path) && p != new_path)
        .collect();
    if children.is_empty() {
        return Nesting::None;
    }
    if let Some((name, _, _)) = children.iter().find(|(_, _, clean)| !clean && !force) {
        return Nesting::DirtyChild(name.clone());
    }
    Nesting::Gobble(children.into_iter().map(|(n, _, _)| n.clone()).collect())
}

/// Whether a gobble prompt answer confirms.
pub fn confirmed(answer: &str) -> bool {
    matches!(answer.trim().to_lowercase().as_str(), "y" | "yes")
}

// @spec INST-RM-001, INST-RM-002, INST-RM-003, CFG-STATE-006
pub fn plan_remove(dest: &Dest, clean: bool, in_state_error: bool, force: bool) -> Removal {
    match dest {
        _ if in_state_error => Removal::EntryOnly,
        Dest::Absent => Removal::EntryOnly,
        Dest::NotADirectory => Removal::Refuse("destination is not a directory".into()),
        Dest::Present { .. } if clean || force => Removal::DeleteAndEntry,
        Dest::Present { .. } => Removal::Refuse(
            "destination has local modifications; use --force to delete them".into(),
        ),
    }
}

pub fn observe_dest(path: &Path) -> Result<Dest, String> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Dest::Absent),
        Err(e) => Err(format!("cannot read `{}`: {e}", path.display())),
        Ok(m) if m.is_dir() => Ok(Dest::Present {
            hash: hash_dir(path)?,
        }),
        Ok(_) => Ok(Dest::NotADirectory),
    }
}

/// Whether `dir` holds any file or symlink outside the `excluded` subtrees.
pub fn has_files_outside(dir: &Path, excluded: &[PathBuf]) -> Result<bool, String> {
    if !dir.is_dir() {
        return Ok(false);
    }
    Ok(walk(dir)?
        .iter()
        .any(|(_, abs)| !excluded.iter().any(|x| abs.starts_with(x))))
}

/// `Some(true)` when `path` is inside a git work tree and `git status --porcelain` is empty for it.
pub fn git_clean(path: &Path) -> Option<bool> {
    let out = Command::new("git")
        .args([
            "status",
            "--porcelain",
            "--ignored",
            "--untracked-files=all",
            "--",
            ".",
        ])
        .current_dir(path)
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout.is_empty())
}

/// A fresh staging directory path beside `dest`.
pub fn staging_dir(dest: &Path) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let name = format!(
        ".athena-staging-{}-{}-{nanos}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    );
    dest.parent().unwrap_or(Path::new("/")).join(name)
}

// @spec INST-APPLY-001, INST-APPLY-002
pub fn install_staged(staging: &Path, dest: &Path) -> Result<(), String> {
    if !staging.is_dir() {
        return Err(format!(
            "staging directory `{}` is missing",
            staging.display()
        ));
    }
    let backup = staging.with_extension("old");
    let had_dest = fs::symlink_metadata(dest).is_ok();
    let result = (|| {
        if had_dest {
            fs::rename(dest, &backup)?;
        }
        fs::rename(staging, dest)
    })();
    match result {
        Ok(()) => {
            if had_dest {
                let _ = fs::remove_dir_all(&backup);
            }
            Ok(())
        }
        Err(e) => {
            if had_dest && !dest.exists() {
                let _ = fs::rename(&backup, dest);
            }
            let _ = fs::remove_dir_all(staging);
            Err(format!("cannot install into `{}`: {e}", dest.display()))
        }
    }
}
