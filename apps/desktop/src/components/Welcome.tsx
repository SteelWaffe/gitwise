import { FolderGit2, FolderPlus, GitFork, KeyRound, ShieldCheck, UserRound, Wrench } from "lucide-react";
import { openSettings, useApp } from "../store";
import { useT } from "../i18n";

export function Welcome() {
  const t = useT();
  const tools = useApp((s) => s.tools);
  return (
    <div className="welcome">
      <div className="inner">
        <img src="/icon.svg" width={64} height={64} alt="" />
        <h1>{t("welcome.title")}</h1>
        <p className="lead">{t("welcome.lead")}</p>
        <div className="pillars">
          <div className="pillar">
            <UserRound size={20} style={{ color: "var(--accent)" }} />
            <div className="t">{t("welcome.p1.title")}</div>
            <div className="b">{t("welcome.p1.body")}</div>
          </div>
          <div className="pillar">
            <KeyRound size={20} style={{ color: "#ff8a3d" }} />
            <div className="t">{t("welcome.p2.title")}</div>
            <div className="b">{t("welcome.p2.body")}</div>
          </div>
          <div className="pillar">
            <ShieldCheck size={20} style={{ color: "var(--ok)" }} />
            <div className="t">{t("welcome.p3.title")}</div>
            <div className="b">{t("welcome.p3.body")}</div>
          </div>
        </div>
        {tools && !tools.git ? (
          <div className="notice" style={{ margin: "0 auto", maxWidth: 520, textAlign: "left" }}>
            <Wrench size={18} />
            <div style={{ flex: 1 }}>
              <b>{t("welcome.gitMissing.title")}</b>
              <div>{t("welcome.gitMissing.body")}</div>
            </div>
            <button className="btn primary" onClick={() => openSettings("tools")}>
              {t("tools.title")}
            </button>
          </div>
        ) : (
          <button className="btn primary lg" onClick={() => useApp.setState({ profileEditor: { id: null } })}>
            <UserRound size={16} /> {t("welcome.cta")}
          </button>
        )}
      </div>
    </div>
  );
}

export function EmptyRepos() {
  const t = useT();
  const open = (tab: "local" | "clone" | "create") => useApp.setState({ addRepo: tab });
  return (
    <div className="welcome">
      <div className="inner">
        <FolderGit2 size={44} style={{ color: "var(--text-3)" }} />
        <h1 style={{ fontSize: 24 }}>{t("empty.title")}</h1>
        <p className="lead">{t("empty.lead")}</p>
        <div className="choice-grid">
          <button className="choice" onClick={() => open("local")}>
            <FolderGit2 size={20} style={{ color: "var(--accent)" }} />
            <div className="t">{t("empty.local.title")}</div>
            <div className="b">{t("empty.local.body")}</div>
          </button>
          <button className="choice" onClick={() => open("clone")}>
            <GitFork size={20} style={{ color: "#ff8a3d" }} />
            <div className="t">{t("empty.clone.title")}</div>
            <div className="b">{t("empty.clone.body")}</div>
          </button>
          <button className="choice" onClick={() => open("create")}>
            <FolderPlus size={20} style={{ color: "var(--ok)" }} />
            <div className="t">{t("empty.create.title")}</div>
            <div className="b">{t("empty.create.body")}</div>
          </button>
        </div>
      </div>
    </div>
  );
}
