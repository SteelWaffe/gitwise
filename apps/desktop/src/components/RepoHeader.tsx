import { useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowDown,
  ArrowUp,
  Check,
  ChevronDown,
  CloudUpload,
  FolderOpen,
  GitBranch,
  GitBranchPlus,
  Loader2,
  MoreHorizontal,
  RefreshCw,
  SquareTerminal,
  Trash2,
  UserRound,
} from "lucide-react";
import { api, confirmDialog, revealFolder } from "../api";
import {
  bindRepo,
  bumpHistory,
  currentRepoEntry,
  openSettings,
  profileById,
  refreshOverview,
  refreshStatus,
  runOp,
  selectRepo,
  setData,
  toast,
  toastError,
  passGuard,
  useApp,
} from "../store";
import { t as tt, useT } from "../i18n";
import type { Branch } from "../types";
import { Avatar, Popover, profileStyle } from "./ui";
import { relativeTime } from "../util";

const SYNC_LABELS = ["push", "pull", "fetch"];

export function RepoHeader() {
  const t = useT();
  const overview = useApp((s) => s.overview);
  const status = useApp((s) => s.status);
  const repoPath = useApp((s) => s.repo);
  const data = useApp((s) => s.data);
  const entry = data?.repos.find((r) => r.path === repoPath);
  const profile = profileById(entry?.profileId);

  const [open, setOpen] = useState<null | "branch" | "profile" | "more">(null);
  const branchRef = useRef<HTMLButtonElement>(null);
  const profileRef = useRef<HTMLButtonElement>(null);
  const moreRef = useRef<HTMLButtonElement>(null);

  if (!repoPath) return null;
  const branchName = status?.branch.head ?? (status ? t("branches.detached") : "…");

  return (
    <div className="repo-header" style={profileStyle(profile)}>
      <div className="repo-title">
        <div className="name ellipsis">{overview?.name ?? entry?.name}</div>
        <div className="path ellipsis" title={repoPath}>
          {repoPath}
        </div>
      </div>

      <button ref={branchRef} className="header-tile" onClick={() => setOpen(open === "branch" ? null : "branch")} style={{ maxWidth: 260 }}>
        <GitBranch size={16} className="dim" />
        <div className="ellipsis">
          <div className="k">{t("branches.title")}</div>
          <div className="v ellipsis">{branchName}</div>
        </div>
        <ChevronDown size={14} className="muted" />
      </button>

      <button
        ref={profileRef}
        className={`header-tile profile-tile ${profile ? "" : "unbound"}`}
        onClick={() => setOpen(open === "profile" ? null : "profile")}
        title={t("header.account")}
        style={{ maxWidth: 300 }}
      >
        {profile ? <Avatar profile={profile} /> : <UserRound size={18} style={{ color: "var(--warn)" }} />}
        <div className="ellipsis">
          <div className="k">{t("header.account")}</div>
          <div className="v ellipsis">
            {profile ? `${profile.label} · ${profile.username || profile.commitEmail}` : t("guard.fix.choose")}
          </div>
        </div>
        <ChevronDown size={14} className="muted" />
      </button>

      <span className="spacer" />

      <SyncTile />

      <button ref={moreRef} className="icon-btn" title={t("header.more")} onClick={() => setOpen(open === "more" ? null : "more")}>
        <MoreHorizontal size={18} />
      </button>

      {open === "branch" && <BranchPopover anchor={branchRef.current} onClose={() => setOpen(null)} />}
      {open === "profile" && <ProfilePopover anchor={profileRef.current} onClose={() => setOpen(null)} />}
      {open === "more" && <MoreMenu anchor={moreRef.current} onClose={() => setOpen(null)} />}
    </div>
  );
}

// ---- Sync ----------------------------------------------------------------------------------------

export async function doPush() {
  const path = useApp.getState().repo;
  if (!path) return;
  if (!(await passGuard(path, "push"))) return;
  const res = await runOp("push", (op) => api.repoPush(path, op));
  if (res) toast({ kind: "success", title: tt("sync.done.push", { branch: res.branch }) });
  await refreshStatus();
  bumpHistory();
}

export async function doPull() {
  const path = useApp.getState().repo;
  if (!path) return;
  const ok = await runOp("pull", (op) => api.repoPull(path, op).then(() => true));
  if (ok) toast({ kind: "success", title: tt("sync.done.pull") });
  await refreshStatus();
  bumpHistory();
}

export async function doFetch(quiet = false) {
  const path = useApp.getState().repo;
  if (!path) return;
  if (quiet) {
    try {
      await api.repoFetch(path);
    } catch {
      /* background fetch failures only show in the activity log */
    }
  } else {
    const ok = await runOp("fetch", (op) => api.repoFetch(path, op).then(() => true));
    if (ok) toast({ kind: "success", title: tt("sync.done.fetch") }, 2000);
  }
  await refreshStatus();
}

