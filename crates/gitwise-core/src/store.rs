//! The app's own data: profiles, rules, the repository list and settings, kept as one JSON file
//! in the platform config folder (`%APPDATA%\Gitwise` on Windows,
//! `~/Library/Application Support/Gitwise` on macOS). Secrets are not in here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::profile::Profile;
use crate::rules::Rule;
use crate::util::path_key;

pub const DATA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoEntry {
    pub path: String,
    pub name: String,
    /// The profile this repository is bound to. `None` until the user (or a rule) picks one.
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub added_at: u64,
    #[serde(default)]
    pub last_opened_at: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum GuardMode {
    /// Warnings need a confirmation before the commit or push goes ahead.
    #[default]
    Confirm,
    /// Warnings are shown but do not stop anything (blocking issues still do).
    Relaxed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub default_profile_id: Option<String>,
    #[serde(default = "yes")]
    pub auto_check_updates: bool,
    #[serde(default)]
    pub guard_mode: GuardMode,
    /// Where new clones go unless the user picks another folder.
    #[serde(default)]
    pub clone_root: Option<String>,
    /// OAuth app client ids per host for the GitHub sign-in flow (GitHub Enterprise servers each
    /// need their own app).
    #[serde(default)]
    pub oauth_client_ids: BTreeMap<String, String>,
    /// Show read-only commands (status, diff, log) in the activity panel too.
    #[serde(default)]
    pub show_read_commands: bool,
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_language() -> String {
    "tr".into()
}

fn default_theme() -> String {
    "system".into()
}

fn yes() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: default_language(),
            default_profile_id: None,
            auto_check_updates: true,
            guard_mode: GuardMode::default(),
            clone_root: None,
            oauth_client_ids: BTreeMap::new(),
            show_read_commands: false,
            theme: default_theme(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppData {
    pub version: u32,
    #[serde(default)]
    pub profiles: Vec<Profile>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub repos: Vec<RepoEntry>,
    #[serde(default)]
    pub settings: Settings,
}

impl Default for AppData {
    fn default() -> Self {
        Self { version: DATA_VERSION, profiles: vec![], rules: vec![], repos: vec![], settings: Settings::default() }
    }
}

impl AppData {
    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn repo(&self, path: &str) -> Option<&RepoEntry> {
        let key = path_key(path);
        self.repos.iter().find(|r| path_key(&r.path) == key)
    }

    pub fn repo_mut(&mut self, path: &str) -> Option<&mut RepoEntry> {
        let key = path_key(path);
        self.repos.iter_mut().find(|r| path_key(&r.path) == key)
    }

    /// The profile whose commit emails include `email`.
    pub fn profile_for_email(&self, email: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.owns_email(email))
    }
}

pub struct Store {
    file: PathBuf,
    data: RwLock<AppData>,
}

impl Store {
    pub fn default_dir() -> PathBuf {
        dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("Gitwise")
    }

    pub fn open(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let file = dir.join("data.json");
        let data = match std::fs::read(&file) {
            Ok(bytes) => match serde_json::from_slice::<AppData>(&bytes) {
                Ok(d) => d,
                Err(e) => {
                    // Keep the unreadable file for recovery instead of silently overwriting it.
                    let backup = dir.join(format!("data.broken-{}.json", crate::util::now_ms()));
                    let _ = std::fs::rename(&file, &backup);
                    tracing::warn!("data.json was unreadable ({e}); moved to {}", backup.display());
                    AppData::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => AppData::default(),
            Err(e) => return Err(e.into()),
        };
        Ok(Self { file, data: RwLock::new(data) })
    }

    pub fn snapshot(&self) -> AppData {
        self.data.read().expect("store lock").clone()
    }

    pub fn read<T>(&self, f: impl FnOnce(&AppData) -> T) -> T {
        f(&self.data.read().expect("store lock"))
    }

    /// Changes the data and writes it to disk before returning.
    pub fn update<T>(&self, f: impl FnOnce(&mut AppData) -> Result<T>) -> Result<T> {
        let mut guard = self.data.write().expect("store lock");
        let mut next = guard.clone();
        let out = f(&mut next)?;
        write_atomic(&self.file, &serde_json::to_vec_pretty(&next)?)?;
        *guard = next;
        Ok(out)
    }

    pub fn profile(&self, id: &str) -> Result<Profile> {
        self.read(|d| d.profile(id).cloned()).ok_or_else(|| Error::ProfileNotFound(id.to_string()))
    }
}

fn write_atomic(file: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, file)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_survives_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        store
            .update(|d| {
                d.repos.push(RepoEntry { path: "D:/x".into(), name: "x".into(), profile_id: None, added_at: 1, last_opened_at: None });
                Ok(())
            })
            .unwrap();
        let again = Store::open(dir.path()).unwrap();
        assert_eq!(again.snapshot().repos.len(), 1);

        std::fs::write(dir.path().join("data.json"), b"{ not json").unwrap();
        let fresh = Store::open(dir.path()).unwrap();
        assert!(fresh.snapshot().repos.is_empty());
        let broken = std::fs::read_dir(dir.path()).unwrap().filter_map(|e| e.ok()).any(|e| e.file_name().to_string_lossy().starts_with("data.broken-"));
        assert!(broken);
    }

    #[test]
    fn failed_update_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let r: Result<()> = store.update(|d| {
            d.settings.language = "en".into();
            Err(Error::invalid("nope"))
        });
        assert!(r.is_err());
        assert_eq!(store.snapshot().settings.language, "tr");
    }
}
