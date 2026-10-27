import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  ACCOUNT_KIND,
  PRESETS,
  PROVIDER_NAME,
  call,
  explain,
  prefsOf,
  presetOf,
  type Approval,
  type Config,
  type Provider,
  type Snapshot,
} from "./api";
import { Modal, PatchViewer, UsageMeter, activityTitle, describeApproval } from "./ui";
import { AccountsDialog, AdvancedDialog, SettingsDialog } from "./dialogs";

const tail = (p: string) => p.split(/[\\/]/).filter(Boolean).at(-1) || p;
const pretty = (v: unknown) => JSON.stringify(v, null, 2);
const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(Math.max(lo, hi), v));
const LIVE = ["Ready", "Busy", "Waiting for Approval"];
type Dock = "left" | "bottom" | "right";
const DOCKS: Dock[] = ["left", "bottom", "right"];
const LIFECYCLE_TEXT: Record<string, string> = {
  Ready: "Ready",
  Busy: "Working…",
  "Waiting for Approval": "Needs you",
  Stopped: "Not connected",
  Failed: "Problem",
  Starting: "Starting…",
  Handshaking: "Starting…",
};

function useStoredFlag(key: string, initial: boolean): [boolean, (v: boolean) => void] {
  const [value, setValue] = useState(() => {
    try {
      const v = localStorage.getItem(key);
      return v === null ? initial : v === "1";
    } catch {
      return initial;
    }
  });
  return [
    value,
    (v: boolean) => {
      setValue(v);
      try {
        localStorage.setItem(key, v ? "1" : "0");
      } catch {}
    },
  ];
}