function SyncTile() {
  const t = useT();
  const status = useApp((s) => s.status);
  const overview = useApp((s) => s.overview);
  const busy = useApp((s) => Object.values(s.busy).find((b) => SYNC_LABELS.includes(b.label)));
  useApp((s) => s.data);
  const entry = currentRepoEntry();
  const profile = profileById(entry?.profileId);

  const remote = overview?.remotes.find((r) => r.name === "origin") ?? overview?.remotes[0];
  const b = status?.branch;
  let kind: "fetch" | "pull" | "push" | "publish" | "none" = "fetch";
  if (!remote) kind = "none";
  else if (b && b.head && b.oid && !b.upstream) kind = "publish";
  else if (b && b.behind > 0) kind = "pull";
  else if (b && b.ahead > 0) kind = "push";

  const label =
    kind === "none"
      ? t("sync.noRemote")
      : kind === "publish"
        ? t("sync.publish")
        : kind === "pull"
          ? `${t("sync.pull")} ${b?.behind}`
          : kind === "push"
            ? `${t("sync.push")} ${b?.ahead}`
            : t("sync.fetch");
  const hint =
    kind === "none"
      ? t("sync.noRemote.hint")
      : kind === "publish"
        ? t("sync.publish.hint", { remote: remote?.name })
        : kind === "pull"
          ? t("sync.pull.hint", { n: b?.behind })
          : kind === "push"
            ? t("sync.push.hint", { n: b?.ahead, remote: remote?.name })
            : t("sync.fetch.hint");
  const Icon = kind === "pull" ? ArrowDown : kind === "push" ? ArrowUp : kind === "publish" ? CloudUpload : RefreshCw;
  const run = kind === "pull" ? doPull : kind === "push" || kind === "publish" ? doPush : () => doFetch();
  const primary = kind === "pull" || kind === "push" || kind === "publish";

  return (
    <button
      className={`header-tile sync-tile ${primary ? "primary" : ""}`}
      disabled={kind === "none" || !!busy || !profile}
      onClick={run}
      title={`${hint}${profile ? "\n" + t("sync.as", { profile: profile.label }) : ""}`}
    >
      {busy ? <Loader2 size={17} className="spin" /> : <Icon size={17} />}
      <div className="ellipsis" style={{ minWidth: 0 }}>
        <div className="v">{busy ? tt(`purpose.${busy.label}` as never) : label}</div>
        <div className="k ellipsis">
          {busy?.phase ? `${busy.phase} ${busy.percent ?? ""}%` : profile ? t("sync.as", { profile: profile.label }) : hint}
        </div>
      </div>
    </button>
  );
}

// ---- Branches ------------------------------------------------------------------------------------

function BranchPopover({ anchor, onClose }: { anchor: HTMLElement | null; onClose: () => void }) {
  const t = useT();
  const path = useApp((s) => s.repo)!;
  const [branches, setBranches] = useState<Branch[] | null>(null);
  const [q, setQ] = useState("");

  useEffect(() => {
    api.repoBranches(path).then(setBranches, toastError);
  }, [path]);

  const local = useMemo(() => (branches ?? []).filter((b) => !b.isRemote), [branches]);
  const remote = useMemo(() => {
    const localNames = new Set(local.map((b) => b.name));
    return (branches ?? []).filter((b) => b.isRemote && !localNames.has(b.name.slice((b.remote?.length ?? 0) + 1)));
  }, [branches, local]);

  const query = q.trim();
  const match = (b: Branch) => !query || b.name.toLowerCase().includes(query.toLowerCase());
  const exists = (branches ?? []).some((b) => b.name === query || b.name.endsWith(`/${query}`));

  async function switchTo(b: Branch) {
    onClose();
    if (b.current) return;
    const ok = await runOp("switch", () => api.repoSwitch(path, b.name, b.isRemote).then(() => true));
    if (ok) toast({ kind: "success", title: t("branches.switched", { name: b.isRemote ? b.name.split("/").slice(1).join("/") : b.name }) }, 2500);
    await refreshStatus();
    bumpHistory();
  }

  async function create() {
    onClose();
    const ok = await runOp("create_branch", () => api.repoCreateBranch(path, query).then(() => true));
    if (ok) toast({ kind: "success", title: t("branches.created", { name: query }) }, 2500);
    await refreshStatus();
    bumpHistory();
  }

  return (
    <Popover anchor={anchor} onClose={onClose} width={360}>
      <div className="popover-head">
        <input
          className="input"
          autoFocus
          placeholder={t("branches.filter")}
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && query && !exists) create();
          }}
        />
      </div>
      <div className="popover-body">
        {query && !exists && (
          <button className="menu-item" onClick={create}>
            <GitBranchPlus size={16} />
            <div>
              <div>{t("branches.create", { name: query })}</div>
              <div className="sub">{t("branches.create.hint")}</div>
            </div>
          </button>
        )}
        {!branches && <div className="muted" style={{ padding: 10 }}>{t("common.loading")}</div>}
        {local.filter(match).length > 0 && <div className="menu-label">{t("branches.local")}</div>}
        {local.filter(match).map((b) => (
          <button key={b.fullRef} className={`menu-item ${b.current ? "selected" : ""}`} onClick={() => switchTo(b)}>
            {b.current ? <Check size={15} /> : <GitBranch size={15} className="muted" />}
            <div className="ellipsis" style={{ flex: 1 }}>
              <div className="ellipsis">{b.name}</div>
              {b.upstream && <div className="sub">{b.upstream}</div>}
            </div>
            <span className="sub">{relativeTime(b.time)}</span>
          </button>
        ))}
        {remote.filter(match).length > 0 && <div className="menu-label">{t("branches.remote")}</div>}
        {remote.filter(match).map((b) => (
          <button key={b.fullRef} className="menu-item" onClick={() => switchTo(b)}>
            <GitBranch size={15} className="muted" />
            <div className="ellipsis" style={{ flex: 1 }}>
              {b.name}
            </div>
            <span className="sub">{relativeTime(b.time)}</span>
          </button>
        ))}
      </div>
    </Popover>
  );
}

