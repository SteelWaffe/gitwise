import { useEffect, useState } from "react";
import { Download, Loader2 } from "lucide-react";
import { create } from "zustand";
import { inTauri } from "../api";
import { toastError, useApp } from "../store";
import { useT } from "../i18n";

interface PendingUpdate {
  version: string;
  notes?: string;
  install: (onProgress: (pct: number) => void) => Promise<void>;
}

const useUpdate = create<{ pending: PendingUpdate | null; dismissed: boolean }>(() => ({ pending: null, dismissed: false }));

/** Asks the release feed for a newer version. Resolves true when one is found. */
export async function checkForUpdates(manual = false): Promise<boolean | undefined> {
  if (!inTauri) return false;
  try {
    const { check } = await import("@tauri-apps/plugin-updater");
    const update = await check();
    if (!update) return false;
    useUpdate.setState({
      dismissed: false,
      pending: {
        version: update.version,
        notes: update.body,
        install: async (onProgress) => {
          let total = 0;
          let done = 0;
          await update.downloadAndInstall((ev) => {
            if (ev.event === "Started") total = ev.data.contentLength ?? 0;
            else if (ev.event === "Progress") {
              done += ev.data.chunkLength;
              if (total) onProgress(Math.round((done / total) * 100));
            }
          });
          const { relaunch } = await import("@tauri-apps/plugin-process");
          await relaunch();
        },
      },
    });
    return true;
  } catch (e) {
    // A failed background check is not worth interrupting anyone; a manual one is.
    if (manual) toastError(e);
    return undefined;
  }
}

export function UpdateBanner() {
  const t = useT();
  const { pending, dismissed } = useUpdate();
  const auto = useApp((s) => s.data?.settings.autoCheckUpdates ?? true);
  const ready = useApp((s) => s.ready);
  const [pct, setPct] = useState<number | null>(null);

  useEffect(() => {
    if (!ready || !auto) return;
    checkForUpdates();
    const h = setInterval(() => checkForUpdates(), 6 * 60 * 60 * 1000);
    return () => clearInterval(h);
  }, [ready, auto]);

  if (!pending || dismissed) return null;
  return (
    <div className="update-banner">
      <Download size={16} style={{ color: "var(--accent)" }} />
      <b style={{ flex: 1 }}>{t("update.available", { version: pending.version })}</b>
      {pct !== null ? (
        <span className="row dim">
          <Loader2 size={14} className="spin" /> {t("update.downloading", { percent: pct })}
        </span>
      ) : (
        <>
          <button className="btn sm ghost" onClick={() => useUpdate.setState({ dismissed: true })}>
            {t("update.later")}
          </button>
          <button
            className="btn sm primary"
            onClick={async () => {
              setPct(0);
              try {
                await pending.install(setPct);
              } catch (e) {
                setPct(null);
                toastError(e);
              }
            }}
          >
            {t("update.install")}
          </button>
        </>
      )}
    </div>
  );
}
