import { useEffect, useRef, useState } from "react";
import { BadgeCheck, ExternalLink, KeyRound, Loader2, LogIn, Trash2 } from "lucide-react";
import { api, confirmDialog, openUrl, pickFile } from "../api";
import { profileById, setData, toastError, useApp } from "../store";
import { useT } from "../i18n";
import type { AccountInfo, DeviceCode, Profile, Provider } from "../types";
import { PROFILE_COLORS } from "../util";
import { Avatar, Modal } from "./ui";

const PROVIDERS: Provider[] = ["github", "githubEnterprise", "gitlab", "bitbucket", "azureDevops", "generic"];
const DEFAULT_HOST: Record<Provider, string> = {
  github: "github.com",
  githubEnterprise: "",
  gitlab: "gitlab.com",
  bitbucket: "bitbucket.org",
  azureDevops: "dev.azure.com",
  generic: "",
};

function blank(used: string[]): Profile {
  return {
    id: "",
    label: "",
    color: PROFILE_COLORS.find((c) => !used.includes(c)) ?? PROFILE_COLORS[0],
    provider: "github",
    host: "github.com",
    username: "",
    commitName: "",
    commitEmail: "",
    extraEmails: [],
    sshKeyPath: null,
    hasToken: false,
    hasSshPassphrase: false,
    avatarUrl: null,
  };
}

function tokenPage(p: Profile): string | null {
  const host = p.host || DEFAULT_HOST[p.provider];
  switch (p.provider) {
    case "github":
    case "githubEnterprise":
      return `https://${host}/settings/tokens/new?scopes=repo,workflow,read:org,read:user,user:email&description=Gitwise`;
    case "gitlab":
      return `https://${host}/-/user_settings/personal_access_tokens?name=Gitwise&scopes=api,read_user,write_repository`;
    case "bitbucket":
      return "https://bitbucket.org/account/settings/app-passwords/new";
    case "azureDevops":
      return "https://dev.azure.com/_usersSettings/tokens";
    default:
      return null;
  }
}

/** Step-by-step token creation for the selected service, with buttons to the right pages. */
function TokenGuide({ profile: p }: { profile: Profile }) {
  const t = useT();
  const isGithub = p.provider === "github" || p.provider === "githubEnterprise";
  const steps = (key: Parameters<typeof t>[0]) => (
    <ol>
      {t(key)
        .split("\n")
        .map((s, i) => (
          <li key={i}>{s}</li>
        ))}
    </ol>
  );
  const page = tokenPage(p);
  return (
    <details className="guide" open={!p.hasToken}>
      <summary>{t("guide.title")}</summary>
      {isGithub ? (
        <>
          {steps("guide.github.fine")}
          <div className="row wrap">
            <button className="btn sm primary" disabled={!p.host} onClick={() => openUrl(`https://${p.host}/settings/personal-access-tokens/new`)}>
              <ExternalLink size={12} /> {t("guide.openFine")}
            </button>
            {page && (
              <button className="btn sm" onClick={() => openUrl(page)}>
                <ExternalLink size={12} /> {t("guide.openClassic")}
              </button>
            )}
          </div>
          <p className="hint">{t("guide.github.classic")}</p>
          <p className="hint">{t("guide.github.sso")}</p>
        </>
      ) : (
        <>
          {steps(
            p.provider === "gitlab" ? "guide.gitlab" : p.provider === "bitbucket" ? "guide.bitbucket" : p.provider === "azureDevops" ? "guide.azure" : "guide.generic",
          )}
          {page && (
            <button className="btn sm primary" onClick={() => openUrl(page)}>
              <ExternalLink size={12} /> {t("guide.openPage")}
            </button>
          )}
        </>
      )}
    </details>
  );
}

