//! Everything the web view can ask the backend to do.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gitwise_core::git::{AuthEnv, GitRunner, Progress, ProgressFn};
use gitwise_core::guard::{self, Action, GuardReport};
use gitwise_core::log::Commit;
use gitwise_core::ops::{self, Branch, CommitFile, ConfigWrite, Diff, PushResult, RepoOverview, RepoStatus};
use gitwise_core::profile::{Profile, Provider};
use gitwise_core::providers::{self, AccountInfo, DeviceCode, DevicePoll, HostedRepo};
use gitwise_core::rules::{self, Rule};
use gitwise_core::secrets::{self, SecretKind};
use gitwise_core::status::FileChange;
use gitwise_core::store::{AppData, RepoEntry, Settings};
use gitwise_core::tools::{self, ToolsStatus};
use gitwise_core::util::now_ms;
use gitwise_core::{Error, Result};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

use crate::state::AppState;

type S<'a> = State<'a, AppState>;

/// Client id of the Gitwise OAuth app on github.com, set by the release build.
const BUILTIN_GITHUB_CLIENT_ID: Option<&str> = option_env!("GITWISE_GITHUB_CLIENT_ID");

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OpProgress<'a> {
    op_id: &'a str,
    phase: String,
    percent: u8,
}

fn progress_for(state: &AppState, op_id: Option<String>) -> Option<ProgressFn> {
    let op_id = op_id?;
    let app = state.app.clone();
    Some(Arc::new(move |p: Progress| {
        let _ = app.emit("op-progress", OpProgress { op_id: &op_id, phase: p.phase, percent: p.percent });
    }))
}

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(path)
}

/// The profile a repository is bound to; network and commit commands refuse to run without one.
fn bound_profile(state: &AppState, path: &str) -> Result<Profile> {
    let id = state.store.read(|d| d.repo(path).and_then(|r| r.profile_id.clone())).ok_or_else(|| Error::invalid("no_profile"))?;
    state.store.profile(&id)
}

/// Authentication for a network command, or `None` for local-path remotes.
fn auth_for(profile: &Profile) -> AuthEnv {
    profile.auth_env()
}

// ---------------------------------------------------------------------------------------------
// App

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    data: AppData,
    tools: ToolsStatus,
    version: String,
    data_dir: String,
    builtin_github_sign_in: bool,
}

#[tauri::command]
pub async fn bootstrap(state: S<'_>) -> Result<Bootstrap> {
    let tools = tools::status(state.git_opt().as_ref()).await;
    Ok(Bootstrap {
        data: state.store.snapshot(),
        tools,
        version: state.app.package_info().version.to_string(),
        data_dir: gitwise_core::store::Store::default_dir().display().to_string(),
        builtin_github_sign_in: BUILTIN_GITHUB_CLIENT_ID.is_some_and(|c| !c.is_empty()),
    })
}

#[tauri::command]
pub async fn settings_save(state: S<'_>, settings: Settings) -> Result<AppData> {
    state.store.update(|d| {
        d.settings = settings;
        Ok(d.clone())
    })
}

// ---------------------------------------------------------------------------------------------
// Profiles

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInput {
    profile: Profile,
    /// A newly typed token; `None` keeps the stored one.
    token: Option<String>,
    /// A token from a finished GitHub sign-in (see `github_device_poll`).
    token_ref: Option<String>,
    ssh_passphrase: Option<String>,
    #[serde(default)]
    clear_token: bool,
    #[serde(default)]
    clear_passphrase: bool,
}

