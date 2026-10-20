import { useEffect, useState } from "react";
import { ArrowUp, History, Loader2, TriangleAlert } from "lucide-react";
import { api } from "../api";
import { currentRepoEntry, fixAuthors, profileById, toastError, useApp } from "../store";
import { useT } from "../i18n";
import type { Commit, CommitFile, Diff } from "../types";
import { relativeTime, splitPath } from "../util";
import { DiffView } from "./DiffView";
import { Avatar, KindBadge } from "./ui";

const PAGE = 100;

export function HistoryView() {
  const t = useT();
  const path = useApp((s) => s.repo)!;
  const version = useApp((s) => s.historyVersion);
  useApp((s) => s.data);
  const [commits, setCommits] = useState<Commit[] | null>(null);
  const [more, setMore] = useState(true);
  const [active, setActive] = useState<Commit | null>(null);

  useEffect(() => {
    let cancelled = false;
    api.repoLog(path, PAGE, 0).then(
      (c) => {
        if (cancelled) return;
        setCommits(c);
        setMore(c.length === PAGE);
        setActive((cur) => c.find((x) => x.oid === cur?.oid) ?? c[0] ?? null);
      },
      (e) => !cancelled && toastError(e),
    );
    return () => {
      cancelled = true;
    };
  }, [path, version]);

  async function loadMore() {
    const next = await api.repoLog(path, PAGE, commits?.length ?? 0).catch(toastError);
    if (!next) return;
    setCommits([...(commits ?? []), ...next]);
    setMore(next.length === PAGE);
  }

  if (!commits) {
    return (
      <div className="center-empty">
        <Loader2 className="spin" />
      </div>
    );
  }
  if (commits.length === 0) {
    return (
      <div className="center-empty">
        <div>
          <History size={34} />
          <div className="big">{t("history.empty")}</div>
        </div>
      </div>
    );
  }

  const bound = profileById(currentRepoEntry()?.profileId);
  const foreignUnpushed = bound ? commits.filter((c) => c.unpushed && c.profileId !== bound.id).length : 0;

  return (
    <div className="split">
      <div className="pane-left">
        {foreignUnpushed > 0 && bound && (
          <div className="banner warn" style={{ flexDirection: "column", alignItems: "stretch", gap: 8 }}>
            <div className="row">
              <TriangleAlert size={16} style={{ color: "var(--warn)", flex: "none" }} />
              <span className="title" style={{ fontSize: 12 }}>
                {t("history.foreignBanner", { n: foreignUnpushed })}
              </span>
            </div>
            <button className="btn sm primary" onClick={() => fixAuthors(path)} title={t("guard.fix.rewriteAuthors.hint")}>
              {t("guard.fix.rewriteAuthors", { profile: bound.label })}
            </button>
          </div>
        )}
        <div className="file-list">
          {commits.map((c) => {
            const p = profileById(c.profileId);
            return (
              <div key={c.oid} className={`commit-row ${active?.oid === c.oid ? "active" : ""}`} onClick={() => setActive(c)}>
                <Avatar profile={p ?? { label: c.authorName, color: "#7c8595" } as never} size="sm" />
                <div className="ellipsis" style={{ flex: 1 }}>
                  <div className="subject ellipsis">{c.subject}</div>
                  <div className="meta row">
                    <span className={`ellipsis ${p ? "" : "foreign"}`} title={p ? c.authorEmail : t("history.foreign")}>
                      {!p && <TriangleAlert size={11} style={{ verticalAlign: -1, marginRight: 3 }} />}
                      {c.authorName}
                    </span>
                    <span>· {relativeTime(c.time)}</span>
                    <span className="mono">· {c.short}</span>
                  </div>
                </div>
                {c.unpushed && (
                  <span className="badge warn" title={t("history.unpushed")}>
                    <ArrowUp size={11} />
                  </span>
                )}
              </div>
            );
          })}
          {more && (
            <div style={{ padding: 10 }}>
              <button className="btn block" onClick={loadMore}>
                {t("history.loadMore")}
              </button>
            </div>
          )}
        </div>
      </div>
      <div className="pane-right">{active ? <CommitDetail commit={active} /> : <div className="center-empty">{t("history.selectCommit")}</div>}</div>
    </div>
  );
}

function CommitDetail({ commit }: { commit: Commit }) {
  const t = useT();
  const path = useApp((s) => s.repo)!;
  const [files, setFiles] = useState<CommitFile[] | null>(null);
  const [active, setActive] = useState<CommitFile | null>(null);
  const [diff, setDiff] = useState<Diff | null>(null);
  const [loading, setLoading] = useState(false);
  const parent = commit.parents[0] ?? null;
  const p = profileById(commit.profileId);

  useEffect(() => {
    let cancelled = false;
    setFiles(null);
    setActive(null);
    setDiff(null);
    api.repoCommitFiles(path, commit.oid, parent).then(
      (f) => {
        if (cancelled) return;
        setFiles(f);
        setActive(f[0] ?? null);
      },
      (e) => !cancelled && toastError(e),
    );
    return () => {
      cancelled = true;
    };
  }, [path, commit.oid, parent]);

  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    setLoading(true);
    api
      .repoCommitDiff(path, commit.oid, parent, active.path, active.origPath)
      .then((d) => !cancelled && setDiff(d), (e) => !cancelled && toastError(e))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [path, commit.oid, parent, active]);

  return (
    <>
      <div className="commit-detail-head">
        <div className="subject selectable">{commit.subject}</div>
        <div className="row dim" style={{ fontSize: 12 }}>
          <Avatar profile={p ?? ({ label: commit.authorName, color: "#7c8595" } as never)} size="sm" />
          <span className="selectable">
            {commit.authorName} &lt;{commit.authorEmail}&gt;
          </span>
          {p ? <span className="badge" style={{ borderColor: p.color }}>{p.label}</span> : <span className="badge warn">{t("history.foreign")}</span>}
          <span>· {new Date(commit.time * 1000).toLocaleString()}</span>
          <span className="mono selectable">· {commit.short}</span>
          {commit.unpushed && <span className="badge warn">{t("history.unpushed")}</span>}
        </div>
      </div>
      <div className="commit-split">
        <div className="pane-left">
          <div className="list-head">{files ? t("history.files", { n: files.length }) : t("common.loading")}</div>
          <div className="file-list">
            {files?.map((f) => {
              const { dir, name } = splitPath(f.path);
              return (
                <div key={f.path} className={`file-row ${active?.path === f.path ? "active" : ""}`} onClick={() => setActive(f)} title={f.path}>
                  <div className="ellipsis" style={{ flex: 1 }}>
                    <span className="fname">{name}</span>
                    {dir && <span className="fdir"> {dir}</span>}
                  </div>
                  <KindBadge kind={f.kind} />
                </div>
              );
            })}
          </div>
        </div>
        <div className="pane-right">
          <DiffView diff={diff} loading={loading && !diff} />
        </div>
      </div>
    </>
  );
}
