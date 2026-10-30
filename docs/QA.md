# Verification record

## Programming tools, New project, Run/Terminal, write permission (2026-10-30)

* `cargo test`: 43 passed (new: tools environment points only into the app folder, zip extraction
  strips/filters and blocks `..`, AGENTS.md block replaced not duplicated, Run detection, project
  name rules, empty template, old read-only default upgraded once, classroom copy leaves out
  `data/` and `projects/`).
* `node tests/ui.mjs`: 19 fixture checks (new: Codex read-only banner and one-click edit mode, Run and
  Terminal, tools dialog with progress and job tray, full New project flow, Restart Codex prompt).
* Each tool proven by hand from an isolated folder with no installed Python/Rust/Node/Git on PATH:
  uv + Python 3.14.10 with Tk 9.0, `uv init --app --no-package` + `uv sync`, Node 26 with
  `npm install convex` (CLI 1.48.0, `convex deploy --help`), PowerShell 7.6.9 (FullLanguage),
  PortableGit 2.56 with bash 5.3, rustup 1.29.2 → Rust 1.99.1 `x86_64-pc-windows-gnu`.
* Rust without Visual Studio: console apps link with the toolchain's bundled MinGW linker. Window apps
  (eframe 0.37) also need `as.exe`/`dlltool.exe`/`ld.exe` + 5 DLLs (8 MB) from WinLibs 16.2.0
  msvcrt r2; with them an egui window builds (53 s first build, 2 s rebuild) and opens. eframe with
  `default-features = false, features = ["default_fonts","glow"]` and the slim dev profile cut the
  build folder from 1.7 GB to under 400 MB. Linking worked with linker paths up to 258 characters.
* **Real built app, end to end** (fresh staged folder, real downloads via curl.exe with SHA-256
  checks): all tools installed into `tools/`; "Restart Codex" banner appeared and Codex's AGENTS.md
  listed the tools; New project created a tkinter app (`.venv`, Git) in 2 s and ▶ Run opened its
  window; a Rust egui app built in 59 s and ran; a Convex project installed convex 1.48.0; Terminal
  opened the portable PowerShell; Claude was started with the tools note. Afterwards nothing new in
  `~/.local/bin`, `~/.gitconfig`, `%APPDATA%\npm`, `~/.convex` or `HKCU\Software\Python`.
* Found and fixed during testing: `uv python install` failed with "Access is denied" renaming its
  freshly unpacked folder (antivirus) and also wrote a registry entry and a `~/.local/bin` shim, so
  the app now unpacks the same pinned Python build itself with Windows' tar.exe straight into place
  (`UV_PYTHON_DOWNLOADS=never`, `UV_PYTHON_INSTALL_REGISTRY=0`). "Get all" now also fetches app
  copies of tools that are only installed on the teacher's PC, so classroom copies include them.
* Not verified here: a real signed-in Codex/Claude turn using the tools, `npx convex dev` sign-in and
  `convex deploy` against a real Convex account, Make a classroom copy end to end through the
  native Save dialog (the ZIP writer is unit-tested), and a managed student PC.

## 0.1 + Claude, accounts, docking (2026-10-27)

Environment: Windows 10 Pro x64, Rust 1.99.1 MSVC, Node 22.12.0, Codex CLI 0.163.0, Claude Code 2.1.308.

* `npm run build`: TypeScript type check and Vite production build pass.
* `cargo test --manifest-path src-tauri/Cargo.toml`: 35 passed. New coverage: Claude stream mapping
  (streamed text finalised without duplicates, tools → command/file/tool cards, tool results,
  permission requests wait for the user, cancel requests, auto-allow only when chosen), Claude usage
  events merge per window, Codex rate-limit updates, API errors shown as notices instead of chat text,
  unexpected Claude exit ends the turn, Claude history load/list/archive, config repair for old or
  hand-edited files (including a Notepad byte-order mark), separate account homes, second-instance
  lock, UNC paths, whole-drive warning, proxy parsing, child environment scrubbing, ZIP-in-temp detection.
* `real_claude_session_signed_out` (ignored by default; `CLAUDE_TEST_EXE`) passed against the real
  `claude.exe` 2.1.308 with an empty isolated config: version check, `auth status --json`, spawn,
  initialize → live model list, user message → `authentication_failed` handled, turn ends, stop.
* `node tests/ui.mjs`: 14 fixture checks pass at 1366×768: Markdown HTML blocked, link confirmation,
  Monaco patch viewer, conversation fills the middle column (regression for the old grid bug), dock
  buttons, **drag-to-dock** to the left, divider resize saved, Enter during a short wait is queued and
  sent, no automatic approval with key correlation, Claude permission dialog, plain sign-in error with
  a Sign in button, accounts panel (usage bars, add account), advanced options kept out of simple
  Settings, 1093×590 lab-monitor layout with no horizontal scroll and Activity as a drawer.
* **Real built app** (release EXE in a staged portable folder with empty data, driven over the
  WebView2 DevTools port): auto-connect to the real Codex app-server (8 models), switch to Claude (real
  `claude.exe`, signed out), new chat loads the live model list, a message gets the friendly
  "You're not signed in" notice, the new chat appears in the sidebar with its title, drag-to-dock is
  saved, Accounts and Advanced panels open, and network mode is detected. A second copy of the app is
  refused with no WebView or agent started. A hard kill of the app leaves no `claude.exe`/`codex.exe`
  behind (Job Object).

Not verified in this round (needs real accounts or a lab PC):

* Completing a real Claude browser sign-in (`claude auth login` without a terminal) and the paste-code
  fallback, a signed-in Claude turn with tool approvals, and live `rate_limit_event` usage values.
* Codex sign-in, live `account/rateLimits/read` values, and **Check all accounts** with several signed-in
  ChatGPT accounts.
* Proxy auto-detection, PAC notice and school certificate on a real school network; OneDrive, quota,
  AppLocker and Controlled folder access behaviour on a managed student account.

## Earlier MVP record

Environment: Windows x64, WebView2 Runtime 155.0.4283.62, Rust 1.99.1 MSVC, Node 22.12.0, npm 11.1.0, Codex CLI 0.163.0.

* `node scripts/protocol-smoke.mjs` passed handshake, model/list, thread/start, thread/list, thread/read,
  thread/turns/list against a clean CODEX_HOME.
* `npm audit --omit=dev --audit-level=high` passed with bundled Monaco 0.56.0 and a patched DOMPurify
  override (3.4.17). Re-run after dependency updates.
* A fresh ZIP was extracted to the Desktop and launched with the working directory set to `%TEMP%`.
* `node scripts/live-acceptance.mjs <portable-codex.exe> <isolated-portable-CODEX_HOME>` passed with a
  signed-in test copy: streaming, kill/restart and resume, read-only write refusal, and a declined
  command approval.
* Still open from the MVP: the accept branch of a real command approval and a live file-change approval,
  a ZIP test on a standard non-admin account with and without WebView2, and school proxy/AppLocker
  validation.
