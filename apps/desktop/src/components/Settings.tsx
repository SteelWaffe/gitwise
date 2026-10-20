import { useEffect, useState } from "react";
import { ArrowDown, ArrowUp, CheckCircle2, CircleAlert, ExternalLink, FolderGit2, HardDrive, Plus, RefreshCw, Trash2, Users, Wrench, SlidersHorizontal, Route } from "lucide-react";
import { api, openUrl, pickFolder } from "../api";
import { applyTheme, runOp, setData, toast, toastError, useApp, type SettingsTab } from "../store";
import { useT } from "../i18n";
import type { Rule, Settings as SettingsT } from "../types";
import { Avatar, Switch, profileStyle } from "./ui";
import { checkForUpdates } from "./UpdateBanner";

export function Settings() {
  const t = useT();
  const tab = useApp((s) => s.settingsTab);
  const items: { id: SettingsTab; label: string; icon: typeof Users }[] = [
    { id: "profiles", label: t("profiles.title"), icon: Users },
    { id: "rules", label: t("rules.title"), icon: Route },
    { id: "tools", label: t("tools.title"), icon: Wrench },
    { id: "general", label: t("general.title"), icon: SlidersHorizontal },
  ];
  return (
    <div className="settings">
      <nav className="settings-nav">
        <h2>{t("nav.settings")}</h2>
        {items.map((i) => (
          <button key={i.id} className={`side-link ${tab === i.id ? "active" : ""}`} onClick={() => useApp.setState({ settingsTab: i.id })}>
            <i.icon size={15} /> {i.label}
          </button>
        ))}
      </nav>
      <div className="settings-body">
        <div className="inner">
          {tab === "profiles" && <ProfilesTab />}
          {tab === "rules" && <RulesTab />}
          {tab === "tools" && <ToolsTab />}
          {tab === "general" && <GeneralTab />}
        </div>
      </div>
    </div>
  );
}

function ProfilesTab() {
  const t = useT();
  const data = useApp((s) => s.data)!;
  const edit = (id: string | null) => useApp.setState({ profileEditor: { id } });

  async function makeDefault(id: string) {
    try {
      setData(await api.settingsSave({ ...data.settings, defaultProfileId: id }));
    } catch (e) {
      toastError(e);
    }
  }

  return (
    <>
      <div className="row">
        <div style={{ flex: 1 }}>
          <h1>{t("profiles.title")}</h1>
          <p className="lead">{t("profiles.lead")}</p>
        </div>
        <button className="btn primary" onClick={() => edit(null)}>
          <Plus size={15} /> {t("profiles.add")}
        </button>
      </div>
      {data.profiles.map((p) => {
        const repos = data.repos.filter((r) => r.profileId === p.id).length;
        return (
          <div key={p.id} className="profile-card" style={profileStyle(p)}>
            <Avatar profile={p} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div className="row">
                <span className="name">{p.label}</span>
                {data.settings.defaultProfileId === p.id && <span className="badge accent">{t("profiles.default")}</span>}
                <span className="badge">{t(`provider.${p.provider}`)}</span>
              </div>
              <div className="sub ellipsis">
                {p.commitName} · {p.commitEmail}
              </div>
              <div className="sub row" style={{ marginTop: 2 }}>
                <span>{p.username ? `@${p.username}` : p.host}</span>
                <span>· {t("profiles.repos", { n: repos })}</span>
                <span>· {p.hasToken ? t("profiles.token") : t("profiles.noToken")}</span>
                {p.sshKeyPath && <span>· {t("profiles.sshKey")}</span>}
              </div>
            </div>
            {data.settings.defaultProfileId !== p.id && (
              <button className="btn sm ghost" onClick={() => makeDefault(p.id)}>
                {t("profiles.makeDefault")}
              </button>
            )}
            <button className="btn sm" onClick={() => edit(p.id)}>
              {t("common.edit")}
            </button>
          </div>
        );
      })}
    </>
  );
}

