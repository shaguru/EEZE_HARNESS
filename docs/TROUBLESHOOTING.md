# Troubleshooting

Most problems show a short explanation in the app with a button that helps (Sign in, Accounts & usage,
or Advanced). If that's not enough:

1. **Settings → Advanced → Show diagnostics** shows the app folder, programs found, versions, network/proxy
   mode, free space, startup warnings and permission mode. **Save diagnostics file** writes
   `data\logs\diagnostics.json` to send to a teacher or IT. It never contains sign-in tokens or chat text.
2. **Stop AI** then **Connect** restarts the AI program. The app never replays a command after a restart.

| Problem | Try |
| --- | --- |
| "You're not signed in" | Accounts & usage → Sign in for the account in use. |
| Sign-in page opens but the app never finishes | Codex: **Sign in with a code**. Claude: paste the code the page shows into the app. Copy the link into another browser if the default one is blocked. |
| "Can't reach Codex/Claude" | Check internet. If the school uses a proxy, set it in Advanced → Network. |
| "The school network is checking secure connections" | Add the school certificate (`.pem`) in Advanced → Network. |
| "Usage limit reached" | Wait for the reset time on the meter, or use another of your own accounts. |
| "Claude is not set up" | Place `claude.exe` next to the app or choose it in Advanced. Claude Code 2.1+ is required. |
| Codex refuses its program | This build needs the official `codex-cli 0.163.0`. Regenerate `generated/` before using another version. |
| Claude can't run commands | It needs Git Bash or PowerShell. Include PortableGit in `tools\PortableGit`, or ask IT. |
| Message box in an awkward place | Drag the ⠿ handle, or use the left/bottom/right buttons on it. Double-click the divider to reset its size. |
| Settings won't save | The folder may be read-only, synced by OneDrive, or blocked by Controlled folder access. See SCHOOL-DEPLOYMENT.md. |
| "Already open from this folder" | Switch to the open window in the taskbar. |
| A Claude chat won't reopen | The chat file may have moved or been archived. Start a new chat. |
| Codex says it cannot write files | Choose **Edit files in this project** on the message box (or click **Let Codex edit this project**). |
| "… is not set up yet" | Open **Programming tools** and get that tool. |
| A tool download fails | The message says why (blocked site, proxy, certificate, disk full). Ask IT to allow the sites in SCHOOL-DEPLOYMENT.md, or use a classroom copy with tools included. |
| The AI can't find python/cargo/npm | After adding tools, press **Restart Codex** in the banner (Claude restarts by itself). |
| ▶ Run does nothing useful | Run starts `uv run main.py`, `cargo run` or `npm run dev`. For other projects use **Terminal**. |
| A Rust build fails with "linker" or "dlltool" | Get Rust again in Programming tools (it adds the Windows helpers), and keep the app folder path short. |
| An approval request looks unfamiliar | Decline it or press Stop turn. The app never answers approvals by itself unless you chose "Work without asking". |
