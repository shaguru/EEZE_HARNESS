# RustPortableHarness — Build Specification for Codex

**Project:** RustPortableHarness (working name)  
**Target:** Windows 10/11 x64, standard non-administrator user  
**Delivery:** Portable graphical Codex client that runs from a Desktop folder without a Microsoft Store installation, MSI installer, Node.js, npm, Rust, or administrator privileges on the *target* machine.  
**Implementation platform:** Develop and compile on a separate Windows development computer with normal build-tool access. The school computer only runs the release artifacts.

## 0. Mission and non-negotiable requirements

Implement a usable, polished, lightweight graphical harness for the **official OpenAI Codex CLI binary** `codex-x86_64-pc-windows-msvc.exe` (packaged in the release as `codex.exe`). The harness should provide a simplified T3/IDE-like agent workflow: projects, threads, streaming chat, agent activity, file changes, diffs, approvals, and model/configuration controls.

The app **must use the Codex App Server protocol**, not scrape terminal ANSI output and not implement its own LLM API calls. The bundled Codex executable is the actual agent, authentication client, and policy enforcement layer.

Success means the user can unzip a folder onto a school Desktop, double-click `RustPortableHarness.exe`, choose an accessible project folder, authenticate using supported Codex authentication, chat with the agent, review its proposed changes, approve or reject execution/file-change requests, and restart the app with conversations restored—**without installing** the harness or Codex CLI.

### Non-negotiable constraints

1. Do not require elevation, registry writes, machine-wide PATH modification, Windows Store, installer execution, or system services.
2. Do not require Node.js, Python, `cargo`, Visual Studio, or CLI installation on the target machine. They are development/build prerequisites only.
3. Keep user-owned harness data alongside the executable in `./data/`, using paths anchored to the app executable rather than the current working directory. Set `CODEX_HOME` for the child process to the portable data folder (`./data/codex-home/`). Confirm supported behavior with the bundled Codex version.
4. Codex may still require access to the internet and an eligible sign-in/API account; portability does **not** mean offline inference.
5. Do not bundle account tokens or credentials; never commit authentic user data. Use supported Codex sign-in flows, and recognize some managed machines may block authentication redirects.
6. Respect Windows security policy. Do not bypass AppLocker, endpoint protection, proxy restrictions, school policy, or Codex sandbox requirements.
7. Make the build **portable as tested**, not merely a `tauri build` installer output. On an appropriately provisioned target, the released folder should run via double-click. Verify dependency on WebView2; provide an alternative as described below when the runtime is missing.
8. Fail safely: never silently enable unconstrained command execution, unrestricted workspace access, or auto-approvals.
9. Prioritize a reliable MVP over an enormous nonfunctional shell. Do not stub core actions and mark them complete.

## 1. Technology decision

### Preferred: Tauri 2 + Rust + TypeScript

- **Backend:** Rust; Tokio async process management; `serde`/`serde_json` for protocol frames; typed commands/events into the UI; SQLite or simple JSON for local app metadata.
- **Frontend:** Vite + TypeScript + React (or lightweight vanilla TypeScript if simpler); clean responsive desktop-style UI; CSS variables for themes.
- **Editor and diffs:** Monaco Editor / Monaco Diff Editor, bundled at build time, not loaded from a remote CDN.
- **Codex integration:** spawn `codex.exe app-server` as a child process and communicate via line-delimited JSON-RPC over piped stdin/stdout. Parse stderr separately for diagnostics.
- **Transport boundary:** only Rust talks to the Codex child. The frontend does not directly control shell execution or raw filesystem paths.
- **Offline UI:** all JS/CSS/fonts bundled into the desktop release. An internet connection is used for Codex authentication and model requests, not for loading the UI.

### WebView2 dependency: important

Tauri on Windows uses **Microsoft Edge WebView2 Runtime**. It is commonly installed on Windows 10/11, but not guaranteed on locked-down machines. A Tauri executable cannot honestly be called self-contained if it needs a missing WebView2 runtime. Implement:

