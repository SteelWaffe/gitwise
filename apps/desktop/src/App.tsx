import { useEffect, useRef } from "react";
import { FolderX, History, ListChecks, Loader2 } from "lucide-react";
import { api } from "./api";
import { boot, currentRepoEntry, refreshStatus, selectRepo, setData, toastError, useApp } from "./store";
import { useT } from "./i18n";
import { Sidebar } from "./components/Sidebar";
import { RepoHeader, doFetch } from "./components/RepoHeader";
import { Banners } from "./components/Banners";
import { ChangesView } from "./components/ChangesView";
import { HistoryView } from "./components/HistoryView";
import { Settings } from "./components/Settings";
import { StatusBar } from "./components/StatusBar";
import { Toasts } from "./components/Toasts";
import { ProfileEditor } from "./components/ProfileEditor";
import { AddRepoDialog } from "./components/AddRepoDialog";
import { GuardDialog } from "./components/GuardDialog";
import { UpdateBanner } from "./components/UpdateBanner";
import { EmptyRepos, Welcome } from "./components/Welcome";

const BACKGROUND_FETCH_MS = 5 * 60 * 1000;

export function App() {
  const ready = useApp((s) => s.ready);
  const bootError = useApp((s) => s.bootError);
  const data = useApp((s) => s.data);
  const repo = useApp((s) => s.repo);
  const view = useApp((s) => s.view);
  const editor = useApp((s) => s.profileEditor);
  const lastFetch = useRef<Record<string, number>>({});

  useEffect(() => {
    boot();
  }, []);

  // Fetch quietly when a bound repository is opened or the window comes back after a while,
  // so ahead/behind counts are current without anyone pressing a button.
  useEffect(() => {
    const maybeFetch = () => {
      const path = useApp.getState().repo;
      if (!path || !currentRepoEntry()?.profileId) return;
      if (Date.now() - (lastFetch.current[path] ?? 0) < BACKGROUND_FETCH_MS) return;
      if (!useApp.getState().overview?.remotes.length) return;
      lastFetch.current[path] = Date.now();
      doFetch(true);
    };
    const onFocus = () => {
      refreshStatus();
      maybeFetch();
    };
    window.addEventListener("focus", onFocus);
    const h = setInterval(() => document.visibilityState === "visible" && refreshStatus(), 30_000);
    const unsub = useApp.subscribe((s, prev) => {
      if (s.overview && s.overview !== prev.overview) maybeFetch();
    });
    return () => {
      window.removeEventListener("focus", onFocus);
      clearInterval(h);
      unsub();
    };
  }, []);

  let content;
  if (!ready) {
    content = (
      <div className="center-empty">
        <Loader2 className="spin" />
      </div>
    );
  } else if (bootError || !data) {
    content = <div className="center-empty">{bootError}</div>;
  } else if (view === "settings") {
    content = <Settings />;
  } else if (data.profiles.length === 0) {
    content = <Welcome />;
  } else if (!repo) {
    content = <EmptyRepos />;
  } else {
    content = <RepoView />;
  }

  return (
    <div className="app">
      <Sidebar />
      <main className="main">
        <UpdateBanner />
        {content}
      </main>
      <StatusBar />
      <Toasts />
      {editor && <ProfileEditor key={editor.id ?? "new"} />}
      <AddRepoDialog />
      <GuardDialog />
    </div>
  );
}

function RepoView() {
  const t = useT();
  const tab = useApp((s) => s.repoTab);
  const status = useApp((s) => s.status);
  const statusError = useApp((s) => s.statusError);
  const repo = useApp((s) => s.repo)!;

  if (statusError && !status) {
    return (
      <div className="center-empty">
        <div style={{ maxWidth: 440 }}>
          <FolderX size={36} />
          <div className="big">{statusError.title}</div>
          <div className="mono" style={{ margin: "6px 0 14px" }}>
            {repo}
          </div>
          <div className="row" style={{ justifyContent: "center" }}>
            <button className="btn" onClick={() => selectRepo(repo)}>
              {t("common.retry")}
            </button>
            <button
              className="btn danger"
              onClick={async () => {
                try {
                  setData(await api.repoRemove(repo));
                  selectRepo(null);
                } catch (e) {
                  toastError(e);
                }
              }}
            >
              {t("header.removeRepo")}
            </button>
          </div>
        </div>
      </div>
    );
  }

  const changes = status?.files.length ?? 0;
  return (
    <>
      <RepoHeader />
      <Banners />
      <div className="tabs">
        <button className={`tab ${tab === "changes" ? "active" : ""}`} onClick={() => useApp.setState({ repoTab: "changes" })}>
          <ListChecks size={15} /> {t("tabs.changes")}
          {changes > 0 && <span className="count">{changes}</span>}
        </button>
        <button className={`tab ${tab === "history" ? "active" : ""}`} onClick={() => useApp.setState({ repoTab: "history" })}>
          <History size={15} /> {t("tabs.history")}
        </button>
      </div>
      {tab === "changes" ? <ChangesView /> : <HistoryView />}
    </>
  );
}
