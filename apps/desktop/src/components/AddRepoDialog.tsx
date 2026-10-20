import { useEffect, useMemo, useState } from "react";
import { FolderPlus, Lock, Search } from "lucide-react";
import { api, pickFolder } from "../api";
import { profileById, runOp, selectRepo, setData, toast, toastError, useApp } from "../store";
import { t, useT } from "../i18n";
import type { HostedRepo, RepoAdded } from "../types";
import { joinPath, repoNameFromUrl } from "../util";
import { Avatar, Modal, Progress } from "./ui";

type Tab = "local" | "clone" | "create";

function defaultParent(): string {
  const { data } = useApp.getState();
  if (data?.settings.cloneRoot) return data.settings.cloneRoot;
  const last = [...(data?.repos ?? [])].sort((a, b) => (b.lastOpenedAt ?? 0) - (a.lastOpenedAt ?? 0))[0];
  if (!last) return "";
  const i = Math.max(last.path.lastIndexOf("\\"), last.path.lastIndexOf("/"));
  return i > 0 ? last.path.slice(0, i) : "";
}

export function announceAdded(res: RepoAdded) {
  setData(res.data);
  const repo = res.data.repos.find((r) => r.path === res.path);
  const p = profileById(res.boundProfileId);
  toast({
    kind: "success",
    title: t("add.added", { name: repo?.name ?? res.path }),
    body: p ? (res.boundBy === "rule" ? t("add.boundRule", { profile: p.label }) : res.boundBy === "local" ? t("add.boundLocal", { profile: p.label }) : undefined) : undefined,
  });
  selectRepo(res.path);
}

export function AddRepoDialog() {
  const t = useT();
  const tab = useApp((s) => s.addRepo);
  const data = useApp((s) => s.data);
  const close = () => useApp.setState({ addRepo: null });
  if (!tab || !data) return null;
  const setTab = (x: Tab) => useApp.setState({ addRepo: x });

  return (
    <Modal wide title={t("add.title")} onClose={close}>
      <div className="seg-tabs">
        {(["local", "clone", "create"] as Tab[]).map((x) => (
          <button key={x} className={tab === x ? "on" : ""} onClick={() => setTab(x)}>
            {t(`add.tab.${x}`)}
          </button>
        ))}
      </div>
      {tab === "local" && <LocalTab onDone={close} />}
      {tab === "clone" && <CloneTab onDone={close} />}
      {tab === "create" && <CreateTab onDone={close} />}
    </Modal>
  );
}

function LocalTab({ onDone }: { onDone: () => void }) {
  const t = useT();
  async function choose() {
    const dir = await pickFolder(t("add.chooseFolder"), defaultParent() || undefined);
    if (!dir) return;
    const res = await runOp("open", () => api.repoAdd(dir));
    if (res) {
      onDone();
      announceAdded(res);
    }
  }
  return (
    <div style={{ padding: "8px 0 4px" }}>
      <p className="dim" style={{ marginTop: 0 }}>
        {t("add.local.hint")}
      </p>
      <button className="btn primary lg" onClick={choose}>
        <FolderPlus size={16} /> {t("add.chooseFolder")}
      </button>
    </div>
  );
}

function ProfileSelect({ value, onChange, auto }: { value: string; onChange: (id: string) => void; auto?: boolean }) {
  const t = useT();
  const profiles = useApp((s) => s.data?.profiles ?? []);
  const p = profiles.find((x) => x.id === value);
  return (
    <div className="field">
      <label>
        {t("add.cloneProfile")} {auto && <span className="badge accent">{t("add.cloneProfile.auto")}</span>}
      </label>
      <div className="row">
        <Avatar profile={p} size="sm" />
        <select className="select" value={value} onChange={(e) => onChange(e.target.value)}>
          {profiles.map((x) => (
            <option key={x.id} value={x.id}>
              {x.label} · {x.commitEmail} · {x.host}
            </option>
          ))}
        </select>
      </div>
    </div>
  );
}

