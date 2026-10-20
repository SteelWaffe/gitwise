//! End-to-end checks against the real git binary: temp repositories, a local bare "remote".

use std::path::Path;

use gitwise_core::git::GitRunner;
use gitwise_core::guard::{self, Action, Severity};
use gitwise_core::ops;
use gitwise_core::profile::{Profile, Provider};
use gitwise_core::status::ChangeKind;
use gitwise_core::store::{AppData, RepoEntry};

fn git() -> GitRunner {
    GitRunner::discover(None, None).expect("git must be installed to run these tests")
}

fn profile(id: &str, label: &str, email: &str) -> Profile {
    Profile {
        id: id.into(),
        label: label.into(),
        color: "#4f8cff".into(),
        provider: Provider::Github,
        host: "github.com".into(),
        username: id.into(),
        commit_name: format!("{label} Person"),
        commit_email: email.into(),
        extra_emails: vec![],
        ssh_key_path: None,
        has_token: true,
        has_ssh_passphrase: false,
        avatar_url: None,
    }
}

async fn raw(g: &GitRunner, dir: &Path, args: &[&str]) -> String {
    g.cmd(args).cwd(dir).stdout().await.unwrap()
}

#[tokio::test]
async fn commit_uses_profile_identity_and_only_selected_files() {
    let g = git();
    let tmp = tempfile::tempdir().unwrap();
    let repo = ops::init(&g, &tmp.path().join("r")).await.unwrap();
    // A global-looking identity that must NOT end up in the commit.
    raw(&g, &repo, &["config", "user.email", "wrong@personal.com"]).await;
    raw(&g, &repo, &["config", "user.name", "Wrong"]).await;

    std::fs::write(repo.join("a.txt"), "a").unwrap();
    std::fs::write(repo.join("b [x].txt"), "b").unwrap();
    let st = ops::status(&g, &repo).await.unwrap();
    assert_eq!(st.files.len(), 2);
    assert!(st.branch.oid.is_none());

    let work = profile("w", "Work", "me@company.com");
    let a = st.files.iter().find(|f| f.path == "a.txt").unwrap().clone();
    ops::commit(&g, &repo, &work, "first", &[a]).await.unwrap();

    let author = raw(&g, &repo, &["log", "-1", "--format=%an <%ae>|%cn <%ce>"]).await;
    assert_eq!(author.trim(), "Work Person <me@company.com>|Work Person <me@company.com>");

    let st = ops::status(&g, &repo).await.unwrap();
    assert_eq!(st.files.len(), 1, "unselected file stays uncommitted");
    assert_eq!(st.files[0].path, "b [x].txt");
    assert_eq!(st.files[0].kind, ChangeKind::Untracked);

    // A file with glob characters is taken literally.
    let b = st.files[0].clone();
    ops::commit(&g, &repo, &work, "second", &[b]).await.unwrap();
    assert!(ops::status(&g, &repo).await.unwrap().files.is_empty());
}

#[tokio::test]
async fn previously_staged_but_unselected_files_are_left_out() {
    let g = git();
    let tmp = tempfile::tempdir().unwrap();
    let repo = ops::init(&g, &tmp.path().join("r")).await.unwrap();
    let work = profile("w", "Work", "me@company.com");
    std::fs::write(repo.join("base.txt"), "0").unwrap();
    let st = ops::status(&g, &repo).await.unwrap();
    ops::commit(&g, &repo, &work, "base", &st.files).await.unwrap();

    std::fs::write(repo.join("base.txt"), "1").unwrap();
    std::fs::write(repo.join("other.txt"), "x").unwrap();
    raw(&g, &repo, &["add", "other.txt"]).await;
    let st = ops::status(&g, &repo).await.unwrap();
    let base = st.files.iter().find(|f| f.path == "base.txt").unwrap().clone();
    ops::commit(&g, &repo, &work, "only base", &[base]).await.unwrap();

    let names = raw(&g, &repo, &["show", "--name-only", "--format=", "HEAD"]).await;
    assert_eq!(names.trim(), "base.txt");
    let st = ops::status(&g, &repo).await.unwrap();
    assert_eq!(st.files.len(), 1);
    assert_eq!(st.files[0].path, "other.txt");
}

