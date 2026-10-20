//! Talking to hosting services: who does this token belong to, which emails may it commit with,
//! which repositories can it see. Plus GitHub's device sign-in flow.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::profile::Provider;

const USER_AGENT: &str = concat!("Gitwise/", env!("CARGO_PKG_VERSION"));

/// Scopes the GitHub sign-in asks for. `workflow` is needed to push changes to
/// `.github/workflows`; without it GitHub rejects such pushes.
pub const GITHUB_SCOPES: &str = "repo workflow read:org read:user user:email";

fn client() -> reqwest::Client {
    reqwest::Client::builder().user_agent(USER_AGENT).timeout(Duration::from_secs(30)).build().expect("http client")
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    /// Verified addresses, primary first.
    pub emails: Vec<String>,
    /// GitHub's private commit address (`id+login@users.noreply.github.com`).
    pub noreply_email: Option<String>,
    pub scopes: Option<String>,
}

pub fn github_api(host: &str) -> String {
    if host.eq_ignore_ascii_case("github.com") {
        "https://api.github.com".into()
    } else {
        format!("https://{host}/api/v3")
    }
}

/// Checks a token and reads the account behind it.
pub async fn verify(provider: Provider, host: &str, username: &str, token: &str) -> Result<AccountInfo> {
    match provider {
        Provider::Github | Provider::GithubEnterprise => github_user(host, token).await,
        Provider::Gitlab => gitlab_user(host, token).await,
        Provider::Bitbucket => bitbucket_user(username, token).await,
        Provider::AzureDevops | Provider::Generic => Ok(AccountInfo { login: username.to_string(), ..Default::default() }),
    }
}

fn rejected(status: reqwest::StatusCode) -> Error {
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        Error::invalid("token_rejected")
    } else {
        Error::Http(format!("HTTP {status}"))
    }
}

async fn github_user(host: &str, token: &str) -> Result<AccountInfo> {
    #[derive(Deserialize)]
    struct User {
        login: String,
        id: u64,
        name: Option<String>,
        email: Option<String>,
        avatar_url: Option<String>,
    }
    #[derive(Deserialize)]
    struct Email {
        email: String,
        primary: bool,
        verified: bool,
    }
    let api = github_api(host);
    let http = client();
    let res = http.get(format!("{api}/user")).bearer_auth(token).header("Accept", "application/vnd.github+json").send().await?;
    if !res.status().is_success() {
        return Err(rejected(res.status()));
    }
    let scopes = res.headers().get("x-oauth-scopes").and_then(|v| v.to_str().ok()).map(str::to_string);
    let u: User = res.json().await?;

    let mut emails: Vec<Email> = match http.get(format!("{api}/user/emails")).bearer_auth(token).header("Accept", "application/vnd.github+json").send().await {
        Ok(r) if r.status().is_success() => r.json().await.unwrap_or_default(),
        _ => vec![],
    };
    emails.retain(|e| e.verified);
    emails.sort_by_key(|e| !e.primary);
    let mut list: Vec<String> = emails.into_iter().map(|e| e.email).filter(|e| !e.ends_with("noreply.github.com")).collect();
    if let Some(e) = u.email.filter(|e| !list.contains(e)) {
        list.push(e);
    }
    let noreply_domain = if host.eq_ignore_ascii_case("github.com") { "users.noreply.github.com".to_string() } else { format!("users.noreply.{host}") };
    Ok(AccountInfo {
        noreply_email: Some(format!("{}+{}@{noreply_domain}", u.id, u.login)),
        login: u.login,
        name: u.name,
        avatar_url: u.avatar_url,
        emails: list,
        scopes,
    })
}

