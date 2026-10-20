//! A profile is one "you": the identity written into commits *and* the credentials used to push.
//! Keeping both in one place is the point; git itself keeps them in unrelated settings, which is
//! how a work repository ends up with commits from a personal account.

use serde::{Deserialize, Serialize};

use crate::git::AuthEnv;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Provider {
    Github,
    GithubEnterprise,
    Gitlab,
    Bitbucket,
    AzureDevops,
    Generic,
}

impl Provider {
    pub fn default_host(self) -> &'static str {
        match self {
            Provider::Github => "github.com",
            Provider::Gitlab => "gitlab.com",
            Provider::Bitbucket => "bitbucket.org",
            Provider::AzureDevops => "dev.azure.com",
            Provider::GithubEnterprise | Provider::Generic => "",
        }
    }

    pub fn is_github(self) -> bool {
        matches!(self, Provider::Github | Provider::GithubEnterprise)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    /// What the user calls it: "Work", "Personal".
    pub label: String,
    /// Shown next to everything this profile touches, so the active account is recognisable at
    /// a glance.
    pub color: String,
    pub provider: Provider,
    pub host: String,
    /// Account login used for HTTPS authentication.
    #[serde(default)]
    pub username: String,
    pub commit_name: String,
    pub commit_email: String,
    /// Other addresses that are also this person under this profile (e.g. the GitHub noreply
    /// address), so older commits signed with them do not raise a warning.
    #[serde(default)]
    pub extra_emails: Vec<String>,
    #[serde(default)]
    pub ssh_key_path: Option<String>,
    #[serde(default)]
    pub has_token: bool,
    #[serde(default)]
    pub has_ssh_passphrase: bool,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

impl Profile {
    pub fn owns_email(&self, email: &str) -> bool {
        let e = email.trim();
        self.commit_email.trim().eq_ignore_ascii_case(e) || self.extra_emails.iter().any(|x| x.trim().eq_ignore_ascii_case(e))
    }

    pub fn has_identity(&self) -> bool {
        !self.commit_name.trim().is_empty() && !self.commit_email.trim().is_empty()
    }

    pub fn auth_env(&self) -> AuthEnv {
        AuthEnv {
            profile_id: self.id.clone(),
            profile_label: self.label.clone(),
            host: self.host.clone(),
            username: self.username.clone(),
            ssh_key: self.ssh_key_path.as_ref().filter(|p| !p.trim().is_empty()).map(Into::into),
        }
    }

    pub fn host_matches(&self, host: &str) -> bool {
        !self.host.is_empty() && crate::remote::normalize_host(&self.host) == crate::remote::normalize_host(host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_ownership_ignores_case_and_includes_extras() {
        let p = Profile {
            id: "1".into(),
            label: "Work".into(),
            color: "#000".into(),
            provider: Provider::Github,
            host: "github.com".into(),
            username: "me".into(),
            commit_name: "Me".into(),
            commit_email: "Me@Company.com".into(),
            extra_emails: vec!["1+me@users.noreply.github.com".into()],
            ssh_key_path: None,
            has_token: true,
            has_ssh_passphrase: false,
            avatar_url: None,
        };
        assert!(p.owns_email("me@company.com"));
        assert!(p.owns_email("1+me@users.noreply.github.com"));
        assert!(!p.owns_email("me@gmail.com"));
        assert!(p.host_matches("SSH.github.com"));
    }
}