#[tokio::test]
async fn push_to_new_remote_sets_upstream_and_guard_flags_foreign_commits() {
    let g = git();
    let tmp = tempfile::tempdir().unwrap();
    let remote = tmp.path().join("remote.git");
    std::fs::create_dir_all(&remote).unwrap();
    raw(&g, &remote, &["init", "--bare", "-b", "main"]).await;

    let repo = ops::init(&g, &tmp.path().join("r")).await.unwrap();
    raw(&g, &repo, &["remote", "add", "origin", &remote.to_string_lossy()]).await;
    let work = profile("w", "Work", "me@company.com");
    let personal = profile("p", "Personal", "me@gmail.com");

    std::fs::write(repo.join("x.txt"), "x").unwrap();
    let st = ops::status(&g, &repo).await.unwrap();
    ops::commit(&g, &repo, &personal, "oops, personal", &st.files).await.unwrap();

    let data = AppData {
        profiles: vec![work.clone(), personal.clone()],
        repos: vec![RepoEntry {
            path: repo.display().to_string(),
            name: "r".into(),
            profile_id: Some("w".into()),
            added_at: 0,
            last_opened_at: None,
        }],
        ..Default::default()
    };

    let report = guard::evaluate(&g, &repo, &data, Action::Push).await.unwrap();
    let foreign = report.issues.iter().find(|i| i.code == "foreign_commits").expect("foreign commit warning");
    assert_eq!(foreign.severity, Severity::Warn);
    assert!(report.needs_confirmation);

    let commit_report = guard::evaluate(&g, &repo, &data, Action::Commit).await.unwrap();
    assert!(commit_report.issues.iter().any(|i| i.code == "history_other_profile"));

    let res = ops::push(&g, &repo, None, None).await.unwrap();
    assert!(res.set_upstream);
    assert_eq!(res.remote, "origin");
    let st = ops::status(&g, &repo).await.unwrap();
    assert_eq!(st.branch.upstream.as_deref(), Some("origin/main"));
    assert_eq!(st.branch.ahead, 0);

    // After the push nothing is unpushed, so no foreign-commit warning remains.
    let report = guard::evaluate(&g, &repo, &data, Action::Push).await.unwrap();
    assert!(!report.issues.iter().any(|i| i.code == "foreign_commits"));
}

#[tokio::test]
async fn rewrite_fixes_only_unpushed_commits() {
    let g = git();
    let tmp = tempfile::tempdir().unwrap();
    let remote = tmp.path().join("remote.git");
    std::fs::create_dir_all(&remote).unwrap();
    raw(&g, &remote, &["init", "--bare", "-b", "main"]).await;
    let repo = ops::init(&g, &tmp.path().join("r")).await.unwrap();
    raw(&g, &repo, &["remote", "add", "origin", &remote.to_string_lossy()]).await;
    let work = profile("w", "Work", "me@company.com");
    let personal = profile("p", "Personal", "me@gmail.com");

    std::fs::write(repo.join("a.txt"), "a").unwrap();
    let st = ops::status(&g, &repo).await.unwrap();
    ops::commit(&g, &repo, &personal, "pushed by personal", &st.files).await.unwrap();
    ops::push(&g, &repo, None, None).await.unwrap();

    for (i, name) in ["b.txt", "c.txt"].iter().enumerate() {
        std::fs::write(repo.join(name), "x").unwrap();
        let st = ops::status(&g, &repo).await.unwrap();
        ops::commit(&g, &repo, &personal, &format!("oops {i}"), &st.files).await.unwrap();
    }
    // A dirty file must survive the rewrite.
    std::fs::write(repo.join("a.txt"), "dirty").unwrap();

    let n = ops::rewrite_unpushed_authors(&g, &repo, &work).await.unwrap();
    assert_eq!(n, 2);
    let authors = raw(&g, &repo, &["log", "--format=%ae|%ce|%s"]).await;
    let lines: Vec<&str> = authors.lines().collect();
    assert_eq!(lines[0], "me@company.com|me@company.com|oops 1");
    assert_eq!(lines[1], "me@company.com|me@company.com|oops 0");
    assert_eq!(lines[2], "me@gmail.com|me@gmail.com|pushed by personal", "pushed history stays untouched");
    assert_eq!(std::fs::read_to_string(repo.join("a.txt")).unwrap(), "dirty");
    assert_eq!(ops::status(&g, &repo).await.unwrap().branch.ahead, 2);
}

