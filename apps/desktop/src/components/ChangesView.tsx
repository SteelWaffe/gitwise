import { useEffect, useMemo, useRef, useState } from "react";
import { CheckCircle2, GitCommitHorizontal, Loader2, RotateCcw, Undo2, UserRound } from "lucide-react";
import { api, confirmDialog } from "../api";
import {
  bumpHistory,
  currentRepoEntry,
  passGuard,
  profileById,
  refreshStatus,
  runOp,
  toast,
  toastError,
  useApp,
} from "../store";
import { t as tt, useT } from "../i18n";
import type { Diff, FileChange } from "../types";
import { matchesLfs, splitPath } from "../util";
import { DiffView } from "./DiffView";
import { Avatar, KindBadge, profileStyle } from "./ui";
import { ProfilePopover } from "./RepoHeader";

export function ChangesView() {
  const t = useT();
  const path = useApp((s) => s.repo)!;
  const status = useApp((s) => s.status);
  const overview = useApp((s) => s.overview);
  const excludedList = useApp((s) => s.excluded[path]);
  const excluded = useMemo(() => new Set(excludedList ?? []), [excludedList]);
  const files = status?.files ?? [];
  const [active, setActive] = useState<string | null>(null);
  const [diff, setDiff] = useState<Diff | null>(null);
  const [loadingDiff, setLoadingDiff] = useState(false);

  const activeFile = files.find((f) => f.path === active) ?? null;
  const lfsPatterns = overview?.lfs.patterns ?? [];

  // Keep a file selected while there are changes.
  useEffect(() => {
    if (files.length === 0) setActive(null);
    else if (!active || !files.some((f) => f.path === active)) setActive(files[0].path);
  }, [files, active]);

  const activeKey = activeFile ? `${activeFile.path}|${activeFile.indexStatus}${activeFile.worktreeStatus}` : null;
  useEffect(() => {
    if (!activeFile) {
      setDiff(null);
      return;
    }
    let cancelled = false;
    setLoadingDiff(true);
    api
      .repoDiff(path, activeFile)
      .then((d) => !cancelled && setDiff(d))
      .catch((e) => !cancelled && (setDiff(null), toastError(e)))
      .finally(() => !cancelled && setLoadingDiff(false));
    return () => {
      cancelled = true;
    };
    // The diff only needs reloading when the file or its state changes (or after a refresh).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path, activeKey, status]);

  const setExcluded = (next: Set<string>) => useApp.setState((s) => ({ excluded: { ...s.excluded, [path]: [...next] } }));
  const toggle = (f: FileChange) => {
    const next = new Set(excluded);
    if (next.has(f.path)) next.delete(f.path);
    else next.add(f.path);
    setExcluded(next);
  };
  const selected = files.filter((f) => !excluded.has(f.path));
  const allChecked = files.length > 0 && selected.length === files.length;
  const someChecked = selected.length > 0 && !allChecked;
  const headCheck = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (headCheck.current) headCheck.current.indeterminate = someChecked;
  }, [someChecked]);

  async function discard(list: FileChange[]) {
    if (list.length === 0) return;
    if (!(await confirmDialog(t("changes.discardConfirm", { n: list.length }), { danger: true }))) return;
    await runOp("discard", () => api.repoDiscard(path, list));
    await refreshStatus();
  }

  if (!status) {
    return (
      <div className="center-empty">
        <Loader2 className="spin" />
      </div>
    );
  }

  return (
    <div className="split">
      <div className="pane-left">
        <div className="list-head">
          <input
            ref={headCheck}
            type="checkbox"
            className="check"
            checked={allChecked}
            disabled={files.length === 0}
            onChange={() => setExcluded(allChecked ? new Set(files.map((f) => f.path)) : new Set())}
            title={t("changes.selectAll")}
          />
          <span style={{ flex: 1 }}>{files.length ? t("changes.count", { n: files.length }) : t("changes.none")}</span>
          {selected.length > 0 && (
            <button className="icon-btn" title={t("changes.discardSelected", { n: selected.length })} onClick={() => discard(selected)}>
              <RotateCcw size={15} />
            </button>
          )}
        </div>

        <div className="file-list">
          {files.length === 0 && (
            <div className="center-empty" style={{ height: "100%" }}>
              <div>
                <CheckCircle2 size={34} style={{ color: "var(--ok)" }} />
                <div className="big">{t("changes.none")}</div>
                <div>{t("changes.none.hint")}</div>
              </div>
            </div>
          )}
          {files.map((f) => {
            const { dir, name } = splitPath(f.path);
            const lfs = lfsPatterns.length > 0 && matchesLfs(f.path, lfsPatterns);
            return (
              <div
                key={f.path}
                className={`file-row ${active === f.path ? "active" : ""}`}
                onClick={() => setActive(f.path)}
                title={f.origPath ? `${f.origPath} → ${f.path}` : f.path}
              >
                <input
                  type="checkbox"
                  className="check"
                  checked={!excluded.has(f.path)}
                  onClick={(e) => e.stopPropagation()}
                  onChange={() => toggle(f)}
                />
                <div className="ellipsis" style={{ flex: 1 }}>
                  <span className="fname">{name}</span>
                  {dir && <span className="fdir"> {dir}</span>}
                </div>
                {lfs && <span className="badge accent">{t("changes.lfsTracked")}</span>}
                <KindBadge kind={f.kind} />
              </div>
            );
          })}
        </div>

        <CommitBox selected={selected} />
      </div>

      <div className="pane-right">
        {activeFile ? (
          <DiffView
            diff={diff}
            loading={loadingDiff && !diff}
            head={
              <>
                <KindBadge kind={activeFile.kind} />
                <span className="mono ellipsis selectable" style={{ flex: 1 }}>
                  {activeFile.origPath ? `${activeFile.origPath} → ` : ""}
                  {activeFile.path}
                </span>
                <span className="muted">{t(`kind.${activeFile.kind}`)}</span>
                <button className="btn sm ghost" onClick={() => discard([activeFile])} title={t("changes.discard")}>
                  <RotateCcw size={13} /> {t("changes.discard")}
                </button>
              </>
            }
          />
        ) : (
          <div className="center-empty">{files.length ? t("changes.selectFile") : null}</div>
        )}
      </div>
    </div>
  );
}

