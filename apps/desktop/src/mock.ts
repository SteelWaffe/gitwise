// A stand-in backend for running the UI in a plain browser (`npm run dev` without Tauri), so the
// interface can be designed and reviewed without a git repository at hand. Never bundled into
// the app's behaviour: api.ts only loads it when the Tauri bridge is missing.

import type {
  AppData,
  Bootstrap,
  Branch,
  Commit,
  CommitFile,
  Diff,
  FileChange,
  GuardReport,
  LogEvent,
  RepoOverview,
  RepoStatus,
} from "./types";

const listeners: ((e: LogEvent) => void)[] = [];
let logId = 1;

export function onMockLog(cb: (e: LogEvent) => void) {
  listeners.push(cb);
  return () => {
    const i = listeners.indexOf(cb);
    if (i >= 0) listeners.splice(i, 1);
  };
}

function log(command: string, purpose: string, profile?: string, readOnly = false, output?: string) {
  const e: LogEvent = {
    id: logId++,
    repo: state.repos[0]?.path,
    command,
    purpose,
    profile,
    readOnly,
    status: "ok",
    exitCode: 0,
    output,
    durationMs: 40 + Math.round(Math.random() * 300),
    startedAt: Date.now(),
  };
  listeners.forEach((l) => l(e));
}

const now = Math.floor(Date.now() / 1000);

const state: AppData = {
  version: 1,
  profiles: [
    {
      id: "work",
      label: "İş",
      color: "#3b74f2",
      provider: "github",
      host: "github.com",
      username: "cihan-hotbunny",
      commitName: "Cihan Alp",
      commitEmail: "cihan@hotbunny.dev",
      extraEmails: [],
      sshKeyPath: null,
      hasToken: true,
      hasSshPassphrase: false,
    },
    {
      id: "personal",
      label: "Kişisel",
      color: "#ff8a3d",
      provider: "github",
      host: "github.com",
      username: "steelwaffe",
      commitName: "Cihan",
      commitEmail: "cihan@example.com",
      extraEmails: ["1234+steelwaffe@users.noreply.github.com"],
      sshKeyPath: "C:\\Users\\Cihan\\.ssh\\id_ed25519_personal",
      hasToken: true,
      hasSshPassphrase: false,
    },
  ],
  rules: [
    { id: "r1", kind: "remote", pattern: "github.com/hotbunny", profileId: "work", enabled: true },
    { id: "r2", kind: "path", pattern: "D:\\Personal", profileId: "personal", enabled: true },
  ],
  repos: [
    { path: "D:\\Repositories\\Hackhub", name: "Hackhub", profileId: "work", addedAt: 1, lastOpenedAt: Date.now() },
    { path: "D:\\Repositories\\hotbunny-landing", name: "hotbunny-landing", profileId: "work", addedAt: 1 },
    { path: "D:\\Personal\\portfolio", name: "portfolio", profileId: "personal", addedAt: 1 },
    { path: "D:\\Repositories\\SurvivalGame", name: "SurvivalGame", profileId: null, addedAt: 1 },
  ],
  settings: {
    language: "tr",
    defaultProfileId: "work",
    autoCheckUpdates: true,
    guardMode: "confirm",
    cloneRoot: "D:\\Repositories",
    oauthClientIds: {},
    showReadCommands: false,
    theme: "system",
  },
};

const files: FileChange[] = [
  { path: "src/components/Lobby.tsx", kind: "modified", indexStatus: ".", worktreeStatus: "M", staged: false, unstaged: true, submodule: false },
  { path: "src/api/session.ts", kind: "modified", indexStatus: "M", worktreeStatus: ".", staged: true, unstaged: false, submodule: false },
  { path: "Content/Maps/Arena.umap", kind: "modified", indexStatus: ".", worktreeStatus: "M", staged: false, unstaged: true, submodule: false },
  { path: "docs/matchmaking.md", kind: "untracked", indexStatus: "?", worktreeStatus: "?", staged: false, unstaged: true, submodule: false },
  { path: "src/legacy/oldLobby.ts", kind: "deleted", indexStatus: ".", worktreeStatus: "D", staged: false, unstaged: true, submodule: false },
];

const sampleDiff = `diff --git a/src/components/Lobby.tsx b/src/components/Lobby.tsx
index 3b1c2a4..9f0e7d1 100644
--- a/src/components/Lobby.tsx
+++ b/src/components/Lobby.tsx
@@ -12,9 +12,14 @@ export function Lobby({ players }: Props) {
   const [ready, setReady] = useState(false);
   const host = players.find((p) => p.isHost);

-  function start() {
-    socket.emit("start");
+  async function start() {
+    if (!ready) return;
+    await socket.emitWithAck("start", { mode: "ranked" });
+    track("lobby_start", { players: players.length });
   }

   return (
     <div className="lobby">
+      <LobbyHeader host={host} />
       {players.map((p) => (
         <PlayerCard key={p.id} player={p} />
       ))}
`;

