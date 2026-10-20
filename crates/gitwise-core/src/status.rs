//! Parses `git status --porcelain=v2 -z --branch`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Untracked,
    Conflicted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    pub orig_path: Option<String>,
    pub kind: ChangeKind,
    /// Raw index (X) and worktree (Y) status letters; `.` means unchanged.
    pub index_status: String,
    pub worktree_status: String,
    pub staged: bool,
    pub unstaged: bool,
    pub submodule: bool,
}

impl FileChange {
    /// Whether HEAD has a version of `path` to go back to.
    pub fn in_head(&self) -> bool {
        !matches!(self.kind, ChangeKind::Untracked) && self.index_status != "A" && !matches!(self.kind, ChangeKind::Renamed | ChangeKind::Copied)
    }
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfo {
    /// `None` when HEAD is detached.
    pub head: Option<String>,
    /// `None` before the first commit.
    pub oid: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusSummary {
    pub branch: BranchInfo,
    pub files: Vec<FileChange>,
}

pub fn parse_porcelain_v2(raw: &[u8]) -> StatusSummary {
    let text = String::from_utf8_lossy(raw);
    let mut out = StatusSummary::default();
    let mut tokens = text.split('\0');
    while let Some(tok) = tokens.next() {
        if tok.is_empty() {
            continue;
        }
        if let Some(h) = tok.strip_prefix("# ") {
            parse_header(h, &mut out.branch);
            continue;
        }
        match tok.as_bytes()[0] {
            b'1' => {
                let parts: Vec<&str> = tok.splitn(9, ' ').collect();
                if parts.len() == 9 {
                    out.files.push(entry(parts[1], parts[2], parts[8], None, false));
                }
            }
            b'2' => {
                let parts: Vec<&str> = tok.splitn(10, ' ').collect();
                let orig = tokens.next().map(str::to_string);
                if parts.len() == 10 {
                    out.files.push(entry(parts[1], parts[2], parts[9], orig, true));
                }
            }
            b'u' => {
                let parts: Vec<&str> = tok.splitn(11, ' ').collect();
                if parts.len() == 11 {
                    let xy = parts[1];
                    out.files.push(FileChange {
                        path: parts[10].to_string(),
                        orig_path: None,
                        kind: ChangeKind::Conflicted,
                        index_status: xy[..1].to_string(),
                        worktree_status: xy[1..2].to_string(),
                        staged: false,
                        unstaged: true,
                        submodule: parts[2].starts_with('S'),
                    });
                }
            }
            b'?' if tok.len() > 2 => out.files.push(FileChange {
                path: tok[2..].to_string(),
                orig_path: None,
                kind: ChangeKind::Untracked,
                index_status: "?".into(),
                worktree_status: "?".into(),
                staged: false,
                unstaged: true,
                submodule: false,
            }),
            _ => {}
        }
    }
    out
}

fn parse_header(h: &str, b: &mut BranchInfo) {
    let (key, value) = h.split_once(' ').unwrap_or((h, ""));
    match key {
        "branch.oid" => b.oid = (value != "(initial)").then(|| value.to_string()),
        "branch.head" => b.head = (value != "(detached)").then(|| value.to_string()),
        "branch.upstream" => b.upstream = Some(value.to_string()),
        "branch.ab" => {
            for part in value.split_whitespace() {
                if let Some(a) = part.strip_prefix('+') {
                    b.ahead = a.parse().unwrap_or(0);
                } else if let Some(x) = part.strip_prefix('-') {
                    b.behind = x.parse().unwrap_or(0);
                }
            }
        }
        _ => {}
    }
}

fn entry(xy: &str, sub: &str, path: &str, orig: Option<String>, renamed: bool) -> FileChange {
    let x = xy.chars().next().unwrap_or('.');
    let y = xy.chars().nth(1).unwrap_or('.');
    let kind = if renamed {
        if x == 'C' {
            ChangeKind::Copied
        } else {
            ChangeKind::Renamed
        }
    } else if x == 'A' {
        ChangeKind::Added
    } else if x == 'D' || y == 'D' {
        ChangeKind::Deleted
    } else if x == 'T' || y == 'T' {
        ChangeKind::TypeChanged
    } else {
        ChangeKind::Modified
    };
    FileChange {
        path: path.to_string(),
        orig_path: orig,
        kind,
        index_status: x.to_string(),
        worktree_status: y.to_string(),
        staged: x != '.',
        unstaged: y != '.',
        submodule: sub.starts_with('S'),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_branch_and_entries() {
        let raw = [
            "# branch.oid 1234567890abcdef",
            "# branch.head main",
            "# branch.upstream origin/main",
            "# branch.ab +2 -1",
            "1 .M N... 100644 100644 100644 aaa bbb src/app.ts",
            "1 A. N... 000000 100644 100644 000 bbb new file.txt",
            "2 R. N... 100644 100644 100644 aaa bbb R100 renamed.txt",
            "old.txt",
            "u UU N... 100644 100644 100644 100644 a b c conflict.txt",
            "? Content/Maps/Level.umap",
            "",
        ]
        .join("\0");
        let s = parse_porcelain_v2(raw.as_bytes());
        assert_eq!(s.branch.head.as_deref(), Some("main"));
        assert_eq!(s.branch.upstream.as_deref(), Some("origin/main"));
        assert_eq!((s.branch.ahead, s.branch.behind), (2, 1));
        assert_eq!(s.files.len(), 5);
        assert_eq!(s.files[0].kind, ChangeKind::Modified);
        assert!(!s.files[0].staged && s.files[0].unstaged);
        assert_eq!(s.files[1].path, "new file.txt");
        assert_eq!(s.files[1].kind, ChangeKind::Added);
        assert!(!s.files[1].in_head());
        assert_eq!(s.files[2].kind, ChangeKind::Renamed);
        assert_eq!(s.files[2].orig_path.as_deref(), Some("old.txt"));
        assert_eq!(s.files[3].kind, ChangeKind::Conflicted);
        assert_eq!(s.files[4].kind, ChangeKind::Untracked);
        assert_eq!(s.files[4].path, "Content/Maps/Level.umap");
    }

    #[test]
    fn initial_and_detached() {
        let s = parse_porcelain_v2(b"# branch.oid (initial)\0# branch.head (detached)\0");
        assert_eq!(s.branch.oid, None);
        assert_eq!(s.branch.head, None);
    }
}