function RulesTab() {
  const t = useT();
  const data = useApp((s) => s.data)!;
  const [rules, setRules] = useState<Rule[]>(data.rules);
  const dirty = JSON.stringify(rules) !== JSON.stringify(data.rules);
  const firstProfile = data.settings.defaultProfileId ?? data.profiles[0]?.id ?? "";

  const upd = (i: number, patch: Partial<Rule>) => setRules(rules.map((r, j) => (j === i ? { ...r, ...patch } : r)));
  const move = (i: number, d: number) => {
    const j = i + d;
    if (j < 0 || j >= rules.length) return;
    const next = rules.slice();
    [next[i], next[j]] = [next[j], next[i]];
    setRules(next);
  };

  async function save() {
    try {
      const next = await api.rulesSave(rules.filter((r) => r.pattern.trim() && r.profileId));
      setData(next);
      setRules(next.rules);
      toast({ kind: "success", title: t("rules.saved") }, 2000);
    } catch (e) {
      toastError(e);
    }
  }

  return (
    <>
      <h1>{t("rules.title")}</h1>
      <p className="lead">{t("rules.lead")}</p>
      {rules.length === 0 && <div className="notice" style={{ margin: "0 0 12px" }}>{t("rules.empty")}</div>}
      {rules.map((r, i) => (
        <div key={r.id || i} className="rule-row">
          <select className="select" value={r.kind} onChange={(e) => upd(i, { kind: e.target.value as Rule["kind"] })}>
            <option value="remote">{t("rules.kind.remote")}</option>
            <option value="path">{t("rules.kind.path")}</option>
          </select>
          <div className="row">
            <input
              className="input mono"
              value={r.pattern}
              placeholder={r.kind === "remote" ? t("rules.placeholder.remote") : t("rules.placeholder.path")}
              onChange={(e) => upd(i, { pattern: e.target.value })}
            />
            {r.kind === "path" && (
              <button
                className="icon-btn"
                title={t("common.browse")}
                onClick={async () => {
                  const d = await pickFolder(t("rules.kind.path"));
                  if (d) upd(i, { pattern: d });
                }}
              >
                <FolderGit2 size={15} />
              </button>
            )}
          </div>
          <span className="muted">→</span>
          <select className="select" value={r.profileId} onChange={(e) => upd(i, { profileId: e.target.value })}>
            {data.profiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.label}
              </option>
            ))}
          </select>
          <Switch checked={r.enabled} onChange={(v) => upd(i, { enabled: v })} title={t("rules.enabled")} />
          <div className="row" style={{ gap: 0 }}>
            <button className="icon-btn" onClick={() => move(i, -1)} disabled={i === 0}>
              <ArrowUp size={14} />
            </button>
            <button className="icon-btn" onClick={() => move(i, 1)} disabled={i === rules.length - 1}>
              <ArrowDown size={14} />
            </button>
            <button className="icon-btn" onClick={() => setRules(rules.filter((_, j) => j !== i))}>
              <Trash2 size={14} />
            </button>
          </div>
        </div>
      ))}
      <div className="row" style={{ marginTop: 12 }}>
        <button
          className="btn"
          disabled={data.profiles.length === 0}
          onClick={() => setRules([...rules, { id: "", kind: "remote", pattern: "", profileId: firstProfile, enabled: true }])}
        >
          <Plus size={14} /> {t("rules.add")}
        </button>
        <span className="spacer" />
        <button className="btn primary" disabled={!dirty} onClick={save}>
          {t("common.save")}
        </button>
      </div>
    </>
  );
}

