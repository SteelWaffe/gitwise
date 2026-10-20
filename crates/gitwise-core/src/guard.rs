//! The guard runs before every commit and push and answers one question: is this about to go
//! out under the right account? Issues come with a code and parameters; the UI explains them in
//! the user's language and offers the fix.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::Result;
use crate::git::GitRunner;
use crate::ops;
use crate::remote::Protocol;
use crate::rules;
use crate::store::{AppData, GuardMode};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Info,
    Warn,
    Block,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub code: &'static str,
    pub severity: Severity,
    pub params: BTreeMap<&'static str, Value>,
}

impl Issue {
    fn new(code: &'static str, severity: Severity) -> Self {
        Self { code, severity, params: BTreeMap::new() }
    }

    fn with(mut self, k: &'static str, v: impl Into<Value>) -> Self {
        self.params.insert(k, v.into());
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    Commit,
    Push,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardReport {
    pub action: Action,
    pub profile_id: Option<String>,
    /// The profile the rules point at, when they point somewhere.
    pub suggested_profile_id: Option<String>,
    pub issues: Vec<Issue>,
    pub can_proceed: bool,
    pub needs_confirmation: bool,
}

pub async fn evaluate(git: &GitRunner, repo: &Path, data: &AppData, action: Action) -> Result<GuardReport> {
    let path_str = repo.to_string_lossy().into_owned();
    let bound = data.repo(&path_str).and_then(|e| e.profile_id.clone());
    let remotes = ops::remotes(git, repo).await.unwrap_or_default();
    let parsed: Vec<_> = remotes.iter().filter_map(|r| r.parsed.clone()).collect();
    let rule = rules::resolve(&data.rules, &path_str, &parsed);
    let suggested_profile_id = rule.map(|r| r.profile_id.clone()).or_else(|| data.settings.default_profile_id.clone());

    let mut issues = Vec::new();
    let profile = bound.as_deref().and_then(|id| data.profile(id));
    if let Some(p) = profile {
        if !p.has_identity() {
            issues.push(Issue::new("identity_missing", Severity::Block).with("profile", p.label.clone()));
        }
        if let Some(rule) = rule.filter(|r| r.profile_id != p.id) {
            let rule_label = data.profile(&rule.profile_id).map(|x| x.label.clone()).unwrap_or_default();
            issues.push(
                Issue::new("rule_mismatch", Severity::Warn)
                    .with("profile", p.label.clone())
                    .with("ruleProfile", rule_label)
                    .with("ruleProfileId", rule.profile_id.clone())
                    .with("pattern", rule.pattern.clone()),
            );
        }

        let branch_upstream = match ops::status(git, repo).await.ok().and_then(|s| s.branch.head) {
            Some(b) => ops::upstream_of(git, repo, &b).await.ok().flatten().map(|(r, _)| r),
            None => None,
        };
        let primary = ops::primary_remote(&remotes, branch_upstream.as_deref()).and_then(|r| r.parsed.clone());
        if let Some(r) = primary.as_ref().filter(|r| r.protocol != Protocol::File) {
            if !p.host_matches(&r.host) {
                issues.push(
                    Issue::new("host_mismatch", Severity::Warn)
                        .with("profile", p.label.clone())
                        .with("profileHost", p.host.clone())
                        .with("remoteHost", r.host.clone()),
                );
            }
        }

        let lfs = ops::lfs_info(repo);
        match action {
            Action::Commit => {
                let emails = ops::recent_author_emails(git, repo, 50).await.unwrap_or_default();
                if !emails.is_empty() && !emails.iter().any(|e| p.owns_email(e)) {
                    if let Some(other) = emails.iter().filter_map(|e| data.profile_for_email(e)).find(|o| o.id != p.id) {
                        let count = emails.iter().filter(|e| other.owns_email(e)).count();
                        issues.push(
                            Issue::new("history_other_profile", Severity::Warn)
                                .with("profile", p.label.clone())
                                .with("other", other.label.clone())
                                .with("otherId", other.id.clone())
                                .with("count", count),
                        );
                    }
                }
                if lfs.uses_lfs && !ops::lfs_filter_configured(git, repo).await {
                    issues.push(Issue::new("lfs_not_ready", Severity::Warn));
                }
            }
            Action::Push => {
                let commits = ops::unpushed(git, repo).await.unwrap_or_default();
                let foreign: Vec<_> = commits.iter().filter(|c| !p.owns_email(&c.author_email)).collect();
                if !foreign.is_empty() {
                    let mut emails: Vec<String> = foreign.iter().map(|c| c.author_email.clone()).collect();
                    emails.sort();
                    emails.dedup();
                    let sample: Vec<Value> =
                        foreign.iter().take(5).map(|c| json!({ "short": c.short, "subject": c.subject, "email": c.author_email })).collect();
                    issues.push(
                        Issue::new("foreign_commits", Severity::Warn)
                            .with("profile", p.label.clone())
                            .with("count", foreign.len())
                            .with("emails", emails)
                            .with("commits", sample),
                    );
                }
                if let Some(r) = &primary {
                    if r.is_http() && !p.has_token {
                        issues.push(Issue::new("no_token", Severity::Warn).with("profile", p.label.clone()));
                    }
                    if r.is_ssh() && p.ssh_key_path.as_deref().is_none_or(|k| k.trim().is_empty()) {
                        issues.push(Issue::new("ssh_agent", Severity::Info).with("profile", p.label.clone()));
                    }
                }
                if lfs.uses_lfs && !ops::lfs_hooks_installed(git, repo).await {
                    issues.push(Issue::new("lfs_hooks_missing", Severity::Warn));
                }
            }
        }
    } else {
        let mut i = Issue::new("no_profile", Severity::Block);
        if let Some(s) = suggested_profile_id.as_deref().and_then(|id| data.profile(id)) {
            i = i.with("suggested", s.label.clone()).with("suggestedId", s.id.clone());
        }
        issues.push(i);
    }

    issues.sort_by_key(|i| std::cmp::Reverse(i.severity));
    let can_proceed = !issues.iter().any(|i| i.severity == Severity::Block);
    let needs_confirmation =
        can_proceed && data.settings.guard_mode == GuardMode::Confirm && issues.iter().any(|i| i.severity == Severity::Warn);
    Ok(GuardReport { action, profile_id: bound, suggested_profile_id, issues, can_proceed, needs_confirmation })
}
