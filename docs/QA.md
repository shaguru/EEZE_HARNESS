# Verification record

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
