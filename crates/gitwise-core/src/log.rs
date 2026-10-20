//! Commit history.

use serde::Serialize;

/// `git log --format=` producing one [`LOG_FORMAT`] record per commit.
pub const LOG_FORMAT: &str = "%H%x1f%h%x1f%an%x1f%ae%x1f%at%x1f%P%x1f%D%x1f%s%x1e";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub oid: String,
    pub short: String,
    pub author_name: String,
    pub author_email: String,
    /// Unix seconds.
    pub time: i64,
    pub parents: Vec<String>,
    pub refs: Vec<String>,
    pub subject: String,
    /// Not on the upstream yet.
    pub unpushed: bool,
    /// Id of the profile whose email wrote it, when it is one of ours.
    pub profile_id: Option<String>,
}

pub fn parse_log(raw: &str) -> Vec<Commit> {
    raw.split('\x1e')
        .filter_map(|rec| {
            let rec = rec.trim_start_matches(['\n', '\r']);
            if rec.is_empty() {
                return None;
            }
            let f: Vec<&str> = rec.splitn(8, '\x1f').collect();
            if f.len() < 8 {
                return None;
            }
            Some(Commit {
                oid: f[0].to_string(),
                short: f[1].to_string(),
                author_name: f[2].to_string(),
                author_email: f[3].to_string(),
                time: f[4].parse().unwrap_or(0),
                parents: f[5].split_whitespace().map(str::to_string).collect(),
                refs: f[6].split(", ").filter(|s| !s.is_empty()).map(str::to_string).collect(),
                subject: f[7].trim_end().to_string(),
                unpushed: false,
                profile_id: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_records() {
        let raw = "aaaa\x1fa\x1fMe\x1fme@x.com\x1f1700000000\x1fpp qq\x1fHEAD -> main, origin/main\x1fMerge thing\x1e\n\
                   bbbb\x1fb\x1fYou\x1fyou@x.com\x1f1600000000\x1f\x1f\x1fFirst\x1e\n";
        let c = parse_log(raw);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].parents, vec!["pp", "qq"]);
        assert_eq!(c[0].refs, vec!["HEAD -> main", "origin/main"]);
        assert_eq!(c[1].subject, "First");
        assert!(c[1].parents.is_empty());
    }
}
