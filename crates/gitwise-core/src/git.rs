//! Runs the real `git` binary.
//!
//! Every invocation is reported to a [`LogSink`] (the activity panel) with the command line a
//! person could paste into a terminal, so the app never does anything the user cannot see.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::askpass;
use crate::error::{Error, GitFailure, Result};
use crate::util::{collapse_carriage_returns, now_ms, tail};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LogStatus {
    Running,
    Ok,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEvent {
    pub id: u64,
    pub repo: Option<String>,
    pub command: String,
    /// Why the app ran this, in a short key the UI translates (e.g. `push`, `status`).
    pub purpose: Option<String>,
    /// Label of the profile whose identity or credentials the command used.
    pub profile: Option<String>,
    /// Read-only commands (status, log, diff) are hidden from the activity panel by default.
    pub read_only: bool,
    pub status: LogStatus,
    pub exit_code: Option<i32>,
    pub output: Option<String>,
    pub duration_ms: Option<u64>,
    pub started_at: u64,
}

pub trait LogSink: Send + Sync {
    fn emit(&self, event: &LogEvent);
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: String,
    pub percent: u8,
}

pub type ProgressFn = Arc<dyn Fn(Progress) + Send + Sync>;

/// Credentials a network command authenticates with. Secrets never travel in here: the askpass
/// helper reads them from the OS credential store by profile id.
#[derive(Debug, Clone)]
pub struct AuthEnv {
    pub profile_id: String,
    pub profile_label: String,
    pub host: String,
    pub username: String,
    pub ssh_key: Option<PathBuf>,
}

#[derive(Clone)]
pub struct GitRunner {
    inner: Arc<Inner>,
}

struct Inner {
    git: PathBuf,
    askpass_exe: Option<PathBuf>,
    sink: Option<Arc<dyn LogSink>>,
    next_id: AtomicU64,
}

impl GitRunner {
    pub fn new(git: PathBuf, askpass_exe: Option<PathBuf>, sink: Option<Arc<dyn LogSink>>) -> Self {
        Self { inner: Arc::new(Inner { git, askpass_exe, sink, next_id: AtomicU64::new(1) }) }
    }

    pub fn discover(askpass_exe: Option<PathBuf>, sink: Option<Arc<dyn LogSink>>) -> Result<Self> {
        let git = find_git().ok_or(Error::GitNotFound)?;
        Ok(Self::new(git, askpass_exe, sink))
    }

    pub fn git_path(&self) -> &Path {
        &self.inner.git
    }

    pub fn cmd<I, S>(&self, args: I) -> Cmd
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        Cmd {
            runner: self.clone(),
            cwd: None,
            args: args.into_iter().map(|a| a.as_ref().to_os_string()).collect(),
            config: Vec::new(),
            env: Vec::new(),
            stdin: None,
            auth: None,
            profile_label: None,
            purpose: None,
            ok_codes: vec![0],
            progress: None,
            read_only: false,
            literal_pathspecs: false,
        }
    }
}

pub struct Output {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

impl Output {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
}

#[must_use]
pub struct Cmd {
    runner: GitRunner,
    cwd: Option<PathBuf>,
    args: Vec<OsString>,
    config: Vec<(String, String)>,
    env: Vec<(String, OsString)>,
    stdin: Option<Vec<u8>>,
    auth: Option<AuthEnv>,
    profile_label: Option<String>,
    purpose: Option<String>,
    ok_codes: Vec<i32>,
    progress: Option<ProgressFn>,
    read_only: bool,
    literal_pathspecs: bool,
}

impl Cmd {
    pub fn cwd(mut self, dir: impl AsRef<Path>) -> Self {
        self.cwd = Some(dir.as_ref().to_path_buf());
        self
    }

    pub fn arg(mut self, a: impl AsRef<OsStr>) -> Self {
        self.args.push(a.as_ref().to_os_string());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args.extend(args.into_iter().map(|a| a.as_ref().to_os_string()));
        self
    }