const commits: Commit[] = [
  { oid: "a1".padEnd(40, "0"), short: "a1b2c3d", authorName: "Cihan Alp", authorEmail: "cihan@hotbunny.dev", time: now - 600, parents: ["b2".padEnd(40, "0")], refs: ["HEAD -> main"], subject: "Lobby: wait for ack before starting a match", unpushed: true, profileId: "work" },
  { oid: "b2".padEnd(40, "0"), short: "b2c3d4e", authorName: "Cihan", authorEmail: "cihan@example.com", time: now - 5400, parents: ["c3".padEnd(40, "0")], refs: [], subject: "Fix typo in matchmaking docs", unpushed: true, profileId: "personal" },
  { oid: "c3".padEnd(40, "0"), short: "c3d4e5f", authorName: "Cihan Alp", authorEmail: "cihan@hotbunny.dev", time: now - 86400, parents: ["d4".padEnd(40, "0")], refs: ["origin/main"], subject: "Merge branch 'development'", unpushed: false, profileId: "work" },
  { oid: "d4".padEnd(40, "0"), short: "d4e5f6a", authorName: "Deniz Kaya", authorEmail: "deniz@hotbunny.dev", time: now - 86400 * 2, parents: [], refs: [], subject: "CI: one release workflow for mac and Windows", unpushed: false, profileId: null },
];

const branches: Branch[] = [
  { name: "main", fullRef: "refs/heads/main", isRemote: false, upstream: "origin/main", current: true, oid: "a1b2c3d", time: now - 600 },
  { name: "development", fullRef: "refs/heads/development", isRemote: false, upstream: "origin/development", current: false, oid: "c3d4e5f", time: now - 7200 },
  { name: "origin/feature/ranked-queue", fullRef: "refs/remotes/origin/feature/ranked-queue", isRemote: true, remote: "origin", current: false, oid: "e5f6a7b", time: now - 3 * 86400 },
];

function overview(path: string): RepoOverview {
  const name = path.split(/[\\/]/).pop() ?? path;
  const unreal = name === "SurvivalGame" || name === "Hackhub";
  return {
    path,
    name,
    remotes: [{ name: "origin", url: `https://github.com/hotbunny/${name}.git`, parsed: { protocol: "https", host: "github.com", path: `hotbunny/${name}` } }],
    lfs: { usesLfs: unreal, patterns: unreal ? ["*.uasset", "*.umap"] : [] },
    localEmail: null,
  };
}

function status(): RepoStatus {
  return { branch: { head: "main", oid: "a1b2c3d", upstream: "origin/main", ahead: 2, behind: 0 }, files, state: "clean", conflicted: 0 };
}

function guard(path: string, action: "commit" | "push"): GuardReport {
  const entry = state.repos.find((r) => r.path === path);
  if (!entry?.profileId) {
    return {
      action,
      profileId: null,
      suggestedProfileId: "work",
      issues: [{ code: "no_profile", severity: "block", params: { suggested: "İş", suggestedId: "work" } }],
      canProceed: false,
      needsConfirmation: false,
    };
  }
  if (action === "push" && commits.some((c) => c.unpushed && c.profileId !== entry.profileId)) {
    return {
      action,
      profileId: entry.profileId,
      suggestedProfileId: "work",
      issues: [
        {
          code: "foreign_commits",
          severity: "warn",
          params: {
            profile: "İş",
            count: 1,
            emails: ["cihan@example.com"],
            commits: [{ short: "b2c3d4e", subject: "Fix typo in matchmaking docs", email: "cihan@example.com" }],
          },
        },
      ],
      canProceed: true,
      needsConfirmation: true,
    };
  }
  return { action, profileId: entry.profileId, suggestedProfileId: "work", issues: [], canProceed: true, needsConfirmation: false };
}

const delay = (ms = 120) => new Promise((r) => setTimeout(r, ms));