- A startup check and readable error explaining the missing runtime.
- **Primary portable release:** standalone Tauri executable + locally bundled `codex.exe`; works only where compatible WebView2 already exists.
- **Fallback release:** optionally provide a separate portable Windows build including a *licensed redistributable fixed-version WebView2 runtime* using supported Tauri/Microsoft packaging/documentation, if a portable launch is verifiably possible with the specific Tauri version. Do not incorrectly assume installer-specific `fixedRuntime` settings automatically make a copied EXE portable. Test from an uninstalled ZIP distribution.
- If the fixed-runtime route proves impractical, clearly label the dependency and document a pure Rust GUI alternative (for example `egui`/`eframe`) as a **future separate client**, using the same Rust app-server backend. Do not switch UI stacks mid-MVP unless the target machine lacks WebView2.

## 2. Folder layout and portable state

Release ZIP contents:

```text
RustPortableHarness/
  RustPortableHarness.exe
  codex.exe
  data/
    config.json                 # created first run
    codex-home/                 # CODEX_HOME for Codex state/auth/config
    app.db                      # optional: local project/index/UI metadata
    logs/
    temp/
  projects/                     # optional convenience folder; project can live anywhere user can access
  README-PORTABLE.md
  THIRD-PARTY-LICENSES.txt
```

Runtime rules:

- Resolve root with `std::env::current_exe()?.parent()`; avoid assuming `C:\Users\...\Desktop` or current working directory.
- Prefer existing `./codex.exe`. Allow an advanced setting for an **explicit** alternate binary path. Show executable version, hash, and path in Settings.
- Create required portable subdirectories with helpful errors on denied permissions or read-only drives.
- Set child `CODEX_HOME` to the resolved portable `data/codex-home`. Set `current_dir` to the selected project working directory, *not* to the app installation root.
- Add child-process environment variables only as needed; do not contaminate global/user environment.
- Do not recursively copy arbitrary user project folders into `data`.
- Store recent projects as canonical absolute paths and handle renamed, deleted, or missing folders gracefully.
- Do not persist raw authentication secrets in `config.json` or application logs.
- On project move, ask the user whether to reconnect rather than silently creating/rewriting paths.

## 3. Codex App Server — authoritative integration

Consult the official documentation and the checked-in Codex version **before** writing protocol models:

- https://learn.chatgpt.com/docs/app-server
- https://openai.com/index/unlocking-the-codex-harness/
- https://github.com/openai/codex
- https://github.com/openai/codex/releases

**Do not guess the full RPC object shapes or hard-code one model name.** The App Server protocol evolves. Generate JSON Schema and/or TypeScript protocol types from the exact Codex executable version used for this release:

```powershell
.\codex.exe --version
.\codex.exe app-server generate-json-schema --out .\generated\json-schema
.\codex.exe app-server generate-ts --out .\generated\ts
```

If the CLI flags or generated files differ, use `codex.exe app-server --help`, check the current docs/source, and adapt the commands. Commit the generated schema (or versioned subset) and record the Codex version in build metadata. Generate Rust message types or use validated `serde_json::Value` at the boundary until stable bindings are established.

### Lifecycle

1. Spawn `codex.exe app-server` with piped stdin/stdout/stderr and inherited or appropriately set environment.
2. Start independent async tasks to consume stdout and stderr continuously. Stdout is protocol JSONL: **one complete JSON object per line**, with sensible maximum frame length and error handling. Never emit debug text to the child's stdin and never interpret stderr as RPC.
3. Send `initialize` with valid `clientInfo`, **wait for its response**, then send the `initialized` notification. Nothing else should be sent first.
4. Request model/auth/config capabilities where supported, especially `model/list` and relevant account/status methods obtained from the generated schema.
5. Create a thread using `thread/start`, or load/resume known threads using the supported thread APIs. Pass the selected project's working directory using the schema-supported field(s).
6. Send a user request with `turn/start`, with the current thread ID and correctly structured input payload obtained from generated types.
7. Consume notifications (`turn/started`, `item/started`, streaming agent text deltas, item completion, command/file-change status, `turn/completed`, etc.). Render incrementally. Never assume events arrive in a perfectly simplistic order.
8. Support `turn/interrupt`, only when supported/valid for the active turn.
9. Support server-originated **requests requiring a client response**, including command approvals and file-change approvals: record request ID, show the exact requested operation, collect an explicit user decision, and reply with the **schema-defined response** to the request ID. Do not treat these as one-way notifications.
10. On shutdown, stop accepting input, persist local UI state, interrupt/terminate outstanding processes gracefully, and prevent orphaned child processes when possible.

### JSON-RPC plumbing