async fn gitlab_user(host: &str, token: &str) -> Result<AccountInfo> {
    #[derive(Deserialize)]
    struct User {
        username: String,
        name: Option<String>,
        avatar_url: Option<String>,
        email: Option<String>,
        commit_email: Option<String>,
        public_email: Option<String>,
    }
    #[derive(Deserialize)]
    struct Email {
        email: String,
    }
    let base = format!("https://{host}/api/v4");
    let http = client();
    let res = http.get(format!("{base}/user")).header("PRIVATE-TOKEN", token).send().await?;
    if !res.status().is_success() {
        return Err(rejected(res.status()));
    }
    let u: User = res.json().await?;
    let mut list: Vec<String> = Vec::new();
    for e in [u.commit_email, u.email, u.public_email].into_iter().flatten() {
        if !e.is_empty() && !list.contains(&e) {
            list.push(e);
        }
    }
    if let Ok(r) = http.get(format!("{base}/user/emails")).header("PRIVATE-TOKEN", token).send().await {
        if r.status().is_success() {
            for e in r.json::<Vec<Email>>().await.unwrap_or_default() {
                if !list.contains(&e.email) {
                    list.push(e.email);
                }
            }
        }
    }
    Ok(AccountInfo { login: u.username, name: u.name, avatar_url: u.avatar_url, emails: list, noreply_email: None, scopes: None })
}

async fn bitbucket_user(username: &str, token: &str) -> Result<AccountInfo> {
    #[derive(Deserialize)]
    struct User {
        username: Option<String>,
        display_name: Option<String>,
    }
    #[derive(Deserialize)]
    struct Email {
        email: String,
        is_primary: bool,
        is_confirmed: bool,
    }
    #[derive(Deserialize)]
    struct Page {
        values: Vec<Email>,
    }
    let http = client();
    let res = http.get("https://api.bitbucket.org/2.0/user").basic_auth(username, Some(token)).send().await?;
    if !res.status().is_success() {
        return Err(rejected(res.status()));
    }
    let u: User = res.json().await?;
    let mut emails: Vec<Email> = match http.get("https://api.bitbucket.org/2.0/user/emails").basic_auth(username, Some(token)).send().await {
        Ok(r) if r.status().is_success() => r.json::<Page>().await.map(|p| p.values).unwrap_or_default(),
        _ => vec![],
    };
    emails.retain(|e| e.is_confirmed);
    emails.sort_by_key(|e| !e.is_primary);
    Ok(AccountInfo {
        login: u.username.unwrap_or_else(|| username.to_string()),
        name: u.display_name,
        avatar_url: None,
        emails: emails.into_iter().map(|e| e.email).collect(),
        noreply_email: None,
        scopes: None,
    })
}

// ---------------------------------------------------------------------------------------------
// GitHub device flow: the app shows a code, the user approves it in the browser.

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum DevicePoll {
    Pending,
    SlowDown { interval: u64 },
    Expired,
    Denied,
    Done {
        #[serde(skip)]
        token: String,
    },
}

#[derive(Deserialize)]
struct DeviceCodeRaw {
    device_code: Option<String>,
    user_code: Option<String>,
    verification_uri: Option<String>,
    expires_in: Option<u64>,
    interval: Option<u64>,
    error: Option<String>,
    error_description: Option<String>,
}

pub async fn github_device_start(host: &str, client_id: &str) -> Result<DeviceCode> {
    let res = client()
        .post(format!("https://{host}/login/device/code"))
        .header("Accept", "application/json")
        .form(&[("client_id", client_id), ("scope", GITHUB_SCOPES)])
        .send()
        .await?;
    let raw: DeviceCodeRaw = res.json().await?;
    if let Some(err) = raw.error {
        return Err(Error::invalid(raw.error_description.unwrap_or(err)));
    }
    match (raw.device_code, raw.user_code, raw.verification_uri) {
        (Some(device_code), Some(user_code), Some(verification_uri)) => Ok(DeviceCode {
            device_code,
            user_code,
            verification_uri,
            expires_in: raw.expires_in.unwrap_or(900),
            interval: raw.interval.unwrap_or(5),
        }),
        _ => Err(Error::Http("unexpected device code response".into())),
    }
}