export default function App() {
  const [s, setS] = useState<Snapshot | null>(null);
  const [error, setError] = useState("");
  const [text, setText] = useState("");
  const [pending, setPending] = useState(false);
  const [dialog, setDialog] = useState<null | "settings" | "advanced" | "accounts">(null);
  const [inspector, setInspector] = useState<"activity" | "changes">("activity");
  const [patch, setPatch] = useState<{ title: string; text: string } | null>(null);
  const [external, setExternal] = useState("");
  const [label, setLabel] = useState<{ id: string; text: string } | null>(null);
  const [follow, setFollow] = useState(true);
  const [dragging, setDragging] = useState<"sidebar" | "inspector" | "composer" | null>(null);
  const [dockDrag, setDockDrag] = useState<{ zone: Dock | null } | null>(null);
  const [size, setSize] = useState({ w: 360, h: 170 });
  const [vw, setVw] = useState(() => window.innerWidth);
  const [drawer, setDrawer] = useState<null | "sidebar" | "inspector">(null);
  const [dismissed, setDismissed] = useState<string[]>([]);
  const [enterToSend, setEnterToSend] = useStoredFlag("enterToSend", true);
  // A message sent while a short action (new chat, connect) is finishing waits here instead of being dropped.
  const [queued, setQueued] = useState(false);
  const scroll = useRef<HTMLDivElement>(null);
  const convRef = useRef<HTMLElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const autoConnected = useRef(false);
  const refresh = useCallback(async () => {
    const next = await call<Snapshot>("snapshot");
    setS(next);
    return next;
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
    const resize = () => setVw(window.innerWidth);
    window.addEventListener("resize", resize);
    return () => {
      disposed = true;
      off?.();
      window.removeEventListener("resize", resize);
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
      await refresh().catch(() => {});
      return false;
    } finally {
      setPending(false);
    }
  }
  const config = s?.config;
  const configRef = useRef<Config | null>(null);
  useEffect(() => {
    configRef.current = config || null;
  }, [config]);
  const provider: Provider = config?.provider || "codex";
  const connected = !!s && LIVE.includes(s.lifecycle);
  const busy =
    (!!s && !!s.turnId) || s?.lifecycle === "Busy" || s?.lifecycle === "Waiting for Approval";
  const locked = pending || !!busy;
  const can = (method: string) => provider === "claude" || (s?.capabilities.includes(method) ?? false);
  const dock: Dock = DOCKS.includes(config?.layout as Dock) ? (config!.layout as Dock) : "bottom";
  const engineFound = provider === "codex" ? s?.engines?.codexFound !== false : s?.engines?.claudeFound !== false;
  const activeAccount =
    config?.accounts.find(
      (a) => a.id === (provider === "codex" ? config.activeCodexAccount : config.activeClaudeAccount),
    ) || null;
  const signedIn = !!s?.account?.account || (!connected && !!activeAccount?.email);

  // Connect once at start (and the button stays available) so students go straight to chatting.
  useEffect(() => {
    if (!s || autoConnected.current) return;
    autoConnected.current = true;
    if (s.config.autoConnect && s.lifecycle === "Stopped" && engineFound) run("connect");
  }, [s]);
  useEffect(() => {
    const theme =
      config?.theme === "system"
        ? matchMedia("(prefers-color-scheme: light)").matches
          ? "light"
          : "dark"
        : config?.theme || "dark";
    document.documentElement.dataset.theme = theme;
    document.documentElement.style.setProperty("--chat-font", `${config?.fontSize || 13}px`);
  }, [config?.theme, config?.fontSize]);
  useEffect(() => {
    if (!dragging && config) setSize({ w: config.composerWidth || 360, h: config.composerHeight || 170 });
  }, [config?.composerWidth, config?.composerHeight, dragging]);
  const sizeRef = useRef(size);
  sizeRef.current = size;
  useEffect(() => {
    if (follow) scroll.current?.scrollTo({ top: scroll.current.scrollHeight });
  }, [s?.items, follow]);

  const messages =
    s?.items.filter((i) => ["userMessage", "agentMessage", "plan"].includes(i.type)) || [];
  const activity = s?.items.filter((i) => !["userMessage", "agentMessage"].includes(i.type)) || [];
  const changes = useMemo(
    () =>
      s?.items
        .filter((i) => i.type === "fileChange")
        .flatMap((i) => (i as any).changes || []) || [],
    [s?.items],
  );
  const threadTitle = (t: any) => config?.labels[t.id] || t.name || t.preview || "New conversation";
  const current = s?.threads.find((t) => t.id === s.threadId);
  const approval = s?.approvals[0];

  /** Saves without the busy overlay, for layout tweaks that should feel instant. */
  async function quietSave(c: Config) {
    setS((cur) => (cur ? { ...cur, config: c } : cur));
    try {
      await call("save_preferences", { prefs: prefsOf(c) });
    } catch (e) {
      setError(String(e));
      refresh().catch(() => {});
    }
  }
  const savePrefs = (c: Config) => run("save_preferences", { prefs: prefsOf(c) });
  const setDock = (d: Dock) => config && d !== dock && quietSave({ ...config, layout: d });

  const narrow = vw < 1100;
  const tiny = vw < 860;
  const sideInGrid = !tiny;
  const inspInGrid = !narrow && !!config?.showInspector;
  const inspectorOpen = narrow ? drawer === "inspector" : !!config?.showInspector;
  const sidebarW = config?.sidebarWidth || 235;
  const inspW = config?.inspectorWidth || 295;
  const setWidth = (kind: "sidebar" | "inspector", value: number) => {
    setS((cur) => {
      if (!cur) return cur;
      const c = cur.config;
      return {
        ...cur,
        config: {
          ...c,
          sidebarWidth: kind === "sidebar" ? clamp(Math.round(value), 180, 420) : c.sidebarWidth,
          inspectorWidth: kind === "inspector" ? clamp(Math.round(value), 220, 460) : c.inspectorWidth,
        },
      };
    });
  };
  useEffect(() => {
    if (!dragging) return;
    const move = (e: PointerEvent) => {
      if (dragging === "sidebar") setWidth("sidebar", e.clientX);
      else if (dragging === "inspector") setWidth("inspector", window.innerWidth - e.clientX);
      else {
        const r = convRef.current?.getBoundingClientRect();
        if (!r) return;
        if (dock === "bottom")
          setSize((z) => ({ ...z, h: Math.round(clamp(r.bottom - e.clientY, 110, Math.min(520, r.height - 140))) }));
        else {
          const w = dock === "left" ? e.clientX - r.left : r.right - e.clientX;
          setSize((z) => ({ ...z, w: Math.round(clamp(w, 260, Math.min(720, r.width - 280))) }));
        }
      }
    };
    const up = () => {
      const c = configRef.current;
      if (c)
        quietSave({
          ...c,
          composerWidth: clamp(sizeRef.current.w, 260, 720),
          composerHeight: clamp(sizeRef.current.h, 110, 520),
        });
      setDragging(null);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up, { once: true });
    document.body.classList.add("is-resizing");
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      document.body.classList.remove("is-resizing");
    };
  }, [dragging, dock]);

  /** Drag the message box by its header and drop it on the left, bottom or right. */
  function startDockDrag(e: React.PointerEvent) {
    if ((e.target as HTMLElement).closest("button, select, textarea, input") || e.button !== 0) return;
    e.preventDefault();
    const start = { x: e.clientX, y: e.clientY };
    let moved = false;
    let zone: Dock | null = null;
    const zoneAt = (x: number, y: number): Dock | null => {
      const r = convRef.current?.getBoundingClientRect();
      if (!r) return null;
      if (y > r.bottom - r.height * 0.34) return "bottom";
      return x < r.left + r.width / 2 ? "left" : "right";
    };
    const move = (ev: PointerEvent) => {
      if (!moved && Math.hypot(ev.clientX - start.x, ev.clientY - start.y) < 6) return;
      moved = true;
      zone = zoneAt(ev.clientX, ev.clientY);
      setDockDrag({ zone });
    };
    const finish = (apply: boolean) => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("keydown", key);
      document.body.classList.remove("is-docking");
      setDockDrag(null);
      if (apply && moved && zone) setDock(zone);
    };
    const up = () => finish(true);
    const key = (ev: KeyboardEvent) => ev.key === "Escape" && finish(false);
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("keydown", key);
    document.body.classList.add("is-docking");
  }

  async function switchProvider(p: Provider) {
    if (p === provider || locked) return;
    if (await run("set_provider", { provider: p })) {
      const found = p === "codex" ? s?.engines?.codexFound !== false : s?.engines?.claudeFound !== false;
      if (found) await run("connect");
    }
  }
  async function send() {
    if (!text.trim() || busy || !connected) return;
    if (pending) {
      setQueued(true);
      return;
    }
    setQueued(false);
    if (!s?.threadId && !(await run("open_thread", { id: null, more: false }))) return;
    const sent = text;
    setFollow(true);
    if (await run("send_turn", { text: sent })) setText("");
  }
  useEffect(() => {
    if (queued && !pending) send();
    if (queued && (busy || !connected)) setQueued(false);
  }, [queued, pending, busy, connected]);
  function markdown(content: string) {
    return (
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        skipHtml
        components={{
          a: ({ href, children }) => (
            <button className="inline-link" title={href} onClick={() => href && setExternal(href)}>
              {children}
            </button>
          ),
          img: ({ alt }) => <span className="muted">[Image: {alt || "external image"}]</span>,
          pre: ({ children }) => (
            <div className="code-block">
              <button
                className="copy-code"
                onClick={(e) => {
                  const code = e.currentTarget.parentElement?.querySelector("code")?.textContent || "";
                  navigator.clipboard.writeText(code).catch(() => setError("Clipboard unavailable"));
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
  function toggleInspector() {
    if (narrow) setDrawer((d) => (d === "inspector" ? null : "inspector"));
    else if (config) quietSave({ ...config, showInspector: !config.showInspector });
  }

  const errText = error || s?.error || "";
  const info = errText ? explain(errText, provider) : null;
  const notices = (s?.notices || []).filter((n) => !dismissed.includes(n.id));
  const preset = config ? presetOf(config) : "ask";
  const modelValue = (provider === "claude" ? config?.claudeModel : config?.model) || "";
  const selectedModel =
    s?.models.find((m: any) => m.id === modelValue || m.model === modelValue) ||
    s?.models.find((m: any) => m.isDefault) ||
    s?.models[0];
  const effortOptions = (selectedModel?.supportedReasoningEfforts || []).map((o: any) => o.reasoningEffort) as string[];
  const effortValue = (provider === "claude" ? config?.claudeEffort : config?.reasoningEffort) || "";
  const columns = [
    sideInGrid && `${sidebarW}px`,
    sideInGrid && "6px",
    "minmax(0, 1fr)",
    inspInGrid && "6px",
    inspInGrid && `${inspW}px`,
  ]
    .filter(Boolean)
    .join(" ");

  const sidebar = (
    <aside className={`sidebar ${sideInGrid ? "" : "drawer drawer-left"}`}>
      <div className="eyebrow">PROJECTS</div>
      <button className="outline full" disabled={locked} onClick={() => run("select_project")}>
        📂 Open project folder
      </button>
      <div className="project-list">
        {config?.projects.map((p) => (
          <div className={`project-card ${p === config.selectedProject ? "active" : ""}`} key={p} title={p}>
            <button
              className="project-select"
              disabled={locked}
              onClick={() => {
                setDrawer(null);
                run("reconnect_project", { path: p });
              }}
            >
              <span className="project-icon">⌂</span>
              <span className="project-info">
                <strong>{tail(p)}</strong>
                <span>{p}</span>
              </span>
            </button>
            <button
              className="project-open"
              aria-label={`Open ${tail(p)} folder`}
              disabled={pending}
              title="Open this folder in File Explorer"
              onClick={() => run("open_project_folder", { path: p })}
            >
              ↗
            </button>
            <button
              className="project-open"
              aria-label={`Remove ${tail(p)} from the list`}
              disabled={locked}
              title="Remove from this list (files are not deleted)"
              onClick={() => run("forget_project", { path: p })}
            >
              ×
            </button>
          </div>
        ))}
      </div>
      {!config?.projects.length && <p className="hint">Choose a folder for your project to begin.</p>}
      <div className="section-row thread-heading">
        <span className="eyebrow">CHATS</span>
        <button
          className="mini"
          aria-label="Refresh chats"
          disabled={!connected || pending}
          onClick={() => run("refresh_threads", { more: false })}
        >
          ↻
        </button>
      </div>
      <button
        className="outline full"
        disabled={!connected || !config?.selectedProject || locked || !can("thread/start")}
        onClick={() => {
          setDrawer(null);
          run("open_thread", { id: null, more: false });
        }}
      >
        ＋ New chat
      </button>
      <div className="thread-list">
        {s?.threads.map((t) => (
          <div className={`thread-entry ${t.id === s.threadId ? "selected" : ""}`} key={t.id}>
            <button
              className="thread-row"
              disabled={locked}
              onClick={() => {
                setFollow(true);
                setDrawer(null);
                run("open_thread", { id: t.id, more: false });
              }}
              title={threadTitle(t)}
            >
              <span className="thread-dot" />
              <span>{threadTitle(t)}</span>
            </button>
            <button className="mini" aria-label={`Rename ${threadTitle(t)}`} onClick={() => setLabel({ id: t.id, text: threadTitle(t) })}>
              ✎
            </button>
            <button
              className="mini"
              aria-label={`Archive ${threadTitle(t)}`}
              disabled={locked || !can("thread/archive")}
              onClick={() => run("archive_thread", { id: t.id })}
            >
              ⌑
            </button>
          </div>
        ))}
      </div>
      {s?.threadCursor && (
        <button className="ghost" onClick={() => run("refresh_threads", { more: true })}>
          More chats
        </button>
      )}
      <div className="sidebar-bottom">
        <button className="ghost full" onClick={() => setDialog("accounts")}>
          Accounts & usage
        </button>
        <button className="ghost full" onClick={() => setDialog("settings")}>
          Settings
        </button>
      </div>
    </aside>
  );

  const inspectorPanel = (
    <aside className={`inspector ${inspInGrid ? "" : "drawer drawer-right"}`}>
      <div className="inspector-tabs">
        <button className={inspector === "activity" ? "active" : ""} onClick={() => setInspector("activity")}>
          Activity <span>{activity.length}</span>
        </button>
        <button className={inspector === "changes" ? "active" : ""} onClick={() => setInspector("changes")}>
          Changes <span>{changes.length}</span>
        </button>
        <button className="mini close-panel" aria-label="Hide activity panel" onClick={toggleInspector}>
          ×
        </button>
      </div>
      {inspector === "activity" ? (
        <>
          <div className="eyebrow inspector-label">WHAT THE AI DID</div>
          {!activity.length && (
            <div className="inspector-empty">
              ◌<p>Commands, file edits and tool use appear here while {PROVIDER_NAME[provider]} works.</p>
            </div>
          )}
          {activity.map((item: any) => (
            <details className="activity-card" key={item.id}>
              <summary>
                <span className={`activity-type ${item.type}`}>
                  {item.type === "commandExecution" ? "›_" : item.type === "fileChange" ? "±" : item.type === "reasoning" ? "…" : "·"}
                </span>
                <span>
                  {activityTitle(item)}
                  <small className={`status-${item.status || "done"}`}>
                    {item.status || (item.harnessCompleted ? "completed" : "activity")}
                  </small>
                </span>
              </summary>
              <pre>
                {item.type === "reasoning"
                  ? `${PROVIDER_NAME[provider]} is thinking. Private reasoning is not displayed.`
                  : item.aggregatedOutput
                    ? item.aggregatedOutput
                    : pretty(item)}
              </pre>
            </details>
          ))}
        </>
      ) : (
        <>
          <div className="section-row inspector-label">
            <span className="eyebrow">FILE CHANGES</span>
            <button className="mini" aria-label="Refresh changes" onClick={() => refresh()}>
              ↻
            </button>
          </div>
          {!changes.length && !s?.diff && (
            <div className="inspector-empty">
              ±<p>File changes made or proposed by the AI appear here.</p>
            </div>
          )}
          {changes.map((c: any, i: number) => (
            <button
              key={`${c.path}-${i}`}
              className="change-row"
              title={c.path}
              onClick={() => setPatch({ title: c.path, text: c.diff || "No patch supplied." })}
            >
              <span>±</span>
              <span>
                {c.path}
                <small>{c.kind?.type || "File change"}</small>
              </span>
            </button>
          ))}
          {s?.diff && (
            <button className="outline full" onClick={() => setPatch({ title: "Latest turn · unified patch", text: s.diff })}>
              Open turn patch
            </button>
          )}
          <p className="hint">Check each item's status in Activity: changes can be proposed, declined or completed.</p>
        </>
      )}
    </aside>
  );

  const steps = [
    {
      done: !!config?.selectedProject,
      label: config?.selectedProject ? `Project: ${tail(config.selectedProject)}` : "Choose a project folder",
      button: "Choose folder",
      action: () => run("select_project"),
      enabled: !locked,
    },
    {
      done: signedIn,
      label: signedIn ? `Signed in${activeAccount?.email ? ` as ${activeAccount.email}` : ""}` : `Sign in to ${ACCOUNT_KIND[provider]}`,
      button: "Sign in",
      action: () => setDialog("accounts"),
      enabled: true,
    },
    {
      done: !!s?.threadId,
      label: "Type your question below",
      button: "New chat",
      action: () => run("open_thread", { id: null, more: false }),
      enabled: connected && !!config?.selectedProject && !locked,
    },
  ];

  return (
    <div className="app-shell">
      <header className="topbar">
        {tiny && (
          <button className="icon-button" aria-label="Show projects and chats" onClick={() => setDrawer((d) => (d === "sidebar" ? null : "sidebar"))}>
            ☰
          </button>
        )}
        <div className="brand">
          <span className="brand-mark">⌁</span>
          <span className="brand-name">RustPortableHarness</span>
        </div>
        <div className="engine-switch" role="tablist" aria-label="Choose the AI">
          {(["codex", "claude"] as Provider[]).map((p) => (
            <button
              key={p}
              role="tab"
              aria-selected={provider === p}
              className={provider === p ? "active" : ""}
              disabled={locked && provider !== p}
              onClick={() => switchProvider(p)}
              title={p === "codex" ? "OpenAI Codex (ChatGPT account)" : "Anthropic Claude (Claude account)"}
            >
              {PROVIDER_NAME[p]}
            </button>
          ))}
        </div>
        <div className="top-controls">
          <UsageMeter account={activeAccount} onOpen={() => setDialog("accounts")} />
          <button className={`account-chip ${signedIn ? "" : "attention"}`} onClick={() => setDialog("accounts")} title="Accounts & usage">
            <span className={`account-avatar ${provider}`}>{(activeAccount?.email || activeAccount?.label || "?")[0].toUpperCase()}</span>
            <span className="account-chip-text">{signedIn ? activeAccount?.email || activeAccount?.label : "Sign in"}</span>
          </button>
          <span className={`pill ${connected ? "ok" : s?.lifecycle === "Failed" ? "bad" : "muted"}`}>
            <i />
            {LIFECYCLE_TEXT[s?.lifecycle || ""] || s?.lifecycle || "Loading"}
          </span>
          <button className={connected ? "outline" : "send"} disabled={locked || !engineFound} onClick={() => run(connected ? "disconnect" : "connect")}>
            {connected ? "Disconnect" : "Connect"}
          </button>
          <button
            className={`icon-button ${inspectorOpen ? "on" : ""}`}
            aria-label={inspectorOpen ? "Hide activity panel" : "Show activity panel"}
            aria-pressed={inspectorOpen}
            title="Activity panel"
            onClick={toggleInspector}
          >
            ▤{activity.length ? <span className="badge">{activity.length}</span> : null}
          </button>
          <button className="icon-button" aria-label="Settings" onClick={() => setDialog("settings")}>
            ⚙
          </button>
        </div>
      </header>
      <main className="workspace" style={{ gridTemplateColumns: columns }}>
        {sideInGrid && sidebar}
        {sideInGrid && (
          <div className="resize-handle" role="separator" aria-label="Resize projects panel" onPointerDown={() => setDragging("sidebar")} />
        )}
        <section
          ref={convRef}
          className={`conversation dock-${dock}`}
          style={{ "--composer-w": `${size.w}px`, "--composer-h": `${size.h}px` } as React.CSSProperties}
        >
          <div className="conversation-main">
            <div className="conversation-head">
              <div className="head-title">
                <div className="eyebrow">
                  {PROVIDER_NAME[provider]} · {config?.selectedProject ? tail(config.selectedProject) : "NO PROJECT"}
                </div>
                <h1>{current ? threadTitle(current) : s?.threadId ? "New chat" : "Start a working session"}</h1>
              </div>
              <div className="head-actions">
                <select
                  aria-label="Model"
                  value={modelValue}
                  disabled={!connected || locked}
                  onChange={(e) =>
                    config &&
                    savePrefs(
                      provider === "claude"
                        ? { ...config, claudeModel: e.target.value || null }
                        : { ...config, model: e.target.value || null, reasoningEffort: null, serviceTier: null },
                    )
                  }
                >
                  <option value="">Default model</option>
                  {s?.models.map((m: any) => (
                    <option value={m.model || m.id} key={m.id} title={m.description || ""}>
                      {m.displayName || m.id}
                    </option>
                  ))}
                </select>
                <select
                  aria-label="Thinking effort"
                  title="Higher effort can help hard tasks but is slower and uses more of your limit"
                  value={effortValue}
                  disabled={!connected || locked || !effortOptions.length}
                  onChange={(e) =>
                    config &&
                    savePrefs(
                      provider === "claude"
                        ? { ...config, claudeEffort: e.target.value || null }
                        : { ...config, reasoningEffort: e.target.value || null },
                    )
                  }
                >
                  <option value="">Default effort</option>
                  {effortOptions.map((o) => (
                    <option value={o} key={o}>
                      {o[0].toUpperCase() + o.slice(1)} effort
                    </option>
                  ))}
                </select>
              </div>
            </div>
            {notices.map((n) => (
              <div className={`notice ${n.level}`} role={n.level === "error" ? "alert" : "status"} key={n.id}>
                <div>
                  <strong>{n.title}</strong>
                  <span>{n.text}</span>
                </div>
                <button className="icon-button" aria-label="Dismiss" onClick={() => setDismissed([...dismissed, n.id])}>
                  ×
                </button>
              </div>
            ))}
            {info && (
              <div role="alert" className="notice error">
                <div>
                  <strong>{info.title}</strong>
                  {info.hint ? <span>{info.hint}</span> : <span>{errText}</span>}
                  {info.hint && (
                    <details>
                      <summary>Details</summary>
                      <code>{errText}</code>
                    </details>
                  )}
                </div>
                {info.action === "signin" && (
                  <button className="send" onClick={() => setDialog("accounts")}>
                    Sign in
                  </button>
                )}
                {info.action === "accounts" && (
                  <button className="outline" onClick={() => setDialog("accounts")}>
                    Accounts & usage
                  </button>
                )}
                {info.action === "advanced" && (
                  <button className="outline" onClick={() => setDialog("advanced")}>
                    Open Advanced
                  </button>
                )}
                <button
                  className="icon-button"
                  aria-label="Dismiss"
                  onClick={() => {
                    setError("");
                    setS((cur) => (cur ? { ...cur, error: null } : cur));
                  }}
                >
                  ×
                </button>
              </div>
            )}
            <div
              className="messages"
              ref={scroll}
              onScroll={() => {
                const el = scroll.current;
                if (el) setFollow(el.scrollHeight - el.scrollTop - el.clientHeight < 90);
              }}
            >
              {s?.historyCursor && (
                <button className="ghost" onClick={() => run("open_thread", { id: s.threadId, more: true })}>
                  Load older messages
                </button>
              )}
              {messages.length === 0 && (
                <div className="empty">
                  <div className="empty-icon">✦</div>
                  {!engineFound ? (
                    <>
                      <h2>{PROVIDER_NAME[provider]} is not set up in this folder</h2>
                      <p>
                        {provider === "claude"
                          ? "This copy of the app does not include claude.exe. Ask your teacher for the Claude version, or choose claude.exe if you have it."
                          : "codex.exe is missing. Extract the ZIP again; if it keeps disappearing, antivirus may be removing it."}
                      </p>
                      <div className="button-row center">
                        <button className="outline" onClick={() => run("choose_binary", { provider })}>
                          Choose {provider}.exe…
                        </button>
                        <button className="send" onClick={() => switchProvider(provider === "claude" ? "codex" : "claude")}>
                          Use {PROVIDER_NAME[provider === "claude" ? "codex" : "claude"]} instead
                        </button>
                      </div>
                    </>
                  ) : (
                    <>
                      <h2>{s?.threadId ? `Ask ${PROVIDER_NAME[provider]} anything about your project` : "Get started in three steps"}</h2>
                      <ol className="steps">
                        {steps.map((st, i) => (
                          <li className={st.done ? "done" : ""} key={i}>
                            <span className="step-num">{st.done ? "✓" : i + 1}</span>
                            <span className="step-label">{st.label}</span>
                            {!st.done && (
                              <button className={i === steps.findIndex((x) => !x.done) ? "send" : "outline"} disabled={!st.enabled} onClick={st.action}>
                                {st.button}
                              </button>
                            )}
                          </li>
                        ))}
                      </ol>
                      <p className="hint">
                        The AI only works inside the project folder you choose. It asks before changing
                        files unless you allow more in Settings.
                      </p>
                    </>
                  )}
                </div>
              )}
              {messages.map((item: any) => (
                <article className={`message ${item.type === "userMessage" ? "user" : "assistant"}`} key={item.id}>
                  <div className={`avatar ${item.type === "userMessage" ? "" : provider}`}>
                    {item.type === "userMessage" ? "You" : provider === "claude" ? "✳" : "C"}
                  </div>
                  <div className="message-body">
                    <div className="message-meta">
                      {item.type === "userMessage" ? "You" : item.type === "plan" ? `${PROVIDER_NAME[provider]} · plan` : PROVIDER_NAME[provider]}
                    </div>
                    <div className="markdown">
                      {markdown(
                        item.type === "userMessage"
                          ? item.content?.filter((c: any) => c.type === "text").map((c: any) => c.text).join("\n") || ""
                          : item.text || "",
                      )}
                    </div>
                  </div>
                </article>
              ))}
              {busy && (
                <div className="working" role="status">
                  <span className="dots"><i /><i /><i /></span>
                  {s?.lifecycle === "Waiting for Approval" ? "Waiting for your answer…" : `${PROVIDER_NAME[provider]} is working…`}
                </div>
              )}
            </div>
            {!follow && messages.length > 0 && (
              <button className="follow-button" onClick={() => setFollow(true)}>
                ↓ Latest
              </button>
            )}
          </div>
          <div
            className={`composer-split ${dock === "bottom" ? "rows" : "cols"}`}
            role="separator"
            aria-label="Resize message box (double-click to reset)"
            title="Drag to resize · double-click to reset"
            onPointerDown={(e) => {
              e.preventDefault();
              setDragging("composer");
            }}
            onDoubleClick={() => config && quietSave({ ...config, composerWidth: 360, composerHeight: 170 })}
          />
          <div className="composer">
            <div className="composer-head" onPointerDown={startDockDrag} title="Drag here to move the message box left, bottom or right">
              <span className="grip" aria-hidden="true">
                ⠿
              </span>
              <select
                aria-label="What the AI may do"
                className="preset-select"
                value={preset}
                disabled={locked}
                onChange={(e) => {
                  const p = PRESETS.find((x) => x.id === e.target.value);
                  if (p && config) savePrefs({ ...config, sandbox: p.sandbox, approvalPolicy: p.approval });
                }}
                title={PRESETS.find((p) => p.id === preset)?.hint || "Custom permissions (see Advanced settings)"}
              >
                {PRESETS.filter((p) => !("advanced" in p && p.advanced) || preset === p.id).map((p) => (
                  <option value={p.id} key={p.id}>
                    {p.label}
                  </option>
                ))}
                {preset === "custom" && <option value="custom">Custom (see Advanced)</option>}
              </select>
              <span className="dock-buttons" role="group" aria-label="Move the message box">
                {DOCKS.map((d) => (
                  <button
                    key={d}
                    className={`dock-btn ${dock === d ? "on" : ""}`}
                    aria-label={`Message box on the ${d}`}
                    aria-pressed={dock === d}
                    title={`Put the message box on the ${d}`}
                    onClick={() => setDock(d)}
                  >
                    <span className={`dock-icon dock-${d}`} />
                  </button>
                ))}
              </span>
            </div>
            <textarea
              aria-label={`Message ${PROVIDER_NAME[provider]}`}
              value={text}
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key !== "Enter" || e.nativeEvent.isComposing) return;
                if (e.ctrlKey || e.metaKey || (enterToSend && !e.shiftKey)) {
                  e.preventDefault();
                  send();
                }
              }}
              placeholder={
                !connected
                  ? `Connect to ${PROVIDER_NAME[provider]} to start…`
                  : !config?.selectedProject
                    ? "Choose a project folder first…"
                    : `Ask ${PROVIDER_NAME[provider]} to explain or change your project…`
              }
              disabled={!connected || !config?.selectedProject}
            />
            <div className="composer-foot">
              <span>
                {busy
                  ? `${PROVIDER_NAME[provider]} is working. You can type your next message; press Stop to interrupt.`
                  : enterToSend
                    ? "Enter to send · Shift+Enter for a new line"
                    : "Ctrl+Enter to send"}
              </span>
              <div>
                <button className="ghost" disabled={!s?.turnId} onClick={() => run("interrupt")}>
                  ■ Stop
                </button>
                <button className="send" disabled={!connected || !config?.selectedProject || !text.trim() || !!busy || queued} onClick={send}>
                  {busy ? "Working…" : queued ? "Sending…" : "Send ↑"}
                </button>
              </div>
            </div>
          </div>
          {dockDrag && (
            <div className="dock-overlay" aria-hidden="true">
              {DOCKS.map((d) => (
                <div key={d} className={`dock-zone zone-${d} ${dockDrag.zone === d ? "hot" : ""}`}>
                  <span>{d === dock ? "Keep here" : `Drop to put it on the ${d}`}</span>
                </div>
              ))}
            </div>
          )}
        </section>
        {inspInGrid && (
          <div className="resize-handle" role="separator" aria-label="Resize activity panel" onPointerDown={() => setDragging("inspector")} />
        )}
        {inspInGrid && inspectorPanel}
        {!sideInGrid && drawer === "sidebar" && sidebar}
        {!inspInGrid && narrow && drawer === "inspector" && inspectorPanel}
        {drawer && (narrow || tiny) && <div className="drawer-scrim" onClick={() => setDrawer(null)} />}
      </main>
      <footer className="status-bar">
        <span>
          <i className={connected ? "connected" : ""} />
          {PROVIDER_NAME[provider]} · {LIFECYCLE_TEXT[s?.lifecycle || ""] || s?.lifecycle || "Loading"}
        </span>
        <span title={config?.selectedProject || ""}>{config?.selectedProject || "No project selected"}</span>
        <span>{provider === "claude" ? s?.claudeBinary?.version || "Claude" : s?.binary?.version || "Codex"}</span>
      </footer>
      {s?.warning && (
        <div className="notification-tray" role="status">
          <span>{s.warning}</span>
          <button className="icon-button" aria-label="Dismiss notification" onClick={() => setS((cur) => (cur ? { ...cur, warning: null } : cur))}>
            ×
          </button>
        </div>
      )}
      {dialog === "settings" && s && (
        <SettingsDialog
          s={s}
          locked={locked}
          onSave={savePrefs}
          onClose={() => setDialog(null)}
          openAccounts={() => setDialog("accounts")}
          openAdvanced={() => setDialog("advanced")}
          enterToSend={enterToSend}
          setEnterToSend={setEnterToSend}
        />
      )}
      {dialog === "advanced" && s && (
        <AdvancedDialog s={s} run={run} locked={locked} onSave={savePrefs} onClose={() => setDialog(null)} />
      )}
      {dialog === "accounts" && s && <AccountsDialog s={s} run={run} locked={locked} onClose={() => setDialog(null)} />}
      {patch && (
        <Modal title={patch.title} onClose={() => setPatch(null)}>
          <p className="hint">Patch supplied by the AI · read only</p>
          <PatchViewer patch={patch.text} theme={document.documentElement.dataset.theme || "dark"} />
          <div className="dialog-actions">
            <button
              className="outline"
              onClick={() => navigator.clipboard.writeText(patch.text).catch(() => setError("Clipboard unavailable"))}
            >
              Copy patch
            </button>
          </div>
        </Modal>
      )}
      {external && (
        <Modal title="Open external link" onClose={() => setExternal("")}>
          <p>Open this web address in your browser?</p>
          <code className="path-text">{external}</code>
          <div className="dialog-actions">
            <button className="outline" onClick={() => setExternal("")}>
              Cancel
            </button>
            <button
              className="send"
              onClick={async () => {
                if (await run("open_external", { url: external })) setExternal("");
              }}
            >
              Open browser
            </button>
          </div>
        </Modal>
      )}
      {label && (
        <Modal title="Rename chat" onClose={() => setLabel(null)}>
          <input
            autoFocus
            aria-label="Chat name"
            className="text-input"
            value={label.text}
            onChange={(e) => setLabel({ ...label, text: e.target.value })}
          />
          <div className="dialog-actions">
            <button
              className="send"
              onClick={async () => {
                if (await run("rename_thread", { id: label.id, label: label.text })) setLabel(null);
              }}
            >
              Save name
            </button>
          </div>
        </Modal>
      )}
      {approval && (() => {
        const d = describeApproval(approval);
        return (
          <Modal
            title={`Permission needed${s!.approvals.length > 1 ? ` · ${s!.approvals.length} waiting` : ""}`}
            onClose={() => {}}
            className="approval-dialog"
          >
            <h3 className="approval-title">{d.title}</h3>
            {d.detail && <p className="hint">{d.detail}</p>}
            {d.code && <pre className="approval-details">{d.code}</pre>}
            {approval.method === "item/fileChange/requestApproval" && (
              <pre className="approval-details">
                {pretty(s?.items.find((i) => i.id === approval.params.itemId)) || "Codex has not supplied a file-change item yet."}
              </pre>
            )}
            <div className="approval-warning">Only approve if you understand it and it matches what you asked for.</div>
            <details>
              <summary className="hint">Technical details</summary>
              <pre className="approval-details">{pretty(approval.params)}</pre>
            </details>
            {!approval.supported && (
              <p className="sensitive">This app cannot answer this request type. Stop the turn to cancel it safely.</p>
            )}
            <div className="dialog-actions">
              <button className="outline" disabled={pending || !approval.supported} onClick={() => decide(approval, "decline")}>
                Decline
              </button>
              <button className="outline danger" disabled={!s?.turnId} onClick={() => run("interrupt")}>
                Stop turn
              </button>
              <button className="send" disabled={pending || !approval.supported} onClick={() => decide(approval, "accept")}>
                Approve once
              </button>
            </div>
          </Modal>
        );
      })()}
    </div>
  );
}
