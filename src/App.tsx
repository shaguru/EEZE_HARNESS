import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { call, type Snapshot, type Config, type Approval } from "./api";

const tail = (p: string) => p.split(/[\\/]/).filter(Boolean).at(-1) || p;
const pretty = (v: unknown) => JSON.stringify(v, null, 2);
function PatchViewer({ patch, theme }: { patch: string; theme: string }) {
  const el = useRef<HTMLDivElement>(null);
  const editorRef = useRef<any>(null);
  useEffect(() => {
    let disposed = false;
    let cleanup = () => {};
    (async () => {
      const monaco = await import("monaco-editor/editor/editor.api.js");
      const worker = (
        await import("monaco-editor/editor/editor.worker.js?worker")
      ).default;
      (globalThis as any).MonacoEnvironment = { getWorker: () => new worker() };
      if (disposed || !el.current) return;
      const editor = monaco.editor.create(el.current, {
        value: patch,
        language: "plaintext",
        readOnly: true,
        theme: theme === "light" ? "vs" : "vs-dark",
        minimap: { enabled: false },
        automaticLayout: true,
        fontSize: 12,
        wordWrap: "off",
        scrollBeyondLastLine: false,
        renderLineHighlight: "none",
      });
      editorRef.current = editor;
      cleanup = () => {
        editor.getModel()?.dispose();
        editor.dispose();
      };
    })();
    return () => {
      disposed = true;
      cleanup();
    };
  }, []);
  useEffect(() => {
    editorRef.current?.setValue(patch);
  }, [patch]);
  return (
    <div
      className="patch-editor"
      ref={el}
      aria-label="Read-only unified patch"
    />
  );
}
function Modal({
  title,
  children,
  onClose,
}: {
  title: string;
  children: React.ReactNode;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    ref.current?.showModal();
  }, []);
  return (
    <dialog ref={ref} onCancel={onClose} className="dialog">
      <div className="dialog-head">
        <h2>{title}</h2>
        <button
          className="icon-button"
          aria-label="Close dialog"
          onClick={onClose}
        >
          ×
        </button>
      </div>
      {children}
    </dialog>
  );
}

