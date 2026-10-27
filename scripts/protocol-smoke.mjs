import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
const exe = path.resolve(process.argv[2] || "codex.exe");
const root = path.resolve(process.argv[3] || ".cache/protocol-smoke");
await mkdir(path.join(root, "codex-home"), { recursive: true });
await mkdir(path.join(root, "project"), { recursive: true });
await writeFile(
  path.join(root, "codex-home/config.toml"),
  'cli_auth_credentials_store = "file"\n',
);
const accountHome = process.argv[4]
  ? path.resolve(process.argv[4])
  : path.join(root, "codex-home");
const env = { ...process.env, CODEX_HOME: accountHome };
delete env.OPENAI_API_KEY;
delete env.CODEX_API_KEY;
const child = spawn(exe, ["app-server", "--listen", "stdio://"], {
  cwd: path.join(root, "project"),
  env,
  windowsHide: true,
  stdio: ["pipe", "pipe", "pipe"],
});
const pending = new Map();
let sequence = 0;
const notifications = [];
const send = (v) => child.stdin.write(JSON.stringify(v) + "\n");
const request = (method, params) =>
  new Promise((resolve, reject) => {
    const id = ++sequence;
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(Error(`Timeout: ${method}`));
    }, 20000);
    pending.set(id, { resolve, reject, timer });
    send({ id, method, params });
  });
createInterface({ input: child.stdout }).on("line", (line) => {
  try {
    const v = JSON.parse(line);
    if (v.method) {
      notifications.push(v.method);
      if (v.id !== undefined)
        send({
          id: v.id,
          error: {
            code: -32601,
            message: "Smoke client grants no permissions",
          },
        });
    } else if (pending.has(v.id)) {
      const p = pending.get(v.id);
      pending.delete(v.id);
      clearTimeout(p.timer);
      v.error ? p.reject(Error(JSON.stringify(v.error))) : p.resolve(v.result);
    }
  } catch {}
});
child.stderr.resume();
try {
  const init = await request("initialize", {
    clientInfo: { name: "rust_portable_harness_smoke", version: "0.1.0" },
  });
  send({ method: "initialized", params: {} });
  const account = await request("account/read", { refreshToken: false });
  const models = await request("model/list", {
    limit: 100,
    includeHidden: false,
  });
  const start = await request("thread/start", {
    cwd: path.join(root, "project"),
    approvalPolicy: "on-request",
    sandbox: "read-only",
    approvalsReviewer: "user",
  });
  const thread = start.thread.id;
  await request("turn/start", {
    threadId: thread,
    input: [
      {
        type: "text",
        text: "Reply with exactly PORTABLE_SMOKE_OK. Do not use tools.",
      },
    ],
  });
  const end = Date.now() + 20000;
  while (!notifications.includes("turn/completed") && Date.now() < end)
    await new Promise((r) => setTimeout(r, 100));
  const list = await request("thread/list", {
    cwd: path.join(root, "project"),
    limit: 50,
    sourceKinds: [],
  });
  const read = await request("thread/read", { threadId: thread });
  const history = await request("thread/turns/list", {
    threadId: thread,
    limit: 25,
    sortDirection: "desc",
    itemsView: "full",
  });
  const report = {
    testedAt: new Date().toISOString(),
    handshake: true,
    initializeUserAgent: init.userAgent,
    accountPresent: !!account.account,
    modelCount: models.data?.length,
    threadStarted: !!thread,
    threadListed: list.data?.some((t) => t.id === thread),
    threadRead: read.thread.id === thread,
    historyTurns: history.data?.length,
    notifications: [...new Set(notifications)],
    conversationVerified:
      !!account.account && notifications.includes("item/agentMessage/delta"),
    note: account.account
      ? "Authenticated run"
      : "Fresh CODEX_HOME has no credentials; live generation is expected to return an authentication error.",
  };
  await writeFile(
    path.join(root, "result.json"),
    JSON.stringify(report, null, 2),
  );
  console.log(JSON.stringify(report, null, 2));
} finally {
  child.kill();
  for (const p of pending.values()) clearTimeout(p.timer);
}
