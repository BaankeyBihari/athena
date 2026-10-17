#![allow(dead_code)]

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A unique directory under the system temp dir, removed on drop.
pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new() -> TempDir {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("athena-test-{}-{n}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        // Resolve /var -> /private/var on macOS so paths compare equal to what git reports.
        TempDir(fs::canonicalize(&dir).unwrap())
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// Runs git in `dir` with a fixed identity, independent of the user's git config.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// Creates a git repo at `dir` with `files` committed on `main`. Returns the commit SHA.
pub fn init_repo(dir: &Path, files: &[(&str, &str)]) -> String {
    fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
    commit(dir, files)
}

/// Writes `files` into the repo at `dir` and commits them. Returns the commit SHA.
pub fn commit(dir: &Path, files: &[(&str, &str)]) -> String {
    for (name, content) in files {
        write(&dir.join(name), content);
    }
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "--allow-empty", "-m", "c"]);
    git(dir, &["rev-parse", "HEAD"])
}

pub struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// An isolated home, XDG dirs, git config, project dir, and fake remote host for driving the binary.
///
/// Remote URLs `https://example.test/<org>/<name>.git` are rewritten by git (`url.insteadOf`)
/// to repositories under `repos/`, so remote sources work without network access.
pub struct Sandbox {
    pub tmp: TempDir,
    pub home: PathBuf,
    pub project: PathBuf,
    pub config_home: PathBuf,
    pub cache_home: PathBuf,
    pub repos: PathBuf,
    pub gitconfig: PathBuf,
}

impl Sandbox {
    pub fn new() -> Sandbox {
        let tmp = TempDir::new();
        let root = tmp.path().to_path_buf();
        let home = root.join("home");
        let project = home.join("project");
        let config_home = root.join("xdg-config");
        let cache_home = root.join("xdg-cache");
        let repos = root.join("repos");
        let gitconfig = root.join("gitconfig");
        for d in [&home, &project, &config_home, &cache_home, &repos] {
            fs::create_dir_all(d).unwrap();
        }
        write(
            &gitconfig,
            &format!(
                "[user]\n\tname = t\n\temail = t@t\n[init]\n\tdefaultBranch = main\n[protocol \"file\"]\n\tallow = always\n[url \"file://{}/\"]\n\tinsteadOf = https://example.test/\n",
                repos.display()
            ),
        );
        Sandbox {
            tmp,
            home,
            project,
            config_home,
            cache_home,
            repos,
            gitconfig,
        }
    }

    pub fn url(&self, name: &str) -> String {
        format!("https://example.test/{name}.git")
    }

    pub fn repo_dir(&self, name: &str) -> PathBuf {
        self.repos.join(format!("{name}.git"))
    }

    /// Creates the fake remote `name` (e.g. `org/a`). Returns the commit SHA.
    pub fn repo(&self, name: &str, files: &[(&str, &str)]) -> String {
        init_repo(&self.repo_dir(name), files)
    }

    pub fn push(&self, name: &str, files: &[(&str, &str)]) -> String {
        commit(&self.repo_dir(name), files)
    }

    pub fn local_file(&self) -> PathBuf {
        self.project.join("athena.toml")
    }

    pub fn global_file(&self) -> PathBuf {
        self.config_home.join("athena/config.toml")
    }

    pub fn write_local(&self, toml: &str) {
        write(&self.local_file(), toml);
    }

    pub fn write_global(&self, toml: &str) {
        write(&self.global_file(), toml);
    }

    pub fn read_local(&self) -> String {
        fs::read_to_string(self.local_file()).unwrap_or_default()
    }

    pub fn read_global(&self) -> String {
        fs::read_to_string(self.global_file()).unwrap_or_default()
    }

    pub fn local_doc(&self) -> toml_edit::DocumentMut {
        self.read_local().parse().unwrap()
    }

    pub fn global_doc(&self) -> toml_edit::DocumentMut {
        self.read_global().parse().unwrap()
    }

    pub fn skill_dir(&self, leaf: &str) -> PathBuf {
        self.project.join(".agents/skills").join(leaf)
    }

    pub fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_athena"));
        cmd.args(args)
            .current_dir(&self.project)
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.config_home)
            .env("XDG_CACHE_HOME", &self.cache_home)
            .env("GIT_CONFIG_GLOBAL", &self.gitconfig)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("NO_COLOR", "1")
            .stdin(Stdio::null());
        cmd
    }

    pub fn run_cmd(mut cmd: Command) -> Run {
        let out = cmd.output().unwrap();
        Run {
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    pub fn athena(&self, args: &[&str]) -> Run {
        Sandbox::run_cmd(self.command(args))
    }

    /// Runs athena under a pseudo-terminal (via `script`), typing `answer` and a newline.
    pub fn athena_tty(&self, args: &[&str], answer: &str) -> Run {
        let bin = env!("CARGO_BIN_EXE_athena");
        let mut script_args = vec!["-q", "/dev/null", bin];
        script_args.extend_from_slice(args);
        let base = self.command(&[]);
        let mut cmd = Command::new("script");
        cmd.args(&script_args).current_dir(&self.project);
        for (k, v) in base.get_envs() {
            if let Some(v) = v {
                cmd.env(k, v);
            }
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        std::thread::sleep(Duration::from_millis(1500));
        let _ = writeln!(stdin, "{answer}"); // athena may exit before reading
        let start = std::time::Instant::now();
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break s;
            }
            assert!(
                start.elapsed() < Duration::from_secs(60),
                "athena under pty timed out"
            );
            std::thread::sleep(Duration::from_millis(50));
        };
        drop(stdin);
        let out = child.wait_with_output().unwrap();
        Run {
            code: status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }
}

pub fn staging_leftovers(dir: &Path) -> Vec<String> {
    let Ok(rd) = fs::read_dir(dir) else {
        return vec![];
    };
    rd.filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(".athena-staging-"))
        .collect()
}
