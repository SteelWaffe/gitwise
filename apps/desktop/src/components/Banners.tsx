import { useEffect, useState } from "react";
import { AlertTriangle, GitMerge, HardDrive } from "lucide-react";
import { api, confirmDialog } from "../api";
import { bindRepo, openSettings, refreshStatus, runOp, bumpHistory, useApp } from "../store";
import { useT } from "../i18n";
import { Avatar } from "./ui";

export function Banners() {
  const t = useT();
  const path = useApp((s) => s.repo);
  const data = useApp((s) => s.data);
  const status = useApp((s) => s.status);
  const overview = useApp((s) => s.overview);
  const tools = useApp((s) => s.tools);
  const entry = data?.repos.find((r) => r.path === path);
  const bound = !!entry?.profileId && !!data?.profiles.some((p) => p.id === entry.profileId);
  const [suggested, setSuggested] = useState<string | null>(null);

  useEffect(() => {
    setSuggested(null);
    if (!path || bound) return;
    api.repoGuard(path, "commit").then((r) => setSuggested(r.suggestedProfileId ?? null), () => {});
  }, [path, bound]);

  if (!path || !data) return null;
  const lfsNotReady = overview?.lfs.usesLfs && (!tools?.lfs || !tools.lfsInitialized);

  return (
    <>
      {!bound && data.profiles.length > 0 && (
        <div className="banner warn">
          <AlertTriangle size={18} className="lead" />
          <div style={{ flex: 1 }}>
            <div className="title">{t("banner.unbound.title")}</div>
            <div className="body">{t("banner.unbound.body")}</div>
          </div>
          {[...data.profiles]
            .sort((a, b) => (a.id === suggested ? -1 : b.id === suggested ? 1 : 0))
            .slice(0, 3)
            .map((p) => (
              <button key={p.id} className={`btn ${p.id === suggested ? "primary" : ""}`} onClick={() => bindRepo(path, p.id)}>
                <Avatar profile={p} size="sm" />
                {p.label}
              </button>
            ))}
        </div>
      )}

      {status && status.state !== "clean" && (
        <div className="banner danger">
          <GitMerge size={18} className="lead" style={{ color: "var(--danger)" }} />
          <div style={{ flex: 1 }}>
            <div className="title">
              {status.state === "merging" ? t("banner.merging.title") : status.state === "rebasing" ? t("banner.rebasing.title") : t("banner.other.title")}
            </div>
            <div className="body">{status.state === "merging" ? t("banner.merging.body") : t("banner.rebasing.body")}</div>
          </div>
          <button
            className="btn"
            onClick={async () => {
              if (!(await confirmDialog(t("banner.abort") + "?", { danger: true }))) return;
              await runOp("abort", () => api.repoAbort(path));
              await refreshStatus();
              bumpHistory();
            }}
          >
            {t("banner.abort")}
          </button>
        </div>
      )}

      {lfsNotReady && (
        <div className="banner warn">
          <HardDrive size={18} className="lead" />
          <div style={{ flex: 1 }}>
            <div className="title">{t("banner.lfs.title")}</div>
            <div className="body">{t("banner.lfs.body")}</div>
          </div>
          <button
            className="btn primary"
            onClick={async () => {
              if (!tools?.lfs) {
                openSettings("tools");
                return;
              }
              await runOp("lfs_install", () => api.repoLfsInstall(path));
              useApp.setState({ tools: await api.toolsStatus() });
            }}
          >
            {t("banner.lfs.fix")}
          </button>
        </div>
      )}
    </>
  );
}
