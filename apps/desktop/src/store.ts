import { create } from "zustand";
import { api, onGitLog, onProgress } from "./api";
import { explain, type Explained } from "./errors";
import { hasKey, t } from "./i18n";
import type { AppData, GuardAction, GuardReport, LogEvent, Profile, RepoOverview, RepoStatus, ToolsStatus } from "./types";

export interface Toast {
  id: number;
  kind: "info" | "success" | "error";
  title: string;
  body?: string;
  detail?: string;
  action?: { label: string; run: () => void };
}

export interface Busy {
  label: string;
  phase?: string;
  percent?: number;
}

export type SettingsTab = "profiles" | "rules" | "tools" | "general";

interface GuardPrompt {
  path: string;
  report: GuardReport;
  resolve: (proceed: boolean) => void;
}

interface AppStore {
  ready: boolean;
  bootError: string | null;
  version: string;
  dataDir: string;
  builtinGithubSignIn: boolean;
  data: AppData | null;
  tools: ToolsStatus | null;

  repo: string | null;
  overview: RepoOverview | null;
  status: RepoStatus | null;
  statusError: Explained | null;
  view: "repo" | "settings";
  repoTab: "changes" | "history";
  settingsTab: SettingsTab;
  /** Bumped after anything that changes history, so the history view reloads. */
  historyVersion: number;

  logs: LogEvent[];
  logOpen: boolean;
  busy: Record<string, Busy>;
  toasts: Toast[];
  /** Per repository: paths the user unticked. Everything else is part of the next commit. */
  excluded: Record<string, string[]>;
  drafts: Record<string, { summary: string; description: string }>;

  profileEditor: { id: string | null } | null;
  addRepo: "local" | "clone" | "create" | null;
  guard: GuardPrompt | null;
}

let toastId = 1;

export const useApp = create<AppStore>(() => ({
  ready: false,
  bootError: null,
  version: "",
  dataDir: "",
  builtinGithubSignIn: false,
  data: null,
  tools: null,
  repo: null,
  overview: null,
  status: null,
  statusError: null,
  view: "repo",
  repoTab: "changes",
  settingsTab: "profiles",
  historyVersion: 0,
  logs: [],
  logOpen: false,
  busy: {},
  toasts: [],
  excluded: {},
  drafts: {},
  profileEditor: null,
  addRepo: null,
  guard: null,
}));

const set = useApp.setState;
const get = useApp.getState;

// ---- Toasts --------------------------------------------------------------------------------------

export function toast(item: Omit<Toast, "id">, ms = 4500) {
  const id = toastId++;
  set((s) => ({ toasts: [...s.toasts.slice(-4), { ...item, id }] }));
  if (item.kind !== "error") setTimeout(() => dismissToast(id), ms);
  return id;
}

export function dismissToast(id: number) {
  set((s) => ({ toasts: s.toasts.filter((x) => x.id !== id) }));
}

export function toastError(e: unknown) {
  const ex = explain(e);
  toast({ kind: "error", title: ex.title, body: ex.body, detail: ex.detail });
}

// ---- Boot ----------------------------------------------------------------------------------------

let booted = false;

export async function boot() {
  if (booted) return;
  booted = true;
  try {
    const b = await api.bootstrap();
    set({
      ready: true,
      data: b.data,
      tools: b.tools,
      version: b.version,
      dataDir: b.dataDir,
      builtinGithubSignIn: b.builtinGithubSignIn,
    });
    applyTheme(b.data.settings.theme);
    await onGitLog(pushLog);
    await onProgress((p) => {
      set((s) => {
        const cur = s.busy[p.opId];
        if (!cur) return {};
        return { busy: { ...s.busy, [p.opId]: { ...cur, phase: p.phase, percent: p.percent } } };
      });
    });
    const last = [...b.data.repos].sort((a, c) => (c.lastOpenedAt ?? 0) - (a.lastOpenedAt ?? 0))[0];
    if (last) await selectRepo(last.path);
  } catch (e) {
    set({ ready: true, bootError: explain(e).title });
  }
}

function pushLog(e: LogEvent) {
  set((s) => {
    const i = s.logs.findIndex((x) => x.id === e.id);
    if (i >= 0) {
      const logs = s.logs.slice();
      logs[i] = e;
      return { logs };
    }
    return { logs: [...s.logs.slice(-499), e] };
  });
}

export function applyTheme(theme: string) {
  const root = document.documentElement;
  if (theme === "dark" || theme === "light") root.dataset.theme = theme;
  else delete root.dataset.theme;
}

export function setData(data: AppData) {
  set({ data });
}

// ---- Repository selection and refresh --------------------------------------------------------------

