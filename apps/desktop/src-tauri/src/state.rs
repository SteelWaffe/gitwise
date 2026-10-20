use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use gitwise_core::git::{GitRunner, LogEvent, LogSink};
use gitwise_core::store::Store;
use gitwise_core::util::path_key;
use gitwise_core::{Error, Result};
use tauri::{AppHandle, Emitter};

pub struct AppState {
    pub store: Store,
    pub app: AppHandle,
    git: RwLock<Option<GitRunner>>,
    /// Tokens from a finished GitHub sign-in, held here until the profile is saved so they never
    /// sit in the web view.
    pub pending_tokens: Mutex<HashMap<String, String>>,
    repo_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

struct EventSink(AppHandle);

impl LogSink for EventSink {
    fn emit(&self, event: &LogEvent) {
        let _ = self.0.emit("git-log", event);
    }
}

impl AppState {
    pub fn new(app: AppHandle) -> Result<Self> {
        let store = Store::open(&Store::default_dir())?;
        let state = Self {
            store,
            app,
            git: RwLock::new(None),
            pending_tokens: Mutex::new(HashMap::new()),
            repo_locks: Mutex::new(HashMap::new()),
        };
        state.rediscover_git();
        Ok(state)
    }

    /// Looks for git again (after the user installed it, for instance).
    pub fn rediscover_git(&self) {
        let askpass = std::env::current_exe().ok();
        let sink: Arc<dyn LogSink> = Arc::new(EventSink(self.app.clone()));
        let runner = GitRunner::discover(askpass, Some(sink)).ok();
        *self.git.write().expect("git lock") = runner;
    }

    pub fn git(&self) -> Result<GitRunner> {
        self.git.read().expect("git lock").clone().ok_or(Error::GitNotFound)
    }

    pub fn git_opt(&self) -> Option<GitRunner> {
        self.git.read().expect("git lock").clone()
    }

    /// One writer per repository at a time, so two operations never fight over `index.lock`.
    pub fn repo_lock(&self, repo: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.repo_locks.lock().expect("locks");
        locks.entry(path_key(repo)).or_default().clone()
    }
}