// ---- Profile -------------------------------------------------------------------------------------

export function ProfilePopover({ anchor, onClose }: { anchor: HTMLElement | null; onClose: () => void }) {
  const t = useT();
  const path = useApp((s) => s.repo)!;
  const data = useApp((s) => s.data);
  const entry = data?.repos.find((r) => r.path === path);
  const [suggested, setSuggested] = useState<string | null>(null);

  useEffect(() => {
    api.repoGuard(path, "commit").then((r) => setSuggested(r.suggestedProfileId ?? null), () => {});
  }, [path]);

  async function pick(id: string) {
    onClose();
    if (id === entry?.profileId) return;
    await bindRepo(path, id);
  }

  return (
    <Popover anchor={anchor} onClose={onClose} width={380}>
      <div className="popover-head">
        <div className="title">{t("picker.title")}</div>
        <div className="hint">{t("picker.hint")}</div>
      </div>
      <div className="popover-body">
        {data?.profiles.map((p) => (
          <button key={p.id} className={`menu-item ${p.id === entry?.profileId ? "selected" : ""}`} onClick={() => pick(p.id)}>
            <Avatar profile={p} />
            <div className="ellipsis" style={{ flex: 1 }}>
              <div className="row">
                <b>{p.label}</b>
                {p.id === suggested && <span className="badge accent">{t("picker.suggested")}</span>}
              </div>
              <div className="sub ellipsis">
                {p.commitEmail} · {p.host}
              </div>
            </div>
            {p.id === entry?.profileId && <Check size={16} />}
          </button>
        ))}
      </div>
      <div className="popover-foot">
        <button
          className="menu-item"
          onClick={() => {
            onClose();
            openSettings("profiles");
          }}
        >
          <UserRound size={15} /> {t("picker.manage")}
        </button>
      </div>
    </Popover>
  );
}

function MoreMenu({ anchor, onClose }: { anchor: HTMLElement | null; onClose: () => void }) {
  const t = useT();
  const path = useApp((s) => s.repo)!;
  const name = useApp((s) => s.overview?.name ?? "");
  return (
    <Popover anchor={anchor} onClose={onClose} width={240} align="right">
      <div className="popover-body">
        <button
          className="menu-item"
          onClick={() => {
            onClose();
            refreshOverview();
            refreshStatus();
            bumpHistory();
          }}
        >
          <RefreshCw size={15} /> {t("common.refresh")}
        </button>
        <button
          className="menu-item"
          onClick={() => {
            onClose();
            revealFolder(path).catch(toastError);
          }}
        >
          <FolderOpen size={15} /> {t("header.openFolder")}
        </button>
        <button
          className="menu-item"
          onClick={() => {
            onClose();
            api.openTerminal(path).catch(toastError);
          }}
        >
          <SquareTerminal size={15} /> {t("header.openTerminal")}
        </button>
        <div className="menu-sep" />
        <button
          className="menu-item"
          style={{ color: "var(--danger)" }}
          onClick={async () => {
            onClose();
            if (!(await confirmDialog(t("header.removeConfirm", { name }), { danger: true }))) return;
            try {
              setData(await api.repoRemove(path));
              selectRepo(null);
            } catch (e) {
              toastError(e);
            }
          }}
        >
          <Trash2 size={15} /> {t("header.removeRepo")}
        </button>
      </div>
    </Popover>
  );
}
