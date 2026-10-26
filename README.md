# RustPortableHarness

RustPortableHarness is a Windows desktop client for the official Codex App Server. It keeps the UI local and portable while the bundled `codex.exe` remains the agent, authentication client, sandbox, and approval policy authority.

## Development

Use Windows 10/11 x64 with Rust stable MSVC, Node.js LTS, and the WebView2 Runtime. This repository was built with Rust 1.99.1, Node 22.12.0, npm 11.1.0, Tauri 2.12.5, and Codex CLI 0.163.0.

```powershell
npm ci
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo build --release --manifest-path src-tauri/Cargo.toml
```

To create the release ZIP, use `scripts/build-portable.ps1 -CodexExecutable <path-to-official-codex.exe>`. The script stages the executable, generated UI, blank portable state, notices, a SHA-256 file, and runs verification.

The machine that runs the ZIP does not need Node.js, Rust, npm, an installer, or administrator rights. It does need an eligible Codex account, network access allowed by local policy, and the Microsoft Edge WebView2 Runtime. The app does not install WebView2 or bypass school controls.
