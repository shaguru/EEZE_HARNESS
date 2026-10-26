// Developer acceptance probe. Uses an explicitly supplied existing portable CODEX_HOME;
// never copies authentication files and only asks the agent to touch the temporary fixture.
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { mkdir, readFile, writeFile, stat } from "node:fs/promises";
import path from "node:path";
const exe = path.resolve(process.argv[2]);
const home = path.resolve(process.argv[3]);
const root = path.resolve(".cache/live-acceptance");
const project = path.join(root, "project");
await mkdir(project, { recursive: true });
let proc;
let seq = 0;
let pending = new Map();
let events = [];
let decisions = [];
let stream = "";
const send = (v) => proc.stdin.write(JSON.stringify(v) + "\n");
const request = (method, params) =>
  new Promise((resolve, reject) => {
    const id = ++seq;
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(Error(`Timeout: ${method}`));
    }, 45000);
    pending.set(id, { resolve, reject, timer });
    send({ id, method, params });
  });
async function start() {
  const env = { ...process.env, CODEX_HOME: home };
  delete env.OPENAI_API_KEY;
  delete env.CODEX_API_KEY;
  proc = spawn(
    exe,
    [
      "app-server",
      "--listen",
      "stdio://",
      "-c",
      'approval_policy="on-request"',
      "-c",
      'sandbox_mode="read-only"',
      "-c",
      'cli_auth_credentials_store="file"',
    ],
    { cwd: project, env, windowsHide: true, stdio: ["pipe", "pipe", "pipe"] },
  );
  proc.stderr.resume();
  createInterface({ input: proc.stdout }).on("line", (line) => {
    const v = JSON.parse(line);
    if (v.method) {
      events.push(v);
      if (v.method === "item/agentMessage/delta") stream += v.params.delta;
      if (v.id !== undefined) {
        const known = [
          "item/fileChange/requestApproval",
          "item/commandExecution/requestApproval",
        ].includes(v.method);
        const command=v.params.command||'';
        const safeProbe=/^(?:"[A-Za-z]:\\[^"\r\n]*\\(?:pwsh|powershell)\.exe"\s+|(?:pwsh|powershell)(?:\.exe)?\s+)?(?:-(?:NoProfile|NoLogo)\s+)*(?:-Command\s+)?["']?Write-Output PORTABLE_COMMAND_APPROVAL_OK["']?$/i.test(command);
        const decision='decline';
        decisions.push({ method: v.method, decision, command });
        send(
          known
            ? { id: v.id, result: { decision } }
            : {
                id: v.id,
                error: {
                  code: -32601,
                  message: "Unsupported probe capability",
                },
              },
        );
      }
    } else if (pending.has(v.id)) {
      const p = pending.get(v.id);
      pending.delete(v.id);
      clearTimeout(p.timer);
      v.error ? p.reject(Error(JSON.stringify(v.error))) : p.resolve(v.result);
    }
  });
  await request("initialize", {
    clientInfo: { name: "rust_portable_harness_acceptance", version: "0.1.0" },
  });
  send({ method: "initialized", params: {} });
}
async function turn(thread, text) {
  events = [];
  stream = "";
  await request("turn/start", {
    threadId: thread,
    input: [{ type: "text", text }],
  });
  const end = Date.now() + 90000;
  while (Date.now() < end) {
    const completed = events.find((e) => e.method === "turn/completed");
    if (completed)
      return {
        status: completed.params.turn.status,
        text: stream,
        methods: [...new Set(events.map((e) => e.method))],
      };
    await new Promise((r) => setTimeout(r, 100));
  }
  throw Error("Turn completion timed out");
}
const report = {};
try {
  await start();
  const first = await request("thread/start", {
    cwd: project,
    sandbox: "read-only",
    approvalPolicy: "on-request",
    approvalsReviewer: "user",
  });
  const id = first.thread.id;
  const greeting = await turn(
    id,
    "Reply with exactly PORTABLE_RECOVERY_OK. Do not use any tools.",
  );
  report.streaming =
    greeting.status === "completed" &&
    greeting.text.includes("PORTABLE_RECOVERY_OK");
  proc.kill();
  await new Promise((r) => proc.once("exit", r));
  await start();
  await request("thread/resume", {
    threadId: id,
    cwd: project,
    sandbox: "read-only",
    approvalPolicy: "on-request",
    approvalsReviewer: "user",
    excludeTurns: true,
  });
  const history = await request("thread/turns/list", {
    threadId: id,
    limit: 25,
    itemsView: "full",
  });
  report.restartRecovery = JSON.stringify(history).includes(
    "PORTABLE_RECOVERY_OK",
  );
  const denied = await turn(
    id,
    "Acceptance test in this temporary fixture only: use apply_patch to create portable-approval-probe.txt containing PORTABLE_APPROVAL_OK. This is read-only mode, so ask the client for explicit file-change approval before writing. If the request is declined, stop and report the refusal; do not use another tool to bypass it.",
  );
  report.declinedTurn = { status: denied.status, methods: denied.methods };
  const commandProbe=await turn(id,"Local approval acceptance test in this temporary project only: request one harmless shell command, Write-Output PORTABLE_COMMAND_APPROVAL_OK, with sandbox_permissions=require_escalated and justification='Verify client command approval UI'. The client is expected to decline it. If declined, stop immediately. Do not use any alternative execution path or modify files. If escalation is unavailable, explain that without running anything.");
  report.commandProbe={status:commandProbe.status,methods:commandProbe.methods,explanation:commandProbe.text.slice(0,1200)};
  report.approvals = decisions;
  report.fileAfterDecline = await stat(
    path.join(project, "portable-approval-probe.txt"),
  ).then(
    () => true,
    () => false,
  );
  if (report.fileAfterDecline)
    throw Error("File appeared after decline in read-only mode");
  await writeFile(
    path.join(root, "result.json"),
    JSON.stringify(report, null, 2),
  );
  console.log(JSON.stringify(report, null, 2));
} finally {
  proc?.kill();
  for (const p of pending.values()) clearTimeout(p.timer);
}
