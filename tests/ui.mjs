// UI fixture tests only. These snapshots never ship in the application.
import { chromium } from "@playwright/test";
import { writeFile, mkdir } from "node:fs/promises";
const browser = await chromium.launch({ channel: "msedge", headless: true });
const page = await browser.newPage({ viewport: { width: 1366, height: 768 } });
const failures = [];
page.on("pageerror", (e) => failures.push(String(e)));
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
  },
  root: "C:\\Portable",
  binary: {
    path: "C:\\Portable\\codex.exe",
    version: "codex-cli 0.163.0",
    sha256: "fixture",
  },
  models: [
    {
      id: "discovered-model",
      model: "discovered-model",
      displayName: "Discovered model",
    },
  ],
  account: { account: null },
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
      changes: [
        {
          path: "hello.txt",
          kind: { type: "update" },
          diff: "--- a/hello.txt\n+++ b/hello.txt\n@@ -1 +1 @@\n-before\n+after\n",
        },
      ],
    },
  ],
  approvals: [],
  diff: "",
  capabilities: ["thread/start", "thread/archive", "account/login/start"],
  threadCursor: null,
  historyCursor: null,
  generation: 1,
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
      if (command === "snapshot") return structuredClone(window.__fixture);
      if (command === "plugin:event|listen") {
        window.__change = () =>
          callbacks.get(args.handler)?.({ event: args.event, payload: 1 });
        return 1;
      }
      return null;
    },
  };
}, fixture);
try {
  await page.goto("http://127.0.0.1:5174");
  await page.getByRole("heading", { name: "Fixture conversation" }).waitFor();
  if (await page.evaluate(() => window.attacked))
    throw Error("Raw Markdown executed JavaScript");
  await page.getByRole("button", { name: "Example", exact: true }).click();
  await page.getByRole("heading", { name: "Open external link" }).waitFor();
  if (
    (await page.evaluate(() => window.__testCalls)).some(
      (c) => c.command === "open_external",
    )
  )
    throw Error("Link opened before explicit confirmation");
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await page.getByRole("button", { name: "Changes 1", exact: true }).click();
  await page.getByRole("button", { name: /hello.txt/ }).click();
  await page.locator(".monaco-editor").waitFor({ timeout: 20000 });
  await page.getByRole("button", { name: "Close dialog" }).click();
  await page.evaluate(() => {
    window.__fixture.approvals = [
      {
        key: "7",
        id: 7,
        method: "item/commandExecution/requestApproval",
        params: { command: "echo fixture", cwd: "C:\\Test Project" },
        supported: true,
      },
    ];
    window.__fixture.turnId = "turn-fixture";
    window.__fixture.lifecycle = "Waiting for Approval";
    window.__change();
  });
  await page
    .getByRole("button", { name: "Approve once", exact: true })
    .waitFor();
  if (
    (await page.evaluate(() => window.__testCalls)).some(
      (c) => c.command === "decide",
    )
  )
    throw Error("Approval was sent automatically");
  await page.getByRole("button", { name: "Decline", exact: true }).click();
  const calls = await page.evaluate(() => window.__testCalls);
  const decision = calls.find((c) => c.command === "decide");
  if (decision?.args?.key !== "7" || decision?.args?.decision !== "decline")
    throw Error("Wrong approval correlation/decision");
  if (failures.length) throw Error(failures.join("\n"));
  await mkdir(".cache/ui-tests", { recursive: true });
  await page.screenshot({ path: ".cache/ui-tests/approval.png" });
  await writeFile(
    ".cache/ui-tests/result.json",
    JSON.stringify(
      {
        passed: true,
        tests: [
          "Markdown HTML blocked",
          "external link requires explicit action",
          "bundled Monaco patch viewer loads",
          "no automatic approval",
          "decline correlates to request ID",
        ],
        viewport: "1366x768",
        errors: failures,
      },
      null,
      2,
    ),
  );
  console.log("Passed 5 UI fixture checks.");
} finally {
  await browser.close();
}