- Assign unique monotonically increasing request IDs per child connection. Maintain a map of `id -> oneshot response channel`, with timeout/cancellation handling.
- Distinguish `request` (has id and method), `notification` (method, no id), and `response` (id and result/error). Handle server-initiated requests as first-class operations.
- Serialize writes through **one** owned stdin writer channel to avoid interleaved JSON.
- Keep frontend UI event dispatch separate from the RPC request/response loop.
- Avoid losing streaming events during UI remounts: backend state should be authoritative; frontend can subscribe and request snapshots.
- Treat incoming protocol values as untrusted content: bound size, validate shape, escape display strings, and avoid arbitrary HTML injection.
- Implement structured errors: spawn failure, protocol version mismatch, invalid JSON, handshake failure, auth failure, network error, app-server exit, denied approval, missing workspace, unsupported RPC method, and interrupted turn.
- Use a lifecycle enum similar to `Stopped / Starting / Handshaking / Ready / Busy / Recovering / Failed`.
- On unexpected child exit, show the reason and provide **Restart Codex** action; on restart reconcile thread state using supported APIs, without duplicating an in-flight command.

### MVP API operations to verify from exact schema

- Initialization handshake.
- Model listing and selection.
- Authentication status and login workflow, if exposed; otherwise present Codex's official supported CLI login flow through a clearly labeled user-initiated action.
- Thread start/list/read/resume and archival if available.
- Turn start/interruption and completion.
- Streaming agent messages, command items, file-change items, and error notifications.
- Approval request/decision mechanisms and the specific approval options supported by the binary.

Implement a `ProtocolCapabilities` layer so UI controls disappear or disable when the bundled Codex version lacks a method. Do not silently ignore an unsupported operation.

## 4. UX design and application layout

Design for an instructor using a locked-down classroom computer. Minimal, fast, no giant hero section or decorative dashboards. Default window around 1200 × 800, resizable; works adequately on 1366 × 768.

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ RustPortableHarness   [Project ▼]  [Model ▼]  [Sandbox]  [Settings ⚙]      │
├────────────────┬────────────────────────────────────┬────────────────────────┤
│ Projects       │ Chat / Agent                        │ Changes / Activity     │
│                │                                     │                        │
│ + Add folder   │ User / assistant messages           │ Changed files          │
│ > Project A    │ streaming text and tool events      │ Diff inspector         │
│   Thread 1     │                                     │ Command log            │
│   Thread 2     │                                     │ Approvals              │
│ > Project B    │                                     │                        │
│                │                                     │                        │
├────────────────┴────────────────────────────────────┴────────────────────────┤
│ Message Codex...                           [Attach] [Stop] [Send]           │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Interaction essentials

**Sidebar:** add/open local project folder (native picker); show threads for selected project; create thread; rename local display label; restore project selection after restart. Persist thread IDs but treat Codex as thread content authority.

**Conversation:** Markdown rendering for agent messages (sanitize), fenced code blocks with Copy, streaming text with stable scrolling and manually controlled scroll lock. Show explicit stages (thinking/activity vs output) without pretending private internal reasoning is exposed. Include Stop button and keyboard shortcut Ctrl+Enter to send.

**Agent activity:** visually distinct entries for shell commands, file edits, tool calls, warnings and errors; expanding item shows command, working directory, exit status, stdout/stderr **when supplied by server**. Never invent results or assume tool activity is already authorized.

**Change review:** list modified files by relative path; show unified or side-by-side diffs. Diffs must reflect true before/after content or authoritative Codex file-change data; do not fabricate them by guessing. Support copying a patch and refreshing changed files. Any undo/discard option must be explicit, scoped, safe, and tested against unrelated changes; defer full rollback to V2 if needed.

**Approvals:** focused confirmation pane showing the requested action and affected paths/commands, with full text expandable. Use exactly the decision options allowed by the protocol. Never auto-approve a request just because the app has focus or user selected a permissive UI mode.

**Settings:** theme (dark/light/system), font size, Codex executable path, version indicator, workspace/portable storage path, selected model, supported sandbox/approval modes, debug log toggle, sign-in status/logout if exposed, and a diagnostic environment panel.

**Status bar:** Connected / Busy / Waiting for Approval / Auth Required / Codex Crashed, plus current project and version. Provide actionable messages rather than raw stack traces.

### Design tokens

