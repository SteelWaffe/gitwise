import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";
import type { ChangeKind, Profile } from "../types";
import { initials, KIND_LETTER } from "../util";
import { useT } from "../i18n";

export function Modal({
  title,
  onClose,
  children,
  footer,
  wide,
  icon,
}: {
  title: ReactNode;
  onClose?: () => void;
  children: ReactNode;
  footer?: ReactNode;
  wide?: boolean;
  icon?: ReactNode;
}) {
  useEffect(() => {
    if (!onClose) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return createPortal(
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose?.()}>
      <div className={`modal ${wide ? "wide" : ""}`} role="dialog" aria-modal="true">
        <div className="modal-head">
          {icon}
          <h2>{title}</h2>
          {onClose && (
            <button className="icon-btn" onClick={onClose} aria-label="close">
              <X size={16} />
            </button>
          )}
        </div>
        <div className="modal-body">{children}</div>
        {footer && <div className="modal-foot">{footer}</div>}
      </div>
    </div>,
    document.body,
  );
}

export function Popover({
  anchor,
  onClose,
  children,
  width = 320,
  align = "left",
}: {
  anchor: HTMLElement | null;
  onClose: () => void;
  children: ReactNode;
  width?: number;
  align?: "left" | "right";
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [style, setStyle] = useState<CSSProperties>({ visibility: "hidden" });

  useLayoutEffect(() => {
    if (!anchor) return;
    const r = anchor.getBoundingClientRect();
    let left = align === "right" ? r.right - width : r.left;
    left = Math.max(8, Math.min(left, window.innerWidth - width - 8));
    setStyle({ top: r.bottom + 6, left, width, maxHeight: window.innerHeight - r.bottom - 24 });
  }, [anchor, width, align]);

  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      const target = e.target as Node;
      if (ref.current?.contains(target) || anchor?.contains(target)) return;
      onClose();
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [anchor, onClose]);

  return createPortal(
    <div className="popover" ref={ref} style={style}>
      {children}
    </div>,
    document.body,
  );
}

export function Avatar({ profile, size }: { profile?: Profile | null; size?: "sm" }) {
  const style = { "--profile": profile?.color ?? "var(--text-3)" } as CSSProperties;
  return (
    <span className={`avatar ${size ?? ""}`} style={style}>
      {profile?.avatarUrl ? <img src={profile.avatarUrl} alt="" /> : initials(profile?.label ?? "?")}
    </span>
  );
}

export function KindBadge({ kind }: { kind: ChangeKind }) {
  const t = useT();
  return (
    <span className={`kind ${kind}`} title={t(`kind.${kind}`)}>
      {KIND_LETTER[kind]}
    </span>
  );
}

export function profileStyle(profile?: Profile | null): CSSProperties {
  return { "--profile": profile?.color ?? "var(--accent)" } as CSSProperties;
}

export function Progress({ percent }: { percent?: number }) {
  const known = typeof percent === "number";
  return (
    <div className={`progress ${known ? "" : "indeterminate"}`}>
      <div style={{ width: known ? `${percent}%` : undefined }} />
    </div>
  );
}

export function Switch({ checked, onChange, title }: { checked: boolean; onChange: (v: boolean) => void; title?: string }) {
  return <input type="checkbox" className="switch" checked={checked} title={title} onChange={(e) => onChange(e.target.checked)} />;
}
