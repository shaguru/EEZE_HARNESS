import { invoke } from "@tauri-apps/api/core";
import type { ThreadItem } from "../generated/ts/v2/ThreadItem";
export type Provider = "codex" | "claude";
export type UsageWindow = {
  usedPercent: number;
  windowMins?: number | null;
  resetsAt?: number | null;
};
export type Usage = {
  primary?: UsageWindow | null;
  secondary?: UsageWindow | null;
  limitReached?: boolean;
  status?: string | null;
  limitResetsAt?: number | null;
  plan?: string | null;
};
export type Account = {
  id: string;
  provider: Provider;
  label: string;
  email: string | null;
  plan: string | null;
  usage: Usage | null;
  usageAt: number | null;
};
export type Config = {
  schemaVersion: number;
  projects: string[];
  selectedProject: string | null;
  selectedThread: string | null;
  binaryPath: string | null;
  model: string | null;
  theme: string;
  fontSize: number;
  sandbox: string;
  approvalPolicy: string;
  reasoningEffort: string | null;
  reasoningSummary: string;
  serviceTier: string | null;
  layout: "bottom" | "left" | "right" | string;
  sidebarWidth: number;
  inspectorWidth: number;
  labels: Record<string, string>;
  debugLog: boolean;
  provider: Provider;
  accounts: Account[];
  activeCodexAccount: string;
  activeClaudeAccount: string;
  claudeBinaryPath: string | null;
  claudeModel: string | null;
  claudeEffort: string | null;
  claudeProjectSettings: boolean;
  composerWidth: number;
  composerHeight: number;
  showInspector: boolean;
  autoConnect: boolean;
  proxy: string;
  caCertificate: string | null;
};
export type Approval = {
  key: string;
  id: string | number;
  method: string;
  params: Record<string, any>;
  supported: boolean;
};
export type Notice = { id: string; level: string; title: string; text: string };
export type BinaryInfo = { path: string; version: string; sha256: string };
export type Snapshot = {
  lifecycle: string;
  error: string | null;
  warning: string | null;
  config: Config;
  root: string;
  binary: BinaryInfo | null;
  models: any[];
  account: any;
  login: any;
  threadId: string | null;
  turnId: string | null;
  threads: any[];
  items: (ThreadItem & Record<string, any>)[];
  approvals: Approval[];
  diff: string;
  capabilities: string[];
  threadCursor: string | null;
  historyCursor: string | null;
  generation: number;
  notices: Notice[];
  claudeBinary: BinaryInfo | null;
  claudeInfo: any;
  engines: {
    codexFound?: boolean;
    claudeFound?: boolean;
    claudePath?: string | null;
    gitBash?: string | null;
    gitBashBundled?: boolean;
  } | null;
};
export const call = <T = void>(
  command: string,
  args?: Record<string, unknown>,
) => invoke<T>(command, args);

/** The fields save_preferences accepts, taken from a config. */
export const prefsOf = (c: Config) => ({
  theme: c.theme,
  fontSize: c.fontSize,
  model: c.model,
  sandbox: c.sandbox,
  approvalPolicy: c.approvalPolicy,
  reasoningEffort: c.reasoningEffort,
  reasoningSummary: c.reasoningSummary,
  serviceTier: c.serviceTier,
  layout: c.layout,
  sidebarWidth: c.sidebarWidth,
  inspectorWidth: c.inspectorWidth,
  debugLog: c.debugLog,
  claudeModel: c.claudeModel,
  claudeEffort: c.claudeEffort,
  claudeProjectSettings: c.claudeProjectSettings,
  composerWidth: c.composerWidth,
  composerHeight: c.composerHeight,
  showInspector: c.showInspector,
  autoConnect: c.autoConnect,
  proxy: c.proxy,
});

export const PROVIDER_NAME: Record<Provider, string> = {
  codex: "Codex",
  claude: "Claude",
};
export const ACCOUNT_KIND: Record<Provider, string> = {
  codex: "ChatGPT",
  claude: "Claude",
};

