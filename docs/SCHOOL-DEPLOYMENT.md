# School deployment guide (for teachers)

The app is built to run from a folder students can write to, usually their Desktop. It needs no
installer, administrator rights, registry changes, PATH entries or services. It starts the official
`codex.exe` / `claude.exe` directly and keeps all of its files inside its own folder.

## Before class: checklist

1. **Build the ZIP** on a teacher machine:
   `scripts/build-portable.ps1 -CodexExecutable <codex.exe> [-ClaudeExecutable <claude.exe>] [-PortableGit <folder>]`.
   - `-ClaudeExecutable`: include the official Claude Code program (2.1 or newer) so students can use
     Claude without installing anything. Claude Code is Anthropic's software. Check that your school's
     use and distribution of it follows Anthropic's terms. Without it, students can install Claude Code for
     their own Windows account and the app finds it automatically.
   - `-PortableGit`: include an extracted *PortableGit* folder so Claude can run Bash commands on
     computers without Git. Without it, Claude uses PowerShell for commands.
2. **Prepare the programming tools once** (if students will write Python, Rust or Convex apps): open
   the app, **Programming tools → Get all missing**, then **Make a classroom copy…**. The classroom ZIP
   contains the tools but not your sign-in, chats or projects, so students don't each download about
   600 MB. You can also pass the prepared folder to the build script: `-ToolsFrom <app>\tools`.
3. **Test the exact ZIP on a student account** on a real lab PC: unzip to the Desktop, open the app,
   pick a project, sign in, send one message, approve one edit, and sign out.
4. **Ask IT to allow** (if needed): the app folder or its programs in AppLocker/WDAC/antivirus, and these
   sites through the proxy/filter: `chatgpt.com`, `auth.openai.com`, `api.openai.com` (Codex);
   `claude.ai`, `claude.com`, `console.anthropic.com`, `api.anthropic.com` (Claude). For the
   programming tools and their packages: `github.com` and `release-assets.githubusercontent.com`
   (uv, Python, Git, PowerShell, Rust helpers), `static.rust-lang.org` and `crates.io` /
   `index.crates.io` / `static.crates.io` (Rust), `pypi.org` and `files.pythonhosted.org`
   (Python packages), `nodejs.org` and `registry.npmjs.org` (Node.js, npm), and `convex.dev` /
   `*.convex.cloud` (Convex).
5. **Explain accounts to students:** each student signs in with their own account. Don't share
   logins or set up shared "pool" accounts. Both providers forbid it and may suspend accounts.
6. **Remind students to sign out** on shared computers (Accounts & usage → Sign out).

## What can go wrong, and what happens