- Dense, modern developer-tool style, minimal borders and 8px spacing rhythm.
- Sidebar ~230px; right inspector ~330px; collapsible sidebars for small screens.
- Monospace font for code, readable sans-serif for UI, CSS-based responsive layout.
- Accessible focus rings, keyboard navigation, distinguishable warning/approval states, color contrast suitable for classroom projection.
- Avoid fake buttons, placeholder animations, or complex motion effects.

## 5. Authentication and trust

- Codex manages its own model-provider authentication. The harness should invoke **documented App Server account authentication** when available, and guide the user through the legitimate browser/device-code workflow.
- If only CLI login is supported for the shipped version, offer a clearly marked **Sign in with Codex CLI** launch action or copyable command, using the bundled executable and same `CODEX_HOME`. Capture only sanitized status and do not log secrets.
- Prefer OS/native safe credential behavior implemented by Codex. Keeping `CODEX_HOME` in the portable folder may make authentication files portable as well; **warn that the folder may contain sensitive account state**. Do not recommend copying logged-in `data/codex-home` between shared or public computers.
- Provide a **Sign out / Wipe portable account state** flow with explicit confirmation, ensuring app and child are stopped before removal. Only delete known harness-owned paths; never delete user projects.
- Document school policy and security implications. A user's ability to launch an EXE does not establish that the school permits development agents or all filesystem operations.

## 6. Sandbox, file access, and approval security

- Default to the most restrictive useful sandbox and approval configuration supported by the released Codex binary. Do not choose danger/full-access overrides implicitly.
- Only access folders the user chooses; project picker validates path existence and reasonable permissions.
- Protect against path traversal and surprising workspace changes in local harness commands; use canonical paths and avoid string-prefix security checks.
- Show process commands exactly as provided and request confirmation through correct protocol pathways.
- Sanitize Markdown/links; do not let model-generated content invoke native Tauri commands, execute JavaScript, or read arbitrary local files through rendering.
- Tauri capability permissions should be allowlisted. No generic `run-shell-command` frontend API, no open-ended filesystem bridge, no arbitrary user-supplied binary invocation from HTML.
- Do not expose an unauthenticated local HTTP/WebSocket server. Stdio to a child process is preferred.
- When showing a link, make URL destination visible/confirmable if it could leave the application.
- Network-enabled agent actions may send source files to remote services; document this clearly and respect school/client data policies.

## 7. Implementation modules / suggested repository structure

```text
rust-portable-harness/
  README.md
  RustPortableHarness.md
  package.json
  vite.config.ts
  index.html
  src/
    main.tsx
    App.tsx
    components/
      Sidebar.tsx
      Conversation.tsx
      ChatComposer.tsx
      ActivityPanel.tsx
      DiffPanel.tsx
      ApprovalDialog.tsx
      SettingsDialog.tsx
      StatusBar.tsx
    state/
      appStore.ts
    styles/
      theme.css
  src-tauri/
    Cargo.toml
    build.rs
    tauri.conf.json
    capabilities/
      default.json
    src/
      main.rs
      lib.rs
      state.rs
      commands.rs
      portable_paths.rs
      process_manager.rs
      rpc_transport.rs
      protocol.rs
      codex_client.rs
      approvals.rs
      auth.rs
      project_store.rs
      diagnostics.rs
  generated/
    json-schema/
    ts/
  tests/
    fixtures/
    protocol_replay/
  scripts/
    build-portable.ps1
    smoke-test.ps1
    verify-release.ps1
  docs/
    ARCHITECTURE.md
    PROTOCOL-VERSION.md
    SECURITY.md
    TROUBLESHOOTING.md
  LICENSE
```

Structure may change if the design improves, but preserve separation between process ownership, JSON-RPC, domain state, storage, and UI. The app must build and run—no large scaffolding project with unimplemented handlers.

### Rust details

- `process_manager.rs`: locate child binary, set environment and cwd, spawn, restart, stop, monitor lifecycle; Windows process cleanup on parent termination when feasible (Job Objects are an option; use a safe tested implementation).
- `rpc_transport.rs`: Tokio buffered JSONL reader/writer, `id` correlation, response/request/notification dispatch, timeouts, frame-size limits, redaction.
- `codex_client.rs`: typed domain operations and capability discovery, handshake, models, threads, turns, approvals.
- `approvals.rs`: pending server-initiated RPC requests with exact decision schema, cancellation on process exit, UI notification.
- `project_store.rs`: persisted project list and UI metadata; atomic writes/backups or SQLite migrations. Codex remains source of truth for thread contents.
- `commands.rs`: narrow Tauri commands: select/add project, get snapshot, start/resume thread, send turn, interrupt, decide approval, select model, auth action, diagnostics, restart backend.
- `state.rs`: shared synchronized state; keep expensive operations off UI thread.

