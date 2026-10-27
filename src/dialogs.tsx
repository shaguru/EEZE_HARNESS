import { useEffect, useState } from "react";
import {
  ACCOUNT_KIND,
  PRESETS,
  PROVIDER_NAME,
  call,
  presetOf,
  type Account,
  type Config,
  type Provider,
  type Snapshot,
} from "./api";
import { Modal, UsageBars } from "./ui";

type Run = (command: string, args?: Record<string, unknown>) => Promise<boolean>;
const LIVE = ["Ready", "Busy", "Waiting for Approval"];

function copy(text: string) {
  navigator.clipboard?.writeText(text).catch(() => {});
}

export function AccountsDialog({
  s,
  run,
  locked,
  onClose,
}: {
  s: Snapshot;
  run: Run;
  locked: boolean;
  onClose: () => void;
}) {
  const c = s.config;
  const [adding, setAdding] = useState<Provider | null>(null);
  const [name, setName] = useState("");
  const [renaming, setRenaming] = useState<{ id: string; label: string } | null>(null);
  const [code, setCode] = useState("");
  const [checking, setChecking] = useState(false);
  const activeId = (p: Provider) => (p === "codex" ? c.activeCodexAccount : c.activeClaudeAccount);
  const live = LIVE.includes(s.lifecycle);
  const signedIn = (a: Account) =>
    !!a.email || (a.id === activeId(a.provider) && a.provider === c.provider && !!s.account?.account);

  /** Makes this account the one in use (switching AI if needed), optionally connecting. */
  async function prepare(a: Account, connect: boolean) {
    if (c.provider !== a.provider && !(await run("set_provider", { provider: a.provider }))) return false;
    if (activeId(a.provider) !== a.id && !(await run("switch_account", { id: a.id }))) return false;
    if (connect) {
      const snap = await call<Snapshot>("snapshot");
      if (!LIVE.includes(snap.lifecycle) && !(await run("connect"))) return false;
    }
    return true;
  }
  async function signIn(a: Account, action = "browser") {
    if (await prepare(a, true)) await run("auth", { action });
  }
  async function signOut(a: Account) {
    if (!window.confirm(`Sign out of "${a.label}" on this computer?`)) return;
    if (await prepare(a, true)) await run("auth", { action: "logout" });
  }
  const login = s.login;
  const loginUrl = login?.verificationUrl || login?.authUrl;
  return (
    <Modal title="Accounts & usage" className="accounts-dialog" onClose={onClose}>
      <p className="hint">
        Each person should sign in with their own account. Usage updates while you chat. Accounts
        you are not using show their last known usage.
      </p>
      {login && (
        <div className="login-card" role="status">
          <h3>Finish signing in to {login.provider === "claude" ? "Claude" : "ChatGPT"}</h3>
          {login.userCode ? (
            <>
              <p>Open the sign-in page and enter this code:</p>
              <code className="device-code">{login.userCode}</code>
            </>
          ) : (
            <p>A browser window should open. Sign in there, then come back here.</p>
          )}
          {loginUrl ? (
            <div className="button-row">
              <button className="send" onClick={() => run("open_external", { url: loginUrl })}>
                Open sign-in page
              </button>
              <button className="outline" onClick={() => copy(loginUrl)}>
                Copy link
              </button>
            </div>
          ) : (
            <p className="hint">Starting sign-in…</p>
          )}
          {login.provider === "claude" && (
            <label className="hint code-label">
              If the page shows a code instead, paste it here:
              <span className="code-row">
                <input
                  className="text-input"
                  value={code}
                  onChange={(e) => setCode(e.target.value)}
                  placeholder="Paste code"
                />
                <button
                  className="outline"
                  disabled={!code.trim()}
                  onClick={async () => {
                    if (await run("auth", { action: "code", code })) setCode("");
                  }}
                >
                  Submit
                </button>
              </span>
            </label>
          )}
          <p className="hint">
            Browser blocked? Copy the link into any browser you are allowed to use.
          </p>
          <button className="ghost" onClick={() => run("auth", { action: "cancel" })}>
            Cancel sign-in
          </button>
        </div>
      )}
      {(["codex", "claude"] as Provider[]).map((p) => (
        <section className="account-section" key={p}>
          <div className="section-row">
            <h3>
              {ACCOUNT_KIND[p]} accounts <span className="muted">· used by {PROVIDER_NAME[p]}</span>
            </h3>
            <button className="ghost" disabled={locked} onClick={() => { setAdding(p); setName(""); }}>
              ＋ Add account
            </button>
          </div>
          {p === "claude" && s.engines && s.engines.claudeFound === false && (
            <p className="sensitive">Claude is not included in this copy of the app. Ask your teacher, or choose claude.exe in Settings → Advanced.</p>
          )}
          {adding === p && (
            <div className="add-row">
              <input
                autoFocus
                className="text-input"
                placeholder={`Name, e.g. "My ${ACCOUNT_KIND[p]}"`}
                value={name}
                onChange={(e) => setName(e.target.value)}
              />
              <button
                className="send"
                onClick={async () => {
                  if (await run("add_account", { provider: p, label: name })) setAdding(null);
                }}
              >
                Add
              </button>
              <button className="ghost" onClick={() => setAdding(null)}>
                Cancel
              </button>
            </div>
          )}
          {c.accounts
            .filter((a) => a.provider === p)
            .map((a) => {
              const active = activeId(p) === a.id;
              const inUse = active && c.provider === p;
              return (
                <div className={`account-card ${inUse ? "active" : ""}`} key={a.id}>
                  <div className="account-top">
                    <span className={`account-avatar ${p}`}>{(a.email || a.label || "?")[0].toUpperCase()}</span>
                    <div className="account-info">
                      {renaming?.id === a.id ? (
                        <span className="code-row">
                          <input
                            autoFocus
                            className="text-input"
                            value={renaming.label}
                            onChange={(e) => setRenaming({ ...renaming, label: e.target.value })}
                          />
                          <button
                            className="outline"
                            onClick={async () => {
                              if (await run("rename_account", { id: a.id, label: renaming.label })) setRenaming(null);
                            }}
                          >
                            Save
                          </button>
                        </span>
                      ) : (
                        <strong>{a.label}</strong>
                      )}
                      <span>
                        {a.email || (signedIn(a) ? "Signed in" : "Not signed in")}
                        {a.plan ? ` · ${a.plan}` : ""}
                      </span>
                    </div>
                    {inUse ? (
                      <span className="pill ok">In use{live ? "" : " · not connected"}</span>
                    ) : (
                      <button className="outline" disabled={locked} onClick={async () => { if (await prepare(a, false)) await run("connect"); }}>
                        Use
                      </button>
                    )}
                  </div>
                  <UsageBars usage={a.usage} usageAt={a.usageAt} />
                  <div className="account-actions">
                    {signedIn(a) ? (
                      <button className="ghost" disabled={locked} onClick={() => signOut(a)}>
                        Sign out
                      </button>
                    ) : (
                      <>
                        <button className="send" disabled={locked} onClick={() => signIn(a)}>
                          Sign in
                        </button>
                        {p === "codex" && (
                          <button className="ghost" disabled={locked} onClick={() => signIn(a, "device")}>
                            Sign in with a code
                          </button>
                        )}
                      </>
                    )}
                    <button className="ghost" onClick={() => setRenaming({ id: a.id, label: a.label })}>
                      Rename
                    </button>
                    {!a.id.endsWith("-default") && (
                      <button className="ghost danger-text" disabled={locked} onClick={() => run("remove_account", { id: a.id })}>
                        Remove
                      </button>
                    )}
                  </div>
                </div>
              );
            })}
        </section>
      ))}
      <div className="dialog-actions">
        <button
          className="outline"
          disabled={checking || locked}
          onClick={async () => {
            setChecking(true);
            await run("refresh_usage");
            setChecking(false);
          }}
        >
          {checking ? "Checking accounts…" : "↻ Check all accounts"}
        </button>
        <button className="send" onClick={onClose}>
          Done
        </button>
      </div>
      <p className="hint">
        Signing in stores the sign-in only in this app folder. Sign out before you share the folder
        or leave a shared computer.
      </p>
    </Modal>
  );
}

