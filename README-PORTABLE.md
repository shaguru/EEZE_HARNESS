# RustPortableHarness portable release

A simple desktop app for using an AI coding assistant on a school computer. It runs from a folder on
your Desktop: no installer, no administrator rights, nothing added to Windows. It can use either:

- **Codex** (the official OpenAI Codex program, `codex.exe`) with a **ChatGPT** account, or
- **Claude** (the official Anthropic Claude Code program, `claude.exe`) with a **Claude** account.

It can also set up **Python (with tkinter)**, **Rust**, **Git**, **PowerShell 7** and **Node.js**
(for the **Convex** CLI) inside its own folder, so students can write and run real programs on
computers where nothing can be installed.

## First launch (students)

1. **Unzip first.** Right-click the ZIP → **Extract All…** → choose your Desktop. Don't run the app from
   inside the ZIP; the app will warn you if you do.
2. Open the new folder and double-click **RustPortableHarness.exe**.
3. At the top, choose **Codex** or **Claude**, whichever you have an account for.
4. Click **＋ New project**, give it a name and choose a type (for example *Python window app*). The app
   creates the folder, its Python or Rust environment, and starter code. Or click **Open existing
   folder** to use a folder you already have.
5. Click **Sign in**. Your browser opens the official sign-in page. Use **your own** account.
6. Type in the message box and press **Enter** (Shift+Enter adds a new line). Press **▶ Run** to try
   your program.

When the AI wants to do something that needs your permission, a **Permission needed** box explains what
it wants. Choose **Approve once**, **Decline** or **Stop turn**.

## New project

| Type | What you get | Needs |
| --- | --- | --- |
| Python window app (tkinter) | `main.py` with a small window, its own `.venv` environment | Python |
| Python program | A console program with its own `.venv` | Python |
| Rust window app (egui) | A Rust window with a text box and a button, already built once | Rust |
| Rust console app | A Rust console program | Rust |
| Web app with a Convex database | A Node.js project with the Convex CLI (`npx convex dev` / `deploy`) | Node.js |
| Empty folder | Just a folder | — |

New projects go into the app's `projects` folder unless you choose another place. You can tick **Start a
Git repository**. Every project gets `AGENTS.md` and `CLAUDE.md` notes so the AI knows how to run it.

## Run and Terminal

- **▶ Run** opens a console window and starts the project (`uv run main.py`, `cargo run` or
  `npm run dev`). The window stays open so you can read errors and type answers.
- **Terminal** opens a console in the project folder where `python`, `uv`, `cargo`, `git`, `node`,
  `npm`, `npx` and `pwsh` all work.

## Programming tools

**Programming tools** (sidebar, or Settings) downloads each tool from its official website, checks it
against its published checksum, and keeps it in the app's `tools` folder:

| Tool | Download | On disk |
| --- | --- | --- |
| Python 3.14 with tkinter (through `uv`) | about 45 MB | about 110 MB |
| Rust (no Visual Studio needed) | about 380 MB | about 850 MB |
| Git (and Git Bash) | about 58 MB | about 360 MB |
| PowerShell 7 | about 105 MB | about 250 MB |
| Node.js with npm/npx (for Convex) | about 40 MB | about 120 MB |

Their caches and settings (pip/uv cache, npm cache, Rust crates, a Git config) also stay in the app
folder. **Remove** frees the space again. Teachers can use **Make a classroom copy…** to save a ZIP of the
app with all tools but without their own sign-in, chats or projects, so students don't each download them.

## The main screen

- **Codex | Claude** switch (top left): changes which AI you are talking to. Each keeps its own chats.
- **Usage meter** (top right): `5h` is the 5-hour limit and `Wk` the weekly limit of the account in use.
  Click it for **Accounts & usage**.
- **Message box**: drag its ⠿ handle onto the **left**, **bottom** or **right**, or click its three
  layout buttons. Drag the line beside it to resize (double-click to reset). Its dropdown sets what the
  AI may do:
  - **Edit files in this project** (Codex) / **Edit files, ask for commands** (Claude): the default.
    The AI can change files inside the project folder only.
  - **Read only** (Codex) / **Ask before changes** (Claude): Codex can only explain and cannot save
    files; Claude asks before every change.
  - **Work without asking**: no questions. Use only in a project you trust and can restore.
- **Activity panel** (▤ button): the commands, file edits and tools the AI used.

## Accounts & usage

- **Add account** keeps several sign-ins on this computer (for example a school and a personal
  account). Each has its own folder, chats and usage history.
- **Use** switches account. **Sign in** / **Sign out** manage it. Codex also offers **Sign in with a code**.
- Each account shows its **5-hour** and **weekly** usage and reset times.

Each person should use their own account. The app doesn't share or rotate accounts automatically; both
OpenAI's and Anthropic's terms forbid sharing logins or using several accounts to get around limits.

## Settings

**Settings** (⚙) keeps everyday options. **Advanced…** is separate, for teachers: proxy and school
certificate, choosing `codex.exe`/`claude.exe`, the Codex Windows sandbox, full access, project Claude
settings, diagnostics and wiping an account.

## What's in the folder

```text
RustPortableHarness/
  RustPortableHarness.exe
  START-HERE.txt                     # the short version of this page
  codex.exe                          # official Codex CLI 0.163.0 x64
  claude.exe                         # official Claude Code 2.1+ (only in the Claude build)
  tools/                             # Python, Rust, Git, PowerShell, Node.js (added by Programming tools)
  projects/                          # new projects go here by default
  data/                              # settings, sign-ins, chats, caches, temp files
```

Everything the app and its tools write stays inside this folder. **`data/` contains your sign-ins.**
Sign out before you share the folder or leave a shared computer, and don't copy someone else's `data`.

If something goes wrong, the app explains common problems with a button that helps. More detail:
[docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md). Teachers: [docs/SCHOOL-DEPLOYMENT.md](docs/SCHOOL-DEPLOYMENT.md).
See also [docs/SECURITY.md](docs/SECURITY.md) and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).