### Frontend details

- Store state centrally (Zustand or lightweight reducer), but avoid duplicate competing copies of active turn state.
- Subscribe to typed backend events; include a resync/snapshot method after app reload.
- Coalesce rapid token-delta UI updates to prevent renderer jank without losing text.
- Monaco and Markdown assets are bundled. Handle code-heavy projects without loading every file into memory.
- Persist panel sizes, collapsed sections, and theme within portable settings.

## 8. Build and distribution

**Develop on personal machine** with Rust stable MSVC toolchain, MSVC build tools, Node.js LTS, and Tauri prerequisites. Build once, copy output ZIP to school Desktop.

Example developer workflow (adjust to the actual selected package manager):

```powershell
npm ci
npm run build
npx tauri build
```

**These commands are developer-only.** Do not tell the school user to run them.

Implement `scripts/build-portable.ps1` to:

1. Validate Rust, Node, and Tauri development prerequisites.
2. Build a Windows x64 **release** executable without depending on an installer to deploy it.
3. Stage `RustPortableHarness.exe` into `dist-portable/RustPortableHarness/`.
4. Copy the verified official Windows x64 Codex executable to that folder and name it `codex.exe`; check PE architecture and executable version. Never download from unofficial mirrors or silently fetch an executable at app startup.
5. Stage README, licenses, blank writable data directory, and any legitimately redistributable runtime files needed for the chosen packaging mode.
6. Do not stage any developer `CODEX_HOME`, cookies, login data, session tokens, or private projects.
7. Produce `RustPortableHarness-win-x64-portable.zip` plus a SHA-256 checksums file.
8. Print release paths and sizes; invoke verification/smoke tests.

A Tauri bundler-generated MSI/NSIS installer is **not** the portable artifact. It is fine to produce installers as optional outputs, but the ZIP must work independently.

### Target-machine smoke test

On Windows standard user account, without administrator privileges:

```powershell
cd "$env:USERPROFILE\Desktop\RustPortableHarness"
.\codex.exe --version
.\RustPortableHarness.exe
```

Verify WebView2 availability, no prompts for administrator or Store, writable data path, first-run initialization, sign-in flow, project selection, streaming output, change review, and graceful shutdown. Check app use both when launched by double click and from PowerShell.

## 9. Implementation phases — work in this order

### Phase 1 — Functional vertical slice (highest priority)

- Initialize repo and Tauri/TypeScript UI.
- Validate companion Codex executable and set `CODEX_HOME`.
- Spawn App Server and complete handshake.
- Select folder, start thread, send prompt, render streaming response, show basic errors.
- Build a tested Windows portable ZIP.

**Exit criterion:** user can have one complete Codex conversation from an uninstalled Desktop folder.

### Phase 2 — Practical daily driver

- Model listing/selection, persistent projects and thread resume.
- Activity and tool/command event viewer.
- Correct interactive approvals.
- Stop/interrupt, reconnect/error recovery, crash state.
- Proper theme, markdown, code blocks, keyboard shortcuts.

**Exit criterion:** a realistic project change requiring command approval is usable and transparent.

### Phase 3 — Change review and robustness

- Monaco diff viewer linked to actual changed files.
- Reopen sessions, project switching, empty/error states.
- Diagnostics/export sanitized logs, configuration reset, portable-state warnings.
- Large-output protection, protocol compatibility handling, schema tests.

**Exit criterion:** app survives restart, subprocess failure, invalid protocol frame, and non-admin restrictions without losing unrelated files or leaving confusing UI states.

### Phase 4 — Optional enhancements (only after MVP passes)

- Workspace tree/editor; searchable history; tabs for multiple simultaneous threads; multiple-agent orchestration; Git panel; snapshot previews; command filters; import/export sanitized transcripts; shortcuts palette.
- Optional standalone `egui` frontend for machines missing WebView2.
- Optional localization to English and Vietnamese.

Avoid building a broad agent framework or plug-in system before the above is working.

## 10. Tests and acceptance criteria

Create automated tests wherever possible and a human QA checklist.

### Backend unit/integration tests

