import { useEffect, useRef } from "react";
import {
  type Account,
  type Approval,
  type Usage,
  percentLevel,
  resetText,
  windowLabel,
  ago,
} from "./api";

export function PatchViewer({ patch, theme }: { patch: string; theme: string }) {
  const el = useRef<HTMLDivElement>(null);
  const editorRef = useRef<any>(null);
  useEffect(() => {
    let disposed = false;
    let cleanup = () => {};
    (async () => {
      const monaco = await import("monaco-editor/editor/editor.api.js");
      const worker = (await import("monaco-editor/editor/editor.worker.js?worker")).default;
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
  return <div className="patch-editor" ref={el} aria-label="Read-only unified patch" />;
}

export function Modal({
  title,
  children,
  onClose,
  className = "",
}: {
  title: string;
  children: React.ReactNode;
  onClose: () => void;
  className?: string;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const d = ref.current;
    if (d && !d.open) d.showModal();
  }, []);
  return (
    <dialog
      ref={ref}
      onCancel={(e) => {
        e.preventDefault();
        onClose();
      }}
      className={`dialog ${className}`}
    >
      <div className="dialog-head">
        <h2>{title}</h2>
        <button className="icon-button" aria-label="Close dialog" onClick={onClose}>
          ×
        </button>
      </div>
      {children}
    </dialog>
  );
}

function Bar({ label, w }: { label: string; w?: { usedPercent: number; resetsAt?: number | null } | null }) {
  if (!w) return null;
  const p = Math.max(0, Math.min(100, Math.round(w.usedPercent)));
  return (
    <div className="usage-line" title={`${label}: ${p}% used. ${resetText(w.resetsAt)}`}>
      <span className="usage-label">{label}</span>
      <span className={`usage-track ${percentLevel(p)}`}>
        <i style={{ width: `${p}%` }} />
      </span>
      <span className="usage-pct">{p}%</span>
      <span className="usage-reset">{resetText(w.resetsAt)}</span>
    </div>
  );
}

export function UsageBars({ usage, usageAt }: { usage: Usage | null; usageAt?: number | null }) {
  if (!usage || (!usage.primary && !usage.secondary))
    return <p className="hint usage-none">No usage seen yet. It appears after you send a message with this account.</p>;
  return (
    <div className="usage-bars">
      <Bar label={windowLabel(usage.primary, "5-hour")} w={usage.primary} />
      <Bar label={windowLabel(usage.secondary, "Weekly")} w={usage.secondary} />
      {usage.limitReached && (
        <p className="usage-limit">Limit reached{usage.limitResetsAt ? ` · ${resetText(usage.limitResetsAt)}` : ""}</p>
      )}
      {usageAt ? <p className="usage-age">Updated {ago(usageAt)}</p> : null}
    </div>
  );
}

/** Compact two-line meter for the top bar; the click opens the accounts panel. */
export function UsageMeter({ account, onOpen }: { account: Account | null; onOpen: () => void }) {
  const u = account?.usage;
  const rows = [
    { short: "5h", w: u?.primary, fallback: "5-hour" },
    { short: "Wk", w: u?.secondary, fallback: "Weekly" },
  ];
  const known = rows.some((r) => r.w);
  const title = known
    ? rows
        .filter((r) => r.w)
        .map((r) => `${windowLabel(r.w, r.fallback)}: ${Math.round(r.w!.usedPercent)}% used, ${resetText(r.w!.resetsAt)}`)
        .join("\n") + `\nUpdated ${ago(account?.usageAt)} · click for all accounts`
    : "Usage appears after your first message. Click for accounts.";
  return (
    <button className={`usage-meter ${u?.limitReached ? "limit" : ""}`} onClick={onOpen} title={title} aria-label="Usage and accounts">
      {known ? (
        rows.map((r) => {
          const p = r.w ? Math.max(0, Math.min(100, Math.round(r.w.usedPercent))) : null;
          return (
            <span className="meter-row" key={r.short}>
              <span className="meter-label">{r.short}</span>
              <span className={`usage-track ${p === null ? "" : percentLevel(p)}`}>
                <i style={{ width: `${p ?? 0}%` }} />
              </span>
              <span className="meter-pct">{p === null ? "–" : `${p}%`}</span>
            </span>
          );
        })
      ) : (
        <span className="meter-empty">Usage —</span>
      )}
    </button>
  );
}

const asText = (v: unknown) =>
  Array.isArray(v) ? v.join(" ") : typeof v === "string" ? v : v == null ? "" : JSON.stringify(v);

/** A plain-language headline and the key detail for any approval request. */
export function describeApproval(a: Approval): { title: string; detail: string; code?: string } {
  const p = a.params || {};
  if (a.method === "claude/canUseTool") {
    const tool = String(p.toolName || "a tool");
    const input = p.input || {};
    if (tool === "Bash" || tool === "PowerShell")
      return { title: "Claude wants to run a command", detail: input.description || p.decisionReason || "", code: input.command };
    if (["Edit", "MultiEdit", "Write", "NotebookEdit"].includes(tool)) {
      const path = input.file_path || input.notebook_path || "a file";
      const code =
        tool === "Write"
          ? input.content
          : tool === "Edit"
            ? `- ${String(input.old_string ?? "").split("\n").join("\n- ")}\n+ ${String(input.new_string ?? "").split("\n").join("\n+ ")}`
            : asText(input.edits ?? input.new_source);
      return { title: `Claude wants to ${tool === "Write" ? "write" : "edit"} ${path}`, detail: p.decisionReason || "", code: String(code ?? "").slice(0, 20000) };
    }
    if (tool === "WebFetch") return { title: "Claude wants to open a web page", detail: input.prompt || "", code: input.url };
    if (tool === "WebSearch") return { title: "Claude wants to search the web", detail: "", code: input.query };
    return { title: `Claude wants to use ${tool}`, detail: p.decisionReason || "", code: JSON.stringify(input, null, 2) };
  }
  switch (a.method) {
    case "item/commandExecution/requestApproval":
    case "execCommandApproval":
      return { title: "Codex wants to run a command", detail: p.reason || (p.cwd ? `In ${p.cwd}` : ""), code: asText(p.command) };
    case "item/fileChange/requestApproval":
    case "applyPatchApproval":
      return { title: "Codex wants to change files", detail: p.reason || "Review the changes below.", code: p.fileChanges ? JSON.stringify(p.fileChanges, null, 2) : undefined };
    case "item/permissions/requestApproval":
      return { title: "Codex asks for extra permissions", detail: p.reason || "", code: JSON.stringify(p.permissions, null, 2) };
    default:
      return { title: "The AI is asking for permission", detail: a.method };
  }
}

/** One short line describing an activity item. */
export function activityTitle(item: any): string {
  switch (item.type) {
    case "reasoning":
      return "Thinking";
    case "commandExecution":
      return item.command || "Command";
    case "fileChange":
      return item.changes?.length === 1
        ? `${item.tool === "Write" ? "Write" : "Edit"} ${item.changes[0].path}`
        : `${item.changes?.length || 0} file changes`;
    case "toolCall": {
      const i = item.input || {};
      const what = i.file_path || i.path || i.pattern || i.url || i.query || i.description || "";
      return `${item.tool}${what ? ` · ${what}` : ""}`;
    }
    case "plan":
      return "Plan";
    default:
      return item.type;
  }
}