function ToolsTab() {
  const t = useT();
  const tools = useApp((s) => s.tools);
  const [checking, setChecking] = useState(false);

  async function recheck() {
    setChecking(true);
    try {
      useApp.setState({ tools: await api.toolsStatus() });
    } catch (e) {
      toastError(e);
    } finally {
      setChecking(false);
    }
  }

  useEffect(() => {
    recheck();
  }, []);

  async function install(pkg: "Git.Git" | "GitHub.GitLFS") {
    const res = await runOp(`install ${pkg}`, () => api.toolsWingetInstall(pkg));
    if (res) useApp.setState({ tools: res });
  }

  async function enableLfs() {
    const res = await runOp("lfs_install", () => api.toolsLfsInstallGlobal());
    if (res) useApp.setState({ tools: res });
  }

  const win = tools?.platform === "windows";
  return (
    <>
      <div className="row">
        <div style={{ flex: 1 }}>
          <h1>{t("tools.title")}</h1>
          <p className="lead">{t("tools.lead")}</p>
        </div>
        <button className="btn" onClick={recheck} disabled={checking}>
          <RefreshCw size={14} className={checking ? "spin" : ""} /> {t("tools.recheck")}
        </button>
      </div>

      <div className="tool-card">
        <FolderGit2 size={22} />
        <div style={{ flex: 1 }}>
          <div className="name">{t("tools.git")}</div>
          <div className="hint">{t("tools.git.desc")}</div>
          {tools?.git ? (
            <div className="row" style={{ marginTop: 6 }}>
              <CheckCircle2 size={14} className="state-ok" />
              <span className="state-ok">{t("tools.installed", { version: tools.git.version })}</span>
              <span className="mono muted ellipsis">{tools.git.path}</span>
            </div>
          ) : (
            <div className="row" style={{ marginTop: 6 }}>
              <CircleAlert size={14} className="state-bad" />
              <span className="state-bad">{t("tools.missing")}</span>
            </div>
          )}
        </div>
        {!tools?.git && (
          <div className="row">
            {win && tools?.winget && (
              <button className="btn primary" onClick={() => install("Git.Git")} title={t("tools.install.hint")}>
                {t("tools.install")}
              </button>
            )}
            <button className="btn" onClick={() => openUrl("https://git-scm.com/downloads")}>
              <ExternalLink size={13} /> {t("tools.downloadPage")}
            </button>
          </div>
        )}
      </div>

      <div className="tool-card">
        <HardDrive size={22} />
        <div style={{ flex: 1 }}>
          <div className="name">{t("tools.lfs")}</div>
          <div className="hint">{t("tools.lfs.desc")}</div>
          {tools?.lfs ? (
            <div className="row" style={{ marginTop: 6 }}>
              <CheckCircle2 size={14} className="state-ok" />
              <span className="state-ok">{t("tools.installed", { version: tools.lfs.version })}</span>
              <span className={tools.lfsInitialized ? "dim" : "state-warn"}>· {tools.lfsInitialized ? t("tools.lfs.on") : t("tools.lfs.off")}</span>
            </div>
          ) : (
            <div className="row" style={{ marginTop: 6 }}>
              <CircleAlert size={14} className="state-bad" />
              <span className="state-bad">{t("tools.missing")}</span>
            </div>
          )}
        </div>
        <div className="row">
          {tools?.lfs && !tools.lfsInitialized && (
            <button className="btn primary" onClick={enableLfs}>
              {t("tools.lfs.enable")}
            </button>
          )}
          {tools?.git && !tools.lfs && win && tools.winget && (
            <button className="btn primary" onClick={() => install("GitHub.GitLFS")} title={t("tools.install.hint")}>
              {t("tools.install")}
            </button>
          )}
          {!tools?.lfs && (
            <button className="btn" onClick={() => openUrl("https://git-lfs.com")}>
              <ExternalLink size={13} /> {t("tools.downloadPage")}
            </button>
          )}
        </div>
      </div>
      {tools?.platform === "macos" && <p className="hint">{t("tools.macHint")}</p>}
    </>
  );
}