    /// A `-c key=value` override for this one command.
    pub fn config(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.config.push((key.into(), value.into()));
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl AsRef<OsStr>) -> Self {
        self.env.push((key.into(), value.as_ref().to_os_string()));
        self
    }

    pub fn stdin(mut self, input: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(input.into());
        self
    }

    /// Paths for `--pathspec-from-file=- --pathspec-file-nul`, taken literally.
    pub fn pathspec_stdin<S: AsRef<str>>(self, paths: &[S]) -> Self {
        let mut buf = Vec::new();
        for p in paths {
            buf.extend_from_slice(p.as_ref().as_bytes());
            buf.push(0);
        }
        self.args(["--pathspec-from-file=-", "--pathspec-file-nul"]).stdin(buf).literal_pathspecs()
    }

    pub fn auth(mut self, auth: Option<&AuthEnv>) -> Self {
        if let Some(a) = auth {
            self.profile_label = Some(a.profile_label.clone());
        }
        self.auth = auth.cloned();
        self
    }

    /// Commit as this identity, whatever the user's global or local config says.
    pub fn identity(self, label: &str, name: &str, email: &str) -> Self {
        let mut c = self.config("user.name", name).config("user.email", email);
        c.profile_label = Some(label.to_string());
        c
    }

    pub fn purpose(mut self, p: impl Into<String>) -> Self {
        self.purpose = Some(p.into());
        self
    }

    pub fn ok_codes(mut self, codes: &[i32]) -> Self {
        self.ok_codes = codes.to_vec();
        self
    }

    pub fn progress(mut self, p: Option<ProgressFn>) -> Self {
        self.progress = p;
        self
    }

    /// Mark as read-only: never takes optional locks (so polling status cannot block a commit)
    /// and is hidden from the activity panel by default.
    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    pub fn literal_pathspecs(mut self) -> Self {
        self.literal_pathspecs = true;
        self
    }

    pub async fn stdout(self) -> Result<String> {
        Ok(self.run().await?.text())
    }

    pub async fn run(self) -> Result<Output> {
        let runner = self.runner.clone();
        let inner = &runner.inner;
        let id = inner.next_id.fetch_add(1, Ordering::Relaxed);
        let display = display_command(&self.args);
        let repo = self.cwd.as_ref().map(|p| p.to_string_lossy().into_owned());

        let mut event = LogEvent {
            id,
            repo,
            command: display.clone(),
            purpose: self.purpose.clone(),
            profile: self.profile_label.clone(),
            read_only: self.read_only,
            status: LogStatus::Running,
            exit_code: None,
            output: None,
            duration_ms: None,
            started_at: now_ms(),
        };
        if let Some(sink) = &inner.sink {
            sink.emit(&event);
        }

        let mut config: Vec<(String, String)> = vec![
            ("core.quotePath".into(), "false".into()),
            ("color.ui".into(), "false".into()),
        ];
        if self.auth.is_some() {
            // An empty value clears every credential helper configured anywhere (system, global,
            // local, URL-scoped), so Git Credential Manager cannot answer with a cached account
            // from the other profile. Our askpass helper is asked instead.
            config.push(("credential.helper".into(), String::new()));
        }
        config.extend(self.config.iter().cloned());

        let mut cmd = tokio::process::Command::new(&inner.git);
        for (k, v) in &config {
            cmd.arg("-c").arg(format!("{k}={v}"));
        }
        cmd.args(&self.args);
        if let Some(cwd) = &self.cwd {
            cmd.current_dir(cwd);
        }
        for k in [
            "GIT_AUTHOR_NAME",
            "GIT_AUTHOR_EMAIL",
            "GIT_COMMITTER_NAME",
            "GIT_COMMITTER_EMAIL",
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_ASKPASS",
            "SSH_ASKPASS",
            "GIT_SSH_COMMAND",
        ] {
            cmd.env_remove(k);
        }
        cmd.env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "never")
            // There is no terminal to open an editor in; ":" is git's own "accept as is".
            .env("GIT_EDITOR", ":")
            .env("GIT_SEQUENCE_EDITOR", ":")
            .env("LC_ALL", "C")
            .env("LANG", "C");
        if let Some(path) = extended_path() {
            cmd.env("PATH", path);
        }
        if self.read_only {
            cmd.env("GIT_OPTIONAL_LOCKS", "0");
        }
        if self.literal_pathspecs {
            cmd.env("GIT_LITERAL_PATHSPECS", "1");
        }
        if let Some(auth) = &self.auth {
            apply_auth(&mut cmd, auth, inner.askpass_exe.as_deref());
        }
        for (k, v) in &self.env {
            cmd.env(k, v);
        }

        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        cmd.stdin(if self.stdin.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let started = Instant::now();
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                event.status = LogStatus::Failed;
                event.output = Some(e.to_string());
                if let Some(sink) = &inner.sink {
                    sink.emit(&event);
                }
                return Err(if e.kind() == std::io::ErrorKind::NotFound { Error::GitNotFound } else { e.into() });
            }
        };

