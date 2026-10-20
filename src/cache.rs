use crate::config::is_locked;
use crate::install::copy_tree;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Runs `git` with `args`, in `dir` when given. Returns trimmed stdout.
// @spec CACHE-002, CACHE-ERR-001
pub fn git(dir: Option<&Path>, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    let out = cmd.output().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => "`git` is required but was not found on PATH".to_string(),
        _ => format!("cannot run git: {e}"),
    })?;
    if !out.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub struct Cache {
    root: PathBuf,
}

impl Cache {
    /// `cache_dir` is `<cache_home>/athena`.
    pub fn new(cache_dir: &Path) -> Cache {
        Cache {
            root: cache_dir.join("remotes"),
        }
    }

    // @spec CACHE-001
    pub fn dir_for(&self, url: &str) -> PathBuf {
        let hex: String = Sha256::digest(url.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        self.root.join(&hex[..16])
    }

    /// The cache repository for `url`, recreated when missing, broken, or pointing elsewhere.
    // @spec CACHE-ERR-002
    fn repo(&self, url: &str) -> Result<PathBuf, String> {
        let dir = self.dir_for(url);
        let healthy = dir.join(".git").is_dir()
            && git(Some(&dir), &["config", "--get", "remote.origin.url"]).is_ok_and(|u| u == url);
        if !healthy {
            if dir.exists() {
                fs::remove_dir_all(&dir)
                    .map_err(|e| format!("cannot reset cache `{}`: {e}", dir.display()))?;
            }
            fs::create_dir_all(&dir)
                .map_err(|e| format!("cannot create cache `{}`: {e}", dir.display()))?;
            git(Some(&dir), &["init", "-q"])?;
            git(Some(&dir), &["remote", "add", "origin", url])?;
        }
        Ok(dir)
    }

    // @spec CACHE-RES-001, CACHE-RES-002, CACHE-RES-003
    pub fn resolve(&self, url: &str, reference: &str) -> Result<String, String> {
        if is_locked(reference) {
            self.ensure(url, reference)?;
            return Ok(reference.to_string());
        }
        let dir = self.repo(url)?;
        git(
            Some(&dir),
            &["fetch", "-q", "--depth", "1", "origin", reference],
        )
        .map_err(|e| {
            format!(
                "cannot resolve `{reference}` in `{url}`; use a full SHA, a branch, or a tag ({e})"
            )
        })?;
        git(Some(&dir), &["rev-parse", "FETCH_HEAD^{commit}"])
    }

    // @spec CACHE-ENS-001, CACHE-ENS-002, CACHE-ENS-003
    pub fn ensure(&self, url: &str, sha: &str) -> Result<(), String> {
        let dir = self.repo(url)?;
        let present = || {
            git(
                Some(&dir),
                &["cat-file", "-e", &format!("{sha}^{{commit}}")],
            )
            .is_ok()
        };
        if present() {
            return Ok(());
        }
        let shallow = git(Some(&dir), &["fetch", "-q", "--depth", "1", "origin", sha]);
        if shallow.is_ok() && present() {
            return Ok(());
        }
        let mut full = vec!["fetch", "-q"];
        if dir.join(".git/shallow").exists() {
            full.push("--unshallow");
        }
        full.extend(["origin", "+refs/heads/*:refs/remotes/origin/*"]);
        let fetched = git(Some(&dir), &full);
        if present() {
            return Ok(());
        }
        let detail = fetched
            .err()
            .or(shallow.err())
            .map(|e| format!(" ({e})"))
            .unwrap_or_default();
        Err(format!("commit `{sha}` not found in `{url}`{detail}"))
    }

    /// Every folder in the commit's tree, sorted. The commit must already be present.
    // @spec CACHE-LIST-001
    pub fn list_dirs(&self, url: &str, sha: &str) -> Result<Vec<String>, String> {
        let dir = self.repo(url)?;
        let out = git(Some(&dir), &["ls-tree", "-r", "-t", "-z", sha])?;
        let mut dirs: Vec<String> = out
            .split('\0')
            .filter_map(|rec| rec.split_once('\t'))
            .filter(|(meta, _)| meta.split(' ').nth(1) == Some("tree"))
            .map(|(_, path)| path.to_string())
            .collect();
        dirs.sort();
        Ok(dirs)
    }

    /// Every regular file in the commit's tree, sorted; symlinks and submodules are left out.
    // @spec CACHE-LIST-002
    pub fn list_files(&self, url: &str, sha: &str) -> Result<Vec<String>, String> {
        let dir = self.repo(url)?;
        let out = git(Some(&dir), &["ls-tree", "-r", "-z", sha])?;
        let mut files: Vec<String> = out
            .split('\0')
            .filter_map(|rec| rec.split_once('\t'))
            .filter(|(meta, _)| matches!(meta.split(' ').next(), Some("100644" | "100755")))
            .map(|(_, path)| path.to_string())
            .collect();
        files.sort();
        Ok(files)
    }

    fn checkout(&self, url: &str, sha: &str) -> Result<PathBuf, String> {
        let dir = self.repo(url)?;
        git(
            Some(&dir),
            &[
                "-c",
                "advice.detachedHead=false",
                "checkout",
                "-q",
                "--detach",
                "--force",
                sha,
            ],
        )?;
        Ok(dir)
    }

    /// Copies the one regular file at `subpath` to `staging`, keeping its executable bit.
    // @spec CACHE-EXP-003, CACHE-EXP-004
    pub fn export_file(
        &self,
        url: &str,
        sha: &str,
        subpath: &str,
        staging: &Path,
    ) -> Result<(), String> {
        let src = self.checkout(url, sha)?.join(subpath);
        match fs::symlink_metadata(&src) {
            Err(_) => Err(format!(
                "subpath `{subpath}` does not exist at commit `{sha}`"
            )),
            Ok(m) if !m.is_file() => Err(format!(
                "subpath `{subpath}` at commit `{sha}` is not a regular file"
            )),
            Ok(_) => fs::copy(&src, staging)
                .map(|_| ())
                .map_err(|e| format!("cannot copy `{}`: {e}", src.display())),
        }
    }

    // @spec CACHE-EXP-001, CACHE-EXP-002
    pub fn export(
        &self,
        url: &str,
        sha: &str,
        subpath: Option<&str>,
        staging: &Path,
    ) -> Result<(), String> {
        let dir = self.checkout(url, sha)?;
        let src = dir.join(subpath.unwrap_or(""));
        if let Some(s) = subpath {
            if !src.exists() {
                return Err(format!("subpath `{s}` does not exist at commit `{sha}`"));
            }
            if !src.is_dir() {
                return Err(format!(
                    "subpath `{s}` at commit `{sha}` is not a directory"
                ));
            }
        }
        copy_tree(&src, staging)
    }
}