#[tauri::command]
pub async fn profile_save(state: S<'_>, input: ProfileInput) -> Result<AppData> {
    let mut p = input.profile;
    if p.id.trim().is_empty() {
        p.id = uuid::Uuid::new_v4().to_string();
    }
    p.label = p.label.trim().to_string();
    p.host = gitwise_core::remote::normalize_host(&p.host);
    if p.host.is_empty() {
        p.host = p.provider.default_host().to_string();
    }
    if p.label.is_empty() {
        return Err(Error::invalid("label_required"));
    }

    let pending = input.token_ref.and_then(|r| state.pending_tokens.lock().expect("tokens").remove(&r));
    if input.clear_token {
        secrets::delete(&p.id, SecretKind::Token)?;
    }
    if let Some(t) = pending.or(input.token).filter(|t| !t.trim().is_empty()) {
        secrets::set(&p.id, SecretKind::Token, t.trim())?;
    }
    if input.clear_passphrase {
        secrets::delete(&p.id, SecretKind::SshPassphrase)?;
    }
    if let Some(pp) = input.ssh_passphrase.filter(|s| !s.is_empty()) {
        secrets::set(&p.id, SecretKind::SshPassphrase, &pp)?;
    }
    p.has_token = secrets::get(&p.id, SecretKind::Token)?.is_some();
    p.has_ssh_passphrase = secrets::get(&p.id, SecretKind::SshPassphrase)?.is_some();

    let saved = p.clone();
    let data = state.store.update(|d| {
        match d.profiles.iter_mut().find(|x| x.id == saved.id) {
            Some(existing) => *existing = saved.clone(),
            None => d.profiles.push(saved.clone()),
        }
        if d.settings.default_profile_id.is_none() {
            d.settings.default_profile_id = Some(saved.id.clone());
        }
        Ok(d.clone())
    })?;

    // Repositories bound to this profile get the edited identity in their local config as well.
    if let Some(git) = state.git_opt() {
        let bound: Vec<String> = data.repos.iter().filter(|r| r.profile_id.as_deref() == Some(&p.id)).map(|r| r.path.clone()).collect();
        for path in bound {
            let repo = repo_path(&path);
            if let Ok(remotes) = ops::remotes(&git, &repo).await {
                let _ = ops::write_local_config(&git, &repo, &ops::binding_config(&p, &remotes), &p.label).await;
            }
        }
    }
    Ok(data)
}