        if let Some(input) = self.stdin {
            if let Some(mut pipe) = child.stdin.take() {
                tokio::spawn(async move {
                    let _ = pipe.write_all(&input).await;
                    let _ = pipe.shutdown().await;
                });
            }
        }

        let mut out_pipe = child.stdout.take().expect("stdout piped");
        let mut err_pipe = child.stderr.take().expect("stderr piped");
        let out_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            out_pipe.read_to_end(&mut buf).await.map(|_| buf)
        });
        let progress = self.progress.clone();
        let err_task = tokio::spawn(async move {
            let mut all = Vec::new();
            let mut line = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = err_pipe.read(&mut chunk).await?;
                if n == 0 {
                    break;
                }
                all.extend_from_slice(&chunk[..n]);
                if let Some(p) = &progress {
                    for &b in &chunk[..n] {
                        if b == b'\r' || b == b'\n' {
                            if let Some(pr) = parse_progress(&String::from_utf8_lossy(&line)) {
                                p(pr);
                            }
                            line.clear();
                        } else {
                            line.push(b);
                        }
                    }
                }
            }
            Ok::<_, std::io::Error>(all)
        });

        let status = child.wait().await?;
        let stdout = out_task.await.map_err(|e| Error::invalid(e.to_string()))??;
        let stderr_raw = err_task.await.map_err(|e| Error::invalid(e.to_string()))??;
        let stderr = String::from_utf8_lossy(&stderr_raw).into_owned();
        let code = status.code().unwrap_or(-1);
        let ok = self.ok_codes.contains(&code);

        let mut shown = collapse_carriage_returns(&stderr);
        if !self.read_only {
            let so = String::from_utf8_lossy(&stdout);
            if !so.trim().is_empty() {
                if !shown.is_empty() {
                    shown.push('\n');
                }
                shown.push_str(so.trim_end());
            }
        }
        event.status = if ok { LogStatus::Ok } else { LogStatus::Failed };
        event.exit_code = Some(code);
        event.duration_ms = Some(started.elapsed().as_millis() as u64);
        event.output = (!shown.is_empty()).then(|| tail(&shown, 6000).to_string());
        if let Some(sink) = &inner.sink {
            sink.emit(&event);
        }

        if !ok {
            return Err(Error::Git(GitFailure { command: display, code, kind: classify_failure(&stderr), stderr }));
        }
        Ok(Output { code, stdout, stderr })
    }
}