pub async fn github_device_poll(host: &str, client_id: &str, device_code: &str) -> Result<DevicePoll> {
    #[derive(Deserialize)]
    struct Raw {
        access_token: Option<String>,
        error: Option<String>,
        interval: Option<u64>,
    }
    let res = client()
        .post(format!("https://{host}/login/oauth/access_token"))
        .header("Accept", "application/json")
        .form(&[("client_id", client_id), ("device_code", device_code), ("grant_type", "urn:ietf:params:oauth:grant-type:device_code")])
        .send()
        .await?;
    let raw: Raw = res.json().await?;
    if let Some(token) = raw.access_token {
        return Ok(DevicePoll::Done { token });
    }
    Ok(match raw.error.as_deref() {
        Some("authorization_pending") | None => DevicePoll::Pending,
        Some("slow_down") => DevicePoll::SlowDown { interval: raw.interval.unwrap_or(10) },
        Some("expired_token") => DevicePoll::Expired,
        Some("access_denied") => DevicePoll::Denied,
        Some(other) => return Err(Error::invalid(other.to_string())),
    })
}

// ---------------------------------------------------------------------------------------------
// Repository listing for the clone dialog.

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostedRepo {
    pub full_name: String,
    pub clone_url: String,
    pub ssh_url: String,
    pub private: bool,
    pub description: Option<String>,
    pub updated_at: Option<String>,
}

pub async fn list_repos(provider: Provider, host: &str, token: &str) -> Result<Vec<HostedRepo>> {
    match provider {
        Provider::Github | Provider::GithubEnterprise => github_repos(host, token).await,
        Provider::Gitlab => gitlab_repos(host, token).await,
        _ => Ok(vec![]),
    }
}

async fn github_repos(host: &str, token: &str) -> Result<Vec<HostedRepo>> {
    #[derive(Deserialize)]
    struct R {
        full_name: String,
        clone_url: String,
        ssh_url: String,
        private: bool,
        description: Option<String>,
        pushed_at: Option<String>,
    }
    let api = github_api(host);
    let http = client();
    let mut out = Vec::new();
    for page in 1..=5 {
        let res = http
            .get(format!("{api}/user/repos"))
            .query(&[("per_page", "100"), ("sort", "pushed"), ("page", &page.to_string())])
            .bearer_auth(token)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await?;
        if !res.status().is_success() {
            return Err(rejected(res.status()));
        }
        let batch: Vec<R> = res.json().await?;
        let n = batch.len();
        out.extend(batch.into_iter().map(|r| HostedRepo {
            full_name: r.full_name,
            clone_url: r.clone_url,
            ssh_url: r.ssh_url,
            private: r.private,
            description: r.description,
            updated_at: r.pushed_at,
        }));
        if n < 100 {
            break;
        }
    }
    Ok(out)
}

async fn gitlab_repos(host: &str, token: &str) -> Result<Vec<HostedRepo>> {
    #[derive(Deserialize)]
    struct R {
        path_with_namespace: String,
        http_url_to_repo: String,
        ssh_url_to_repo: String,
        visibility: Option<String>,
        description: Option<String>,
        last_activity_at: Option<String>,
    }
    let res = client()
        .get(format!("https://{host}/api/v4/projects"))
        .query(&[("membership", "true"), ("per_page", "100"), ("order_by", "last_activity_at"), ("simple", "true")])
        .header("PRIVATE-TOKEN", token)
        .send()
        .await?;
    if !res.status().is_success() {
        return Err(rejected(res.status()));
    }
    let list: Vec<R> = res.json().await?;
    Ok(list
        .into_iter()
        .map(|r| HostedRepo {
            full_name: r.path_with_namespace,
            clone_url: r.http_url_to_repo,
            ssh_url: r.ssh_url_to_repo,
            private: r.visibility.as_deref() != Some("public"),
            description: r.description,
            updated_at: r.last_activity_at,
        })
        .collect())
}
