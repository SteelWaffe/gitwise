//! Remote URLs in every shape git accepts: `https://host/owner/repo.git`,
//! `ssh://git@host:22/owner/repo.git`, scp-like `git@host:owner/repo.git`, and local paths.

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Protocol {
    Https,
    Http,
    Ssh,
    Git,
    File,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteUrl {
    pub protocol: Protocol,
    /// Lowercase, with the SSH-only aliases of the big hosts folded into their web host
    /// (`ssh.github.com` -> `github.com`), so one profile matches both protocols.
    pub host: String,
    pub port: Option<u16>,
    pub user: Option<String>,
    /// `owner/repo` (or deeper, e.g. GitLab subgroups), without `.git` or surrounding slashes.
    pub path: String,
}

impl RemoteUrl {
    pub fn parse(url: &str) -> Option<Self> {
        let url = url.trim();
        if url.is_empty() {
            return None;
        }
        if let Some((scheme, rest)) = url.split_once("://") {
            let protocol = match scheme.to_ascii_lowercase().as_str() {
                "https" => Protocol::Https,
                "http" => Protocol::Http,
                "ssh" | "git+ssh" | "ssh+git" => Protocol::Ssh,
                "git" => Protocol::Git,
                "file" => return Some(Self::file(rest)),
                _ => return None,
            };
            let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
            let (user, hostport) = match authority.rsplit_once('@') {
                Some((u, h)) => (Some(u.split(':').next().unwrap_or(u).to_string()), h),
                None => (None, authority),
            };
            let (host, port) = split_port(hostport);
            if host.is_empty() {
                return None;
            }
            let host = normalize_host(host);
            let path = clean_path(&host, path);
            return Some(Self { protocol, host, port, user, path });
        }
        if looks_like_local_path(url) {
            return Some(Self::file(url));
        }
        // scp-like: [user@]host:path
        let (left, path) = url.split_once(':')?;
        let (user, host) = match left.rsplit_once('@') {
            Some((u, h)) => (Some(u.to_string()), h),
            None => (None, left),
        };
        if host.is_empty() || host.contains('/') {
            return None;
        }
        let host = normalize_host(host);
        let path = clean_path(&host, path);
        Some(Self { protocol: Protocol::Ssh, host, port: None, user, path })
    }

    fn file(path: &str) -> Self {
        Self { protocol: Protocol::File, host: String::new(), port: None, user: None, path: path.to_string() }
    }

    /// `host/owner/repo`, lowercase: the form rules match against.
    pub fn normalized(&self) -> String {
        format!("{}/{}", self.host, self.path).to_lowercase()
    }

    pub fn owner(&self) -> Option<&str> {
        self.path.split('/').next().filter(|s| !s.is_empty())
    }

    pub fn is_ssh(&self) -> bool {
        self.protocol == Protocol::Ssh
    }

    pub fn is_http(&self) -> bool {
        matches!(self.protocol, Protocol::Https | Protocol::Http)
    }

    /// The `https://host` prefix git scopes credential config by.
    pub fn credential_scope(&self) -> Option<String> {
        if !self.is_http() {
            return None;
        }
        let scheme = if self.protocol == Protocol::Https { "https" } else { "http" };
        Some(match self.port {
            Some(p) => format!("{scheme}://{}:{p}", self.host),
            None => format!("{scheme}://{}", self.host),
        })
    }
}

fn split_port(hostport: &str) -> (&str, Option<u16>) {
    // Bracketed IPv6 hosts are rare enough on git remotes to treat as plain hosts.
    match hostport.rsplit_once(':') {
        Some((h, p)) if !h.is_empty() && p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => (h, p.parse().ok()),
        _ => (hostport, None),
    }
}

pub fn normalize_host(host: &str) -> String {
    let h = host.trim().trim_end_matches('.').to_ascii_lowercase();
    match h.as_str() {
        "ssh.github.com" => "github.com".into(),
        "altssh.gitlab.com" => "gitlab.com".into(),
        "altssh.bitbucket.org" => "bitbucket.org".into(),
        "ssh.dev.azure.com" | "vs-ssh.visualstudio.com" => "dev.azure.com".into(),
        _ => h,
    }
}

fn clean_path(host: &str, path: &str) -> String {
    let mut p = path.trim().trim_matches('/');
    p = p.strip_suffix(".git").unwrap_or(p).trim_end_matches('/');
    if let Some(rest) = p.strip_prefix('~') {
        p = rest.trim_start_matches('/');
    }
    // Azure DevOps SSH paths carry a protocol version segment: v3/org/project/repo.
    if host == "dev.azure.com" {
        if let Some(rest) = p.strip_prefix("v3/") {
            return rest.to_string();
        }
    }
    p.to_string()
}

fn looks_like_local_path(u: &str) -> bool {
    let b = u.as_bytes();
    let drive = b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b.len() == 2 || b[2] == b'\\' || b[2] == b'/');
    drive || u.starts_with('/') || u.starts_with('.') || u.starts_with("\\\\") || u.starts_with('~')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(u: &str) -> RemoteUrl {
        RemoteUrl::parse(u).unwrap_or_else(|| panic!("failed to parse {u}"))
    }

    #[test]
    fn https() {
        let r = p("https://github.com/HotBunny/Hackhub.git");
        assert_eq!(r.protocol, Protocol::Https);
        assert_eq!(r.host, "github.com");
        assert_eq!(r.path, "HotBunny/Hackhub");
        assert_eq!(r.normalized(), "github.com/hotbunny/hackhub");
        assert_eq!(r.owner(), Some("HotBunny"));
        assert_eq!(r.credential_scope().as_deref(), Some("https://github.com"));
    }

    #[test]
    fn https_with_user_and_port() {
        let r = p("https://me:secret@git.example.com:8443/team/sub/repo/");
        assert_eq!(r.user.as_deref(), Some("me"));
        assert_eq!(r.port, Some(8443));
        assert_eq!(r.path, "team/sub/repo");
        assert_eq!(r.credential_scope().as_deref(), Some("https://git.example.com:8443"));
    }

    #[test]
    fn scp_like() {
        let r = p("git@github.com:owner/repo.git");
        assert_eq!(r.protocol, Protocol::Ssh);
        assert_eq!(r.user.as_deref(), Some("git"));
        assert_eq!(r.normalized(), "github.com/owner/repo");
    }

    #[test]
    fn ssh_url_with_alias_host() {
        let r = p("ssh://git@ssh.github.com:443/owner/repo.git");
        assert_eq!(r.host, "github.com");
        assert_eq!(r.port, Some(443));
    }

    #[test]
    fn azure_ssh() {
        let r = p("git@ssh.dev.azure.com:v3/org/project/repo");
        assert_eq!(r.host, "dev.azure.com");
        assert_eq!(r.path, "org/project/repo");
    }

    #[test]
    fn local_paths() {
        assert_eq!(p(r"C:\repos\x").protocol, Protocol::File);
        assert_eq!(p("D:/repos/x").protocol, Protocol::File);
        assert_eq!(p("/srv/git/x.git").protocol, Protocol::File);
        assert_eq!(p("../other").protocol, Protocol::File);
    }

    #[test]
    fn rejects_garbage() {
        assert!(RemoteUrl::parse("").is_none());
        assert!(RemoteUrl::parse("not a url").is_none());
    }
}