| Situation | What the student sees | What the app does / how to fix |
| --- | --- | --- |
| Opened from inside the ZIP | Red "Unzip the app first" notice | Explains Extract All. Settings can't be saved from a ZIP. |
| Downloaded ZIP is marked "from the internet" | "Windows protected your PC" (SmartScreen) | Before extracting: right-click the ZIP → Properties → **Unblock**. Or ask IT to allow the app. |
| AppLocker / WDAC / Smart App Control blocks programs | App won't open, or "Windows blocked a program" | IT allow rule for the folder or the programs' publishers/hashes. The app never bypasses this. |
| Antivirus removes `codex.exe` / `claude.exe` | "AI programs are missing" notice | Re-extract; ask IT for an exclusion. |
| First start is slow (antivirus scans 200–350 MB programs) | "Starting…" for up to a minute | The app waits up to 90 s and says Windows may still be scanning. |
| WebView2 Runtime missing | Error dialog at start | Ask IT to install the WebView2 Runtime (normally built into Windows 10/11). |
| Desktop is read-only, or Controlled folder access blocks writes | Error dialog at start | The message names Controlled folder access; ask IT to allow the app or use a USB drive. |
| Desktop is synced by OneDrive (Known Folder Move) | Yellow OneDrive notice | Sign-ins sync to OneDrive and files can lock briefly; settings saves retry. Right-click the folder → "Always keep on this device" so OneDrive doesn't free up the programs. |
| Profile storage quota / disk nearly full | "Almost out of space" notice (< 500 MB) | The app with both AI programs needs about 600 MB plus chat history. |
| PC resets at log-off (Deep Freeze, mandatory profiles) | App or sign-in gone next lesson | Keep the folder on a USB drive or network home; students sign in again each time. |
| App on a network share (`\\server\...`) | "Running from a network folder" notice | Usually works but can be slow or blocked. Copy to the local Desktop if possible. |
| Very long folder paths | "Folder path is very long" notice | Move the folder closer to the drive root. |
| Explicit school proxy | AI can't connect although the browser works | Command-line programs ignore Windows proxy settings, so the app copies the Windows proxy to them automatically. |
| Proxy set by a setup script (PAC) | Blue notice | Ask IT for the proxy address; enter it in Settings → Advanced → Network. |
| HTTPS inspection (school certificate) | "The school network is checking secure connections" | Get the school root certificate as `.pem` and add it in Advanced → Network. |
| Sign-in sites blocked | Sign-in page doesn't load | Allow-list the sites above, or copy the link into another allowed browser. |
| Browser can't return to the app (localhost blocked) | Sign-in never finishes | Codex: **Sign in with a code**. Claude: paste the code shown in the browser into the app. |
| App opened twice | "Already open from this folder" | Second copy closes, so the two can't corrupt each other's data. |
| No Git Bash and PowerShell blocked | Claude can't run commands | Claude can still read and edit files. Include PortableGit in the ZIP. |
| Usage limit reached | "Usage limit reached" and a red meter | Wait for the reset time shown, or switch to another of the student's own accounts. |
| Student picks the whole Desktop/drive as a project | Confirmation dialog | Recommends one folder per project. |
| Downloaded project contains `.claude/settings.json` hooks | Nothing | Project Claude settings are ignored by default (Advanced setting to allow). |
| Launched with stray `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` or a parent Claude session | Nothing | Those variables are removed for the AI programs, so the portable sign-in is always used. |
| `%TEMP%` unavailable or tightly limited | Nothing | AI temp files go to `data\temp` and are cleaned after 3 days. |
| Claude tries to update itself | Nothing | Auto-update is turned off; update by replacing `claude.exe` in the ZIP. |
| Small lab monitor or 125% scaling | Normal layout | Starts maximized on small screens, the Activity panel becomes a drawer, and the window can shrink to 760×500. |
| Codex says it has no permission to write | The AI explains but changes nothing | Codex 0.163 cannot ask before a file change: in **Read only** it refuses. The default is now **Edit files in this project** (only inside the project folder); a banner offers it with one click. |
| A tool download is blocked | "Could not download …" with the reason | Allow the sites above, or prepare a classroom copy with the tools already inside. Downloads use Windows' own `curl.exe` (Windows proxy and certificate store). Every file is checked against its official SHA-256; a changed file is deleted. |
| Students can only write to the Desktop | Nothing; tools work | Python environments, the uv/pip/npm caches, Rust crates, Git's config and temp files all stay in the app folder. Nothing goes to `%USERPROFILE%`, `%APPDATA%` or the registry. |
| Rust window app won't build | Error in the New project log | Rust uses the GNU toolchain with a small set of MinGW helpers (no Visual Studio). Keep the app folder path short (for example `Desktop\\Harness`). Crates that need a full C/C++ compiler, CMake or Visual Studio aren't supported. |
| Rust build folders fill the disk | "Almost out of space" | Window apps use slimmed build settings (about 400 MB per project). Delete a project's `target` folder to free space; it rebuilds when run. |
| PowerShell is blocked by policy | Terminal or AI commands fail | PowerShell 7 from Programming tools usually runs; if Group Policy blocks it too, Terminal falls back to Command Prompt. The app never changes execution policy beyond its own windows (RemoteSigned). |
| `npx convex dev` asks to sign in | Questions in the Run window | That's normal the first time: sign in to Convex in the browser and answer in the console. For deploying without questions, put a deploy key in `.env.local` (`CONVEX_DEPLOY_KEY=…`) and run `npx convex deploy`. |
| Wrong PC clock | Secure-connection or sign-in errors | Fix the clock; certificates and sign-in depend on it. |
| Newer `claude.exe` changes its output format | Claude errors after an update | Keep the version you tested in the ZIP. The app needs Claude Code 2.1 or newer. Codex is pinned to 0.163.0. |

## Privacy and data

- Sign-ins, chats and settings are stored in `data\` beside the app, one folder per account. Diagnostics
  never include sign-in tokens or chat contents.
- Some Claude Code versions may keep the sign-in in Windows Credential Manager (per Windows user)
  instead of a file. **Sign out** clears it either way.
- Student prompts and the project files the AI reads are sent to OpenAI or Anthropic. Only use projects
  and accounts your school allows for this.
