//! Git and its extensions: what is installed, and installing what is missing.

use std::path::PathBuf;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::git::GitRunner;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub path: Option<String>,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsStatus {
    pub git: Option<ToolInfo>,
    pub lfs: Option<ToolInfo>,
    /// `git lfs install` has set up the LFS filters for this user.
    pub lfs_initialized: bool,
    pub platform: &'static str,
    /// Windows package manager, used for one-click installs.
    pub winget: bool,
}

fn platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

pub async fn status(git: Option<&GitRunner>) -> ToolsStatus {
    let mut st = ToolsStatus { git: None, lfs: None, lfs_initialized: false, platform: platform(), winget: find_on_path("winget").is_some() };
    let Some(git) = git else { return st };
    let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
    if let Ok(v) = git.cmd(["--version"]).cwd(&home).read_only().purpose("tools").stdout().await {
        st.git = Some(ToolInfo {
            path: Some(git.git_path().display().to_string()),
            version: v.trim().trim_start_matches("git version ").to_string(),
        });
    }
    if let Ok(v) = git.cmd(["lfs", "version"]).cwd(&home).read_only().purpose("tools").stdout().await {
        // "git-lfs/3.7.1 (GitHub; windows amd64; go 1.25.1; git b84b3384)"
        let ver = v.trim().strip_prefix("git-lfs/").and_then(|r| r.split_whitespace().next()).unwrap_or(v.trim()).to_string();
        st.lfs = Some(ToolInfo { path: None, version: ver });
    }
    if let Ok(v) = git.cmd(["config", "--get", "filter.lfs.process"]).cwd(&home).read_only().ok_codes(&[0, 1]).stdout().await {
        st.lfs_initialized = !v.trim().is_empty();
    }
    st
}

/// `git lfs install --skip-repo`: sets up the LFS filters for this user without touching any
/// repository.
pub async fn lfs_install_global(git: &GitRunner) -> Result<()> {
    let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
    git.cmd(["lfs", "install", "--skip-repo"]).cwd(home).purpose("lfs_install").run().await?;
    Ok(())
}

/// Packages the app may install through winget. Anything else is refused.
pub const WINGET_PACKAGES: &[&str] = &["Git.Git", "GitHub.GitLFS"];

pub async fn winget_install(package: &str) -> Result<()> {
    if !WINGET_PACKAGES.contains(&package) {
        return Err(Error::invalid("package_not_allowed"));
    }
    let winget = find_on_path("winget").ok_or_else(|| Error::invalid("winget_missing"))?;
    let mut cmd = tokio::process::Command::new(winget);
    cmd.args(["install", "--id", package, "-e", "--silent", "--accept-source-agreements", "--accept-package-agreements"]);
    #[cfg(windows)]
    {
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().await?;
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
        return Err(Error::invalid(format!("winget: {}", crate::util::tail(msg.trim(), 600))));
    }
    Ok(())
}

pub fn find_on_path(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    let paths = std::env::var_os("PATH")?;
    // winget is an app execution alias: a reparse point that only symlink_metadata can see.
    std::env::split_paths(&paths).map(|d| d.join(&exe)).find(|p| p.is_file() || p.symlink_metadata().is_ok())
}