function CloneTab({ onDone }: { onDone: () => void }) {
  const t = useT();
  const data = useApp((s) => s.data)!;
  const [profileId, setProfileId] = useState(data.settings.defaultProfileId ?? data.profiles[0]?.id ?? "");
  const [auto, setAuto] = useState(false);
  const [url, setUrl] = useState("");
  const [parent, setParent] = useState(defaultParent());
  const [name, setName] = useState("");
  const [hosted, setHosted] = useState<HostedRepo[] | null>(null);
  const [q, setQ] = useState("");
  const busy = useApp((s) => Object.values(s.busy).find((b) => b.label === "clone"));
  const profile = data.profiles.find((p) => p.id === profileId);

  useEffect(() => {
    setHosted(null);
    if (!profile?.hasToken || !["github", "githubEnterprise", "gitlab"].includes(profile.provider)) return;
    api.hostedRepos(profile.id).then(setHosted, () => setHosted([]));
  }, [profile?.id, profile?.hasToken, profile?.provider]);

  useEffect(() => {
    if (!url.trim()) return;
    const h = setTimeout(() => {
      api.profileForUrl(url).then((id) => {
        if (id && id !== profileId) {
          setProfileId(id);
          setAuto(true);
        }
      }, () => {});
    }, 300);
    return () => clearTimeout(h);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [url]);

  const filtered = useMemo(
    () => (hosted ?? []).filter((r) => !q || r.fullName.toLowerCase().includes(q.toLowerCase())).slice(0, 200),
    [hosted, q],
  );

  const effectiveName = name || repoNameFromUrl(url);
  const dest = parent && effectiveName ? joinPath(parent, effectiveName) : "";

  async function clone() {
    if (!url.trim() || !dest || !profileId) return;
    const res = await runOp("clone", (op) => api.repoClone(url.trim(), dest, profileId, op));
    if (res) {
      onDone();
      announceAdded(res);
    }
  }

  if (data.profiles.length === 0) return <p className="dim">{t("add.noProfiles")}</p>;

  return (
    <>
      <ProfileSelect
        value={profileId}
        auto={auto}
        onChange={(id) => {
          setProfileId(id);
          setAuto(false);
        }}
      />

      {hosted && hosted.length > 0 && (
        <div className="field">
          <label>{t("add.yourRepos", { profile: profile?.label ?? "" })}</label>
          <div className="row" style={{ marginBottom: 6 }}>
            <Search size={14} className="muted" />
            <input className="input" placeholder={t("add.searchRepos")} value={q} onChange={(e) => setQ(e.target.value)} />
          </div>
          <div className="hosted-list">
            {filtered.map((r) => {
              const u = profile?.sshKeyPath ? r.sshUrl : r.cloneUrl;
              return (
                <div key={r.fullName} className={`hosted-item ${url === u ? "on" : ""}`} onClick={() => setUrl(u)}>
                  {r.private && <Lock size={12} className="muted" />}
                  <span className="ellipsis" style={{ flex: 1 }}>
                    {r.fullName}
                  </span>
                </div>
              );
            })}
          </div>
        </div>
      )}

      <div className="field">
        <label>{t("add.cloneUrl")}</label>
        <input className="input mono" value={url} placeholder="https://github.com/owner/repo.git" onChange={(e) => setUrl(e.target.value)} />
      </div>

      <div className="row" style={{ gap: 12, alignItems: "flex-end" }}>
        <div className="field" style={{ flex: 2 }}>
          <label>{t("add.destination")}</label>
          <div className="row">
            <input className="input mono" value={parent} onChange={(e) => setParent(e.target.value)} />
            <button
              className="btn"
              onClick={async () => {
                const d = await pickFolder(t("add.destination"), parent || undefined);
                if (d) setParent(d);
              }}
            >
              {t("common.browse")}
            </button>
          </div>
        </div>
        <div className="field" style={{ flex: 1 }}>
          <label>{t("add.createName")}</label>
          <input className="input mono" value={effectiveName} onChange={(e) => setName(e.target.value)} />
        </div>
      </div>
      {dest && <div className="hint mono" style={{ marginTop: -8, marginBottom: 12 }}>{dest}</div>}

      {busy && (
        <div className="field">
          <div className="row hint">
            <span style={{ flex: 1 }}>{busy.phase ?? t("add.cloning")}</span>
            {busy.percent !== undefined && <span>{busy.percent}%</span>}
          </div>
          <Progress percent={busy.percent} />
          <div className="hint">{t("add.lfsNote")}</div>
        </div>
      )}

      <div className="row">
        <span className="spacer" />
        <button className="btn primary" disabled={!url.trim() || !dest || !!busy} onClick={clone}>
          {t("add.cloneButton")}
        </button>
      </div>
    </>
  );
}

function CreateTab({ onDone }: { onDone: () => void }) {
  const t = useT();
  const data = useApp((s) => s.data)!;
  const [profileId, setProfileId] = useState(data.settings.defaultProfileId ?? data.profiles[0]?.id ?? "");
  const [parent, setParent] = useState(defaultParent());
  const [name, setName] = useState("");
  const dest = parent && name.trim() ? joinPath(parent, name.trim()) : "";

  async function create() {
    if (!dest) return;
    try {
      const res = await api.repoInit(dest, profileId || null);
      onDone();
      announceAdded(res);
    } catch (e) {
      toastError(e);
    }
  }

  return (
    <>
      {data.profiles.length > 0 && <ProfileSelect value={profileId} onChange={setProfileId} />}
      <div className="field">
        <label>{t("add.createName")}</label>
        <input className="input mono" value={name} autoFocus onChange={(e) => setName(e.target.value)} />
      </div>
      <div className="field">
        <label>{t("add.createParent")}</label>
        <div className="row">
          <input className="input mono" value={parent} onChange={(e) => setParent(e.target.value)} />
          <button
            className="btn"
            onClick={async () => {
              const d = await pickFolder(t("add.createParent"), parent || undefined);
              if (d) setParent(d);
            }}
          >
            {t("common.browse")}
          </button>
        </div>
        {dest && <div className="hint mono">{dest}</div>}
      </div>
      <div className="row">
        <span className="spacer" />
        <button className="btn primary" disabled={!dest} onClick={create}>
          {t("add.createButton")}
        </button>
      </div>
    </>
  );
}
