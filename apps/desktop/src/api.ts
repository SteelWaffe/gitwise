import type {
  AccountInfo,
  AppData,
  BindResult,
  Bootstrap,
  Branch,
  Commit,
  CommitFile,
  DeviceCode,
  DevicePollOut,
  Diff,
  FileChange,
  GuardAction,
  GuardReport,
  HostedRepo,
  LogEvent,
  OpProgress,
  Profile,
  Provider,
  PushResult,
  RepoAdded,
  RepoOverview,
  RepoStatus,
  Rule,
  Settings,
  ToolsStatus,
} from "./types";

/** Running inside the Tauri shell (as opposed to a plain browser, where a mock backend answers). */
export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type Invoke = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

let invokeImpl: Invoke | null = null;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!invokeImpl) {
    if (inTauri) {
      const core = await import("@tauri-apps/api/core");
      invokeImpl = core.invoke as Invoke;
    } else {
      const mock = await import("./mock");
      invokeImpl = mock.mockInvoke as Invoke;
    }
  }
  return invokeImpl<T>(cmd, args);
}

export type Unlisten = () => void;

export async function onGitLog(cb: (e: LogEvent) => void): Promise<Unlisten> {
  if (!inTauri) {
    const mock = await import("./mock");
    return mock.onMockLog(cb);
  }
  const { listen } = await import("@tauri-apps/api/event");
  return listen<LogEvent>("git-log", (e) => cb(e.payload));
}

export async function onProgress(cb: (e: OpProgress) => void): Promise<Unlisten> {
  if (!inTauri) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen<OpProgress>("op-progress", (e) => cb(e.payload));
}

export interface ProfileInput {
  profile: Profile;
  token?: string | null;
  tokenRef?: string | null;
  sshPassphrase?: string | null;
  clearToken?: boolean;
  clearPassphrase?: boolean;
}

export const api = {
  bootstrap: () => call<Bootstrap>("bootstrap"),
  settingsSave: (settings: Settings) => call<AppData>("settings_save", { settings }),

  profileSave: (input: ProfileInput) => call<AppData>("profile_save", { input }),
  profileDelete: (id: string) => call<AppData>("profile_delete", { id }),
  profileVerify: (input: {
    provider: Provider;
    host: string;
    username: string;
    token?: string | null;
    tokenRef?: string | null;
    profileId?: string | null;
  }) => call<AccountInfo>("profile_verify", { input }),
  profileTestSsh: (host: string, keyPath?: string | null) => call<string>("profile_test_ssh", { host, keyPath }),
  profileForUrl: (url: string) => call<string | null>("profile_for_url", { url }),
  githubDeviceStart: (host: string) => call<DeviceCode>("github_device_start", { host }),
  githubDevicePoll: (host: string, deviceCode: string) => call<DevicePollOut>("github_device_poll", { host, deviceCode }),
  hostedRepos: (profileId: string) => call<HostedRepo[]>("hosted_repos", { profileId }),

  rulesSave: (rules: Rule[]) => call<AppData>("rules_save", { rules }),

  repoAdd: (path: string) => call<RepoAdded>("repo_add", { path }),
  repoRemove: (path: string) => call<AppData>("repo_remove", { path }),
  repoBind: (path: string, profileId: string) => call<BindResult>("repo_bind", { path, profileId }),
  repoOverview: (path: string) => call<RepoOverview>("repo_overview", { path }),
  repoStatus: (path: string) => call<RepoStatus>("repo_status", { path }),
  repoDiff: (path: string, file: FileChange) => call<Diff>("repo_diff", { path, file }),
  repoGuard: (path: string, action: GuardAction) => call<GuardReport>("repo_guard", { path, action }),
  repoCommit: (path: string, message: string, files: FileChange[]) => call<string>("repo_commit", { path, message, files }),
  repoUndoCommit: (path: string) => call<void>("repo_undo_commit", { path }),
  repoDiscard: (path: string, files: FileChange[]) => call<void>("repo_discard", { path, files }),
  repoFixAuthors: (path: string) => call<number>("repo_fix_authors", { path }),
  repoAbort: (path: string) => call<void>("repo_abort", { path }),
  repoFetch: (path: string, opId?: string) => call<void>("repo_fetch", { path, opId }),
  repoPull: (path: string, opId?: string) => call<void>("repo_pull", { path, opId }),
  repoPush: (path: string, opId?: string) => call<PushResult>("repo_push", { path, opId }),
  repoBranches: (path: string) => call<Branch[]>("repo_branches", { path }),
  repoSwitch: (path: string, name: string, remote: boolean) => call<void>("repo_switch", { path, name, remote }),
  repoCreateBranch: (path: string, name: string) => call<void>("repo_create_branch", { path, name }),
  repoLog: (path: string, limit: number, skip: number) => call<Commit[]>("repo_log", { path, limit, skip }),
  repoCommitFiles: (path: string, oid: string, parent?: string | null) => call<CommitFile[]>("repo_commit_files", { path, oid, parent }),
  repoCommitDiff: (path: string, oid: string, parent: string | null | undefined, file: string, origPath?: string | null) =>
    call<Diff>("repo_commit_diff", { path, oid, parent, file, origPath }),
  repoClone: (url: string, dest: string, profileId: string, opId?: string) => call<RepoAdded>("repo_clone", { url, dest, profileId, opId }),
  repoInit: (path: string, profileId?: string | null) => call<RepoAdded>("repo_init", { path, profileId }),
  repoLfsInstall: (path: string) => call<void>("repo_lfs_install", { path }),

  toolsStatus: () => call<ToolsStatus>("tools_status"),
  toolsLfsInstallGlobal: () => call<ToolsStatus>("tools_lfs_install_global"),
  toolsWingetInstall: (pkg: "Git.Git" | "GitHub.GitLFS") => call<ToolsStatus>("tools_winget_install", { package: pkg }),
  openTerminal: (path: string) => call<void>("open_terminal", { path }),
};

// ---- Native bits with browser fallbacks -------------------------------------------------------

export async function pickFolder(title: string, defaultPath?: string): Promise<string | null> {
  if (!inTauri) return window.prompt(title, defaultPath ?? "") || null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const r = await open({ directory: true, multiple: false, title, defaultPath });
  return typeof r === "string" ? r : null;
}

export async function pickFile(title: string, defaultPath?: string): Promise<string | null> {
  if (!inTauri) return window.prompt(title, defaultPath ?? "") || null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const r = await open({ directory: false, multiple: false, title, defaultPath });
  return typeof r === "string" ? r : null;
}

/** A native yes/no question (window.confirm is not reliable in every web view). */
export async function confirmDialog(message: string, opts?: { title?: string; danger?: boolean; okLabel?: string; cancelLabel?: string }): Promise<boolean> {
  if (!inTauri) return window.confirm(message);
  const { ask } = await import("@tauri-apps/plugin-dialog");
  return ask(message, {
    title: opts?.title ?? "Gitwise",
    kind: opts?.danger ? "warning" : "info",
    okLabel: opts?.okLabel,
    cancelLabel: opts?.cancelLabel,
  });
}

export async function openUrl(url: string) {
  if (!inTauri) {
    window.open(url, "_blank");
    return;
  }
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}

export async function revealFolder(path: string) {
  if (!inTauri) return;
  const { openPath } = await import("@tauri-apps/plugin-opener");
  await openPath(path);
}
