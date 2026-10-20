//! Repository operations: what the buttons in the UI do.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{Error, Result};
use crate::git::{ssh_command_for_key, AuthEnv, GitRunner, ProgressFn};
use crate::log::{parse_log, Commit, LOG_FORMAT};
use crate::profile::Profile;
use crate::remote::RemoteUrl;
use crate::status::{parse_porcelain_v2, BranchInfo, ChangeKind, FileChange};
use crate::util::native_path;

// ---------------------------------------------------------------------------------------------
// Opening

pub async fn toplevel(git: &GitRunner, path: &Path) -> Result<PathBuf> {
    if !path.is_dir() {
        return Err(Error::NotARepo(path.display().to_string()));
    }
    match git.cmd(["rev-parse", "--show-toplevel"]).cwd(path).read_only().purpose("open").stdout().await {
        Ok(s) if !s.trim().is_empty() => Ok(native_path(s.trim())),
        Ok(_) | Err(Error::Git(_)) => Err(Error::NotARepo(path.display().to_string())),
        Err(e) => Err(e),
    }
}

pub async fn git_dir(git: &GitRunner, repo: &Path) -> Result<PathBuf> {
    let s = git.cmd(["rev-parse", "--absolute-git-dir"]).cwd(repo).read_only().stdout().await?;
    Ok(native_path(s.trim()))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Remote {
    pub name: String,
    pub url: String,
    pub parsed: Option<RemoteUrl>,
}

pub async fn remotes(git: &GitRunner, repo: &Path) -> Result<Vec<Remote>> {
    let out = git.cmd(["remote", "-v"]).cwd(repo).read_only().purpose("remotes").stdout().await?;
    let mut v: Vec<Remote> = Vec::new();
    for line in out.lines() {
        let mut parts = line.split_whitespace();
        let (Some(name), Some(url), Some("(fetch)")) = (parts.next(), parts.next(), parts.next()) else { continue };
        v.push(Remote { name: name.to_string(), url: url.to_string(), parsed: RemoteUrl::parse(url) });
    }
    Ok(v)
}

/// The remote a branch talks to: its upstream's remote, else `origin`, else the first one.
pub fn primary_remote<'a>(remotes: &'a [Remote], upstream_remote: Option<&str>) -> Option<&'a Remote> {
    upstream_remote
        .and_then(|u| remotes.iter().find(|r| r.name == u))
        .or_else(|| remotes.iter().find(|r| r.name == "origin"))
        .or_else(|| remotes.first())
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LfsRepoInfo {
    pub uses_lfs: bool,
    pub patterns: Vec<String>,
}

/// Reads `.gitattributes` for `filter=lfs` patterns.
pub fn lfs_info(repo: &Path) -> LfsRepoInfo {
    let Ok(text) = std::fs::read_to_string(repo.join(".gitattributes")) else { return LfsRepoInfo::default() };
    let patterns: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#') && l.contains("filter=lfs"))
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect();
    LfsRepoInfo { uses_lfs: !patterns.is_empty(), patterns }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoOverview {
    pub path: String,
    pub name: String,
    pub remotes: Vec<Remote>,
    pub lfs: LfsRepoInfo,
    pub local_name: Option<String>,
    pub local_email: Option<String>,
    /// `gitwise.profile` from the repository's own config: the binding travels with the folder.
    pub local_profile: Option<String>,
}

pub async fn overview(git: &GitRunner, repo: &Path) -> Result<RepoOverview> {
    let remotes = remotes(git, repo).await?;
    let name = repo.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| repo.display().to_string());
    Ok(RepoOverview {
        path: repo.display().to_string(),
        name,
        remotes,
        lfs: lfs_info(repo),
        local_name: config_get(git, repo, "user.name", true).await?,
        local_email: config_get(git, repo, "user.email", true).await?,
        local_profile: config_get(git, repo, "gitwise.profile", true).await?,
    })
}

pub async fn config_get(git: &GitRunner, repo: &Path, key: &str, local_only: bool) -> Result<Option<String>> {
    let mut args = vec!["config"];
    if local_only {
        args.push("--local");
    }
    args.extend(["--get", key]);
    let out = git.cmd(args).cwd(repo).read_only().ok_codes(&[0, 1]).run().await?;
    let v = out.text().trim().to_string();
    Ok((out.code == 0 && !v.is_empty()).then_some(v))
}

