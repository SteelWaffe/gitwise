import { useState } from "react";
import { CheckCircle2, ChevronDown, ChevronUp, Loader2, Terminal, XCircle } from "lucide-react";
import { api } from "../api";
import { setData, toastError, useApp } from "../store";
import { hasKey, useT } from "../i18n";
import type { LogEvent } from "../types";
import { Progress } from "./ui";

function StatusIcon({ e }: { e: LogEvent }) {
  if (e.status === "running") return <Loader2 size={13} className="spin" />;
  if (e.status === "ok") return <CheckCircle2 size={13} style={{ color: "var(--ok)" }} />;
  return <XCircle size={13} style={{ color: "var(--danger)" }} />;
}

export function StatusBar() {
  const t = useT();
  const logs = useApp((s) => s.logs);
  const open = useApp((s) => s.logOpen);
  const busy = useApp((s) => Object.values(s.busy)[0]);
  const settings = useApp((s) => s.data?.settings);
  const showRead = settings?.showReadCommands ?? false;
  const visible = logs.filter((l) => showRead || !l.readOnly);
  const last = visible[visible.length - 1];
  const [expanded, setExpanded] = useState<number | null>(null);

  const purpose = (p?: string | null) => (p && hasKey(`purpose.${p}`) ? t(`purpose.${p}` as never) : p ?? "");

  return (
    <div className="statusbar">
      {open && (
        <div className="activity">
          <div className="list-head">
            <Terminal size={14} />
            <b style={{ flex: 1 }}>{t("activity.title")}</b>
            <label className="row hint" style={{ cursor: "pointer" }}>
              <input
                type="checkbox"
                className="check"
                checked={showRead}
                onChange={async (e) => {
                  if (!settings) return;
                  try {
                    setData(await api.settingsSave({ ...settings, showReadCommands: e.target.checked }));
                  } catch (err) {
                    toastError(err);
                  }
                }}
              />
              {t("activity.showRead")}
            </label>
            <button className="btn sm ghost" onClick={() => useApp.setState({ logs: [] })}>
              {t("activity.clear")}
            </button>
          </div>
          <div className="activity-list">
            {visible.length === 0 && <div className="muted" style={{ padding: 12 }}>{t("activity.empty")}</div>}
            {[...visible].reverse().map((e) => (
              <div key={e.id} className="log-row">
                <div className="line" onClick={() => setExpanded(expanded === e.id ? null : e.id)}>
                  <StatusIcon e={e} />
                  <span className="muted" style={{ width: 64, flex: "none" }}>
                    {new Date(e.startedAt).toLocaleTimeString()}
                  </span>
                  <span className="badge" style={{ flex: "none" }}>
                    {purpose(e.purpose) || "git"}
                  </span>
                  <span className="log-cmd ellipsis" style={{ flex: 1 }}>
                    {e.command}
                  </span>
                  {e.profile && <span className="badge accent">{t("activity.as", { profile: e.profile })}</span>}
                  {e.durationMs != null && <span className="muted">{e.durationMs} ms</span>}
                </div>
                {expanded === e.id && (
                  <pre>
                    {e.repo ? `# ${e.repo}\n` : ""}$ {e.command}
                    {e.output ? `\n\n${e.output}` : ""}
                    {e.exitCode != null && e.exitCode !== 0 ? `\n\nexit ${e.exitCode}` : ""}
                  </pre>
                )}
              </div>
            ))}
          </div>
        </div>
      )}
      <div className="status-line" onClick={() => useApp.setState({ logOpen: !open })}>
        <Terminal size={13} />
        {busy ? (
          <>
            <Loader2 size={13} className="spin" />
            <span>{purpose(busy.label)}</span>
            {busy.phase && (
              <span className="muted">
                {busy.phase} {busy.percent ?? ""}%
              </span>
            )}
            <div style={{ width: 160 }}>
              <Progress percent={busy.percent} />
            </div>
          </>
        ) : last ? (
          <>
            <StatusIcon e={last} />
            <span className="log-cmd ellipsis" style={{ maxWidth: "60vw" }}>
              {last.command}
            </span>
            {last.profile && <span className="muted">· {t("activity.as", { profile: last.profile })}</span>}
          </>
        ) : (
          <span>{t("activity.idle")}</span>
        )}
        <span className="spacer" />
        <span>{t("activity.title")}</span>
        {open ? <ChevronDown size={14} /> : <ChevronUp size={14} />}
      </div>
    </div>
  );
}