#[tauri::command]
pub async fn profile_delete(state: S<'_>, id: String) -> Result<AppData> {
    secrets::delete_all(&id)?;
    state.store.update(|d| {
        d.profiles.retain(|p| p.id != id);
        d.rules.retain(|r| r.profile_id != id);
        for r in d.repos.iter_mut().filter(|r| r.profile_id.as_deref() == Some(&id)) {
            r.profile_id = None;
        }
        if d.settings.default_profile_id.as_deref() == Some(&id) {
            d.settings.default_profile_id = d.profiles.first().map(|p| p.id.clone());
        }
        Ok(d.clone())
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyInput {
    provider: Provider,
    host: String,
    username: String,
    token: Option<String>,
    token_ref: Option<String>,
    profile_id: Option<String>,
}

#[tauri::command]
pub async fn profile_verify(state: S<'_>, input: VerifyInput) -> Result<AccountInfo> {
    let token = match (input.token.filter(|t| !t.trim().is_empty()), input.token_ref) {
        (Some(t), _) => t,
        (None, Some(r)) => state.pending_tokens.lock().expect("tokens").get(&r).cloned().ok_or_else(|| Error::invalid("token_missing"))?,
        (None, None) => {
            let id = input.profile_id.ok_or_else(|| Error::invalid("token_missing"))?;
            secrets::get(&id, SecretKind::Token)?.ok_or_else(|| Error::invalid("token_missing"))?
        }
    };
    let host = if input.host.trim().is_empty() { input.provider.default_host().to_string() } else { input.host.trim().to_string() };
    providers::verify(input.provider, &host, input.username.trim(), token.trim()).await
}

/// `ssh -T git@host` with the profile's key; hosts answer with the account the key belongs to.
#[tauri::command]
pub async fn profile_test_ssh(state: S<'_>, host: String, key_path: Option<String>) -> Result<String> {
    let git = state.git()?;
    let ssh = ssh_binary(git.git_path());
    let mut cmd = tokio::process::Command::new(ssh);
    cmd.arg("-T").args(["-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=accept-new", "-o", "ConnectTimeout=15"]);
    if let Some(k) = key_path.filter(|k| !k.trim().is_empty()) {
        cmd.args(["-i", &k, "-o", "IdentitiesOnly=yes"]);
    }
    cmd.arg(format!("git@{}", host.trim()));
    #[cfg(windows)]
    {
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().await?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    Ok(text.trim().to_string())
}

/// The ssh that ships with Git for Windows (the one git itself uses), else whatever is on PATH.
fn ssh_binary(git: &Path) -> PathBuf {
    if cfg!(windows) {
        if let Some(root) = git.parent().and_then(|p| p.parent()) {
            let bundled = root.join("usr").join("bin").join("ssh.exe");
            if bundled.is_file() {
                return bundled;
            }
        }
    }
    PathBuf::from("ssh")
}

fn github_client_id(state: &AppState, host: &str) -> Result<String> {
    let configured = state.store.read(|d| d.settings.oauth_client_ids.get(host).cloned()).filter(|c| !c.trim().is_empty());
    configured
        .or_else(|| (host == "github.com").then(|| BUILTIN_GITHUB_CLIENT_ID.unwrap_or("").to_string()).filter(|c| !c.is_empty()))
        .ok_or_else(|| Error::invalid("oauth_client_missing"))
}

#[tauri::command]
pub async fn github_device_start(state: S<'_>, host: String) -> Result<DeviceCode> {
    let host = gitwise_core::remote::normalize_host(&host);
    let client_id = github_client_id(&state, &host)?;
    providers::github_device_start(&host, &client_id).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicePollOut {
    #[serde(flatten)]
    poll: DevicePoll,
    token_ref: Option<String>,
    account: Option<AccountInfo>,
}

#[tauri::command]
pub async fn github_device_poll(state: S<'_>, host: String, device_code: String) -> Result<DevicePollOut> {
    let host = gitwise_core::remote::normalize_host(&host);
    let client_id = github_client_id(&state, &host)?;
    let poll = providers::github_device_poll(&host, &client_id, &device_code).await?;
    if let DevicePoll::Done { token } = &poll {
        let account = providers::verify(Provider::Github, &host, "", token).await?;
        let r = uuid::Uuid::new_v4().to_string();
        state.pending_tokens.lock().expect("tokens").insert(r.clone(), token.clone());
        return Ok(DevicePollOut { poll, token_ref: Some(r), account: Some(account) });
    }
    Ok(DevicePollOut { poll, token_ref: None, account: None })
}

#[tauri::command]
pub async fn hosted_repos(state: S<'_>, profile_id: String) -> Result<Vec<HostedRepo>> {
    let p = state.store.profile(&profile_id)?;
    let token = secrets::get(&p.id, SecretKind::Token)?.ok_or_else(|| Error::invalid("token_missing"))?;
    providers::list_repos(p.provider, &p.host, &token).await
}

// ---------------------------------------------------------------------------------------------
// Rules

#[tauri::command]
pub async fn rules_save(state: S<'_>, rules: Vec<Rule>) -> Result<AppData> {
    state.store.update(|d| {
        d.rules = rules
            .into_iter()
            .map(|mut r| {
                if r.id.is_empty() {
                    r.id = uuid::Uuid::new_v4().to_string();
                }
                r
            })
            .collect();
        Ok(d.clone())
    })
}

/// Which profile the rules pick for a URL about to be cloned.
#[tauri::command]
pub async fn profile_for_url(state: S<'_>, url: String) -> Result<Option<String>> {
    let Some(parsed) = gitwise_core::remote::RemoteUrl::parse(&url) else { return Ok(None) };
    Ok(state.store.read(|d| {
        rules::resolve(&d.rules, "", std::slice::from_ref(&parsed))
            .map(|r| r.profile_id.clone())
            .or_else(|| {
                // Without a rule, only guess when exactly one profile lives on that host.
                let mut on_host = d.profiles.iter().filter(|p| p.host_matches(&parsed.host));
                match (on_host.next(), on_host.next()) {
                    (Some(p), None) => Some(p.id.clone()),
                    _ => None,
                }
            })
    }))
}

// ---------------------------------------------------------------------------------------------
// Repository list and binding

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoAdded {
    data: AppData,
    path: String,
    /// Which profile was picked automatically and why (`local`, `rule`), if any.
    bound_profile_id: Option<String>,
    bound_by: Option<&'static str>,
    writes: Vec<ConfigWrite>,
}

async fn register_repo(state: &AppState, git: &GitRunner, top: &Path, explicit_profile: Option<String>) -> Result<RepoAdded> {
    let path = top.display().to_string();
    let name = top.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.clone());
    let ov = ops::overview(git, top).await?;
    let parsed: Vec<_> = ov.remotes.iter().filter_map(|r| r.parsed.clone()).collect();

    let snapshot = state.store.snapshot();
    let existing = snapshot.repo(&path).and_then(|r| r.profile_id.clone());
    let (profile_id, by) = if let Some(p) = explicit_profile {
        (Some(p), Some("explicit"))
    } else if let Some(p) = existing {
        (Some(p), None)
    } else if let Some(p) = ov.local_profile.clone().filter(|id| snapshot.profile(id).is_some()) {
        (Some(p), Some("local"))
    } else if let Some(rule) = rules::resolve(&snapshot.rules, &path, &parsed) {
        (Some(rule.profile_id.clone()), Some("rule"))
    } else {
        (None, None)
    };

    let mut writes = Vec::new();
    if let (Some(id), Some(_)) = (&profile_id, by) {
        let p = state.store.profile(id)?;
        writes = ops::binding_config(&p, &ov.remotes);
        ops::write_local_config(git, top, &writes, &p.label).await?;
    }

    let data = state.store.update(|d| {
        match d.repo_mut(&path) {
            Some(r) => {
                if profile_id.is_some() {
                    r.profile_id = profile_id.clone();
                }
                r.last_opened_at = Some(now_ms());
            }
            None => d.repos.push(RepoEntry {
                path: path.clone(),
                name: name.clone(),
                profile_id: profile_id.clone(),
                added_at: now_ms(),
                last_opened_at: Some(now_ms()),
            }),
        }
        Ok(d.clone())
    })?;
    Ok(RepoAdded { data, path, bound_profile_id: profile_id.filter(|_| by.is_some()), bound_by: by, writes })
}

#[tauri::command]
pub async fn repo_add(state: S<'_>, path: String) -> Result<RepoAdded> {
    let git = state.git()?;
    let top = ops::toplevel(&git, Path::new(&path)).await?;
    register_repo(&state, &git, &top, None).await
}

#[tauri::command]
pub async fn repo_remove(state: S<'_>, path: String) -> Result<AppData> {
    state.store.update(|d| {
        let key = gitwise_core::util::path_key(&path);
        d.repos.retain(|r| gitwise_core::util::path_key(&r.path) != key);
        Ok(d.clone())
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BindResult {
    data: AppData,
    writes: Vec<ConfigWrite>,
}

#[tauri::command]
pub async fn repo_bind(state: S<'_>, path: String, profile_id: String) -> Result<BindResult> {
    let git = state.git()?;
    let p = state.store.profile(&profile_id)?;
    let repo = repo_path(&path);
    let lock = state.repo_lock(&path);
    let _g = lock.lock().await;
    let remotes = ops::remotes(&git, &repo).await?;
    let writes = ops::binding_config(&p, &remotes);
    ops::write_local_config(&git, &repo, &writes, &p.label).await?;
    let data = state.store.update(|d| {
        let r = d.repo_mut(&path).ok_or_else(|| Error::invalid("repo_unknown"))?;
        r.profile_id = Some(profile_id.clone());
        Ok(d.clone())
    })?;
    Ok(BindResult { data, writes })
}

#[tauri::command]
pub async fn repo_overview(state: S<'_>, path: String) -> Result<RepoOverview> {
    let git = state.git()?;
    let ov = ops::overview(&git, &repo_path(&path)).await?;
    let _ = state.store.update(|d| {
        if let Some(r) = d.repo_mut(&path) {
            r.last_opened_at = Some(now_ms());
        }
        Ok(())
    });
    Ok(ov)
}

// ---------------------------------------------------------------------------------------------
// Working tree

#[tauri::command]
pub async fn repo_status(state: S<'_>, path: String) -> Result<RepoStatus> {
    ops::status(&state.git()?, &repo_path(&path)).await
}

#[tauri::command]
pub async fn repo_diff(state: S<'_>, path: String, file: FileChange) -> Result<Diff> {
    ops::file_diff(&state.git()?, &repo_path(&path), &file).await
}

#[tauri::command]
pub async fn repo_guard(state: S<'_>, path: String, action: Action) -> Result<GuardReport> {
    let data = state.store.snapshot();
    guard::evaluate(&state.git()?, &repo_path(&path), &data, action).await
}

async fn ensure_guard(state: &AppState, git: &GitRunner, path: &str, action: Action) -> Result<()> {
    let data = state.store.snapshot();
    let report = guard::evaluate(git, &repo_path(path), &data, action).await?;
    if !report.can_proceed {
        let code = report.issues.first().map(|i| i.code).unwrap_or("blocked");
        return Err(Error::invalid(code));
    }
    Ok(())
}

#[tauri::command]
pub async fn repo_commit(state: S<'_>, path: String, message: String, files: Vec<FileChange>) -> Result<String> {
    let git = state.git()?;
    let lock = state.repo_lock(&path);
    let _g = lock.lock().await;
    ensure_guard(&state, &git, &path, Action::Commit).await?;
    let p = bound_profile(&state, &path)?;
    ops::commit(&git, &repo_path(&path), &p, &message, &files).await
}

#[tauri::command]
pub async fn repo_undo_commit(state: S<'_>, path: String) -> Result<()> {
    let git = state.git()?;
    let lock = state.repo_lock(&path);
    let _g = lock.lock().await;
    ops::undo_last_commit(&git, &repo_path(&path)).await
}

/// Rewrites unpushed commits to the bound profile's identity; returns how many changed.
#[tauri::command]
pub async fn repo_fix_authors(state: S<'_>, path: String) -> Result<usize> {
    let git = state.git()?;
    let p = bound_profile(&state, &path)?;
    let lock = state.repo_lock(&path);
    let _g = lock.lock().await;
    ops::rewrite_unpushed_authors(&git, &repo_path(&path), &p).await
}

#[tauri::command]
pub async fn repo_discard(state: S<'_>, path: String, files: Vec<FileChange>) -> Result<()> {
    let git = state.git()?;
    let lock = state.repo_lock(&path);
    let _g = lock.lock().await;
    ops::discard(&git, &repo_path(&path), &files).await
}

#[tauri::command]
pub async fn repo_abort(state: S<'_>, path: String) -> Result<()> {
    let git = state.git()?;
    let lock = state.repo_lock(&path);
    let _g = lock.lock().await;
    let repo = repo_path(&path);
    let st = ops::status(&git, &repo).await?;
    ops::abort_operation(&git, &repo, st.state).await
}

// ---------------------------------------------------------------------------------------------
// Sync

#[tauri::command]
pub async fn repo_fetch(state: S<'_>, path: String, op_id: Option<String>) -> Result<()> {
    let git = state.git()?;
    let p = bound_profile(&state, &path)?;
    let lock = state.repo_lock(&path);
    let _g = lock.lock().await;
    ops::fetch(&git, &repo_path(&path), Some(&auth_for(&p)), progress_for(&state, op_id)).await
}

#[tauri::command]
pub async fn repo_pull(state: S<'_>, path: String, op_id: Option<String>) -> Result<()> {
    let git = state.git()?;
    let p = bound_profile(&state, &path)?;
    let lock = state.repo_lock(&path);
    let _g = lock.lock().await;
    let repo = repo_path(&path);
    // A merge commit made by pull is a commit too: it must carry the profile's identity.
    let writes = ops::binding_config(&p, &ops::remotes(&git, &repo).await?);
    ops::write_local_config(&git, &repo, &writes, &p.label).await?;
    ops::pull(&git, &repo, Some(&auth_for(&p)), progress_for(&state, op_id)).await
}

#[tauri::command]
pub async fn repo_push(state: S<'_>, path: String, op_id: Option<String>) -> Result<PushResult> {
    let git = state.git()?;
    let lock = state.repo_lock(&path);
    let _g = lock.lock().await;
    ensure_guard(&state, &git, &path, Action::Push).await?;
    let p = bound_profile(&state, &path)?;
    ops::push(&git, &repo_path(&path), Some(&auth_for(&p)), progress_for(&state, op_id)).await
}

// ---------------------------------------------------------------------------------------------
// Branches and history

#[tauri::command]
pub async fn repo_branches(state: S<'_>, path: String) -> Result<Vec<Branch>> {
    ops::branches(&state.git()?, &repo_path(&path)).await
}

#[tauri::command]
pub async fn repo_switch(state: S<'_>, path: String, name: String, remote: bool) -> Result<()> {
    let git = state.git()?;
    let lock = state.repo_lock(&path);
    let _g = lock.lock().await;
    ops::switch_branch(&git, &repo_path(&path), &name, remote).await
}

#[tauri::command]
pub async fn repo_create_branch(state: S<'_>, path: String, name: String) -> Result<()> {
    let git = state.git()?;
    let lock = state.repo_lock(&path);
    let _g = lock.lock().await;
    ops::create_branch(&git, &repo_path(&path), name.trim()).await
}

#[tauri::command]
pub async fn repo_log(state: S<'_>, path: String, limit: u32, skip: u32) -> Result<Vec<Commit>> {
    let profiles = state.store.read(|d| d.profiles.clone());
    ops::log(&state.git()?, &repo_path(&path), limit.min(1000), skip, &profiles).await
}

#[tauri::command]
pub async fn repo_commit_files(state: S<'_>, path: String, oid: String, parent: Option<String>) -> Result<Vec<CommitFile>> {
    ops::commit_files(&state.git()?, &repo_path(&path), &oid, parent.as_deref()).await
}

#[tauri::command]
pub async fn repo_commit_diff(
    state: S<'_>,
    path: String,
    oid: String,
    parent: Option<String>,
    file: String,
    orig_path: Option<String>,
) -> Result<Diff> {
    ops::commit_file_diff(&state.git()?, &repo_path(&path), &oid, parent.as_deref(), &file, orig_path.as_deref()).await
}

// ---------------------------------------------------------------------------------------------
// Creating repositories

#[tauri::command]
pub async fn repo_clone(state: S<'_>, url: String, dest: String, profile_id: String, op_id: Option<String>) -> Result<RepoAdded> {
    let git = state.git()?;
    let p = state.store.profile(&profile_id)?;
    let dest = PathBuf::from(dest.trim());
    let top = ops::clone(&git, url.trim(), &dest, Some(&auth_for(&p)), progress_for(&state, op_id)).await?;
    register_repo(&state, &git, &top, Some(profile_id)).await
}

#[tauri::command]
pub async fn repo_init(state: S<'_>, path: String, profile_id: Option<String>) -> Result<RepoAdded> {
    let git = state.git()?;
    let top = ops::init(&git, Path::new(path.trim())).await?;
    register_repo(&state, &git, &top, profile_id).await
}

// ---------------------------------------------------------------------------------------------
// Tools

#[tauri::command]
pub async fn tools_status(state: S<'_>) -> Result<ToolsStatus> {
    state.rediscover_git();
    Ok(tools::status(state.git_opt().as_ref()).await)
}

#[tauri::command]
pub async fn tools_lfs_install_global(state: S<'_>) -> Result<ToolsStatus> {
    tools::lfs_install_global(&state.git()?).await?;
    Ok(tools::status(state.git_opt().as_ref()).await)
}

#[tauri::command]
pub async fn tools_winget_install(state: S<'_>, package: String) -> Result<ToolsStatus> {
    tools::winget_install(&package).await?;
    state.rediscover_git();
    Ok(tools::status(state.git_opt().as_ref()).await)
}

#[tauri::command]
pub async fn repo_lfs_install(state: S<'_>, path: String) -> Result<()> {
    let git = state.git()?;
    ops::lfs_install_repo(&git, &repo_path(&path)).await
}

#[tauri::command]
pub async fn open_terminal(path: String) -> Result<()> {
    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return Err(Error::invalid("not_a_folder"));
    }
    #[cfg(windows)]
    {
        let wt = tools::find_on_path("wt");
        let mut cmd = match wt {
            Some(wt) => {
                let mut c = std::process::Command::new(wt);
                c.arg("-d").arg(&dir);
                c
            }
            None => {
                let mut c = std::process::Command::new("cmd");
                c.args(["/c", "start", "cmd", "/k"]).current_dir(&dir);
                c
            }
        };
        cmd.spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").args(["-a", "Terminal"]).arg(&dir).spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("x-terminal-emulator").current_dir(&dir).spawn()?;
    }
    Ok(())
}
