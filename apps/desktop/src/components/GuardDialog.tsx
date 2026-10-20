import { useState } from "react";
import { AlertOctagon, AlertTriangle, Info, ShieldCheck } from "lucide-react";
import { api } from "../api";
import { bindRepo, closeGuard, currentRepoEntry, fixAuthors, reevaluateGuard, runOp, useApp } from "../store";
import { hasKey, useT } from "../i18n";
import type { Issue, Profile } from "../types";
import { Avatar, Modal } from "./ui";

export function GuardDialog() {
  const t = useT();
  const guard = useApp((s) => s.guard);
  const data = useApp((s) => s.data);
  const [working, setWorking] = useState(false);
  if (!guard || !data) return null;
  const { report, path } = guard;
  const profiles = data.profiles;
  const current = profiles.find((p) => p.id === currentRepoEntry()?.profileId);
  const blocked = !report.canProceed;

  async function switchTo(id: string) {
    setWorking(true);
    const ok = await bindRepo(path, id, true);
    if (ok) await reevaluateGuard();
    setWorking(false);
  }

  async function setupLfs() {
    setWorking(true);
    await runOp("lfs_install", () => api.repoLfsInstall(path));
    useApp.setState({ tools: await api.toolsStatus().catch(() => useApp.getState().tools) });
    await reevaluateGuard();
    setWorking(false);
  }

  async function rewrite() {
    setWorking(true);
    if (await fixAuthors(path)) await reevaluateGuard();
    setWorking(false);
  }

  function editProfile(id?: string | null) {
    closeGuard(false);
    useApp.setState({ profileEditor: { id: id ?? null } });
  }

  function fixes(issue: Issue) {
    const p = issue.params;
    const profileButtons = (list: Profile[]) =>
      list
        .filter((x) => x.id !== current?.id)
        .map((x) => (
          <button key={x.id} className="btn sm" disabled={working} onClick={() => switchTo(x.id)}>
            <Avatar profile={x} size="sm" /> {t("guard.fix.switchTo", { profile: x.label })}
          </button>
        ));
    switch (issue.code) {
      case "no_profile": {
        const sorted = [...profiles].sort((a, b) => (a.id === p.suggestedId ? -1 : b.id === p.suggestedId ? 1 : 0));
        return profileButtons(sorted);
      }
      case "rule_mismatch":
        return profileButtons(profiles.filter((x) => x.id === p.ruleProfileId));
      case "history_other_profile":
        return profileButtons(profiles.filter((x) => x.id === p.otherId));
      case "host_mismatch":
        return profileButtons(profiles.filter((x) => x.host === p.remoteHost));
      case "identity_missing":
      case "no_token":
      case "ssh_agent":
        return (
          <button className="btn sm" onClick={() => editProfile(current?.id)}>
            {t("guard.fix.editProfile")}
          </button>
        );
      case "foreign_commits":
        return current ? (
          <>
            <button className="btn sm primary" disabled={working} onClick={rewrite}>
              {t("guard.fix.rewriteAuthors", { profile: current.label })}
            </button>
            {profileButtons(profiles.filter((x) => (p.emails as string[] | undefined)?.some((e) => x.commitEmail === e || x.extraEmails.includes(e))))}
            <div className="hint" style={{ width: "100%" }}>
              {t("guard.fix.rewriteAuthors.hint")}
            </div>
          </>
        ) : null;
      case "lfs_not_ready":
      case "lfs_hooks_missing":
        return (
          <button className="btn sm" disabled={working} onClick={setupLfs}>
            {t("guard.fix.setupLfs")}
          </button>
        );
      default:
        return null;
    }
  }

  function text(issue: Issue) {
    const params: Record<string, string | number> = {};
    for (const [k, v] of Object.entries(issue.params)) params[k] = Array.isArray(v) ? v.filter((x) => typeof x === "string").join(", ") : (v as string);
    const titleKey = `guard.${issue.code}.title`;
    const bodyKey = `guard.${issue.code}.body`;
    return {
      title: hasKey(titleKey) ? t(titleKey, params) : issue.code,
      body: hasKey(bodyKey) ? t(bodyKey, params) : "",
    };
  }

  return (
    <Modal
      title={report.action === "commit" ? t("guard.title.commit") : t("guard.title.push")}
      icon={<ShieldCheck size={20} style={{ color: blocked ? "var(--danger)" : "var(--warn)" }} />}
      onClose={() => closeGuard(false)}
      footer={
        <>
          {blocked && <span className="hint">{t("guard.blocked")}</span>}
          <span className="spacer" />
          <button className="btn" onClick={() => closeGuard(false)}>
            {t("common.cancel")}
          </button>
          {!blocked && (
            <button className="btn primary" disabled={working} onClick={() => closeGuard(true)}>
              {t("guard.continue")}
            </button>
          )}
        </>
      }
    >
      {report.issues.map((issue, i) => {
        const { title, body } = text(issue);
        const Icon = issue.severity === "block" ? AlertOctagon : issue.severity === "warn" ? AlertTriangle : Info;
        const color = issue.severity === "block" ? "var(--danger)" : issue.severity === "warn" ? "var(--warn)" : "var(--accent)";
        const commits = issue.code === "foreign_commits" ? (issue.params.commits as { short: string; subject: string; email: string }[]) : [];
        const f = fixes(issue);
        return (
          <div key={i} className={`issue ${issue.severity}`}>
            <Icon size={18} style={{ color, flex: "none", marginTop: 1 }} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div className="title">{title}</div>
              <div className="body">{body}</div>
              {commits.length > 0 && (
                <div style={{ marginTop: 8 }}>
                  {commits.map((c) => (
                    <div key={c.short} className="row" style={{ fontSize: 12 }}>
                      <span className="mono muted">{c.short}</span>
                      <span className="ellipsis" style={{ flex: 1 }}>
                        {c.subject}
                      </span>
                      <span className="muted ellipsis" style={{ maxWidth: 180 }}>
                        {c.email}
                      </span>
                    </div>
                  ))}
                </div>
              )}
              {f && <div className="fixes">{f}</div>}
            </div>
          </div>
        );
      })}
    </Modal>
  );
}