function GeneralTab() {
  const t = useT();
  const data = useApp((s) => s.data)!;
  const version = useApp((s) => s.version);
  const dataDir = useApp((s) => s.dataDir);
  const [checking, setChecking] = useState(false);
  const [oauthHost, setOauthHost] = useState("github.com");
  const [oauthId, setOauthId] = useState(data.settings.oauthClientIds["github.com"] ?? "");

  async function save(patch: Partial<SettingsT>) {
    try {
      const next = await api.settingsSave({ ...data.settings, ...patch });
      setData(next);
      if (patch.theme) applyTheme(patch.theme);
    } catch (e) {
      toastError(e);
    }
  }

  return (
    <>
      <h1>{t("general.title")}</h1>
      <p className="lead" />

      <div className="setting-row">
        <div className="k">
          <div className="t">{t("general.language")}</div>
        </div>
        <div className="seg">
          {(["tr", "en"] as const).map((l) => (
            <button key={l} className={data.settings.language === l ? "on" : ""} onClick={() => save({ language: l })}>
              {l === "tr" ? "Türkçe" : "English"}
            </button>
          ))}
        </div>
      </div>

      <div className="setting-row">
        <div className="k">
          <div className="t">{t("general.theme")}</div>
        </div>
        <div className="seg">
          {(["system", "light", "dark"] as const).map((th) => (
            <button key={th} className={data.settings.theme === th ? "on" : ""} onClick={() => save({ theme: th })}>
              {t(`theme.${th}`)}
            </button>
          ))}
        </div>
      </div>

      <div className="setting-row">
        <div className="k">
          <div className="t">{t("general.guard")}</div>
        </div>
        <div className="seg">
          {(["confirm", "relaxed"] as const).map((g) => (
            <button key={g} className={data.settings.guardMode === g ? "on" : ""} onClick={() => save({ guardMode: g })}>
              {t(`general.guard.${g}`)}
            </button>
          ))}
        </div>
      </div>

      <div className="setting-row">
        <div className="k">
          <div className="t">{t("general.updates")}</div>
          <div className="hint">{t("general.autoUpdate")}</div>
        </div>
        <Switch checked={data.settings.autoCheckUpdates} onChange={(v) => save({ autoCheckUpdates: v })} />
        <button
          className="btn"
          disabled={checking}
          onClick={async () => {
            setChecking(true);
            const found = await checkForUpdates(true);
            setChecking(false);
            if (found === false) toast({ kind: "info", title: t("general.upToDate", { version }) });
          }}
        >
          <RefreshCw size={13} className={checking ? "spin" : ""} /> {t("general.checkNow")}
        </button>
      </div>

      <div className="setting-row">
        <div className="k">
          <div className="t">{t("general.cloneRoot")}</div>
          <div className="hint mono">{data.settings.cloneRoot || "—"}</div>
        </div>
        <button
          className="btn"
          onClick={async () => {
            const d = await pickFolder(t("general.cloneRoot"), data.settings.cloneRoot ?? undefined);
            if (d) save({ cloneRoot: d });
          }}
        >
          {t("common.browse")}
        </button>
      </div>

      <div className="section" style={{ marginTop: 24 }}>
        <h3>{t("general.oauth")}</h3>
        <p className="hint" style={{ marginTop: -4 }}>
          {t("general.oauth.hint")}
        </p>
        <div className="row">
          <input
            className="input mono"
            style={{ maxWidth: 200 }}
            value={oauthHost}
            placeholder={t("general.oauth.host")}
            onChange={(e) => {
              setOauthHost(e.target.value.trim());
              setOauthId(data.settings.oauthClientIds[e.target.value.trim()] ?? "");
            }}
          />
          <input className="input mono" value={oauthId} placeholder={t("general.oauth.client")} onChange={(e) => setOauthId(e.target.value.trim())} />
          <button
            className="btn"
            disabled={!oauthHost}
            onClick={() => {
              const ids = { ...data.settings.oauthClientIds };
              if (oauthId) ids[oauthHost] = oauthId;
              else delete ids[oauthHost];
              save({ oauthClientIds: ids }).then(() => toast({ kind: "success", title: t("general.saved") }, 2000));
            }}
          >
            {t("common.save")}
          </button>
        </div>
      </div>

      <div className="setting-row">
        <div className="k">
          <div className="t">{t("general.dataDir")}</div>
          <div className="hint mono selectable">{dataDir}</div>
        </div>
      </div>
      <div className="setting-row">
        <div className="k">
          <div className="t">{t("general.version")}</div>
          <div className="hint mono">{version}</div>
        </div>
      </div>
    </>
  );
}
