import { useMemo, useState, type ReactNode } from "react";
import { FileWarning, HardDrive, Loader2 } from "lucide-react";
import type { Diff } from "../types";
import { useT } from "../i18n";

interface Line {
  type: "add" | "del" | "ctx" | "hunk" | "meta";
  text: string;
  old?: number;
  new?: number;
}

const META = /^(new file mode|deleted file mode|rename from|rename to|similarity index|old mode|new mode|Binary files)/;

export function parseDiff(text: string): Line[] {
  const out: Line[] = [];
  let o = 0;
  let n = 0;
  let inHunk = false;
  for (const l of text.split("\n")) {
    if (l.startsWith("diff --git")) {
      inHunk = false;
      continue;
    }
    if (l.startsWith("@@")) {
      const m = /@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@(.*)/.exec(l);
      if (m) {
        o = Number(m[1]);
        n = Number(m[2]);
      }
      out.push({ type: "hunk", text: l });
      inHunk = true;
      continue;
    }
    if (!inHunk) {
      if (META.test(l)) out.push({ type: "meta", text: l });
      continue;
    }
    if (l.startsWith("+")) out.push({ type: "add", text: l.slice(1), new: n++ });
    else if (l.startsWith("-")) out.push({ type: "del", text: l.slice(1), old: o++ });
    else if (l.startsWith("\\")) out.push({ type: "meta", text: l });
    else if (l.length > 0 || out.length) out.push({ type: "ctx", text: l.slice(1), old: o++, new: n++ });
  }
  // A trailing empty context line comes from the final newline, not from the file.
  while (out.length && out[out.length - 1].type === "ctx" && out[out.length - 1].text === "") out.pop();
  return out;
}

const PAGE = 4000;

export function DiffView({ diff, loading, head }: { diff: Diff | null; loading: boolean; head?: ReactNode }) {
  const t = useT();
  const [limit, setLimit] = useState(PAGE);
  const lines = useMemo(() => (diff && !diff.binary ? parseDiff(diff.text) : []), [diff]);

  return (
    <>
      {head && <div className="diff-head">{head}</div>}
      <div className="diff-scroll">
        {loading && (
          <div className="center-empty">
            <Loader2 className="spin" />
          </div>
        )}
        {!loading && diff && (
          <>
            {diff.lfsPointer && (
              <div className="notice">
                <HardDrive size={16} />
                {t("diff.lfs")}
              </div>
            )}
            {diff.tooLarge && (
              <div className="notice">
                <FileWarning size={16} />
                {t("diff.tooLarge")}
              </div>
            )}
            {diff.binary && !diff.lfsPointer && (
              <div className="notice">
                <FileWarning size={16} />
                {t("diff.binary")}
              </div>
            )}
            {diff.truncated && (
              <div className="notice">
                <FileWarning size={16} />
                {t("diff.truncated")}
              </div>
            )}
            {!diff.binary && !diff.tooLarge && lines.length === 0 && <div className="notice">{t("diff.empty")}</div>}
            {lines.length > 0 && (
              <table className="diff-table">
                <tbody>
                  {lines.slice(0, limit).map((l, i) =>
                    l.type === "hunk" || l.type === "meta" ? (
                      <tr key={i} className={l.type}>
                        <td colSpan={3}>{l.text}</td>
                      </tr>
                    ) : (
                      <tr key={i} className={l.type}>
                        <td className="ln">{l.old ?? ""}</td>
                        <td className="ln">{l.new ?? ""}</td>
                        <td className="code">
                          {l.type === "add" ? "+" : l.type === "del" ? "-" : " "}
                          {l.text}
                        </td>
                      </tr>
                    ),
                  )}
                </tbody>
              </table>
            )}
            {lines.length > limit && (
              <div style={{ padding: 12 }}>
                <button className="btn" onClick={() => setLimit(limit + PAGE)}>
                  {t("history.loadMore")} ({lines.length - limit})
                </button>
              </div>
            )}
          </>
        )}
      </div>
    </>
  );
}