- JSONL framing: fragmented lines, multiple lines per read, Unicode, max frame limits, malformed JSON.
- Request/response matching: concurrent requests, out-of-order responses, timeout, duplicate IDs, server-initiated requests, restart clearing pending IDs.
- Initialization ordering: `initialize` response precedes `initialized`; no premature API requests.
- Child process exits unexpectedly during an active turn; display useful error; no UI deadlock.
- Approvals: exact server request correlation; reject and allow paths; process death during dialog; no automatic approval.
- Canonical path checks, invalid project paths, missing executable, read-only data directory.
- Config write crash safety and upgrade/migration behavior.
- Sensitive-value redaction: tokens/secrets absent from logs.
- UI state machine: stop/restart and thread switching while streaming.

### End-to-end test matrix

| Test | Expected |
|---|---|
| Open EXE from Desktop as non-admin | Launches without installation or elevation |
| No system Node/Rust/npm | Fully functional target app |
| Codex binary missing | Explains problem, lets user select binary |
| Codex executable incompatible | Clear version/protocol error |
| No WebView2 | Clear dependency error or verified fixed-runtime fallback |
| New project and thread | Thread starts with correct project cwd |
| Ask Codex to create a small text file | Live events shown, requested approvals handled |
| Reject an approval | Refusal respected; no silent command execution |
| End a long turn | Correct interrupt and eventual settled state |
| Close/relaunch | Projects and supported threads restored |
| School network denies API/sign-in | Actionable network/auth failure |
| Move ZIP folder to another path | Harness still resolves own binaries/data |
| No write access in project | Clear error without damaging files |
| Folder copied between computers | No bundled credentials; fresh login if needed |

### Definition of done

Delivery is complete only if:

1. Full source builds in documented steps.
2. The independent ZIP is produced and tested from a fresh folder, not just from Tauri dev mode.
3. Its Codex binary is official and version-recorded.
4. A real Codex App Server handshake succeeds.
5. Streaming conversation, thread recovery, command approval, and agent activity work against the bundled version.
6. No admin rights or target-side installer is required on the tested setup.
7. Limitations (WebView2, school policies, network, authentication) are documented truthfully.
8. README includes a novice-friendly unzip → double-click walkthrough and troubleshooting.

## 11. Deliverables

Produce all of the following:

1. Git repository containing complete Rust/Tauri + frontend source.
2. `dist-portable/RustPortableHarness-win-x64-portable.zip` with actual runnable executables.
3. `README-PORTABLE.md`, explaining installation-free operation, sign-in, project access, and limitation of shared-school PCs.
4. `docs/ARCHITECTURE.md` with accurate request/event diagrams and subprocess lifecycle.
5. `docs/PROTOCOL-VERSION.md` listing Codex version, generated schema command and supported protocol capabilities.
6. `docs/SECURITY.md` explaining auth state, sandbox/approvals, local data, and sharing risks.
7. Automated tests plus manual QA results indicating precisely which target environments were tested.
8. A brief release summary of what works, what is deferred, and any remaining blockers.

## 12. Operating instructions for the coding agent (Codex)

**Execute this document as an engineering task, not as a writing exercise.** Create source files, run build/test commands where available, fix compiler/runtime errors, and leave a runnable artifact if the environment supports Windows compilation.

- First inspect the working folder and detect the available toolchain/OS.
- Confirm current official App Server documentation and generate protocol schemas from the **actual Codex version** you will ship. Treat examples here as conceptual when schema differs.
- Build Phase 1 as the first working vertical slice before expanding UI.
- For missing source/runtime prerequisites, document an exact, narrow blocker and continue with testable tasks; don't claim you verified portability if you lack a suitable Windows test environment.
- Use careful security defaults. Do not attempt to bypass institutional security controls.
- Keep the release folder relocatable, and ensure all paths point to resources alongside the EXE when intended.
- Prefer writing actual code and tests over exhaustive progress prose.
- When complete, report: commands executed, build/test results, actual release ZIP path, supported capabilities, and any unverified Windows-specific requirements.

---

## References

- OpenAI Codex App Server: https://learn.chatgpt.com/docs/app-server
- OpenAI App Server architecture: https://openai.com/index/unlocking-the-codex-harness/
- Official Codex source/releases: https://github.com/openai/codex
- Tauri Windows/WebView2 details: https://v2.tauri.app/distribute/windows-installer/

**Design intent:** An independently runnable, trustworthy, local UI for the official Codex agent—portable deployment, not a replacement for Codex's inference engine.
