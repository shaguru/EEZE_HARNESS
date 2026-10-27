# RustPortableHarness

RustPortableHarness is a portable Windows desktop client for two official coding agents:

- **Codex**: the official Codex App Server (`codex.exe`, ChatGPT sign-in), and
- **Claude**: the official Claude Code CLI (`claude.exe`, Claude sign-in) in its headless stream-json mode.

The bundled programs remain the agent, authentication client, sandbox and permission authority. The
harness adds a simple classroom UI: projects, chats, a message box you can dock left, bottom or right,
plain-language approvals, several accounts with 5-hour and weekly usage meters, and a separate Advanced
panel. See [README-PORTABLE.md](README-PORTABLE.md) for the user guide and
[docs/SCHOOL-DEPLOYMENT.md](docs/SCHOOL-DEPLOYMENT.md) for classroom setup.

## Development

Use Windows 10/11 x64 with Rust stable MSVC, Node.js LTS, and the WebView2 Runtime. This repository was
built with Rust 1.99.1, Node 22.12.0, npm 11.1.0, Tauri 2.12.5, Codex CLI 0.163.0 and Claude Code 2.1.308.

```powershell
npm ci
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo build --release --manifest-path src-tauri/Cargo.toml
```

UI fixture tests (mocked backend): start `npx vite --host 127.0.0.1 --port 5174`, then `node tests/ui.mjs`.
Live Claude transport test (signed out, isolated config):
`$env:CLAUDE_TEST_EXE="<claude.exe>"; cargo test --manifest-path src-tauri/Cargo.toml real_claude -- --ignored`.

To create the release ZIP:
`scripts/build-portable.ps1 -CodexExecutable <codex.exe> [-ClaudeExecutable <claude.exe>] [-PortableGit <PortableGit folder>]`.
The script stages the executables, generated UI, blank portable state, notices and SHA-256 files, then
verifies the release.

The machine that runs the ZIP does not need Node.js, Rust, npm, an installer, or administrator rights. It
does need an eligible account, network access allowed by local policy, and the Microsoft Edge WebView2
Runtime. The app does not install WebView2 or bypass school controls.
