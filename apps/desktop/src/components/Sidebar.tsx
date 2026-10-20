import { useMemo, useState } from "react";
import { FolderGit2, Plus, Settings as SettingsIcon, Users } from "lucide-react";
import { useApp, openSettings, selectRepo } from "../store";
import { useT } from "../i18n";
import type { RepoEntry } from "../types";

export function Sidebar() {
  const t = useT();
  const data = useApp((s) => s.data);
  const repo = useApp((s) => s.repo);
  const view = useApp((s) => s.view);
  const [filter, setFilter] = useState("");

  const groups = useMemo(() => {
    if (!data) return [];
    const q = filter.trim().toLowerCase();
    const repos = data.repos.filter((r) => !q || r.name.toLowerCase().includes(q) || r.path.toLowerCase().includes(q));
    const out: { key: string; label: string; color?: string; repos: RepoEntry[] }[] = [];
    for (const p of data.profiles) {
      const list = repos.filter((r) => r.profileId === p.id);
      if (list.length) out.push({ key: p.id, label: p.label, color: p.color, repos: list });
    }
    const unbound = repos.filter((r) => !r.profileId || !data.profiles.some((p) => p.id === r.profileId));
    if (unbound.length) out.push({ key: "_none", label: t("nav.unbound"), repos: unbound });
    for (const g of out) g.repos.sort((a, b) => a.name.localeCompare(b.name));
    return out;
  }, [data, filter, t]);

  return (
    <aside className="sidebar">
      <div className="brand">
        <img src="/icon.svg" alt="" />
        Gitwise
      </div>

      <div className="side-section">
        {t("nav.repos")}
        <span className="spacer" />
        <button className="icon-btn" title={t("nav.addRepo")} onClick={() => useApp.setState({ addRepo: "local" })}>
          <Plus size={15} />
        </button>
      </div>
      {(data?.repos.length ?? 0) > 6 && (
        <input className="input side-filter" placeholder={t("nav.filter")} value={filter} onChange={(e) => setFilter(e.target.value)} />
      )}

      <div className="repo-list">
        {data?.repos.length === 0 && <div className="muted" style={{ padding: "10px 8px" }}>{t("nav.noRepos")}</div>}
        {groups.map((g) => (
          <div key={g.key}>
            <div className="repo-group-label">
              <span className={`dot ${g.color ? "" : "hollow"}`} style={g.color ? { background: g.color } : undefined} />
              {g.label}
            </div>
            {g.repos.map((r) => (
              <button
                key={r.path}
                className={`repo-item ${view === "repo" && repo === r.path ? "active" : ""}`}
                onClick={() => selectRepo(r.path)}
                title={r.path}
              >
                <FolderGit2 size={15} style={{ color: g.color ?? "var(--text-3)", flex: "none" }} />
                <span className="ellipsis name">{r.name}</span>
              </button>
            ))}
          </div>
        ))}
      </div>

      <div className="side-footer">
        <button className={`side-link ${view === "settings" ? "active" : ""}`} onClick={() => openSettings("profiles")}>
          <Users size={15} />
          <span className="spacer" style={{ textAlign: "left" }}>
            {t("profiles.title")}
          </span>
          <span className="row" style={{ gap: 3 }}>
            {data?.profiles.slice(0, 4).map((p) => (
              <span key={p.id} className="dot" style={{ background: p.color, width: 8, height: 8 }} />
            ))}
          </span>
        </button>
        <button className="side-link" onClick={() => openSettings("general")}>
          <SettingsIcon size={15} />
          {t("nav.settings")}
        </button>
      </div>
    </aside>
  );
}