export async function selectRepo(path: string | null) {
  set({ repo: path, overview: null, status: null, statusError: null, view: "repo" });
  if (!path) return;
  try {
    const overview = await api.repoOverview(path);
    if (get().repo !== path) return;
    set({ overview });
  } catch (e) {
    if (get().repo === path) set({ statusError: explain(e) });
    return;
  }
  await refreshStatus();
}

let refreshing: Promise<void> | null = null;

export async function refreshStatus() {
  const path = get().repo;
  if (!path) return;
  if (refreshing) return refreshing;
  refreshing = (async () => {
    try {
      const status: RepoStatus = await api.repoStatus(path);
      if (get().repo === path) set({ status, statusError: null });
    } catch (e) {
      if (get().repo === path) set({ statusError: explain(e) });
    } finally {
      refreshing = null;
    }
  })();
  return refreshing;
}

export async function refreshOverview() {
  const path = get().repo;
  if (!path) return;
  try {
    const overview = await api.repoOverview(path);
    if (get().repo === path) set({ overview });
  } catch {
    /* status error shows instead */
  }
}

export function bumpHistory() {
  set((s) => ({ historyVersion: s.historyVersion + 1 }));
}

// ---- Long operations -------------------------------------------------------------------------------

let opCounter = 1;

/** Runs a backend operation with a busy indicator; errors become toasts. Returns undefined on failure. */
export async function runOp<T>(label: string, fn: (opId: string) => Promise<T>): Promise<T | undefined> {
  const opId = `op-${opCounter++}`;
  set((s) => ({ busy: { ...s.busy, [opId]: { label } } }));
  try {
    return await fn(opId);
  } catch (e) {
    toastError(e);
    return undefined;
  } finally {
    set((s) => {
      const busy = { ...s.busy };
      delete busy[opId];
      return { busy };
    });
  }
}

export function isBusy() {
  return Object.keys(get().busy).length > 0;
}

// ---- Guard -----------------------------------------------------------------------------------------

/** Asks the guard; shows the dialog when there is anything to say. Resolves true to go ahead. */
export async function passGuard(path: string, action: GuardAction): Promise<boolean> {
  let report: GuardReport;
  try {
    report = await api.repoGuard(path, action);
  } catch (e) {
    toastError(e);
    return false;
  }
  if (report.canProceed && !report.needsConfirmation) {
    // Relaxed mode: say it, do not stop.
    const warn = report.issues.find((i) => i.severity === "warn");
    if (warn) {
      const key = `guard.${warn.code}.title`;
      if (hasKey(key)) toast({ kind: "info", title: t(key, warn.params) });
    }
    return true;
  }
  return new Promise<boolean>((resolve) => set({ guard: { path, report, resolve } }));
}

export function closeGuard(proceed: boolean) {
  const g = get().guard;
  set({ guard: null });
  g?.resolve(proceed);
}

export async function reevaluateGuard() {
  const g = get().guard;
  if (!g) return;
  try {
    const report = await api.repoGuard(g.path, g.report.action);
    if (report.canProceed && !report.needsConfirmation) {
      closeGuard(true);
    } else {
      set({ guard: { ...g, report } });
    }
  } catch (e) {
    toastError(e);
  }
}

// ---- Profiles ---------------------------------------------------------------------------------------

export function profileById(id?: string | null): Profile | undefined {
  if (!id) return undefined;
  return get().data?.profiles.find((p) => p.id === id);
}

export function currentRepoEntry() {
  const { repo, data } = get();
  return data?.repos.find((r) => r.path === repo);
}

export async function bindRepo(path: string, profileId: string, quiet = false) {
  const res = await runOp("bind", () => api.repoBind(path, profileId));
  if (!res) return false;
  setData(res.data);
  if (!quiet) {
    const p = profileById(profileId);
    const repo = res.data.repos.find((r) => r.path === path);
    toast({
      kind: "success",
      title: t("picker.bound", { repo: repo?.name ?? path, profile: p?.label ?? "" }),
      body: t("picker.writes", { keys: res.writes.map((w) => w.key).join(", ") }),
    });
  }
  await refreshOverview();
  return true;
}

/** Gives the unpushed commits of the open repository the bound profile's identity. */
export async function fixAuthors(path: string): Promise<boolean> {
  const n = await runOp("fix_authors", () => api.repoFixAuthors(path));
  if (n === undefined) return false;
  const p = profileById(currentRepoEntry()?.profileId);
  toast({ kind: "success", title: t("fix.done", { n, profile: p?.label ?? "" }) });
  await refreshStatus();
  bumpHistory();
  return true;
}

export function openSettings(tab: SettingsTab) {
  set({ view: "settings", settingsTab: tab });
}