#[tokio::test]
async fn rewrite_without_any_remote_rewrites_from_root() {
    let g = git();
    let tmp = tempfile::tempdir().unwrap();
    let repo = ops::init(&g, &tmp.path().join("r")).await.unwrap();
    let work = profile("w", "Work", "me@company.com");
    let personal = profile("p", "Personal", "me@gmail.com");
    std::fs::write(repo.join("a.txt"), "a").unwrap();
    let st = ops::status(&g, &repo).await.unwrap();
    ops::commit(&g, &repo, &personal, "root", &st.files).await.unwrap();
    assert_eq!(ops::rewrite_unpushed_authors(&g, &repo, &work).await.unwrap(), 1);
    assert_eq!(raw(&g, &repo, &["log", "-1", "--format=%ae"]).await.trim(), "me@company.com");
}

#[tokio::test]
async fn no_profile_blocks() {
    let g = git();
    let tmp = tempfile::tempdir().unwrap();
    let repo = ops::init(&g, &tmp.path().join("r")).await.unwrap();
    let report = guard::evaluate(&g, &repo, &AppData::default(), Action::Commit).await.unwrap();
    assert!(!report.can_proceed);
    assert_eq!(report.issues[0].code, "no_profile");
}

#[tokio::test]
async fn discard_restores_and_trashes() {
    let g = git();
    let tmp = tempfile::tempdir().unwrap();
    let repo = ops::init(&g, &tmp.path().join("r")).await.unwrap();
    let work = profile("w", "Work", "me@company.com");
    std::fs::write(repo.join("keep.txt"), "original").unwrap();
    let st = ops::status(&g, &repo).await.unwrap();
    ops::commit(&g, &repo, &work, "base", &st.files).await.unwrap();

    std::fs::write(repo.join("keep.txt"), "changed").unwrap();
    let st = ops::status(&g, &repo).await.unwrap();
    ops::discard(&g, &repo, &st.files).await.unwrap();
    assert_eq!(std::fs::read_to_string(repo.join("keep.txt")).unwrap(), "original");
    assert!(ops::status(&g, &repo).await.unwrap().files.is_empty());
}

#[tokio::test]
async fn binding_writes_local_config() {
    let g = git();
    let tmp = tempfile::tempdir().unwrap();
    let repo = ops::init(&g, &tmp.path().join("r")).await.unwrap();
    raw(&g, &repo, &["remote", "add", "origin", "https://github.com/acme/site.git"]).await;
    let mut work = profile("w", "Work", "me@company.com");
    work.username = "acme-me".into();
    let remotes = ops::remotes(&g, &repo).await.unwrap();
    let writes = ops::binding_config(&work, &remotes);
    ops::write_local_config(&g, &repo, &writes, "Work").await.unwrap();
    let ov = ops::overview(&g, &repo).await.unwrap();
    assert_eq!(ov.local_email.as_deref(), Some("me@company.com"));
    assert_eq!(ov.local_profile.as_deref(), Some("w"));
    let user = ops::config_get(&g, &repo, "credential.https://github.com.username", true).await.unwrap();
    assert_eq!(user.as_deref(), Some("acme-me"));
}