/** Plain-language permission presets. Each maps to the sandbox + approval pair both agents understand. */
export const PRESETS = [
  {
    id: "ask",
    label: "Ask before changes",
    sandbox: "read-only",
    approval: "on-request",
    hint: "Reads your project and explains. Asks you before it edits a file or runs a command.",
  },
  {
    id: "edit",
    label: "Edit files, ask for commands",
    sandbox: "workspace-write",
    approval: "on-request",
    hint: "Can change files inside this project. Still asks before running commands.",
  },
  {
    id: "auto",
    label: "Work without asking",
    sandbox: "workspace-write",
    approval: "never",
    hint: "Edits and runs commands without asking. Only for projects you trust.",
  },
  {
    id: "full",
    label: "Full access (advanced)",
    sandbox: "danger-full-access",
    approval: "never",
    advanced: true,
    hint: "No limits, even outside the project. Only for teachers who know the risks.",
  },
] as const;
export const presetOf = (c: Pick<Config, "sandbox" | "approvalPolicy">) =>
  PRESETS.find((p) => p.sandbox === c.sandbox && p.approval === c.approvalPolicy)
    ?.id ?? "custom";

/** Turns raw errors into a short title, a next step, and the button that helps. */
export function explain(
  raw: string,
  provider: Provider,
): { title: string; hint: string; action?: "signin" | "accounts" | "advanced" } {
  const t = raw || "";
  const name = PROVIDER_NAME[provider];
  if (/not logged in|authentication_failed|unauthori[sz]ed|\b401\b|invalid api key|please run \/login|sign in again|requiresOpenaiAuth/i.test(t))
    return {
      title: "You're not signed in",
      hint: `Sign in to your ${ACCOUNT_KIND[provider]} account to use ${name}.`,
      action: "signin",
    };
  if (/rate.?limit|usage limit|limit reached|\b429\b|quota|usage_limit/i.test(t))
    return {
      title: "Usage limit reached",
      hint: "Wait until your limit resets (see the usage meter), or switch to another of your own accounts.",
      action: "accounts",
    };
  if (/certificate|self.signed|UNABLE_TO_VERIFY|CERT_|unknown issuer|tls/i.test(t))
    return {
      title: "The school network is checking secure connections",
      hint: "Ask IT for the school's security certificate (.pem) and add it in Settings → Advanced → Network.",
      action: "advanced",
    };
  if (/ENOTFOUND|ECONNREFUSED|ECONNRESET|ETIMEDOUT|network|timed out|proxy|dns|could not connect|unable to connect|fetch failed/i.test(t))
    return {
      title: `Can't reach ${name}`,
      hint: "Check the internet connection. If your school uses a proxy, set it in Settings → Advanced → Network.",
      action: "advanced",
    };
  if (/requires a shell tool|git.?bash|PowerShell tool is disabled/i.test(t))
    return {
      title: "No command shell available",
      hint: "Claude can still read and edit files. To run commands, ask your teacher for the copy that includes PortableGit.",
    };
  if (/os error 1260|os error 4551|blocked by group policy|Application Control|AppLocker|os error 5\b|Access is denied/i.test(t))
    return {
      title: "Windows blocked a program",
      hint: "School security rules stopped the AI program from starting. Ask IT to allow this app folder.",
    };
  if (/os error 112|not enough space|disk full|ENOSPC/i.test(t))
    return { title: "The disk is full", hint: "Free some space, then try again." };
  if (/not included in this copy|claude\.exe is missing|codex\.exe is missing/i.test(t))
    return { title: `${name} is not set up`, hint: t, action: "advanced" };
  return { title: "Something needs attention", hint: "" };
}

export const percentLevel = (p: number) =>
  p >= 90 ? "high" : p >= 70 ? "mid" : "low";

export function windowLabel(w?: UsageWindow | null, fallback = "") {
  const m = w?.windowMins;
  if (!m) return fallback;
  if (m <= 360) return `${Math.round(m / 60)}-hour`;
  if (m >= 10000) return "Weekly";
  if (m >= 1440) return `${Math.round(m / 1440)}-day`;
  return `${Math.round(m / 60)}-hour`;
}

export function resetText(resetsAt?: number | null) {
  if (!resetsAt) return "";
  const d = new Date(resetsAt * 1000);
  const now = Date.now();
  const diff = d.getTime() - now;
  if (diff <= 0) return "resets soon";
  if (diff < 60 * 60 * 1000) return `resets in ${Math.max(1, Math.round(diff / 60000))} min`;
  if (diff < 24 * 60 * 60 * 1000)
    return `resets ${d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}`;
  return `resets ${d.toLocaleDateString([], { weekday: "short", hour: "numeric", minute: "2-digit" })}`;
}

export function ago(secs?: number | null) {
  if (!secs) return "never";
  const diff = Math.max(0, Date.now() / 1000 - secs);
  if (diff < 90) return "just now";
  if (diff < 3600) return `${Math.round(diff / 60)} min ago`;
  if (diff < 86400) return `${Math.round(diff / 3600)} h ago`;
  return `${Math.round(diff / 86400)} days ago`;
}
