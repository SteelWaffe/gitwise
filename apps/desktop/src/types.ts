// Mirrors of the Rust types that cross the IPC boundary (serde camelCase).

export type Provider = "github" | "githubEnterprise" | "gitlab" | "bitbucket" | "azureDevops" | "generic";

export interface Profile {
  id: string;
  label: string;
  color: string;
  provider: Provider;
  host: string;
  username: string;
  commitName: string;
  commitEmail: string;
  extraEmails: string[];
  sshKeyPath?: string | null;
  hasToken: boolean;
  hasSshPassphrase: boolean;
  avatarUrl?: string | null;
}

export type RuleKind = "remote" | "path";

export interface Rule {
  id: string;
  kind: RuleKind;
  pattern: string;
  profileId: string;
  enabled: boolean;
}

export interface RepoEntry {
  path: string;
  name: string;
  profileId?: string | null;
  addedAt: number;
  lastOpenedAt?: number | null;
}

export type Language = "tr" | "en";
export type Theme = "system" | "dark" | "light";

export interface Settings {
  language: Language;
  defaultProfileId?: string | null;
  autoCheckUpdates: boolean;
  guardMode: "confirm" | "relaxed";
  cloneRoot?: string | null;
  oauthClientIds: Record<string, string>;
  showReadCommands: boolean;
  theme: Theme;
}

export interface AppData {
  version: number;
  profiles: Profile[];
  rules: Rule[];
  repos: RepoEntry[];
  settings: Settings;
}

export interface ToolInfo {
  path?: string | null;
  version: string;
}

export interface ToolsStatus {
  git?: ToolInfo | null;
  lfs?: ToolInfo | null;
  lfsInitialized: boolean;
  platform: "windows" | "macos" | "linux";
  winget: boolean;
}

export interface Bootstrap {
  data: AppData;
  tools: ToolsStatus;
  version: string;
  dataDir: string;
  builtinGithubSignIn: boolean;
}

export type ChangeKind = "added" | "modified" | "deleted" | "renamed" | "copied" | "typeChanged" | "untracked" | "conflicted";

export interface FileChange {
  path: string;
  origPath?: string | null;
  kind: ChangeKind;
  indexStatus: string;
  worktreeStatus: string;
  staged: boolean;
  unstaged: boolean;
  submodule: boolean;
}

export interface BranchInfo {
  head?: string | null;
  oid?: string | null;
  upstream?: string | null;
  ahead: number;
  behind: number;
}

export type RepoState = "clean" | "merging" | "rebasing" | "cherryPicking" | "reverting";

export interface RepoStatus {
  branch: BranchInfo;
  files: FileChange[];
  state: RepoState;
  conflicted: number;
}

export interface RemoteUrl {
  protocol: "https" | "http" | "ssh" | "git" | "file";
  host: string;
  port?: number | null;
  user?: string | null;
  path: string;
}

export interface Remote {
  name: string;
  url: string;
  parsed?: RemoteUrl | null;
}

export interface RepoOverview {
  path: string;
  name: string;
  remotes: Remote[];
  lfs: { usesLfs: boolean; patterns: string[] };
  localName?: string | null;
  localEmail?: string | null;
  localProfile?: string | null;
}

export interface Diff {
  text: string;
  binary: boolean;
  lfsPointer: boolean;
  truncated: boolean;
  tooLarge: boolean;
}

export type Severity = "info" | "warn" | "block";

export interface Issue {
  code: string;
  severity: Severity;
  params: Record<string, any>;
}

export type GuardAction = "commit" | "push";

export interface GuardReport {
  action: GuardAction;
  profileId?: string | null;
  suggestedProfileId?: string | null;
  issues: Issue[];
  canProceed: boolean;
  needsConfirmation: boolean;
}

export interface Commit {
  oid: string;
  short: string;
  authorName: string;
  authorEmail: string;
  time: number;
  parents: string[];
  refs: string[];
  subject: string;
  unpushed: boolean;
  profileId?: string | null;
}

export interface CommitFile {
  path: string;
  origPath?: string | null;
  kind: ChangeKind;
}

export interface Branch {
  name: string;
  fullRef: string;
  isRemote: boolean;
  remote?: string | null;
  upstream?: string | null;
  current: boolean;
  oid: string;
  time: number;
}

export interface PushResult {
  remote: string;
  branch: string;
  setUpstream: boolean;
}

export interface ConfigWrite {
  key: string;
  value: string;
}

export interface RepoAdded {
  data: AppData;
  path: string;
  boundProfileId?: string | null;
  boundBy?: "explicit" | "local" | "rule" | null;
  writes: ConfigWrite[];
}

export interface BindResult {
  data: AppData;
  writes: ConfigWrite[];
}

export interface AccountInfo {
  login: string;
  name?: string | null;
  avatarUrl?: string | null;
  emails: string[];
  noreplyEmail?: string | null;
  scopes?: string | null;
}

export interface DeviceCode {
  deviceCode: string;
  userCode: string;
  verificationUri: string;
  expiresIn: number;
  interval: number;
}

export type DevicePollOut = (
  | { status: "pending" }
  | { status: "slowDown"; interval: number }
  | { status: "expired" }
  | { status: "denied" }
  | { status: "done" }
) & { tokenRef?: string | null; account?: AccountInfo | null };

export interface HostedRepo {
  fullName: string;
  cloneUrl: string;
  sshUrl: string;
  private: boolean;
  description?: string | null;
  updatedAt?: string | null;
}

export interface LogEvent {
  id: number;
  repo?: string | null;
  command: string;
  purpose?: string | null;
  profile?: string | null;
  readOnly: boolean;
  status: "running" | "ok" | "failed";
  exitCode?: number | null;
  output?: string | null;
  durationMs?: number | null;
  startedAt: number;
}

export interface OpProgress {
  opId: string;
  phase: string;
  percent: number;
}

export interface ApiError {
  code: string;
  message: string;
  git?: { command: string; code: number; stderr: string; kind?: string | null } | null;
}