function CommitBox({ selected }: { selected: FileChange[] }) {
  const t = useT();
  const path = useApp((s) => s.repo)!;
  const status = useApp((s) => s.status);
  const draft = useApp((s) => s.drafts[path]) ?? { summary: "", description: "" };
  const busy = useApp((s) => Object.values(s.busy).some((b) => b.label === "commit"));
  useApp((s) => s.data);
  const profile = profileById(currentRepoEntry()?.profileId);
  const [pickerOpen, setPickerOpen] = useState(false);
  const idRef = useRef<HTMLButtonElement>(null);

  const setDraft = (d: Partial<typeof draft>) =>
    useApp.setState((s) => ({ drafts: { ...s.drafts, [path]: { ...draft, ...d } } }));

  const branch = status?.branch.head;
  const canCommit = !!profile && selected.length > 0 && draft.summary.trim().length > 0 && !busy;

  async function commit() {
    if (!canCommit) return;
    if (!(await passGuard(path, "commit"))) return;
    const message = draft.description.trim() ? `${draft.summary.trim()}\n\n${draft.description.trim()}` : draft.summary.trim();
    const oid = await runOp("commit", () => api.repoCommit(path, message, selected));
    if (!oid) return;
    setDraft({ summary: "", description: "" });
    toast(
      {
        kind: "success",
        title: `${tt("commit.done")} · ${oid.slice(0, 7)}`,
        body: tt("commit.as", { name: profile!.commitName, email: profile!.commitEmail }),
        action: { label: tt("commit.undo"), run: () => undoLast() },
      },
      8000,
    );
    await refreshStatus();
    bumpHistory();
  }

  async function undoLast() {
    const ok = await runOp("undo_commit", () => api.repoUndoCommit(path).then(() => true));
    if (ok) toast({ kind: "info", title: tt("commit.undone") });
    await refreshStatus();
    bumpHistory();
  }

  return (
    <div className="commit-box" style={profileStyle(profile)}>
      <button ref={idRef} className={`commit-identity ${profile ? "" : "unbound"}`} onClick={() => setPickerOpen(!pickerOpen)}>
        {profile ? <Avatar profile={profile} size="sm" /> : <UserRound size={16} style={{ color: "var(--warn)" }} />}
        <div className="ellipsis" style={{ flex: 1 }}>
          <div className="who ellipsis">{profile ? `${profile.commitName} · ${profile.label}` : t("commit.needProfile")}</div>
          {profile && <div className="mail ellipsis">{profile.commitEmail}</div>}
        </div>
      </button>
      {pickerOpen && <ProfilePopover anchor={idRef.current} onClose={() => setPickerOpen(false)} />}

      <input
        className="input"
        placeholder={t("commit.summary")}
        value={draft.summary}
        maxLength={200}
        onChange={(e) => setDraft({ summary: e.target.value })}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) commit();
        }}
      />
      <textarea
        className="textarea"
        placeholder={t("commit.description")}
        value={draft.description}
        rows={3}
        onChange={(e) => setDraft({ description: e.target.value })}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) commit();
        }}
      />
      <button className="btn primary block lg" disabled={!canCommit} onClick={commit} title="Ctrl+Enter">
        {busy ? <Loader2 size={16} className="spin" /> : <GitCommitHorizontal size={16} />}
        <span className="ellipsis">{branch ? t("commit.button", { branch }) : t("commit.buttonNoBranch")}</span>
      </button>
      <div className="hint">
        {!profile
          ? t("commit.needProfile")
          : selected.length === 0
            ? t("commit.nothingSelected")
            : t("commit.explain", { n: selected.length })}
      </div>
      {status && status.branch.ahead > 0 && (
        <button className="btn sm ghost" style={{ alignSelf: "flex-start" }} onClick={undoLast} title={t("commit.undo.hint")}>
          <Undo2 size={13} /> {t("commit.undo")}
        </button>
      )}
    </div>
  );
}
