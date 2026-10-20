//! The askpass helper.
//!
//! For network commands the app points `GIT_ASKPASS` and `SSH_ASKPASS` at its own executable and
//! clears every credential helper, so git asks *us* for credentials instead of Git Credential
//! Manager guessing an account. The executable starts in askpass mode (see [`ENV_FLAG`]), reads
//! the profile from the environment, takes the secret from the OS credential store and prints it.
//!
//! It answers only for the host the profile belongs to: a submodule or LFS endpoint on another
//! host never receives this profile's token.

use crate::secrets::SecretKind;

pub const ENV_FLAG: &str = "GITWISE_ASKPASS";
pub const ENV_PROFILE: &str = "GITWISE_PROFILE";
pub const ENV_HOST: &str = "GITWISE_HOST";
pub const ENV_USERNAME: &str = "GITWISE_USERNAME";

#[derive(Debug, PartialEq, Eq)]
pub enum Prompt {
    Username { host: Option<String> },
    Password { host: Option<String> },
    SshPassphrase,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Context {
    pub profile_id: String,
    pub host: String,
    pub username: String,
}

impl Context {
    pub fn from_env() -> Option<Self> {
        std::env::var_os(ENV_FLAG)?;
        Some(Self {
            profile_id: std::env::var(ENV_PROFILE).ok()?,
            host: std::env::var(ENV_HOST).unwrap_or_default(),
            username: std::env::var(ENV_USERNAME).unwrap_or_default(),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Answer {
    Text(String),
    Refuse,
}

pub fn is_askpass_invocation() -> bool {
    std::env::var_os(ENV_FLAG).is_some()
}

pub fn parse_prompt(prompt: &str) -> Prompt {
    let p = prompt.trim();
    let lower = p.to_ascii_lowercase();
    if lower.starts_with("username for") {
        return Prompt::Username { host: quoted_host(p) };
    }
    if lower.starts_with("password for") {
        return Prompt::Password { host: quoted_host(p) };
    }
    if lower.starts_with("enter passphrase for") {
        return Prompt::SshPassphrase;
    }
    Prompt::Unknown
}

/// `Password for 'https://user@github.com': ` -> `github.com`
fn quoted_host(p: &str) -> Option<String> {
    let start = p.find('\'')? + 1;
    let end = start + p[start..].find('\'')?;
    host_of_url(&p[start..end])
}

pub fn host_of_url(url: &str) -> Option<String> {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let authority = rest.split('/').next()?;
    let hostport = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
    let host = hostport.split(':').next()?;
    (!host.is_empty()).then(|| crate::remote::normalize_host(host))
}

fn host_ok(asked: &Option<String>, ctx: &Context) -> bool {
    match asked {
        // Without a host we cannot tell where the secret would go; refuse rather than guess.
        None => false,
        Some(h) => !ctx.host.is_empty() && crate::remote::normalize_host(&ctx.host) == *h,
    }
}

pub fn answer(prompt: &Prompt, ctx: &Context, secret: impl Fn(SecretKind) -> Option<String>) -> Answer {
    match prompt {
        Prompt::Username { host } => {
            if !host_ok(host, ctx) {
                return Answer::Refuse;
            }
            // GitHub accepts any user name with a token; the login keeps the log readable.
            let user = if ctx.username.is_empty() { "x-access-token".to_string() } else { ctx.username.clone() };
            Answer::Text(user)
        }
        Prompt::Password { host } => {
            if !host_ok(host, ctx) {
                return Answer::Refuse;
            }
            secret(SecretKind::Token).map(Answer::Text).unwrap_or(Answer::Refuse)
        }
        Prompt::SshPassphrase => secret(SecretKind::SshPassphrase).map(Answer::Text).unwrap_or(Answer::Refuse),
        Prompt::Unknown => Answer::Refuse,
    }
}

/// Entry point of askpass mode: answers the prompt in `argv[1]` and returns the exit code.
pub fn run() -> i32 {
    let Some(ctx) = Context::from_env() else { return 1 };
    let prompt_text = std::env::args().nth(1).unwrap_or_default();
    let prompt = parse_prompt(&prompt_text);
    let ans = answer(&prompt, &ctx, |kind| crate::secrets::get(&ctx.profile_id, kind).ok().flatten());
    match ans {
        Answer::Text(t) => {
            println!("{t}");
            0
        }
        Answer::Refuse => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Context {
        Context { profile_id: "p1".into(), host: "github.com".into(), username: "work-login".into() }
    }

    fn secrets(kind: SecretKind) -> Option<String> {
        Some(match kind {
            SecretKind::Token => "tok".into(),
            SecretKind::SshPassphrase => "phrase".into(),
        })
    }

    #[test]
    fn parses_git_prompts() {
        assert_eq!(
            parse_prompt("Username for 'https://github.com': "),
            Prompt::Username { host: Some("github.com".into()) }
        );
        assert_eq!(
            parse_prompt("Password for 'https://work-login@github.com': "),
            Prompt::Password { host: Some("github.com".into()) }
        );
        assert_eq!(parse_prompt("Enter passphrase for key '/c/Users/me/.ssh/id_ed25519': "), Prompt::SshPassphrase);
        assert_eq!(parse_prompt("Are you sure you want to continue connecting (yes/no)?"), Prompt::Unknown);
    }

    #[test]
    fn answers_only_for_its_host() {
        let c = ctx();
        let u = parse_prompt("Username for 'https://github.com': ");
        assert_eq!(answer(&u, &c, secrets), Answer::Text("work-login".into()));
        let pw = parse_prompt("Password for 'https://work-login@github.com': ");
        assert_eq!(answer(&pw, &c, secrets), Answer::Text("tok".into()));

        let other = parse_prompt("Password for 'https://gitlab.com': ");
        assert_eq!(answer(&other, &c, secrets), Answer::Refuse);
        let no_host = parse_prompt("Password: ");
        assert_eq!(answer(&no_host, &c, secrets), Answer::Refuse);
    }

    #[test]
    fn missing_secret_refuses() {
        let pw = parse_prompt("Password for 'https://github.com': ");
        assert_eq!(answer(&pw, &ctx(), |_| None), Answer::Refuse);
    }

    #[test]
    fn host_with_port() {
        assert_eq!(host_of_url("https://user@ghe.corp:8443/x").as_deref(), Some("ghe.corp"));
    }
}
