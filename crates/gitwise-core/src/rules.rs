//! Rules decide which profile a repository belongs to, so the right account is picked before the
//! user has to think about it: "everything under github.com/my-company is Work", "everything in
//! D:\Work is Work". First matching rule wins.

use globset::GlobBuilder;
use serde::{Deserialize, Serialize};

use crate::remote::RemoteUrl;
use crate::util::path_key;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RuleKind {
    /// Matches remote URLs as `host/owner/repo`, e.g. `github.com/my-company` or
    /// `github.com/my-company/*-web`.
    Remote,
    /// Matches the repository's folder, e.g. `D:\Work`.
    Path,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: String,
    pub kind: RuleKind,
    pub pattern: String,
    pub profile_id: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}

pub fn resolve<'a>(rules: &'a [Rule], repo_path: &str, remotes: &[RemoteUrl]) -> Option<&'a Rule> {
    rules.iter().filter(|r| r.enabled).find(|r| rule_matches(r, repo_path, remotes))
}

pub fn rule_matches(rule: &Rule, repo_path: &str, remotes: &[RemoteUrl]) -> bool {
    match rule.kind {
        RuleKind::Remote => {
            let pat = normalize_remote_pattern(&rule.pattern);
            !pat.is_empty() && remotes.iter().any(|r| pattern_matches(&pat, &r.normalized()))
        }
        RuleKind::Path => {
            let pat = path_key(rule.pattern.trim());
            !pat.is_empty() && pattern_matches(&pat, &path_key(repo_path))
        }
    }
}

/// Accepts whatever a user pastes (`https://github.com/acme/`, `git@github.com:acme`,
/// `github.com/acme`) and reduces it to `github.com/acme`.
pub fn normalize_remote_pattern(p: &str) -> String {
    let p = p.trim();
    if p.contains("://") || (p.contains('@') && p.contains(':')) {
        if let Some(r) = RemoteUrl::parse(p) {
            let n = r.normalized();
            return n.trim_end_matches('/').to_string();
        }
    }
    p.to_lowercase().trim_end_matches('/').trim_end_matches(".git").to_string()
}

fn pattern_matches(pat: &str, value: &str) -> bool {
    if pat.contains(['*', '?', '[']) {
        match GlobBuilder::new(pat).case_insensitive(true).literal_separator(false).build() {
            Ok(g) => g.compile_matcher().is_match(value),
            Err(_) => false,
        }
    } else {
        value == pat || value.starts_with(&format!("{pat}/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(kind: RuleKind, pattern: &str, profile: &str) -> Rule {
        Rule { id: pattern.into(), kind, pattern: pattern.into(), profile_id: profile.into(), enabled: true }
    }

    fn remotes(urls: &[&str]) -> Vec<RemoteUrl> {
        urls.iter().filter_map(|u| RemoteUrl::parse(u)).collect()
    }

    #[test]
    fn owner_prefix_matches_both_protocols() {
        let rules = vec![rule(RuleKind::Remote, "github.com/Acme", "work")];
        let https = remotes(&["https://github.com/acme/site.git"]);
        let ssh = remotes(&["git@github.com:ACME/site.git"]);
        let other = remotes(&["git@github.com:acme-personal/site.git"]);
        assert_eq!(resolve(&rules, "", &https).map(|r| r.profile_id.as_str()), Some("work"));
        assert_eq!(resolve(&rules, "", &ssh).map(|r| r.profile_id.as_str()), Some("work"));
        assert!(resolve(&rules, "", &other).is_none(), "prefix must stop at a path segment");
    }

    #[test]
    fn pasted_urls_work_as_patterns() {
        assert_eq!(normalize_remote_pattern("https://github.com/Acme/"), "github.com/acme");
        assert_eq!(normalize_remote_pattern("git@github.com:acme"), "github.com/acme");
    }

    #[test]
    fn globs() {
        let rules = vec![rule(RuleKind::Remote, "github.com/acme/*-web", "work")];
        assert!(resolve(&rules, "", &remotes(&["https://github.com/acme/shop-web"])).is_some());
        assert!(resolve(&rules, "", &remotes(&["https://github.com/acme/shop-api"])).is_none());
    }

    #[test]
    fn path_rules() {
        let rules = vec![rule(RuleKind::Path, r"D:\Work", "work")];
        if cfg!(windows) {
            assert!(resolve(&rules, r"d:\work\game", &[]).is_some());
        }
        assert!(resolve(&rules, "D:/Work/game", &[]).is_some());
        assert!(resolve(&rules, "D:/Workshop/game", &[]).is_none());
    }

    #[test]
    fn first_enabled_rule_wins() {
        let mut first = rule(RuleKind::Remote, "github.com/acme", "a");
        first.enabled = false;
        let rules = vec![first, rule(RuleKind::Remote, "github.com", "b"), rule(RuleKind::Remote, "github.com/acme", "c")];
        assert_eq!(resolve(&rules, "", &remotes(&["https://github.com/acme/x"])).unwrap().profile_id, "b");
    }
}
