# RustPortableHarness portable release

A simple desktop app for using an AI coding assistant on a school computer. It runs from a folder on
your Desktop: no installer, no administrator rights, nothing added to Windows. It can use either:

- **Codex** (the official OpenAI Codex program, `codex.exe`) with a **ChatGPT** account, or
- **Claude** (the official Anthropic Claude Code program, `claude.exe`) with a **Claude** account.

The official programs do the AI work, the sign-in and the safety checks. This app is the window around them.

## First launch (students)

1. **Unzip first.** Right-click the ZIP → **Extract All…** → choose your Desktop. Don't run the app from
   inside the ZIP; the app will warn you if you do.
2. Open the new folder and double-click **RustPortableHarness.exe**.
3. At the top, choose **Codex** or **Claude**, whichever you have an account for.
4. Click **Choose folder** and pick your project folder. Make one folder per project, for example
   `Desktop\my-game`. Don't pick your whole Desktop; the app asks you to confirm if you do.
5. Click **Sign in**. Your browser opens the official sign-in page. Use **your own** account.
6. Type in the message box and press **Enter** (Shift+Enter adds a new line).

When the AI wants to edit a file or run a command, a **Permission needed** box explains what it wants to
do. Choose **Approve once**, **Decline** or **Stop turn**.

## The main screen

- **Codex | Claude** switch (top left): changes which AI you are talking to. Each keeps its own chats.
- **Usage meter** (top right): the `5h` bar is the 5-hour limit and `Wk` is the weekly limit, for the
  account in use. Green is fine, amber means getting close, red means nearly out. Click it to open
  **Accounts & usage**.
- **Message box**: drag its ⠿ handle and drop it on the **left**, **bottom** or **right**, or click the
  three small layout buttons on it. Drag the line beside it to make it bigger or smaller (double-click
  that line to reset). The dropdown on the message box sets what the AI may do:
  - **Ask before changes** (default): reads and explains, and asks before every edit or command.
  - **Edit files, ask for commands**: may edit files inside the project, and asks before commands.
  - **Work without asking**: no questions. Use only in a project you trust and can restore.
- **Activity panel** (▤ button): shows the commands, file edits and tools the AI used. On small screens it
  opens as a side drawer.

## Accounts & usage

Open it by clicking the usage meter or your account name.

- **Add account** keeps several sign-ins on this computer, for example a school account and a personal
  account. Each account has its own folder, chats and usage history.
- **Use** switches to that account. **Sign in** / **Sign out** manage it. Codex also offers **Sign in with
  a code** if the browser can't return to the app.
- Each account shows its **5-hour** and **weekly** usage and when they reset. The account in use updates
  while you chat. **Check all accounts** refreshes every ChatGPT account. Claude only reports usage
  while it's answering, so other Claude accounts show their last known values.

Each person should use their own account. The app doesn't share or rotate accounts automatically. Both
OpenAI's and Anthropic's terms forbid sharing logins or using several accounts to get around usage
limits, and doing so can get accounts suspended.

## Settings

**Settings** (⚙) keeps the everyday options: what the AI may do, message box position, Enter-to-send,
theme, text size, showing the Activity panel, and auto-connect.

**Advanced…** is a separate panel for teachers: proxy and school certificate, choosing `codex.exe` or
`claude.exe`, the Codex Windows sandbox, reasoning summaries and service tier, full access, whether a
project's own Claude settings may apply, diagnostics, and wiping an account.

## What's in the folder

```text
RustPortableHarness/
  RustPortableHarness.exe
  START-HERE.txt                     # the short version of this page
  codex.exe                          # official Codex CLI 0.163.0 x64
  claude.exe                         # official Claude Code 2.1+ (only in the Claude build)
  tools/PortableGit/                 # optional: gives Claude a Bash shell for commands
  data/                              # settings, sign-ins, chats, temp files (created on first launch)
    codex-home/  claude-home/  accounts/  logs/  temp/  webview2/
  projects/                          # empty convenience folder
```

Everything the app writes stays inside this folder, including the AI programs' temp files. The folder can
be moved or copied to a USB drive. Your projects stay where they are.

**`data/` contains your sign-ins.** Sign out before you share the folder or leave a shared computer, and
don't copy someone else's `data` folder.

If `claude.exe` isn't in the folder, the app also finds Claude Code installed for your Windows account
(`%USERPROFILE%\.local\bin\claude.exe`), or you can choose it in **Settings → Advanced**.

## If something goes wrong

The app explains common problems in plain words and offers a button that helps. More detail:
[docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md). Teachers preparing a classroom should read
[docs/SCHOOL-DEPLOYMENT.md](docs/SCHOOL-DEPLOYMENT.md).

The release doesn't need PowerShell, Node.js, Python, Rust or npm on the student computer. It does need
the Microsoft Edge WebView2 Runtime (part of Windows 10/11), network access to the AI service, and an
eligible account.

See also [docs/SECURITY.md](docs/SECURITY.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and
[docs/PROTOCOL-VERSION.md](docs/PROTOCOL-VERSION.md).