export function SettingsDialog({
  s,
  locked,
  onSave,
  onClose,
  openAccounts,
  openAdvanced,
  enterToSend,
  setEnterToSend,
}: {
  s: Snapshot;
  locked: boolean;
  onSave: (c: Config) => Promise<boolean>;
  onClose: () => void;
  openAccounts: () => void;
  openAdvanced: () => void;
  enterToSend: boolean;
  setEnterToSend: (v: boolean) => void;
}) {
  const [prefs, setPrefs] = useState<Config>({ ...s.config });
  const preset = presetOf(prefs);
  const choices = PRESETS.filter((p) => !("advanced" in p && p.advanced) || preset === p.id);
  return (
    <Modal title="Settings" className="settings-dialog" onClose={onClose}>
      <h3 className="settings-heading">What can the AI do?</h3>
      <div className="preset-list" role="radiogroup" aria-label="What can the AI do">
        {choices.map((p) => (
          <label className={`preset ${preset === p.id ? "selected" : ""}`} key={p.id}>
            <input
              type="radio"
              name="preset"
              checked={preset === p.id}
              disabled={locked}
              onChange={() => setPrefs({ ...prefs, sandbox: p.sandbox, approvalPolicy: p.approval })}
            />
            <span>
              <strong>{p.label}</strong>
              <small>{p.hint}</small>
            </span>
          </label>
        ))}
      </div>
      {preset === "auto" || preset === "full" ? (
        <p className="sensitive">The AI will act without asking. Use this only in a project you trust and can restore.</p>
      ) : null}
      <h3 className="settings-heading">Message box</h3>
      <div className="dock-choice" role="radiogroup" aria-label="Message box position">
        {(["left", "bottom", "right"] as const).map((d) => (
          <button
            key={d}
            className={`dock-option ${prefs.layout === d ? "selected" : ""}`}
            role="radio"
            aria-checked={prefs.layout === d}
            onClick={() => setPrefs({ ...prefs, layout: d })}
          >
            <span className={`dock-icon dock-${d}`} />
            {d[0].toUpperCase() + d.slice(1)}
          </button>
        ))}
      </div>
      <p className="hint">Tip: you can also drag the ⠿ handle on the message box to the left, bottom or right.</p>
      <label className="check">
        <input type="checkbox" checked={enterToSend} onChange={(e) => setEnterToSend(e.target.checked)} />
        Press Enter to send (Shift + Enter for a new line)
      </label>
      <h3 className="settings-heading">Appearance</h3>
      <div className="settings-grid">
        <label>
          Theme
          <select value={prefs.theme} onChange={(e) => setPrefs({ ...prefs, theme: e.target.value })}>
            <option value="dark">Dark</option>
            <option value="light">Light</option>
            <option value="system">Same as Windows</option>
          </select>
        </label>
        <label>
          Text size
          <select value={prefs.fontSize} onChange={(e) => setPrefs({ ...prefs, fontSize: Number(e.target.value) })}>
            {[11, 12, 13, 14, 15, 16, 18, 20].map((x) => (
              <option key={x} value={x}>
                {x}
              </option>
            ))}
          </select>
        </label>
      </div>
      <label className="check">
        <input type="checkbox" checked={prefs.showInspector} onChange={(e) => setPrefs({ ...prefs, showInspector: e.target.checked })} />
        Show the Activity panel (commands and file changes)
      </label>
      <label className="check">
        <input type="checkbox" checked={prefs.autoConnect} onChange={(e) => setPrefs({ ...prefs, autoConnect: e.target.checked })} />
        Connect automatically when the app opens
      </label>
      <div className="dialog-actions spread">
        <span className="button-row">
          <button className="outline" onClick={openAccounts}>
            Accounts & usage…
          </button>
          <button className="outline" onClick={openAdvanced}>
            Advanced…
          </button>
        </span>
        <button
          className="send"
          onClick={async () => {
            if (await onSave(prefs)) onClose();
          }}
        >
          Save
        </button>
      </div>
    </Modal>
  );
}