fn apply_auth(cmd: &mut tokio::process::Command, auth: &AuthEnv, askpass_exe: Option<&Path>) {
    if let Some(exe) = askpass_exe {
        cmd.env("GIT_ASKPASS", exe)
            .env("SSH_ASKPASS", exe)
            .env("SSH_ASKPASS_REQUIRE", "force")
            .env(askpass::ENV_FLAG, "1")
            .env(askpass::ENV_PROFILE, &auth.profile_id)
            .env(askpass::ENV_HOST, &auth.host)
            .env(askpass::ENV_USERNAME, &auth.username);
        if std::env::var_os("DISPLAY").is_none() {
            // Older OpenSSH builds only consult SSH_ASKPASS when DISPLAY is set.
            cmd.env("DISPLAY", ":0");
        }
    }
    if let Some(key) = &auth.ssh_key {
        cmd.env("GIT_SSH_COMMAND", ssh_command_for_key(key));
    }
}

/// The ssh command that forces one key, so a second GitHub account's key in the agent or in
/// `~/.ssh` can never be offered instead.
pub fn ssh_command_for_key(key: &Path) -> String {
    let key = key.to_string_lossy().replace('\\', "/").replace('\'', "'\\''");
    format!("ssh -i '{key}' -o IdentitiesOnly=yes -o StrictHostKeyChecking=accept-new")
}

/// GUI apps on macOS start with a bare PATH, which hides Homebrew's git-lfs from git.
fn extended_path() -> Option<OsString> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let mut dirs: Vec<PathBuf> = vec!["/opt/homebrew/bin".into(), "/usr/local/bin".into()];
    if let Some(cur) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&cur));
    }
    std::env::join_paths(dirs).ok()
}

pub fn display_command(args: &[OsString]) -> String {
    let mut s = String::from("git");
    for a in args {
        let a = a.to_string_lossy();
        s.push(' ');
        if a.is_empty() || a.contains(char::is_whitespace) || a.contains('"') {
            s.push('"');
            s.push_str(&a.replace('"', "\\\""));
            s.push('"');
        } else {
            s.push_str(&a);
        }
    }
    s
}

/// Parses git's progress lines, e.g. `Receiving objects:  45% (12/27)` or
/// `Downloading LFS objects:  50% (1/2), 1.2 MB | 2 MB/s`.
pub fn parse_progress(line: &str) -> Option<Progress> {
    let l = line.trim();
    let l = l.strip_prefix("remote:").unwrap_or(l).trim();
    let colon = l.find(':')?;
    let phase = l[..colon].trim();
    if phase.is_empty() || phase.len() > 48 {
        return None;
    }
    let rest = l[colon + 1..].trim_start();
    let pct_end = rest.find('%')?;
    let percent: u8 = rest[..pct_end].trim().parse().ok()?;
    Some(Progress { phase: phase.to_string(), percent: percent.min(100) })
}

/// Turns git's stderr into a short reason the UI can explain and offer a fix for.
pub fn classify_failure(stderr: &str) -> Option<&'static str> {
    let s = stderr.to_ascii_lowercase();
    let has = |needle: &str| s.contains(needle);
    if has("refusing to allow an oauth app to create or update workflow")
        || has("refusing to allow a personal access token to create or update workflow")
    {
        return Some("workflow_scope");
    }
    if has("gh001: large files detected") || has("exceeds github's file size limit") || has("this exceeds") && has("file size limit") {
        return Some("file_too_large");
    }
    // Fine-grained GitHub tokens without the Contents permission: the account is fine, the
    // token just may not touch code. GitHub says "write access" even for a pull.
    if has("write access to repository not granted")
        || has("resource not accessible by personal access token")
        || has("resource not accessible by integration")
    {
        return Some("token_permissions");
    }
    if has("authentication failed")
        || has("could not read username")
        || has("could not read password")
        || has("permission denied (publickey")
        || has("invalid username or password")
        || has("the requested url returned error: 403")
        || has("the requested url returned error: 401")
        || has("permission to ") && has(" denied to ")
    {
        return Some("auth");
    }
    if has("repository not found") || has("does not appear to be a git repository") {
        return Some("repo_not_found");
    }
    if has("[rejected]") && (has("non-fast-forward") || has("fetch first")) || has("updates were rejected because") {
        return Some("push_behind");
    }
    if has("conflict (") || has("automatic merge failed") || has("fix conflicts and then commit") {
        return Some("conflict");
    }
    if has("would be overwritten by merge") || has("would be overwritten by checkout") || has("please commit your changes or stash them") {
        return Some("local_changes_block");
    }
    if has("git-lfs") && (has("not found") || has("was not found on your path") || has("command not found")) {
        return Some("lfs_missing");
    }
    if has("could not resolve host") || has("failed to connect") || has("connection timed out") || has("network is unreachable") {
        return Some("network");
    }
    if has("index.lock") {
        return Some("index_locked");
    }
    if has("please tell me who you are") {
        return Some("identity_missing");
    }
    if has("there is no tracking information") || has("has no upstream branch") {
        return Some("no_upstream");
    }
    if has("not possible to fast-forward") || has("divergent branches") {
        return Some("diverged");
    }
    if has("nothing to commit") {
        return Some("nothing_to_commit");
    }
    if has("host key verification failed") {
        return Some("host_key");
    }
    if has("already exists and is not an empty directory") {
        return Some("dest_exists");
    }
    if has("is not a valid branch name") || has("not a valid branch name") {
        return Some("bad_branch_name");
    }
    if has("already exists") && has("branch") {
        return Some("branch_exists");
    }
    None
}