export default function App() {
  const [s, setS] = useState<Snapshot | null>(null);
  const [error, setError] = useState("");
  const [text, setText] = useState("");
  const [pending, setPending] = useState(false);
  const [settings, setSettings] = useState(false);
  const [prefs, setPrefs] = useState<Config | null>(null);
  const [inspector, setInspector] = useState<"activity" | "changes">(
    "activity",
  );
  const [patch, setPatch] = useState<{ title: string; text: string } | null>(
    null,
  );
  const [external, setExternal] = useState("");
  const [diagnostics, setDiagnostics] = useState("");
  const [label, setLabel] = useState<{ id: string; text: string } | null>(null);
  const [follow, setFollow] = useState(true);
  const scroll = useRef<HTMLDivElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const refresh = useCallback(async () => {
    const next = await call<Snapshot>("snapshot");
    setS(next);
  }, []);
  useEffect(() => {
    let disposed = false;
    let off: undefined | (() => void);
    listen("harness-changed", () => {
      if (!timer.current)
        timer.current = setTimeout(() => {
          timer.current = null;
          refresh().catch((e) => setError(String(e)));
        }, 50);
    }).then((fn) => {
      if (disposed) fn();
      else off = fn;
    });
    refresh().catch((e) => setError(String(e)));
    return () => {
      disposed = true;
      off?.();
      if (timer.current) clearTimeout(timer.current);
    };
  }, [refresh]);
  async function run(command: string, args?: Record<string, unknown>) {
    setError("");
    setPending(true);
    try {
      await call(command, args);
      await refresh();
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    } finally {
      setPending(false);
    }
  }
  const config = s?.config;
  const connected =
    !!s && ["Ready", "Busy", "Waiting for Approval"].includes(s.lifecycle);
  const busy =
    (!!s && !!s.turnId) ||
    s?.lifecycle === "Busy" ||
    s?.lifecycle === "Waiting for Approval";
  const can = (method: string) => s?.capabilities.includes(method) ?? false;
  useEffect(() => {
    const theme =
      config?.theme === "system"
        ? matchMedia("(prefers-color-scheme: light)").matches
          ? "light"
          : "dark"
        : config?.theme || "dark";
    document.documentElement.dataset.theme = theme;
    document.documentElement.style.setProperty(
      "--chat-font",
      `${config?.fontSize || 13}px`,
    );
  }, [config?.theme, config?.fontSize]);
  useEffect(() => {
    if (follow) scroll.current?.scrollTo({ top: scroll.current.scrollHeight });
  }, [s?.items, follow]);
  const messages =
    s?.items.filter((i) =>
      ["userMessage", "agentMessage", "plan"].includes(i.type),
    ) || [];
  const activity =
    s?.items.filter((i) => !["userMessage", "agentMessage"].includes(i.type)) ||
    [];
  const changes = useMemo(
    () =>
      s?.items
        .filter((i) => i.type === "fileChange")
        .flatMap((i) => (i as any).changes || []) || [],
    [s?.items],
  );
  const threadTitle = (t: any) =>
    config?.labels[t.id] || t.name || t.preview || "New conversation";
  const current = s?.threads.find((t) => t.id === s.threadId);
  const approval = s?.approvals[0];
  const modelValue = config?.model || "";
  async function savePrefs(p: Config) {
    return run("save_preferences", {
      theme: p.theme,
      fontSize: p.fontSize,
      model: p.model,
      sandbox: p.sandbox,
      approvalPolicy: p.approvalPolicy,
      debugLog: p.debugLog,
    });
  }
  async function send() {
    if (!text.trim() || pending || busy) return;
    const sent = text;
    if (await run("send_turn", { text: sent })) setText("");
  }
  function markdown(content: string) {
    return (
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        skipHtml
        components={{
          a: ({ href, children }) => (
            <button
              className="inline-link"
              title={href}
              onClick={() => href && setExternal(href)}
            >
              {children}
            </button>
          ),
          img: ({ alt }) => (
            <span className="muted">[Image: {alt || "external image"}]</span>
          ),
          pre: ({ children }) => (
            <div className="code-block">
              <button
                className="copy-code"
                onClick={(e) => {
                  const code =
                    e.currentTarget.parentElement?.querySelector("code")
                      ?.textContent || "";
                  navigator.clipboard
                    .writeText(code)
                    .catch(() => setError("Clipboard unavailable"));
                }}
              >
                Copy
              </button>
              <pre>{children}</pre>
            </div>
          ),
        }}
      >
        {content}
      </ReactMarkdown>
    );
  }
  function decide(a: Approval, decision: string) {
    return run("decide", { key: a.key, decision });
  }

  return (
    <div className="app-shell">
      <header className="topbar">
        <div className="brand">
          <span className="brand-mark">⌁</span>
          <span>RustPortableHarness</span>
          <span className="version">0.1</span>
        </div>
        <div className="top-controls">
          <span className={`pill ${connected ? "ok" : "muted"}`}>
            <i />
            {s?.lifecycle || "Loading"}
          </span>
          <button
            className="ghost"
            disabled={pending || !!busy}
            onClick={() => run("connect")}
          >
            {connected ? "Restart Codex" : "Connect Codex"}
          </button>
          <button
            className="icon-button"
            aria-label="Settings"
            onClick={() => {
              setPrefs(config ? { ...config } : null);
              setSettings(true);
            }}
          >
            ⚙
          </button>
        </div>
      </header>
      <main className="workspace">
        <aside className="sidebar">
          <div className="eyebrow">PROJECTS</div>
          <button
            className="outline full"
            disabled={pending || !!busy}
            onClick={() => run("select_project")}
          >
            ＋ Add folder
          </button>
          <div className="project-list">
            {config?.projects.map((p) => (
              <button
                className={`project-card ${p === config.selectedProject ? "active" : ""}`}
                key={p}
                title={p}
                disabled={pending || !!busy}
                onClick={() => run("reconnect_project", { path: p })}
              >
                <span className="project-icon">⌂</span>
                <span className="project-info">
                  <strong>{tail(p)}</strong>
                  <span>{p}</span>
                </span>
              </button>
            ))}
          </div>
          {!config?.projects.length && (
            <p className="hint">Choose an accessible local folder to begin.</p>
          )}
          <div className="section-row thread-heading">
            <span className="eyebrow">THREADS</span>
            <button
              className="mini"
              aria-label="Refresh threads"
              disabled={!connected || pending}
              onClick={() => run("refresh_threads", { more: false })}
            >
              ↻
            </button>
          </div>
          <button
            className="outline full"
            disabled={
              !connected ||
              !config?.selectedProject ||
              !!busy ||
              pending ||
              !can("thread/start")
            }
            onClick={() => run("open_thread", { id: null, more: false })}
          >
            ＋ New thread
          </button>
          <div className="thread-list">
            {s?.threads.map((t) => (
              <div
                className={`thread-entry ${t.id === s.threadId ? "selected" : ""}`}
                key={t.id}
              >
                <button
                  className="thread-row"
                  disabled={!!busy || pending}
                  onClick={() => {
                    setFollow(true);
                    run("open_thread", { id: t.id, more: false });
                  }}
                  title={threadTitle(t)}
                >
                  <span className="thread-dot" />
                  <span>{threadTitle(t)}</span>
                </button>
                <button
                  className="mini"
                  aria-label={`Rename ${threadTitle(t)}`}
                  onClick={() => setLabel({ id: t.id, text: threadTitle(t) })}
                >
                  ✎
                </button>
                <button
                  className="mini"
                  aria-label={`Archive ${threadTitle(t)}`}
                  disabled={!!busy || pending || !can("thread/archive")}
                  onClick={() => run("archive_thread", { id: t.id })}
                >
                  ⌑
                </button>
              </div>
            ))}
          </div>
          {s?.threadCursor && (
            <button
              className="ghost"
              onClick={() => run("refresh_threads", { more: true })}
            >
              More threads
            </button>
          )}
          <div className="sidebar-bottom">
            <div className="eyebrow">PORTABLE · LOCAL UI</div>
            <p className="hint">
              Agent actions follow Codex sandbox and approval rules.
            </p>
            <button
              className="ghost full"
              onClick={() => {
                setPrefs(config ? { ...config } : null);
                setSettings(true);
              }}
            >
              Account & settings
            </button>
          </div>
        </aside>
        <section className="conversation">
          <div className="conversation-head">
            <div>
              <div className="eyebrow">
                {config?.selectedProject
                  ? tail(config.selectedProject)
                  : "WORKSPACE"}
              </div>
              <h1>
                {current
                  ? threadTitle(current)
                  : s?.threadId
                    ? "New conversation"
                    : "Start a working session"}
              </h1>
            </div>
            <div className="head-actions">
              <select
                aria-label="Model"
                value={modelValue}
                disabled={!connected || !!busy || pending}
                onChange={(e) =>
                  config &&
                  savePrefs({ ...config, model: e.target.value || null })
                }
              >
                <option value="">Codex default</option>
                {s?.models.map((m) => (
                  <option value={m.model || m.id} key={m.id}>
                    {m.displayName || m.id}
                  </option>
                ))}
              </select>
            </div>
          </div>
          {(error || s?.error) && (
            <div role="alert" className="notice error">
              <strong>Needs attention</strong>
              <span>{error || s?.error}</span>
            </div>
          )}
          {s?.warning && (
            <div className="notice">
              <span>{s.warning}</span>
            </div>
          )}
          <div
            className="messages"
            ref={scroll}
            onScroll={() => {
              if (scroll.current) {
                const el = scroll.current;
                setFollow(
                  el.scrollHeight - el.scrollTop - el.clientHeight < 90,
                );
              }
            }}
          >
            {s?.historyCursor && (
              <button
                className="ghost"
                onClick={() =>
                  run("open_thread", { id: s.threadId, more: true })
                }
              >
                Load older turns
              </button>
            )}
            {messages.length === 0 && (
              <div className="empty">
                <div className="empty-icon">✦</div>
                <h2>
                  {!config?.selectedProject
                    ? "Choose your workspace"
                    : !connected
                      ? "Connect to Codex"
                      : !s?.threadId
                        ? "Create or reopen a thread"
                        : "Ready when you are"}
                </h2>
                <p>
                  A portable workspace for the official Codex agent.
                  <br />
                  Choose a folder, connect, and start a conversation.
                </p>
                <div className="steps">
                  <span>01 &nbsp; Choose folder</span>
                  <span>02 &nbsp; Connect & sign in</span>
                  <span>03 &nbsp; Start thread</span>
                </div>
              </div>
            )}
            {messages.map((item: any) => (
              <article
                className={`message ${item.type === "userMessage" ? "user" : "assistant"}`}
                key={item.id}
              >
                <div className="avatar">
                  {item.type === "userMessage" ? "T" : "C"}
                </div>
                <div className="message-body">
                  <div className="message-meta">
                    {item.type === "userMessage"
                      ? "You"
                      : item.type === "plan"
                        ? "Codex · plan"
                        : "Codex"}
                  </div>
                  <div className="markdown">
                    {markdown(
                      item.type === "userMessage"
                        ? item.content
                            ?.filter((c: any) => c.type === "text")
                            .map((c: any) => c.text)
                            .join("\n") || ""
                        : item.text || "",
                    )}
                  </div>
                </div>
              </article>
            ))}
          </div>
          {!follow && messages.length > 0 && (
            <button className="follow-button" onClick={() => setFollow(true)}>
              ↓ Follow latest
            </button>
          )}
          <div className="composer-wrap">
            <div className="composer-context">
              <span className="pill muted">
                {config?.sandbox === "workspace-write"
                  ? "Workspace write"
                  : "Read only"}
              </span>
              <span>Ask on request</span>
            </div>
            <textarea
              aria-label="Message Codex"
              value={text}
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => {
                if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
                  e.preventDefault();
                  send();
                }
              }}
              placeholder={
                s?.threadId
                  ? "Ask Codex to inspect or change your project…"
                  : "Create or reopen a thread to begin…"
              }
              disabled={!connected || !s?.threadId}
            />
            <div className="composer-foot">
              <span>Ctrl + Enter to send</span>
              <div>
                <button
                  className="ghost"
                  disabled={!s?.turnId}
                  onClick={() => run("interrupt")}
                >
                  ■ Stop
                </button>
                <button
                  className="send"
                  disabled={
                    !connected ||
                    !s?.threadId ||
                    !text.trim() ||
                    !!busy ||
                    pending
                  }
                  onClick={send}
                >
                  {busy ? "Working…" : "Send ↑"}
                </button>
              </div>
            </div>
          </div>
        </section>
        <aside className="inspector">
          <div className="inspector-tabs">
            <button
              className={inspector === "activity" ? "active" : ""}
              onClick={() => setInspector("activity")}
            >
              Activity <span>{activity.length}</span>
            </button>
            <button
              className={inspector === "changes" ? "active" : ""}
              onClick={() => setInspector("changes")}
            >
              Changes <span>{changes.length}</span>
            </button>
          </div>
          {inspector === "activity" ? (
            <>
              <div className="eyebrow inspector-label">AGENT ACTIVITY</div>
              {!activity.length && (
                <div className="inspector-empty">
                  ◌
                  <p>
                    Commands, edits, and tool calls appear here as Codex works.
                  </p>
                </div>
              )}
              {activity.map((item: any) => (
                <details className="activity-card" key={item.id}>
                  <summary>
                    <span className={`activity-type ${item.type}`}>
                      {item.type === "commandExecution"
                        ? "›_"
                        : item.type === "fileChange"
                          ? "±"
                          : "·"}
                    </span>
                    <span>
                      {item.type === "reasoning"
                        ? "Planning activity"
                        : item.type === "commandExecution"
                          ? item.command
                          : item.type === "fileChange"
                            ? `${item.changes?.length || 0} file changes`
                            : item.type}
                      <small>
                        {item.status ||
                          (item.harnessCompleted ? "Completed" : "Activity")}
                      </small>
                    </span>
                  </summary>
                  <pre>
                    {item.type === "reasoning"
                      ? "Codex is planning. Private reasoning is not displayed."
                      : pretty(item)}
                  </pre>
                </details>
              ))}
            </>
          ) : (
            <>
              <div className="section-row inspector-label">
                <span className="eyebrow">CODEX PATCHES</span>
                <button
                  className="mini"
                  aria-label="Refresh changes from backend snapshot"
                  onClick={() => refresh()}
                >
                  ↻
                </button>
              </div>
              {!changes.length && !s?.diff && (
                <div className="inspector-empty">
                  ±
                  <p>
                    Authoritative file changes appear here. No patch is inferred
                    from your working tree.
                  </p>
                </div>
              )}
              {changes.map((c: any, i: number) => (
                <button
                  key={`${c.path}-${i}`}
                  className="change-row"
                  title={c.path}
                  onClick={() =>
                    setPatch({
                      title: c.path,
                      text: c.diff || "No patch supplied by Codex.",
                    })
                  }
                >
                  <span>±</span>
                  <span>
                    {c.path}
                    <small>{c.kind?.type || "File change"}</small>
                  </span>
                </button>
              ))}
              {s?.diff && (
                <button
                  className="outline full"
                  onClick={() =>
                    setPatch({
                      title: "Latest turn · unified patch",
                      text: s.diff,
                    })
                  }
                >
                  Open turn patch
                </button>
              )}
              <p className="hint">
                Patches may describe proposed or completed edits. Check the item
                status in Activity.
              </p>
            </>
          )}
        </aside>
      </main>
      <footer className="status-bar">
        <span>
          <i className={connected ? "connected" : ""} />
          {s?.lifecycle || "Loading"}
          {!s?.account?.account && connected ? " · Sign in from Settings" : ""}
        </span>
        <span title={config?.selectedProject || ""}>
          {config?.selectedProject || "No project selected"}
        </span>
        <span>{s?.binary?.version || "Codex 0.163.0"}</span>
      </footer>
      {settings && prefs && (
        <Modal title="Settings" onClose={() => setSettings(false)}>
          <div className="settings-grid">
            <label>
              Theme
              <select
                value={prefs.theme}
                onChange={(e) => setPrefs({ ...prefs, theme: e.target.value })}
              >
                {["dark", "light", "system"].map((x) => (
                  <option key={x}>{x}</option>
                ))}
              </select>
            </label>
            <label>
              Text size
              <select
                value={prefs.fontSize}
                onChange={(e) =>
                  setPrefs({ ...prefs, fontSize: Number(e.target.value) })
                }
              >
                {[11, 12, 13, 14, 15, 16, 18, 20].map((x) => (
                  <option key={x}>{x}</option>
                ))}
              </select>
            </label>
            <label>
              Sandbox
              <select
                value={prefs.sandbox}
                onChange={(e) =>
                  setPrefs({ ...prefs, sandbox: e.target.value })
                }
              >
                <option value="read-only">Read only (default)</option>
                <option value="workspace-write">Workspace write</option>
              </select>
            </label>
            <label>
              Approval policy
              <select
                value={prefs.approvalPolicy}
                onChange={(e) =>
                  setPrefs({ ...prefs, approvalPolicy: e.target.value })
                }
              >
                <option value="on-request">Ask when Codex requests</option>
              </select>
            </label>
          </div>
          <p className="hint">
            Sandbox changes apply when creating or reopening a thread. Read only
            is the default; network access for tools remains restricted.
            Workspace write allows edits inside the selected project.
          </p>
          <div className="dialog-actions">
            <button
              className="send"
              disabled={pending || !!busy}
              onClick={async () => {
                if (await savePrefs(prefs)) setSettings(false);
              }}
            >
              Save preferences
            </button>
          </div>
          <div className="settings-section">
            <h3>Codex account</h3>
            <p>
              {s?.account?.account?.email ||
                s?.account?.account?.type ||
                "Not signed in"}
            </p>
            <div className="button-row">
              <button
                className="outline"
                disabled={!connected || pending || !can("account/login/start")}
                onClick={() => run("auth", { action: "device" })}
              >
                Device sign-in
              </button>
              <button
                className="outline"
                disabled={!connected || pending || !can("account/login/start")}
                onClick={() => run("auth", { action: "browser" })}
              >
                Browser sign-in
              </button>
              <button
                className="ghost"
                disabled={!connected || pending}
                onClick={() => run("auth", { action: "status" })}
              >
                Refresh status
              </button>
              <button
                className="ghost"
                disabled={!connected || pending || !!busy}
                onClick={() => {
                  if (window.confirm("Sign out of the portable Codex account?"))
                    run("auth", { action: "logout" });
                }}
              >
                Sign out
              </button>
            </div>
            {s?.login && (
              <div className="login-card">
                <p>Complete sign-in in your browser. This code is temporary.</p>
                {s.login.userCode && (
                  <code className="device-code">{s.login.userCode}</code>
                )}
                <button
                  className="inline-link"
                  onClick={() =>
                    setExternal(s.login.verificationUrl || s.login.authUrl)
                  }
                >
                  {s.login.verificationUrl || "Open secure Codex sign-in"}
                </button>
                <button
                  className="ghost"
                  onClick={() => run("auth", { action: "cancel" })}
                >
                  Cancel sign-in
                </button>
              </div>
            )}
            <p className="sensitive">
              The portable data folder may contain sensitive account state. Sign
              out before sharing this folder or leaving a public PC.
            </p>
          </div>
          <div className="settings-section">
            <h3>Executable & diagnostics</h3>
            <label className="hint">
              <input
                type="checkbox"
                checked={prefs.debugLog}
                onChange={(e) =>
                  setPrefs({ ...prefs, debugLog: e.target.checked })
                }
              />{" "}
              Log protocol method names only (no message contents)
            </label>
            <code className="path-text">
              {s?.binary?.path || `${s?.root}\\codex.exe`}
            </code>
            <p className="hint">
              {s?.binary?.version || "Not checked"}
              <br />
              SHA-256: {s?.binary?.sha256 || "Connect to validate the binary"}
            </p>
            <div className="button-row">
              <button
                className="outline"
                disabled={!!busy || pending}
                onClick={() => run("choose_binary")}
              >
                Choose Codex EXE
              </button>
              <button
                className="ghost"
                disabled={
                  !can("windowsSandbox/setupStart") ||
                  pending ||
                  !!busy ||
                  !connected
                }
                onClick={() => run("auth", { action: "sandbox" })}
              >
                Set up unelevated sandbox
              </button>
              <button
                className="ghost"
                onClick={async () =>
                  setDiagnostics(pretty(await call("diagnostics")))
                }
              >
                Diagnostics
              </button>
              <button
                className="ghost"
                onClick={async () => {
                  try {
                    setDiagnostics(
                      `Saved: ${await call("export_diagnostics")}`,
                    );
                  } catch (e) {
                    setError(String(e));
                  }
                }}
              >
                Export diagnostics
              </button>
              <button
                className="ghost"
                disabled={!connected || pending}
                onClick={() => run("disconnect")}
              >
                Stop Codex
              </button>
              <button
                className="outline danger"
                disabled={pending}
                onClick={() => run("wipe_account")}
              >
                Wipe portable account &amp; history
              </button>
            </div>
            <code className="path-text">Storage: {s?.root}\\data</code>
            {diagnostics && <pre className="diagnostics">{diagnostics}</pre>}
          </div>
        </Modal>
      )}
      {patch && (
        <Modal title={patch.title} onClose={() => setPatch(null)}>
          <p className="hint">Unified patch supplied by Codex · read only</p>
          <PatchViewer patch={patch.text} theme={config?.theme || "dark"} />
          <div className="dialog-actions">
            <button
              className="outline"
              onClick={() =>
                navigator.clipboard
                  .writeText(patch.text)
                  .catch(() => setError("Clipboard unavailable"))
              }
            >
              Copy patch
            </button>
          </div>
        </Modal>
      )}
      {external && (
        <Modal title="Open external link" onClose={() => setExternal("")}>
          <p>Open this destination in your default browser:</p>
          <code className="path-text">{external}</code>
          <div className="dialog-actions">
            <button className="outline" onClick={() => setExternal("")}>
              Cancel
            </button>
            <button
              className="send"
              onClick={async () => {
                if (await run("open_external", { url: external }))
                  setExternal("");
              }}
            >
              Open browser
            </button>
          </div>
        </Modal>
      )}
      {label && (
        <Modal title="Rename thread label" onClose={() => setLabel(null)}>
          <input
            autoFocus
            aria-label="Thread label"
            className="text-input"
            value={label.text}
            onChange={(e) => setLabel({ ...label, text: e.target.value })}
          />
          <div className="dialog-actions">
            <button
              className="send"
              onClick={async () => {
                if (
                  await run("rename_thread", {
                    id: label.id,
                    label: label.text,
                  })
                )
                  setLabel(null);
              }}
            >
              Save label
            </button>
          </div>
        </Modal>
      )}
      {approval && (
        <Modal
          title={`Permission required${s!.approvals.length > 1 ? ` · ${s!.approvals.length} pending` : ""}`}
          onClose={() => {}}
        >
          <div className="approval-warning">
            Review the exact operation before deciding.
          </div>
          <h3>{approval.method}</h3>
          <pre className="approval-details">{pretty(approval.params)}</pre>
          {approval.method === "item/fileChange/requestApproval" && (
            <pre className="approval-details">
              {pretty(s?.items.find((i) => i.id === approval.params.itemId)) ||
                "Codex has not supplied a file-change item yet."}
            </pre>
          )}
          {!approval.supported && (
            <p className="sensitive">
              This client cannot answer this request type. Stop the turn to
              cancel it safely.
            </p>
          )}
          <div className="dialog-actions">
            <button
              className="outline"
              disabled={pending || !approval.supported}
              onClick={() => decide(approval, "decline")}
            >
              Decline
            </button>
            <button
              className="outline danger"
              disabled={!s?.turnId}
              onClick={() => run("interrupt")}
            >
              Stop turn
            </button>
            <button
              className="send"
              disabled={pending || !approval.supported}
              onClick={() => decide(approval, "accept")}
            >
              Approve once
            </button>
          </div>
        </Modal>
      )}
    </div>
  );
}