export function AdvancedDialog({
  s,
  run,
  locked,
  onSave,
  onClose,
}: {
  s: Snapshot;
  run: Run;
  locked: boolean;
  onSave: (c: Config) => Promise<boolean>;
  onClose: () => void;
}) {
  const [prefs, setPrefs] = useState<Config>({ ...s.config });
  const [diag, setDiag] = useState<any>(null);
  const [diagText, setDiagText] = useState("");
  const proxyMode = prefs.proxy === "" ? "auto" : prefs.proxy === "none" ? "none" : "custom";
  const [proxyUrl, setProxyUrl] = useState(proxyMode === "custom" ? prefs.proxy : "");
  useEffect(() => {
    call<any>("diagnostics").then(setDiag).catch(() => {});
  }, [s.config.proxy, s.config.caCertificate]);
  const preset = presetOf(prefs);
  const live = LIVE.includes(s.lifecycle);
  const model = s.models.find((m: any) => m.id === prefs.model || m.model === prefs.model) || s.models.find((m: any) => m.isDefault);
  const tiers = (s.config.provider === "codex" ? model?.serviceTiers || [] : []) as any[];
  return (
    <Modal title="Advanced settings" className="settings-dialog" onClose={onClose}>
      <p className="hint">For teachers and confident users. Most students never need these.</p>
      <section className="settings-section">
        <h3>AI behaviour</h3>
        <div className="settings-grid">
          <label>
            Permissions
            <select
              value={preset}
              disabled={locked}
              onChange={(e) => {
                const p = PRESETS.find((x) => x.id === e.target.value);
                if (p) setPrefs({ ...prefs, sandbox: p.sandbox, approvalPolicy: p.approval });
              }}
            >
              {PRESETS.map((p) => (
                <option value={p.id} key={p.id}>
                  {p.label}
                </option>
              ))}
              {preset === "custom" && <option value="custom">Custom</option>}
            </select>
          </label>
          <label>
            Reasoning summaries (Codex)
            <select value={prefs.reasoningSummary} onChange={(e) => setPrefs({ ...prefs, reasoningSummary: e.target.value })}>
              <option value="auto">Automatic</option>
              <option value="concise">Concise</option>
              <option value="detailed">Detailed activity</option>
              <option value="none">Hide summaries</option>
            </select>
          </label>
          <label>
            Service tier (Codex)
            <select value={prefs.serviceTier || ""} onChange={(e) => setPrefs({ ...prefs, serviceTier: e.target.value || null })}>
              <option value="">Model default</option>
              {tiers.map((t: any) => (
                <option value={t.id} key={t.id}>
                  {t.name || t.id}
                </option>
              ))}
            </select>
          </label>
        </div>
        {preset === "full" && (
          <p className="sensitive">Full access removes the project boundary. The AI can change any file this Windows account can reach.</p>
        )}
        <label className="check">
          <input
            type="checkbox"
            checked={prefs.claudeProjectSettings}
            onChange={(e) => setPrefs({ ...prefs, claudeProjectSettings: e.target.checked })}
          />
          Let a project's own Claude settings apply (.claude/settings.json). These can run commands
          automatically (hooks); leave off for downloaded projects.
        </label>
      </section>
      <section className="settings-section">
        <h3>Network</h3>
        <p className="hint">{diag?.network || "Checking network settings…"}</p>
        <div className="settings-grid">
          <label>
            Proxy
            <select
              value={proxyMode}
              onChange={(e) => {
                const v = e.target.value;
                setPrefs({ ...prefs, proxy: v === "auto" ? "" : v === "none" ? "none" : proxyUrl || "http://" });
              }}
            >
              <option value="auto">Automatic (use Windows settings)</option>
              <option value="none">No proxy</option>
              <option value="custom">Custom address</option>
            </select>
          </label>
          {proxyMode === "custom" && (
            <label>
              Proxy address
              <input
                className="text-input"
                value={proxyUrl}
                placeholder="http://proxy.school.edu:8080"
                onChange={(e) => {
                  setProxyUrl(e.target.value);
                  setPrefs({ ...prefs, proxy: e.target.value });
                }}
              />
            </label>
          )}
        </div>
        <p className="hint">
          School certificate: {s.config.caCertificate || "none"}. Add one only if IT says the network
          inspects secure connections.
        </p>
        <div className="button-row">
          <button className="outline" onClick={() => run("set_ca_certificate", { clear: false })}>
            Choose certificate…
          </button>
          {s.config.caCertificate && (
            <button className="ghost" onClick={() => run("set_ca_certificate", { clear: true })}>
              Remove certificate
            </button>
          )}
        </div>
      </section>
      <section className="settings-section">
        <h3>Programs</h3>
        <p className="hint">
          <strong>Codex:</strong> {s.binary?.version || (s.engines?.codexFound ? "found, not checked yet" : "not found")}
          <br />
          <code className="path-text">{s.binary?.path || `${s.root}\\codex.exe`}</code>
          {s.binary?.sha256 && <span className="path-text">SHA-256 {s.binary.sha256}</span>}
        </p>
        <p className="hint">
          <strong>Claude:</strong>{" "}
          {s.claudeBinary?.version || (s.engines?.claudeFound ? "found, not checked yet" : "not found")}
          <br />
          <code className="path-text">{s.claudeBinary?.path || s.engines?.claudePath || "Place claude.exe next to the app, or choose it below."}</code>
          <br />
          Commands: {s.engines?.gitBash ? `Git Bash${s.engines.gitBashBundled ? " (included)" : ""}` : "PowerShell (Git Bash not found)"}
        </p>
        <div className="button-row">
          <button className="outline" disabled={locked} onClick={() => run("choose_binary", { provider: "codex" })}>
            Choose codex.exe…
          </button>
          <button className="outline" disabled={locked} onClick={() => run("choose_binary", { provider: "claude" })}>
            Choose claude.exe…
          </button>
          <button
            className="ghost"
            disabled={!s.capabilities.includes("windowsSandbox/setupStart") || locked || !live || s.config.provider !== "codex"}
            onClick={() => run("auth", { action: "sandbox" })}
          >
            Set up Codex Windows sandbox
          </button>
          {s.config.provider === "claude" && (
            <button className="ghost" disabled={locked} onClick={() => run("auth", { action: "console" })}>
              Sign in with Anthropic Console (API billing)
            </button>
          )}
        </div>
      </section>
      <section className="settings-section">
        <h3>Troubleshooting</h3>
        <label className="check">
          <input type="checkbox" checked={prefs.debugLog} onChange={(e) => setPrefs({ ...prefs, debugLog: e.target.checked })} />
          Log Codex message types only (no message contents) to data\logs
        </label>
        <div className="button-row">
          <button className="ghost" onClick={async () => setDiagText(JSON.stringify(await call("diagnostics"), null, 2))}>
            Show diagnostics
          </button>
          <button
            className="ghost"
            onClick={async () => {
              try {
                setDiagText(`Saved: ${await call<string>("export_diagnostics")}`);
              } catch (e) {
                setDiagText(String(e));
              }
            }}
          >
            Save diagnostics file
          </button>
          <button className="ghost" disabled={!live} onClick={() => run("disconnect")}>
            Stop AI
          </button>
          <button className="outline danger" disabled={locked} onClick={() => run("wipe_account")}>
            Wipe this account's sign-in & history
          </button>
        </div>
        <code className="path-text">Storage: {s.root}\data</code>
        {diagText && <pre className="diagnostics">{diagText}</pre>}
      </section>
      <div className="dialog-actions">
        <button className="outline" onClick={onClose}>
          Cancel
        </button>
        <button
          className="send"
          onClick={async () => {
            if (await onSave(prefs)) onClose();
          }}
        >
          Save
        </button>
      </div>
    </Modal>
  );
}