/// Looks for git on PATH, then in the places installers put it.
pub fn find_git() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "git.exe" } else { "git" };
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let p = dir.join(exe);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        for var in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
            if let Some(pf) = std::env::var_os(var) {
                candidates.push(PathBuf::from(pf).join("Git").join("cmd").join("git.exe"));
            }
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            candidates.push(PathBuf::from(local).join("Programs").join("Git").join("cmd").join("git.exe"));
        }
    } else {
        candidates.extend(["/opt/homebrew/bin/git", "/usr/local/bin/git", "/usr/bin/git"].map(PathBuf::from));
    }
    candidates.into_iter().find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_lines() {
        let p = parse_progress("Receiving objects:  45% (12/27), 1.20 MiB | 2.00 MiB/s").unwrap();
        assert_eq!((p.phase.as_str(), p.percent), ("Receiving objects", 45));
        let p = parse_progress("remote: Compressing objects: 100% (3/3), done.").unwrap();
        assert_eq!((p.phase.as_str(), p.percent), ("Compressing objects", 100));
        let p = parse_progress("Downloading LFS objects:  50% (1/2), 1.2 MB | 2 MB/s").unwrap();
        assert_eq!(p.percent, 50);
        assert!(parse_progress("To github.com:a/b.git").is_none());
    }

    #[test]
    fn classifies_common_failures() {
        assert_eq!(classify_failure("remote: Permission to a/b.git denied to someone.\nfatal: unable to access"), Some("auth"));
        assert_eq!(
            classify_failure(" ! [rejected]        main -> main (fetch first)\nerror: failed to push some refs"),
            Some("push_behind")
        );
        assert_eq!(classify_failure("remote: error: GH001: Large files detected."), Some("file_too_large"));
        assert_eq!(classify_failure("CONFLICT (content): Merge conflict in a.txt"), Some("conflict"));
        assert_eq!(
            classify_failure("remote: Write access to repository not granted.\nfatal: unable to access 'https://github.com/a/b.git/': The requested URL returned error: 403"),
            Some("token_permissions")
        );
        assert_eq!(classify_failure("something else"), None);
    }

    #[test]
    fn ssh_command_quotes_windows_paths() {
        let cmd = ssh_command_for_key(Path::new(r"C:\Users\Me\.ssh\id_work"));
        assert_eq!(cmd, "ssh -i 'C:/Users/Me/.ssh/id_work' -o IdentitiesOnly=yes -o StrictHostKeyChecking=accept-new");
    }

    #[test]
    fn display_quotes_spaces() {
        let args: Vec<OsString> = ["commit", "-m", "hello world"].iter().map(OsString::from).collect();
        assert_eq!(display_command(&args), r#"git commit -m "hello world""#);
    }
}