export function ProfileEditor() {
  const t = useT();
  const editor = useApp((s) => s.profileEditor);
  const data = useApp((s) => s.data);
  const builtin = useApp((s) => s.builtinGithubSignIn);
  const existing = profileById(editor?.id);
  const [p, setP] = useState<Profile>(() => existing ?? blank(data?.profiles.map((x) => x.color) ?? []));
  const [token, setToken] = useState("");
  const [tokenRef, setTokenRef] = useState<string | null>(null);
  const [account, setAccount] = useState<AccountInfo | null>(null);
  const [verifying, setVerifying] = useState(false);
  const [device, setDevice] = useState<DeviceCode | null>(null);
  const [passphrase, setPassphrase] = useState("");
  const [clearToken, setClearToken] = useState(false);
  const [sshResult, setSshResult] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const polling = useRef(false);

  const close = () => {
    polling.current = false;
    useApp.setState({ profileEditor: null });
  };
  const upd = (patch: Partial<Profile>) => setP((cur) => ({ ...cur, ...patch }));
  const isGithub = p.provider === "github" || p.provider === "githubEnterprise";
  const host = p.host || DEFAULT_HOST[p.provider];
  const deviceAvailable = isGithub && (builtin && host === "github.com" ? true : !!data?.settings.oauthClientIds[host]);
  const needsUsername = p.provider === "bitbucket" || p.provider === "azureDevops" || p.provider === "generic";

  useEffect(() => () => void (polling.current = false), []);

  function applyAccount(a: AccountInfo) {
    setAccount(a);
    setP((cur) => {
      const email = cur.commitEmail || a.emails[0] || a.noreplyEmail || "";
      const extra = new Set(cur.extraEmails);
      for (const e of [...a.emails, a.noreplyEmail ?? ""]) if (e && e !== email) extra.add(e);
      return {
        ...cur,
        username: a.login || cur.username,
        commitName: cur.commitName || a.name || a.login,
        commitEmail: email,
        avatarUrl: a.avatarUrl ?? cur.avatarUrl,
        extraEmails: [...extra],
        label: cur.label || a.login,
      };
    });
  }

  async function verify() {
    setVerifying(true);
    try {
      const a = await api.profileVerify({
        provider: p.provider,
        host,
        username: p.username,
        token: token || null,
        tokenRef,
        profileId: existing?.id ?? null,
      });
      applyAccount(a);
    } catch (e) {
      toastError(e);
    } finally {
      setVerifying(false);
    }
  }

  async function startDevice() {
    try {
      const code = await api.githubDeviceStart(host);
      setDevice(code);
      openUrl(code.verificationUri).catch(() => {});
      polling.current = true;
      let interval = code.interval;
      const deadline = Date.now() + code.expiresIn * 1000;
      while (polling.current && Date.now() < deadline) {
        await new Promise((r) => setTimeout(r, interval * 1000));
        if (!polling.current) return;
        const res = await api.githubDevicePoll(host, code.deviceCode);
        if (res.status === "slowDown") interval = res.interval;
        else if (res.status === "done") {
          setTokenRef(res.tokenRef ?? null);
          if (res.account) applyAccount(res.account);
          setDevice(null);
          polling.current = false;
          return;
        } else if (res.status === "expired" || res.status === "denied") {
          setDevice(null);
          polling.current = false;
          return;
        }
      }
      setDevice(null);
    } catch (e) {
      setDevice(null);
      polling.current = false;
      toastError(e);
    }
  }

  async function testSsh() {
    setSshResult("…");
    try {
      setSshResult(await api.profileTestSsh(host, p.sshKeyPath));
    } catch (e) {
      setSshResult(null);
      toastError(e);
    }
  }

  async function save() {
    setSaving(true);
    try {
      const next = await api.profileSave({
        profile: { ...p, host },
        token: token.trim() || null,
        tokenRef,
        sshPassphrase: passphrase || null,
        clearToken,
      });
      setData(next);
      close();
    } catch (e) {
      toastError(e);
    } finally {
      setSaving(false);
    }
  }

  async function remove() {
    if (!existing) return;
    if (!(await confirmDialog(t("profiles.deleteConfirm", { label: existing.label }), { danger: true }))) return;
    try {
      setData(await api.profileDelete(existing.id));
      close();
    } catch (e) {
      toastError(e);
    }
  }

  const hasCredential = !!tokenRef || !!token.trim() || (p.hasToken && !clearToken);
  const emailChoices = Array.from(new Set([...(account?.emails ?? []), account?.noreplyEmail ?? "", p.commitEmail, ...p.extraEmails].filter(Boolean)));
  const canSave = p.label.trim() && p.commitName.trim() && p.commitEmail.trim() && host;

  return (
    <Modal
      wide
      title={existing ? t("editor.editTitle") : t("editor.newTitle")}
      icon={<Avatar profile={p.label ? p : { ...p, label: "?" }} />}
      onClose={close}
      footer={
        <>
          {existing && (
            <button className="btn danger" onClick={remove}>
              <Trash2 size={14} /> {t("common.delete")}
            </button>
          )}
          <span className="spacer" />
          <button className="btn" onClick={close}>
            {t("common.cancel")}
          </button>
          <button className="btn primary" disabled={!canSave || saving} onClick={save}>
            {saving && <Loader2 size={14} className="spin" />}
            {t("common.save")}
          </button>
        </>
      }
    >
      <div className="row" style={{ gap: 12, alignItems: "flex-end" }}>
        <div className="field" style={{ flex: 1 }}>
          <label>{t("editor.label")}</label>
          <input className="input" autoFocus value={p.label} placeholder={t("editor.label.placeholder")} onChange={(e) => upd({ label: e.target.value })} />
        </div>
        <div className="field">
          <label>{t("editor.color")}</label>
          <div className="swatches" style={{ height: 32, alignItems: "center" }}>
            {PROFILE_COLORS.map((c) => (
              <button key={c} className={`swatch ${p.color === c ? "on" : ""}`} style={{ background: c }} onClick={() => upd({ color: c })} aria-label={c} />
            ))}
          </div>
        </div>
      </div>

      <div className="field">
        <label>{t("editor.provider")}</label>
        <div className="provider-grid">
          {PROVIDERS.map((pr) => (
            <button
              key={pr}
              className={`provider-btn ${p.provider === pr ? "on" : ""}`}
              onClick={() => upd({ provider: pr, host: DEFAULT_HOST[pr] || (pr === p.provider ? p.host : "") })}
            >
              {t(`provider.${pr}`)}
            </button>
          ))}
        </div>
      </div>

      {(p.provider === "githubEnterprise" || p.provider === "generic" || p.provider === "gitlab") && (
        <div className="field">
          <label>{t("editor.host")}</label>
          <input className="input mono" value={p.host} placeholder="git.company.com" onChange={(e) => upd({ host: e.target.value.trim() })} />
        </div>
      )}

      <div className="subsection">
        <div className="head">
          <LogIn size={15} /> {t("editor.section.signin")}
        </div>

        {account ? (
          <div className="account-box" style={{ marginBottom: 12 }}>
            <BadgeCheck size={18} style={{ color: "var(--ok)" }} />
            {account.avatarUrl && <img src={account.avatarUrl} alt="" width={24} height={24} style={{ borderRadius: "50%" }} />}
            <div style={{ flex: 1 }}>
              <b>{t("editor.verified", { login: account.login })}</b>
              {account.scopes && <div className="hint mono">{account.scopes}</div>}
            </div>
          </div>
        ) : null}

        {isGithub && !device && (
          <>
            <button className="btn primary" disabled={!deviceAvailable} onClick={startDevice}>
              <LogIn size={14} /> {t("editor.signInBrowser")}
            </button>
            <div className="hint" style={{ margin: "6px 0 12px" }}>
              {deviceAvailable ? t("editor.signIn.hint") : t("editor.oauthMissing")}
            </div>
          </>
        )}
        {device && (
          <div className="field">
            <label>{t("editor.deviceCode")}</label>
            <div className="device-code">{device.userCode}</div>
            <div className="row">
              <Loader2 size={14} className="spin" />
              <span className="hint" style={{ flex: 1 }}>
                {t("editor.waiting")}
              </span>
              <button className="btn sm" onClick={() => navigator.clipboard?.writeText(device.userCode)}>
                {t("common.copy")}
              </button>
              <button className="btn sm" onClick={() => openUrl(device.verificationUri)}>
                <ExternalLink size={12} /> {t("editor.openGithub")}
              </button>
            </div>
          </div>
        )}

        {needsUsername && (
          <div className="field">
            <label>{t("editor.username")}</label>
            <input className="input" value={p.username} onChange={(e) => upd({ username: e.target.value })} />
          </div>
        )}

        <div className="field" style={{ marginBottom: 0 }}>
          <label>
            {isGithub ? `${t("editor.or")} · ` : ""}
            {t("editor.token")}
          </label>
          <div className="row">
            <input
              className="input mono"
              type="password"
              value={token}
              placeholder={p.hasToken && !clearToken ? "••••••••••••" : "ghp_… / glpat-…"}
              onChange={(e) => {
                setToken(e.target.value);
                setClearToken(false);
              }}
            />
            <button className="btn" disabled={verifying || !hasCredential} onClick={verify}>
              {verifying ? <Loader2 size={14} className="spin" /> : <BadgeCheck size={14} />}
              {t("editor.verify")}
            </button>
          </div>
          <div className="row hint">
            {p.hasToken && !clearToken && !token && <span>{t("editor.token.keep")}</span>}
            <span className="spacer" />
            {tokenPage(p) && (
              <button className="btn sm ghost" onClick={() => openUrl(tokenPage(p)!)}>
                <ExternalLink size={12} /> {t("editor.token.create")}
              </button>
            )}
            {p.hasToken && !clearToken && (
              <button className="btn sm ghost danger" onClick={() => setClearToken(true)}>
                {t("editor.removeToken")}
              </button>
            )}
          </div>
        </div>
        <TokenGuide profile={{ ...p, host }} />
      </div>

      <div className="subsection">
        <div className="head">{t("editor.identity")}</div>
        <div className="hint" style={{ marginTop: -4, marginBottom: 12 }}>
          {t("editor.identity.hint")}
        </div>
        <div className="row" style={{ gap: 12 }}>
          <div className="field" style={{ flex: 1 }}>
            <label>{t("editor.commitName")}</label>
            <input className="input" value={p.commitName} onChange={(e) => upd({ commitName: e.target.value })} />
          </div>
          <div className="field" style={{ flex: 1.3 }}>
            <label>{t("editor.commitEmail")}</label>
            <input className="input" list="gw-emails" value={p.commitEmail} onChange={(e) => upd({ commitEmail: e.target.value.trim() })} />
            <datalist id="gw-emails">
              {emailChoices.map((e) => (
                <option key={e} value={e} />
              ))}
            </datalist>
          </div>
        </div>
        {emailChoices.length > 1 && (
          <div className="row wrap" style={{ marginTop: -6, marginBottom: 12 }}>
            {emailChoices.map((e) => (
              <button key={e} className={`btn sm ${e === p.commitEmail ? "primary" : ""}`} onClick={() => upd({ commitEmail: e })}>
                {e}
                {e === account?.noreplyEmail && <span className="muted"> ({t("editor.noreply")})</span>}
              </button>
            ))}
          </div>
        )}
        <div className="field" style={{ marginBottom: 0 }}>
          <label>{t("editor.extraEmails")}</label>
          <input
            className="input"
            value={p.extraEmails.join(", ")}
            onChange={(e) =>
              upd({
                extraEmails: e.target.value
                  .split(",")
                  .map((s) => s.trim())
                  .filter(Boolean),
              })
            }
          />
          <div className="hint">{t("editor.extraEmails.hint")}</div>
        </div>
      </div>

      <div className="subsection" style={{ marginBottom: 0 }}>
        <div className="head">
          <KeyRound size={15} /> {t("editor.ssh")} <span className="muted" style={{ fontWeight: 400 }}>({t("common.optional")})</span>
        </div>
        <div className="hint" style={{ marginTop: -4, marginBottom: 12 }}>
          {t("editor.ssh.hint")}
        </div>
        <div className="field">
          <label>{t("editor.sshKey")}</label>
          <div className="row">
            <input className="input mono" value={p.sshKeyPath ?? ""} placeholder="C:\Users\…\.ssh\id_ed25519_work" onChange={(e) => upd({ sshKeyPath: e.target.value || null })} />
            <button
              className="btn"
              onClick={async () => {
                const f = await pickFile(t("editor.sshKey"));
                if (f) upd({ sshKeyPath: f });
              }}
            >
              {t("common.browse")}
            </button>
          </div>
        </div>
        {p.sshKeyPath && (
          <div className="field" style={{ marginBottom: 8 }}>
            <label>
              {t("editor.sshPassphrase")} <span className="muted">({t("common.optional")})</span>
            </label>
            <input
              className="input"
              type="password"
              value={passphrase}
              placeholder={p.hasSshPassphrase ? "••••••••" : ""}
              onChange={(e) => setPassphrase(e.target.value)}
            />
            {p.hasSshPassphrase && !passphrase && <div className="hint">{t("editor.sshPassphrase.keep")}</div>}
          </div>
        )}
        <button className="btn sm" disabled={!host} onClick={testSsh}>
          {t("editor.testSsh")}
        </button>
        {sshResult && (
          <pre className="mono selectable" style={{ whiteSpace: "pre-wrap", margin: "8px 0 0", fontSize: 11.5, color: "var(--text-2)" }}>
            {sshResult}
          </pre>
        )}
      </div>
    </Modal>
  );
}
