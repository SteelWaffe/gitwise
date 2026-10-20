import { useState } from "react";
import { CheckCircle2, Info, X, XCircle } from "lucide-react";
import { dismissToast, useApp, type Toast } from "../store";
import { useT } from "../i18n";

function ToastItem({ toast }: { toast: Toast }) {
  const t = useT();
  const [details, setDetails] = useState(false);
  const Icon = toast.kind === "error" ? XCircle : toast.kind === "success" ? CheckCircle2 : Info;
  const color = toast.kind === "error" ? "var(--danger)" : toast.kind === "success" ? "var(--ok)" : "var(--accent)";
  return (
    <div className={`toast ${toast.kind}`}>
      <Icon size={18} style={{ color, flex: "none", marginTop: 1 }} />
      <div style={{ flex: 1, minWidth: 0 }}>
        <div className="title">{toast.title}</div>
        {toast.body && <div className="body">{toast.body}</div>}
        <div className="row" style={{ marginTop: toast.action || toast.detail ? 6 : 0 }}>
          {toast.action && (
            <button
              className="btn sm"
              onClick={() => {
                dismissToast(toast.id);
                toast.action!.run();
              }}
            >
              {toast.action.label}
            </button>
          )}
          {toast.detail && (
            <button className="btn sm ghost" onClick={() => setDetails(!details)}>
              {t("common.details")}
            </button>
          )}
        </div>
        {details && toast.detail && <pre>{toast.detail}</pre>}
      </div>
      <button className="icon-btn" style={{ width: 22, height: 22 }} onClick={() => dismissToast(toast.id)}>
        <X size={13} />
      </button>
    </div>
  );
}

export function Toasts() {
  const toasts = useApp((s) => s.toasts);
  return (
    <div className="toasts">
      {toasts.map((x) => (
        <ToastItem key={x.id} toast={x} />
      ))}
    </div>
  );
}