export async function mockInvoke<T>(cmd: string, args: Record<string, any> = {}): Promise<T> {
  await delay();
  const profileLabel = () => state.profiles.find((p) => p.id === state.repos.find((r) => r.path === args.path)?.profileId)?.label;
  const r = ((): unknown => {
    switch (cmd) {
      case "bootstrap":
        return {
          data: state,
          tools: { git: { path: "C:\\Program Files\\Git\\cmd\\git.exe", version: "2.55.0.windows.5" }, lfs: { version: "3.7.1" }, lfsInitialized: true, platform: "windows", winget: true },
          version: "0.1.0",
          dataDir: "C:\\Users\\Cihan\\AppData\\Roaming\\Gitwise",
          builtinGithubSignIn: true,
        } satisfies Bootstrap;
      case "repo_overview":
        return overview(args.path);
      case "repo_status":
        log("git status --porcelain=v2 -z --branch --untracked-files=all", "status", undefined, true);
        return status();
      case "repo_diff":
        if (String(args.file.path).endsWith(".umap")) return { text: "", binary: true, lfsPointer: true, truncated: false, tooLarge: false } satisfies Diff;
        return { text: sampleDiff, binary: false, lfsPointer: false, truncated: false, tooLarge: false } satisfies Diff;
      case "repo_guard":
        return guard(args.path, args.action);
      case "repo_branches":
        return branches;
      case "repo_log":
        return args.skip ? [] : commits;
      case "repo_commit_files":
        return [
          { path: "src/components/Lobby.tsx", kind: "modified" },
          { path: "src/components/LobbyHeader.tsx", kind: "added" },
        ] satisfies CommitFile[];
      case "repo_commit_diff":
        return { text: sampleDiff, binary: false, lfsPointer: false, truncated: false, tooLarge: false } satisfies Diff;
      case "repo_commit":
        log("git add -A --pathspec-from-file=- --pathspec-file-nul", "stage", profileLabel());
        log("git commit --file=-", "commit", profileLabel(), false, "[main f00ba12] " + String(args.message).split("\n")[0]);
        return "f00ba12".padEnd(40, "0");
      case "repo_push":
        log("git push --progress origin refs/heads/main:refs/heads/main", "push", profileLabel(), false, "To github.com:hotbunny/Hackhub.git\n   c3d4e5f..a1b2c3d  main -> main");
        return { remote: "origin", branch: "main", setUpstream: false };
      case "repo_fetch":
        log("git fetch --progress --prune origin", "fetch", profileLabel());
        return null;
      case "repo_pull":
        log("git pull --progress --no-rebase --no-edit", "pull", profileLabel());
        return null;
      case "repo_bind": {
        const e = state.repos.find((x) => x.path === args.path);
        if (e) e.profileId = args.profileId;
        const p = state.profiles.find((x) => x.id === args.profileId)!;
        log(`git config --local user.email ${p.commitEmail}`, "bind", p.label);
        return { data: state, writes: [{ key: "user.name", value: p.commitName }, { key: "user.email", value: p.commitEmail }, { key: "gitwise.profile", value: p.id }] };
      }
      case "settings_save":
        state.settings = args.settings;
        return state;
      case "rules_save":
        state.rules = args.rules.map((x: any, i: number) => ({ ...x, id: x.id || `r${Date.now()}${i}` }));
        return state;
      case "profile_save": {
        const p = { ...args.input.profile, id: args.input.profile.id || `p${Date.now()}`, hasToken: args.input.profile.hasToken || !!args.input.token };
        const i = state.profiles.findIndex((x) => x.id === p.id);
        if (i >= 0) state.profiles[i] = p;
        else state.profiles.push(p);
        return state;
      }
      case "profile_delete":
        state.profiles = state.profiles.filter((p) => p.id !== args.id);
        return state;
      case "profile_verify":
        return { login: "steelwaffe", name: "Cihan", emails: ["cihan@example.com"], noreplyEmail: "1234+steelwaffe@users.noreply.github.com", scopes: "repo, workflow, read:org" };
      case "profile_for_url":
        return String(args.url).includes("hotbunny") ? "work" : null;
      case "hosted_repos":
        return [
          { fullName: "hotbunny/Hackhub", cloneUrl: "https://github.com/hotbunny/Hackhub.git", sshUrl: "git@github.com:hotbunny/Hackhub.git", private: true },
          { fullName: "hotbunny/hotbunny-landing", cloneUrl: "https://github.com/hotbunny/hotbunny-landing.git", sshUrl: "git@github.com:hotbunny/hotbunny-landing.git", private: false },
        ];
      case "tools_status":
        return { git: { path: "C:\\Program Files\\Git\\cmd\\git.exe", version: "2.55.0.windows.5" }, lfs: { version: "3.7.1" }, lfsInitialized: true, platform: "windows", winget: true };
      case "repo_fix_authors": {
        const p = state.profiles.find((x) => x.id === state.repos.find((r) => r.path === args.path)?.profileId)!;
        let n = 0;
        for (const c of commits.filter((c) => c.unpushed && c.profileId !== p.id)) {
          c.authorName = p.commitName;
          c.authorEmail = p.commitEmail;
          c.profileId = p.id;
          n++;
        }
        log("git rebase --rebase-merges --autostash --exec \"git commit --amend --no-edit --reset-author --allow-empty --no-verify\" c3d4e5f", "fix_authors", p.label);
        return n;
      }
      case "repo_remove":
        state.repos = state.repos.filter((x) => x.path !== args.path);
        return state;
      default:
        return null;
    }
  })();
  return r as T;
}