// ---------------------------------------------------------------------------------------------
// Status and diffs

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RepoState {
    Clean,
    Merging,
    Rebasing,
    CherryPicking,
    Reverting,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoStatus {
    pub branch: BranchInfo,
    pub files: Vec<FileChange>,
    pub state: RepoState,
    pub conflicted: u32,
}

pub async fn status(git: &GitRunner, repo: &Path) -> Result<RepoStatus> {
    let out = git
        .cmd(["status", "--porcelain=v2", "-z", "--branch", "--untracked-files=all"])
        .cwd(repo)
        .read_only()
        .purpose("status")
        .run()
        .await?;
    let s = parse_porcelain_v2(&out.stdout);
    let gd = git_dir(git, repo).await?;
    let state = if gd.join("rebase-merge").exists() || gd.join("rebase-apply").exists() {
        RepoState::Rebasing
    } else if gd.join("MERGE_HEAD").exists() {
        RepoState::Merging
    } else if gd.join("CHERRY_PICK_HEAD").exists() {
        RepoState::CherryPicking
    } else if gd.join("REVERT_HEAD").exists() {
        RepoState::Reverting
    } else {
        RepoState::Clean
    };
    let conflicted = s.files.iter().filter(|f| f.kind == ChangeKind::Conflicted).count() as u32;
    Ok(RepoStatus { branch: s.branch, files: s.files, state, conflicted })
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Diff {
    pub text: String,
    pub binary: bool,
    pub lfs_pointer: bool,
    pub truncated: bool,
    pub too_large: bool,
}

const DIFF_LIMIT: usize = 512 * 1024;
const UNTRACKED_DIFF_MAX_FILE: u64 = 5 * 1024 * 1024;

fn make_diff(raw: &[u8]) -> Diff {
    let mut text = String::from_utf8_lossy(raw).into_owned();
    let binary = text.starts_with("Binary files ") || text.contains("\nBinary files ") || text.contains("GIT binary patch");
    let lfs_pointer = text.contains("version https://git-lfs.github.com/spec/v1");
    let mut truncated = false;
    if text.len() > DIFF_LIMIT {
        let mut cut = DIFF_LIMIT;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        if let Some(nl) = text[..cut].rfind('\n') {
            cut = nl;
        }
        text.truncate(cut);
        truncated = true;
    }
    Diff { text, binary, lfs_pointer, truncated, too_large: false }
}

/// The working-tree change of one file against HEAD (staged and unstaged together, which is what
/// the user sees and what a commit of that file would contain).
pub async fn file_diff(git: &GitRunner, repo: &Path, file: &FileChange) -> Result<Diff> {
    let has_head = head_exists(git, repo).await?;
    if file.kind == ChangeKind::Untracked || !has_head {
        let full = repo.join(native_path(&file.path));
        match std::fs::metadata(&full) {
            Ok(m) if m.len() > UNTRACKED_DIFF_MAX_FILE => return Ok(Diff { too_large: true, ..Default::default() }),
            Ok(m) if m.is_dir() => return Ok(Diff::default()),
            Ok(_) => {}
            Err(_) => return Ok(Diff::default()),
        }
        let out = git
            .cmd(["diff", "--no-color", "--no-ext-diff", "--no-index", "--", "/dev/null"])
            .arg(&file.path)
            .cwd(repo)
            .ok_codes(&[0, 1])
            .read_only()
            .purpose("diff")
            .run()
            .await?;
        return Ok(make_diff(&out.stdout));
    }
    let mut cmd = git.cmd(["diff", "--no-color", "--no-ext-diff", "-M", "HEAD", "--"]);
    if let Some(o) = &file.orig_path {
        cmd = cmd.arg(o);
    }
    let out = cmd.arg(&file.path).cwd(repo).literal_pathspecs().read_only().purpose("diff").run().await?;
    Ok(make_diff(&out.stdout))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitFile {
    pub path: String,
    pub orig_path: Option<String>,
    pub kind: ChangeKind,
}

/// Files a commit changed, against its first parent.
pub async fn commit_files(git: &GitRunner, repo: &Path, oid: &str, parent: Option<&str>) -> Result<Vec<CommitFile>> {
    let cmd = match parent {
        Some(p) => git.cmd(["diff", "--name-status", "-M", "-z", p, oid]),
        None => git.cmd(["diff-tree", "-r", "-M", "-z", "--name-status", "--root", "--no-commit-id", oid]),
    };
    let out = cmd.cwd(repo).read_only().purpose("show").stdout().await?;
    let mut tokens = out.split('\0').filter(|t| !t.is_empty());
    let mut files = Vec::new();
    while let Some(status) = tokens.next() {
        let letter = status.chars().next().unwrap_or('M');
        let (kind, two) = match letter {
            'A' => (ChangeKind::Added, false),
            'D' => (ChangeKind::Deleted, false),
            'R' => (ChangeKind::Renamed, true),
            'C' => (ChangeKind::Copied, true),
            'T' => (ChangeKind::TypeChanged, false),
            _ => (ChangeKind::Modified, false),
        };
        if two {
            let (Some(orig), Some(path)) = (tokens.next(), tokens.next()) else { break };
            files.push(CommitFile { path: path.into(), orig_path: Some(orig.into()), kind });
        } else {
            let Some(path) = tokens.next() else { break };
            files.push(CommitFile { path: path.into(), orig_path: None, kind });
        }
    }
    Ok(files)
}

pub async fn commit_file_diff(
    git: &GitRunner,
    repo: &Path,
    oid: &str,
    parent: Option<&str>,
    path: &str,
    orig_path: Option<&str>,
) -> Result<Diff> {
    let mut cmd = match parent {
        Some(p) => git.cmd(["diff", "--no-color", "--no-ext-diff", "-M", p, oid, "--"]),
        None => git.cmd(["diff-tree", "-p", "--root", "-M", "--no-color", "--no-ext-diff", "--no-commit-id", oid, "--"]),
    };
    if let Some(o) = orig_path {
        cmd = cmd.arg(o);
    }
    let out = cmd.arg(path).cwd(repo).literal_pathspecs().read_only().purpose("show").run().await?;
    Ok(make_diff(&out.stdout))
}

pub async fn head_exists(git: &GitRunner, repo: &Path) -> Result<bool> {
    let out = git.cmd(["rev-parse", "--verify", "-q", "HEAD"]).cwd(repo).read_only().ok_codes(&[0, 1, 128]).run().await?;
    Ok(out.code == 0)
}

// ---------------------------------------------------------------------------------------------
// Committing

/// Commits exactly the selected files, as `profile`. Staging is an implementation detail here:
/// whatever was staged before but is not selected is unstaged (its changes stay on disk).
pub async fn commit(git: &GitRunner, repo: &Path, profile: &Profile, message: &str, selected: &[FileChange]) -> Result<String> {
    if message.trim().is_empty() {
        return Err(Error::invalid("empty_message"));
    }
    if selected.is_empty() {
        return Err(Error::invalid("nothing_selected"));
    }
    if !profile.has_identity() {
        return Err(Error::invalid("identity_missing"));
    }
    let st = status(git, repo).await?;
    let has_head = st.branch.oid.is_some();

    let mut include: Vec<String> = Vec::new();
    for f in selected {
        include.push(f.path.clone());
        if let Some(o) = &f.orig_path {
            include.push(o.clone());
        }
    }
    let wanted: HashSet<&str> = include.iter().map(String::as_str).collect();
    let mut unstage: Vec<String> = Vec::new();
    for f in st.files.iter().filter(|f| f.staged) {
        if !wanted.contains(f.path.as_str()) {
            unstage.push(f.path.clone());
            if let Some(o) = f.orig_path.as_deref().filter(|o| !wanted.contains(o)) {
                unstage.push(o.to_string());
            }
        }
    }
    if !unstage.is_empty() {
        let cmd = if has_head {
            git.cmd(["restore", "--staged"])
        } else {
            git.cmd(["rm", "--cached", "-r", "-q", "--ignore-unmatch"])
        };
        cmd.pathspec_stdin(&unstage).cwd(repo).purpose("unstage").run().await?;
    }
    git.cmd(["add", "-A"]).pathspec_stdin(&include).cwd(repo).purpose("stage").run().await?;
    git.cmd(["commit", "--file=-"])
        .stdin(message.as_bytes().to_vec())
        .identity(&profile.label, &profile.commit_name, &profile.commit_email)
        .cwd(repo)
        .purpose("commit")
        .run()
        .await?;
    Ok(git.cmd(["rev-parse", "HEAD"]).cwd(repo).read_only().stdout().await?.trim().to_string())
}

/// Takes back the last commit but keeps its changes in the working tree. Only offered for
/// commits that have not been pushed.
pub async fn undo_last_commit(git: &GitRunner, repo: &Path) -> Result<()> {
    let parents = git.cmd(["rev-list", "--parents", "-n", "1", "HEAD"]).cwd(repo).read_only().stdout().await?;
    if parents.split_whitespace().count() < 2 {
        return Err(Error::invalid("first_commit"));
    }
    if !unpushed(git, repo).await?.iter().any(|c| parents.starts_with(&c.oid)) {
        return Err(Error::invalid("already_pushed"));
    }
    git.cmd(["reset", "--soft", "HEAD~1"]).cwd(repo).purpose("undo_commit").run().await?;
    Ok(())
}

/// Rewrites the author and committer of every commit that has not been pushed yet to `profile`,
/// keeping their content. This is the fix for "I committed with the wrong account": commits that
/// already left this computer are never touched.
pub async fn rewrite_unpushed_authors(git: &GitRunner, repo: &Path, profile: &Profile) -> Result<usize> {
    if !profile.has_identity() {
        return Err(Error::invalid("identity_missing"));
    }
    if status(git, repo).await?.state != RepoState::Clean {
        return Err(Error::invalid("operation_in_progress"));
    }
    let commits = unpushed(git, repo).await?;
    if commits.is_empty() {
        return Ok(0);
    }
    // The rebase base: where this branch left what the remote already has.
    let base: Option<String> = if has_upstream(git, repo).await? {
        Some(git.cmd(["merge-base", "HEAD", "@{u}"]).cwd(repo).read_only().stdout().await?.trim().to_string())
    } else {
        let oldest = git.cmd(["rev-list", "--topo-order", "--reverse", "HEAD", "--not", "--remotes"]).cwd(repo).read_only().stdout().await?;
        let oldest = oldest.lines().next().unwrap_or_default().to_string();
        let parents = git.cmd(["rev-list", "--parents", "-n", "1", &oldest]).cwd(repo).read_only().stdout().await?;
        parents.split_whitespace().nth(1).map(str::to_string)
    };
    let mut cmd = git.cmd([
        "rebase",
        "--rebase-merges",
        "--autostash",
        "--exec",
        "git commit --amend --no-edit --reset-author --allow-empty --no-verify",
    ]);
    cmd = match &base {
        Some(b) => cmd.arg(b),
        None => cmd.arg("--root"),
    };
    let res = cmd
        .identity(&profile.label, &profile.commit_name, &profile.commit_email)
        .cwd(repo)
        .purpose("fix_authors")
        .run()
        .await;
    if let Err(e) = res {
        let _ = git.cmd(["rebase", "--abort"]).cwd(repo).purpose("abort").ok_codes(&[0, 1, 128]).run().await;
        return Err(e);
    }
    Ok(commits.len())
}

/// Throws away local changes. Current versions go to the Recycle Bin / Trash first, so a
/// mistaken discard can still be recovered.
pub async fn discard(git: &GitRunner, repo: &Path, files: &[FileChange]) -> Result<()> {
    let mut to_trash: Vec<PathBuf> = Vec::new();
    for f in files {
        let p = repo.join(native_path(&f.path));
        if std::fs::symlink_metadata(&p).is_ok() {
            to_trash.push(p);
        }
    }
    if !to_trash.is_empty() {
        tokio::task::spawn_blocking(move || {
            #[allow(unused_mut)]
            let mut ctx = trash::TrashContext::default();
            #[cfg(target_os = "macos")]
            {
                // The default asks Finder over AppleScript, which needs an automation permission
                // prompt; NSFileManager does not.
                use trash::macos::{DeleteMethod, TrashContextExtMacos};
                ctx.set_delete_method(DeleteMethod::NsFileManager);
            }
            ctx.delete_all(&to_trash)
        })
            .await
            .map_err(|e| Error::invalid(e.to_string()))?
            .map_err(|e| Error::invalid(format!("trash: {e}")))?;
    }
    let has_head = head_exists(git, repo).await?;
    let mut restore: Vec<String> = Vec::new();
    let mut unindex: Vec<String> = Vec::new();
    for f in files {
        match f.kind {
            ChangeKind::Untracked => {}
            ChangeKind::Renamed => {
                if let Some(o) = &f.orig_path {
                    restore.push(o.clone());
                }
                unindex.push(f.path.clone());
            }
            ChangeKind::Copied => unindex.push(f.path.clone()),
            _ if f.index_status == "A" || !has_head => unindex.push(f.path.clone()),
            _ => restore.push(f.path.clone()),
        }
    }
    if !restore.is_empty() {
        git.cmd(["restore", "--source=HEAD", "--staged", "--worktree"]).pathspec_stdin(&restore).cwd(repo).purpose("discard").run().await?;
    }
    if !unindex.is_empty() {
        git.cmd(["rm", "--cached", "-r", "-q", "--ignore-unmatch"]).pathspec_stdin(&unindex).cwd(repo).purpose("discard").run().await?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Syncing

pub async fn upstream_of(git: &GitRunner, repo: &Path, branch: &str) -> Result<Option<(String, String)>> {
    let remote = config_get(git, repo, &format!("branch.{branch}.remote"), false).await?;
    let merge = config_get(git, repo, &format!("branch.{branch}.merge"), false).await?;
    Ok(match (remote, merge) {
        (Some(r), Some(m)) if r != "." => Some((r, m)),
        _ => None,
    })
}

async fn sync_remote(git: &GitRunner, repo: &Path) -> Result<String> {
    let st = status(git, repo).await?;
    let up = match &st.branch.head {
        Some(b) => upstream_of(git, repo, b).await?.map(|(r, _)| r),
        None => None,
    };
    let remotes = remotes(git, repo).await?;
    primary_remote(&remotes, up.as_deref()).map(|r| r.name.clone()).ok_or_else(|| Error::invalid("no_remote"))
}

pub async fn fetch(git: &GitRunner, repo: &Path, auth: Option<&AuthEnv>, progress: Option<ProgressFn>) -> Result<()> {
    let remote = sync_remote(git, repo).await?;
    git.cmd(["fetch", "--progress", "--prune", &remote]).auth(auth).progress(progress).cwd(repo).purpose("fetch").run().await?;
    Ok(())
}

pub async fn pull(git: &GitRunner, repo: &Path, auth: Option<&AuthEnv>, progress: Option<ProgressFn>) -> Result<()> {
    git.cmd(["pull", "--progress", "--no-rebase", "--no-edit"]).auth(auth).progress(progress).cwd(repo).purpose("pull").run().await?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushResult {
    pub remote: String,
    pub branch: String,
    pub set_upstream: bool,
}

pub async fn push(git: &GitRunner, repo: &Path, auth: Option<&AuthEnv>, progress: Option<ProgressFn>) -> Result<PushResult> {
    let st = status(git, repo).await?;
    let branch = st.branch.head.clone().ok_or_else(|| Error::invalid("detached_head"))?;
    if st.branch.oid.is_none() {
        return Err(Error::invalid("no_commits"));
    }
    let (remote, merge, set_upstream) = match upstream_of(git, repo, &branch).await? {
        Some((r, m)) => (r, m, false),
        None => {
            let remotes = remotes(git, repo).await?;
            let r = primary_remote(&remotes, None).ok_or_else(|| Error::invalid("no_remote"))?;
            (r.name.clone(), format!("refs/heads/{branch}"), true)
        }
    };
    let mut cmd = git.cmd(["push", "--progress"]);
    if set_upstream {
        cmd = cmd.arg("--set-upstream");
    }
    cmd.arg(&remote)
        .arg(format!("refs/heads/{branch}:{merge}"))
        .auth(auth)
        .progress(progress)
        .cwd(repo)
        .purpose("push")
        .run()
        .await?;
    Ok(PushResult { remote, branch, set_upstream })
}

pub async fn abort_operation(git: &GitRunner, repo: &Path, state: RepoState) -> Result<()> {
    let args: &[&str] = match state {
        RepoState::Merging => &["merge", "--abort"],
        RepoState::Rebasing => &["rebase", "--abort"],
        RepoState::CherryPicking => &["cherry-pick", "--abort"],
        RepoState::Reverting => &["revert", "--abort"],
        RepoState::Clean => return Ok(()),
    };
    git.cmd(args).cwd(repo).purpose("abort").run().await?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Branches

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub name: String,
    pub full_ref: String,
    pub is_remote: bool,
    pub remote: Option<String>,
    pub upstream: Option<String>,
    pub current: bool,
    pub oid: String,
    pub time: i64,
}

pub async fn branches(git: &GitRunner, repo: &Path) -> Result<Vec<Branch>> {
    let fmt = "--format=%(refname)%1f%(refname:short)%1f%(upstream:short)%1f%(objectname:short)%1f%(committerdate:unix)%1f%(HEAD)%1f%(symref)";
    let out = git
        .cmd(["for-each-ref", "--sort=-committerdate", fmt, "refs/heads", "refs/remotes"])
        .cwd(repo)
        .read_only()
        .purpose("branches")
        .stdout()
        .await?;
    let mut v = Vec::new();
    for line in out.lines() {
        let f: Vec<&str> = line.split('\x1f').collect();
        if f.len() < 7 || !f[6].is_empty() {
            continue;
        }
        let is_remote = f[0].starts_with("refs/remotes/");
        let remote = is_remote.then(|| f[0]["refs/remotes/".len()..].split('/').next().unwrap_or("").to_string());
        v.push(Branch {
            name: f[1].to_string(),
            full_ref: f[0].to_string(),
            is_remote,
            remote,
            upstream: (!f[2].is_empty()).then(|| f[2].to_string()),
            current: f[5] == "*",
            oid: f[3].to_string(),
            time: f[4].parse().unwrap_or(0),
        });
    }
    Ok(v)
}

/// Switches to a local branch, or creates a local branch tracking a remote one.
pub async fn switch_branch(git: &GitRunner, repo: &Path, name: &str, remote: bool) -> Result<()> {
    let cmd = if remote { git.cmd(["switch", "--track", name]) } else { git.cmd(["switch", name]) };
    cmd.cwd(repo).purpose("switch").run().await?;
    Ok(())
}

pub async fn create_branch(git: &GitRunner, repo: &Path, name: &str) -> Result<()> {
    git.cmd(["switch", "-c", name]).cwd(repo).purpose("create_branch").run().await?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// History

pub async fn has_upstream(git: &GitRunner, repo: &Path) -> Result<bool> {
    let out = git
        .cmd(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
        .cwd(repo)
        .read_only()
        .ok_codes(&[0, 128])
        .run()
        .await?;
    Ok(out.code == 0)
}

async fn unpushed_range_args(git: &GitRunner, repo: &Path) -> Result<Vec<&'static str>> {
    Ok(if has_upstream(git, repo).await? { vec!["@{u}..HEAD"] } else { vec!["HEAD", "--not", "--remotes"] })
}

pub async fn log(git: &GitRunner, repo: &Path, limit: u32, skip: u32, profiles: &[Profile]) -> Result<Vec<Commit>> {
    if !head_exists(git, repo).await? {
        return Ok(vec![]);
    }
    let out = git
        .cmd(["log", &format!("--format={LOG_FORMAT}"), "-n", &limit.to_string(), "--skip", &skip.to_string(), "HEAD"])
        .cwd(repo)
        .read_only()
        .purpose("log")
        .stdout()
        .await?;
    let mut commits = parse_log(&out);
    let range = unpushed_range_args(git, repo).await?;
    let unpushed: HashSet<String> =
        git.cmd(["rev-list"]).args(&range).cwd(repo).read_only().stdout().await?.lines().map(str::to_string).collect();
    for c in &mut commits {
        c.unpushed = unpushed.contains(&c.oid);
        c.profile_id = profiles.iter().find(|p| p.owns_email(&c.author_email)).map(|p| p.id.clone());
    }
    Ok(commits)
}

pub async fn unpushed(git: &GitRunner, repo: &Path) -> Result<Vec<Commit>> {
    if !head_exists(git, repo).await? {
        return Ok(vec![]);
    }
    let range = unpushed_range_args(git, repo).await?;
    let out = git
        .cmd(["log", &format!("--format={LOG_FORMAT}"), "-n", "500"])
        .args(&range)
        .cwd(repo)
        .read_only()
        .purpose("log")
        .stdout()
        .await?;
    let mut commits = parse_log(&out);
    for c in &mut commits {
        c.unpushed = true;
    }
    Ok(commits)
}

pub async fn recent_author_emails(git: &GitRunner, repo: &Path, n: u32) -> Result<Vec<String>> {
    if !head_exists(git, repo).await? {
        return Ok(vec![]);
    }
    let out = git.cmd(["log", "-n", &n.to_string(), "--format=%ae", "HEAD"]).cwd(repo).read_only().stdout().await?;
    Ok(out.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
}

// ---------------------------------------------------------------------------------------------
// Creating repositories

pub async fn clone(git: &GitRunner, url: &str, dest: &Path, auth: Option<&AuthEnv>, progress: Option<ProgressFn>) -> Result<PathBuf> {
    if dest.exists() && dest.read_dir()?.next().is_some() {
        return Err(Error::invalid("dest_exists"));
    }
    let parent = dest.parent().ok_or_else(|| Error::invalid("bad_destination"))?;
    std::fs::create_dir_all(parent)?;
    git.cmd(["clone", "--progress", url]).arg(dest).cwd(parent).auth(auth).progress(progress).purpose("clone").run().await?;
    Ok(dest.to_path_buf())
}

pub async fn init(git: &GitRunner, path: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(path)?;
    git.cmd(["init", "-b", "main"]).cwd(path).purpose("init").run().await?;
    toplevel(git, path).await
}

// ---------------------------------------------------------------------------------------------
// Binding a repository to a profile

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigWrite {
    pub key: String,
    pub value: String,
}

/// What binding `profile` writes into the repository's own `.git/config`, so terminals, IDEs and
/// other git tools working in this folder use the same identity and account as Gitwise does.
pub fn binding_config(profile: &Profile, remotes: &[Remote]) -> Vec<ConfigWrite> {
    let mut w = vec![
        ConfigWrite { key: "user.name".into(), value: profile.commit_name.clone() },
        ConfigWrite { key: "user.email".into(), value: profile.commit_email.clone() },
        ConfigWrite { key: "gitwise.profile".into(), value: profile.id.clone() },
    ];
    let mut scopes: Vec<String> = Vec::new();
    let mut has_ssh = false;
    for r in remotes.iter().filter_map(|r| r.parsed.as_ref()) {
        if !profile.host_matches(&r.host) {
            continue;
        }
        if let Some(scope) = r.credential_scope() {
            if !profile.username.is_empty() && !scopes.contains(&scope) {
                scopes.push(scope);
            }
        }
        has_ssh |= r.is_ssh();
    }
    for scope in scopes {
        // Git Credential Manager keeps one entry per user name, so naming the user here makes it
        // pick this profile's account in a terminal too.
        w.push(ConfigWrite { key: format!("credential.{scope}.username"), value: profile.username.clone() });
    }
    if let (true, Some(key)) = (has_ssh, profile.ssh_key_path.as_deref().filter(|k| !k.trim().is_empty())) {
        w.push(ConfigWrite { key: "core.sshCommand".into(), value: ssh_command_for_key(Path::new(key)) });
    }
    w
}

pub async fn write_local_config(git: &GitRunner, repo: &Path, writes: &[ConfigWrite], label: &str) -> Result<()> {
    for w in writes {
        git.cmd(["config", "--local", &w.key, &w.value]).cwd(repo).purpose("bind").run().await.map_err(|e| {
            tracing::warn!("writing {} for {label} failed: {e}", w.key);
            e
        })?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// LFS in a repository

pub async fn lfs_filter_configured(git: &GitRunner, repo: &Path) -> bool {
    matches!(config_get(git, repo, "filter.lfs.process", false).await, Ok(Some(_)))
}

/// Whether the hook that uploads LFS objects on push is in place (honours `core.hooksPath`, which
/// tools such as husky set).
pub async fn lfs_hooks_installed(git: &GitRunner, repo: &Path) -> bool {
    let hooks_dir = match config_get(git, repo, "core.hooksPath", false).await {
        Ok(Some(p)) => {
            let p = native_path(&p);
            if p.is_absolute() {
                p
            } else {
                repo.join(p)
            }
        }
        _ => match git_dir(git, repo).await {
            Ok(gd) => gd.join("hooks"),
            Err(_) => return false,
        },
    };
    std::fs::read_to_string(hooks_dir.join("pre-push")).map(|s| s.contains("git lfs") || s.contains("git-lfs")).unwrap_or(false)
}

pub async fn lfs_install_repo(git: &GitRunner, repo: &Path) -> Result<()> {
    git.cmd(["lfs", "install"]).cwd(repo).purpose("lfs_install").run().await?;
    Ok(())
}
