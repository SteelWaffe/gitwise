import { hasKey, t } from "./i18n";
import type { ApiError } from "./types";

export interface Explained {
  title: string;
  body?: string;
  detail?: string;
}

export function asApiError(e: unknown): ApiError {
  if (e && typeof e === "object" && "code" in e && "message" in e) return e as ApiError;
  return { code: "unknown", message: e instanceof Error ? e.message : String(e) };
}

/** Turns a backend error into something a person can act on. */
export function explain(e: unknown): Explained {
  const err = asApiError(e);
  if (err.code === "git" && err.git) {
    const kind = err.git.kind;
    const detail = `${err.git.command}\n\n${err.git.stderr.trim()}`;
    if (kind) {
      const title = `error.kind.${kind}`;
      const body = `error.kind.${kind}.body`;
      if (hasKey(title)) return { title: t(title), body: hasKey(body) ? t(body) : undefined, detail };
    }
    const firstLine = err.git.stderr.split("\n").find((l) => l.trim()) ?? err.message;
    return { title: t("error.generic"), body: firstLine.replace(/^(fatal|error):\s*/i, ""), detail };
  }
  const codeKey = `error.code.${err.code}`;
  if (hasKey(codeKey)) return { title: t(codeKey) };
  // `invalid` errors carry their reason code as the message.
  const msgKey = `error.code.${err.message}`;
  if (hasKey(msgKey)) return { title: t(msgKey) };
  const kindKey = `error.kind.${err.message}`;
  if (hasKey(kindKey)) {
    const b = `${kindKey}.body`;
    return { title: t(kindKey), body: hasKey(b) ? t(b) : undefined };
  }
  return { title: t("error.generic"), body: err.message };
}
