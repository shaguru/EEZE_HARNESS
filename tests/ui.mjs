// UI fixture tests only. These snapshots never ship in the application.
// Start the dev server first: npx vite --host 127.0.0.1 --port 5174
import { chromium } from "@playwright/test";
import { writeFile, mkdir } from "node:fs/promises";
const browser = await chromium.launch({ channel: "msedge", headless: true });
const page = await browser.newPage({ viewport: { width: 1366, height: 768 } });
const failures = [];
const passed = [];
page.on("pageerror", (e) => failures.push(String(e)));
const now = Math.floor(Date.now() / 1000);
const fixture = {
  lifecycle: "Ready",
  error: null,
  warning: null,
  config: {
    schemaVersion: 1,
    projects: ["C:\\Test Project"],
    selectedProject: "C:\\Test Project",
    selectedThread: "thread-fixture",
    binaryPath: null,
    model: null,
    theme: "dark",
    fontSize: 13,
    sandbox: "read-only",
    approvalPolicy: "on-request",
    reasoningEffort: null,
    reasoningSummary: "auto",
    serviceTier: null,
    layout: "bottom",
    sidebarWidth: 235,
    inspectorWidth: 295,
    labels: {},
    debugLog: false,
    provider: "codex",
    accounts: [
      { id: "codex-default", provider: "codex", label: "ChatGPT account", email: "student@school.edu", plan: "edu", usage: { primary: { usedPercent: 42, windowMins: 300, resetsAt: now + 5400 }, secondary: { usedPercent: 18, windowMins: 10080, resetsAt: now + 3 * 86400 } }, usageAt: now - 60 },
      { id: "claude-default", provider: "claude", label: "Claude account", email: null, plan: null, usage: null, usageAt: null },
    ],
    activeCodexAccount: "codex-default",
    activeClaudeAccount: "claude-default",
    claudeBinaryPath: null,
    claudeModel: null,
    claudeEffort: null,
    claudeProjectSettings: false,
    composerWidth: 360,
    composerHeight: 170,
    showInspector: true,
    autoConnect: false,
    proxy: "",
    caCertificate: null,
  },
  root: "C:\\Portable",
  binary: { path: "C:\\Portable\\codex.exe", version: "codex-cli 0.163.0", sha256: "fixture" },
  models: [{ id: "discovered-model", model: "discovered-model", displayName: "Discovered model" }],
  account: { account: { type: "chatgpt", email: "student@school.edu" } },
  login: null,
  threadId: "thread-fixture",
  turnId: null,
  threads: [{ id: "thread-fixture", name: "Fixture conversation" }],
  items: [
    {
      id: "agent-1",
      type: "agentMessage",
      text: "**Safe Markdown**\n\n<script>window.attacked=true</script>\n\n[Example](https://example.com)\n\n```rust\nfn main() {}\n```",
    },
    {
      id: "edit-1",
      type: "fileChange",
      status: "completed",
      changes: [{ path: "hello.txt", kind: { type: "update" }, diff: "--- a/hello.txt\n+++ b/hello.txt\n@@ -1 +1 @@\n-before\n+after\n" }],
    },
  ],
  approvals: [],
  diff: "",
  capabilities: ["thread/start", "thread/archive", "account/login/start"],
  threadCursor: null,
  historyCursor: null,
  generation: 1,
  notices: [],
  claudeBinary: null,
  claudeInfo: null,
  engines: { codexFound: true, claudeFound: true, gitBash: null },
  tools: [
    { id: "python", name: "Python 3.14 (with tkinter)", purpose: "Python programs", downloadMb: 45, diskMb: 110, state: "missing", detail: "" },
    { id: "rust", name: "Rust", purpose: "Rust apps", downloadMb: 380, diskMb: 850, state: "missing", detail: "" },
    { id: "git", name: "Git", purpose: "Version control", downloadMb: 58, diskMb: 360, state: "ready", detail: "PortableGit 2.56" },
    { id: "pwsh", name: "PowerShell 7", purpose: "Shell", downloadMb: 105, diskMb: 250, state: "system", detail: "C:\\Program Files\\PowerShell\\7\\pwsh.exe" },
    { id: "node", name: "Node.js (npm, npx, Convex CLI)", purpose: "Convex", downloadMb: 40, diskMb: 120, state: "missing", detail: "" },
  ],
  templates: [
    { id: "python-tk", name: "Python window app (tkinter)", description: "A small window.", needs: "python" },
    { id: "rust", name: "Rust console app", description: "Console.", needs: "rust" },
    { id: "empty", name: "Empty folder", description: "Just a folder.", needs: "" },
  ],
  jobs: [],
  projectInfo: { run: "uv run main.py", tool: "python", toolReady: true },
  toolsStale: false,
  projectsDir: "C:\\Portable\\projects",
};
await page.addInitScript((fixture) => {
  let id = 0;
  const callbacks = new Map();
  window.__testCalls = [];
  window.__fixture = fixture;
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.__TAURI_INTERNALS__ = {
    transformCallback(fn) {
      callbacks.set(++id, fn);
      return id;
    },
    invoke: async (command, args) => {
      window.__testCalls.push({ command, args });
      const delay = window.__slow?.[command];
      if (delay) await new Promise((r) => setTimeout(r, delay));
      if (command === "snapshot") return structuredClone(window.__fixture);
      if (command === "diagnostics") return { network: "Automatic: no proxy found (direct connection)" };
      if (command === "plugin:event|listen") {
        window.__change = () => callbacks.get(args.handler)?.({ event: args.event, payload: 1 });
        return 1;
      }
      return null;
    },
  };
}, fixture);
const calls = (command) => page.evaluate((c) => window.__testCalls.filter((x) => x.command === c), command);
const clearCalls = () => page.evaluate(() => (window.__testCalls = []));
const update = (fn) => page.evaluate(`(${fn})(window.__fixture); window.__change();`);
const box = (sel) => page.locator(sel).first().boundingBox();
async function check(name, fn) {
  try {
    await fn();
    passed.push(name);
  } catch (e) {
    failures.push(`${name}: ${e.message}`);
  }
}
try {
  await page.goto("http://127.0.0.1:5174");
  await page.getByRole("heading", { name: "Fixture conversation" }).waitFor();

  await check("Markdown HTML blocked", async () => {
    if (await page.evaluate(() => window.attacked)) throw Error("Raw Markdown executed JavaScript");
  });
  await check("external link requires explicit action", async () => {
    await page.getByRole("button", { name: "Example", exact: true }).click();
    await page.getByRole("heading", { name: "Open external link" }).waitFor();
    if ((await calls("open_external")).length) throw Error("Link opened before confirmation");
    await page.getByRole("button", { name: "Cancel", exact: true }).click();
  });
  await check("bundled Monaco patch viewer loads", async () => {
    await page.getByRole("button", { name: "Changes 1", exact: true }).click();
    await page.getByRole("button", { name: /hello.txt/ }).click();
    await page.locator(".monaco-editor").waitFor({ timeout: 20000 });
    await page.getByRole("button", { name: "Close dialog" }).click();
  });
  await check("conversation fills the middle column (layout regression)", async () => {
    const conv = await box(".conversation");
    const insp = await box(".inspector");
    if (!conv || conv.width < 600) throw Error(`conversation too narrow: ${JSON.stringify(conv)}`);
    if (!insp || insp.x < conv.x + conv.width - 1) throw Error("activity panel is not right of the chat");
    const composer = await box(".composer");
    if (!composer || composer.y < conv.y + conv.height / 2) throw Error("bottom composer not at the bottom");
  });
  await check("dock buttons move the message box", async () => {
    await clearCalls();
    await page.getByRole("button", { name: "Message box on the right" }).click();
    const saved = await calls("save_preferences");
    if (saved.at(-1)?.args?.prefs?.layout !== "right") throw Error("layout right not saved");
    const conv = await box(".conversation");
    const composer = await box(".composer");
    if (composer.x < conv.x + conv.width / 2 || composer.height < conv.height * 0.9) throw Error(`right dock wrong: ${JSON.stringify(composer)}`);
  });
  await check("drag the message box header to the left", async () => {
    await clearCalls();
    const head = await box(".composer-head");
    const conv = await box(".conversation");
    await page.mouse.move(head.x + 12, head.y + head.height / 2);
    await page.mouse.down();
    await page.mouse.move(conv.x + 120, conv.y + 200, { steps: 8 });
    await page.locator(".dock-zone.zone-left.hot").waitFor({ timeout: 3000 });
    await page.mouse.up();
    const saved = await calls("save_preferences");
    if (saved.at(-1)?.args?.prefs?.layout !== "left") throw Error("drop on left not saved");
    const composer = await box(".composer");
    if (composer.x > conv.x + 10) throw Error("composer not on the left after drop");
  });
  await check("drag the divider to resize the message box", async () => {
    await clearCalls();
    const split = await box(".composer-split");
    await page.mouse.move(split.x + 3, split.y + 300);
    await page.mouse.down();
    await page.mouse.move(split.x + 103, split.y + 300, { steps: 5 });
    await page.mouse.up();
    const saved = await calls("save_preferences");
    const w = saved.at(-1)?.args?.prefs?.composerWidth;
    if (!(w > 400 && w <= 720)) throw Error(`composer width not saved: ${w}`);
  });
  await check("Enter during a short wait queues the message instead of dropping it", async () => {
    await clearCalls();
    await page.evaluate(() => (window.__slow = { refresh_threads: 1500 }));
    await page.getByRole("button", { name: "Refresh chats" }).click();
    const input = page.getByRole("textbox", { name: "Message Codex" });
    await input.fill("queued hello");
    await input.press("Enter");
    await page.getByRole("button", { name: "Sending…" }).waitFor({ timeout: 1000 });
    await page.waitForFunction(() => window.__testCalls.some((c) => c.command === "send_turn"), null, { timeout: 5000 });
    const sent = (await calls("send_turn"))[0];
    if (sent?.args?.text !== "queued hello") throw Error("queued text not sent");
    await page.evaluate(() => (window.__slow = {}));
  });
  await check("no automatic approval; decline correlates to request ID", async () => {
    await clearCalls();
    await update((f) => {
      f.approvals = [{ key: "7", id: 7, method: "item/commandExecution/requestApproval", params: { command: "echo fixture", cwd: "C:\\Test Project" }, supported: true }];
      f.turnId = "turn-fixture";
      f.lifecycle = "Waiting for Approval";
    });
    await page.getByRole("button", { name: "Approve once", exact: true }).waitFor();
    if ((await calls("decide")).length) throw Error("Approval was sent automatically");
    await page.getByRole("heading", { name: "Codex wants to run a command" }).waitFor();
    await page.getByRole("button", { name: "Decline", exact: true }).click();
    const d = (await calls("decide"))[0];
    if (d?.args?.key !== "7" || d?.args?.decision !== "decline") throw Error("Wrong approval correlation/decision");
    await update((f) => {
      f.approvals = [];
      f.turnId = null;
      f.lifecycle = "Ready";
    });
  });
  await check("Claude permission request is explained and answered by key", async () => {
    await clearCalls();
    await update((f) => {
      f.config.provider = "claude";
      f.approvals = [{ key: "claude:req-1", id: "req-1", method: "claude/canUseTool", params: { toolName: "Bash", input: { command: "del notes.txt", description: "Delete notes" } }, supported: true }];
      f.turnId = "claude-turn-1";
      f.lifecycle = "Waiting for Approval";
    });
    await page.getByRole("heading", { name: "Claude wants to run a command" }).waitFor();
    await page.getByText("del notes.txt").first().waitFor();
    if ((await calls("decide")).length) throw Error("Claude approval sent automatically");
    await page.getByRole("button", { name: "Approve once", exact: true }).click();
    const d = (await calls("decide"))[0];
    if (d?.args?.key !== "claude:req-1" || d?.args?.decision !== "accept") throw Error("wrong Claude decision");
    await update((f) => {
      f.approvals = [];
      f.turnId = null;
      f.lifecycle = "Ready";
    });
  });
  await check("sign-in errors become a plain message with a Sign in button", async () => {
    await update((f) => {
      f.error = "Not logged in · Please run /login (authentication_failed)";
      f.account = { account: null, loggedIn: false };
    });
    await page.getByText("You're not signed in").waitFor();
    await page.locator(".notice.error").getByRole("button", { name: "Sign in" }).click();
    await page.getByRole("heading", { name: "Accounts & usage" }).waitFor();
  });
  await check("accounts panel lists both kinds, usage, and adds an account", async () => {
    await page.getByRole("heading", { name: /ChatGPT accounts/ }).waitFor();
    await page.getByRole("heading", { name: /Claude accounts/ }).waitFor();
    await page.locator(".accounts-dialog .usage-line").first().waitFor();
    await clearCalls();
    await page.locator(".accounts-dialog .account-section").nth(1).getByRole("button", { name: "＋ Add account" }).click();
    await page.getByPlaceholder(/Name, e.g./).fill("Second Claude");
    await page.getByRole("button", { name: "Add", exact: true }).click();
    const add = (await calls("add_account"))[0];
    if (add?.args?.provider !== "claude" || add?.args?.label !== "Second Claude") throw Error("add_account args wrong");
    await page.getByRole("button", { name: "Close dialog" }).click();
    await update((f) => {
      f.error = null;
    });
  });
  await check("advanced options live in a separate panel", async () => {
    await page.getByRole("button", { name: "Settings", exact: true }).first().click();
    await page.getByRole("heading", { name: /What can (Codex|Claude) do?/ }).waitFor();
    if (await page.getByText("Proxy", { exact: true }).count()) throw Error("network options leaked into simple settings");
    await page.getByRole("button", { name: "Advanced…" }).click();
    await page.getByRole("heading", { name: "Advanced settings" }).waitFor();
    await page.getByText("Automatic: no proxy found").waitFor();
    await page.getByRole("button", { name: "Close dialog" }).click();
  });
  await check("Codex read-only explains itself and one click allows edits", async () => {
    await update((f) => {
      f.config.provider = "codex";
      f.config.sandbox = "read-only";
      f.config.approvalPolicy = "on-request";
      f.error = null;
    });
    await page.getByText("Read only: Codex can explain but cannot save any file.").waitFor();
    await clearCalls();
    await page.getByRole("button", { name: "Let Codex edit this project" }).click();
    const saved = (await calls("save_preferences")).at(-1);
    if (saved?.args?.prefs?.sandbox !== "workspace-write") throw Error("edit mode not saved");
    await update((f) => {
      f.config.sandbox = "workspace-write";
    });
    await page.waitForFunction(() => document.querySelector(".preset-select option:checked")?.textContent === "Edit files in this project", null, { timeout: 5000 });
    if (await page.getByText("Read only: Codex can explain").count()) throw Error("banner still shown in edit mode");
  });
  await check("Run and Terminal open consoles for the project", async () => {
    await clearCalls();
    await page.getByRole("button", { name: "▶ Run" }).click();
    await page.getByRole("button", { name: "Terminal", exact: true }).click();
    if (!(await calls("run_project")).length || !(await calls("open_terminal")).length) throw Error("run/terminal not called");
  });
  await check("Programming tools: get a tool and watch its progress", async () => {
    await clearCalls();
    await page.getByRole("button", { name: "Programming tools", exact: true }).click();
    await page.getByRole("heading", { name: "Programming tools" }).waitFor();
    await page.locator(".tool-card", { hasText: "Python 3.14" }).getByRole("button", { name: /Get · 45 MB/ }).click();
    const call = (await calls("install_tools"))[0];
    if (JSON.stringify(call?.args?.ids) !== '["python"]') throw Error("install_tools args wrong");
    await update((f) => {
      f.jobs = [{ id: "tools", title: "Getting Python", state: "running", percent: 42, message: "Python: Downloading uv: 6 of 14 MB", log: [], result: null }];
    });
    await page.locator(".tools-dialog .job-card").getByText("42%").waitFor();
    await page.getByRole("button", { name: "Close dialog" }).click();
    await page.locator(".job-tray .job-card").waitFor();
    await update((f) => {
      f.jobs = [];
      f.tools[0].state = "ready";
    });
  });
  await check("New project: pick a type, create, then open it", async () => {
    await clearCalls();
    await page.getByRole("button", { name: "＋ New project" }).click();
    await page.getByRole("heading", { name: "New project" }).waitFor();
    await page.getByPlaceholder("For example: my-game").fill("My Game");
    await page.locator(".template", { hasText: "Rust console app" }).click();
    await page.getByText("This project type needs").waitFor();
    if (!(await page.getByRole("button", { name: "Create project" }).isDisabled())) throw Error("create allowed without Rust");
    await page.locator(".template", { hasText: "Python window app" }).click();
    await page.getByRole("button", { name: "Create project" }).click();
    const created = (await calls("create_project"))[0];
    if (created?.args?.name !== "My Game" || created?.args?.template !== "python-tk" || created?.args?.git !== true) throw Error(`create args: ${JSON.stringify(created?.args)}`);
    await update((f) => {
      f.jobs = [{ id: "project", title: "Creating My Game", state: "done", percent: 100, message: "Your project is ready.", log: [], result: "C:\\Portable\\projects\\My Game" }];
    });
    await page.waitForFunction(() => window.__testCalls.some((c) => c.command === "reconnect_project"), null, { timeout: 5000 });
    const opened = (await calls("reconnect_project"))[0];
    if (!opened?.args?.path?.endsWith("My Game")) throw Error("new project not opened");
    await update((f) => {
      f.jobs = [];
    });
  });
  await check("Restart prompt appears when Codex predates new tools", async () => {
    await clearCalls();
    await update((f) => {
      f.toolsStale = true;
    });
    await page.getByRole("button", { name: "Restart Codex" }).click();
    if (!(await calls("restart_ai")).length) throw Error("restart_ai not called");
    await update((f) => {
      f.toolsStale = false;
    });
  });
  await check("lab monitor size: no horizontal scroll, activity becomes a toggle", async () => {
    await page.setViewportSize({ width: 1093, height: 590 });
    await page.waitForTimeout(200);
    const scrollW = await page.evaluate(() => document.documentElement.scrollWidth);
    if (scrollW > 1093) throw Error(`horizontal scroll: ${scrollW}`);
    if (await page.locator(".inspector").count()) throw Error("inspector should be hidden on narrow screens");
    await page.getByRole("button", { name: "Show activity panel" }).click();
    await page.locator(".inspector.drawer").waitFor();
    await page.locator(".drawer-scrim").click();
    await page.setViewportSize({ width: 1366, height: 768 });
  });
  await mkdir(".cache/ui-tests", { recursive: true });
  await page.screenshot({ path: ".cache/ui-tests/final.png" });
} finally {
  await writeFile(
    ".cache/ui-tests/result.json",
    JSON.stringify({ passed: failures.length === 0, tests: passed, failures, viewport: "1366x768" }, null, 2),
  ).catch(() => {});
  await browser.close();
}
if (failures.length) {
  console.error(`Failed ${failures.length} UI fixture checks:\n${failures.join("\n")}`);
  process.exit(1);
}
console.log(`Passed ${passed.length} UI fixture checks.`);
