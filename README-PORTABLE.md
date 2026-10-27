# RustPortableHarness portable release

## First launch

1. Unzip the complete `RustPortableHarness-win-x64-portable.zip` to a folder you own, such as `Desktop\RustPortableHarness`.
2. Double-click `RustPortableHarness.exe`. If Windows presents a policy warning, follow your school's software policy; this app cannot and should not bypass it.
3. Click **Add folder** and choose the project folder Codex is allowed to access.
4. Click **Connect Codex**, then open **Account & settings** and complete **Device sign-in** or **Browser sign-in**. The browser flow is the official Codex account flow.
5. Click **New thread**, write a request, and press **Ctrl+Enter** or **Send**.
6. Review each command or file-change approval. **Approve once** sends only the decision allowed by the shipped App Server schema. **Decline** lets Codex continue without the operation; **Stop turn** cancels the active turn.

## Permission and quality controls

Settings exposes the same kinds of controls as the Codex desktop client:

- **Read only + Ask** is the safe default. Codex can inspect the selected project, and requests that need more access pause for review.
- **Workspace write + Ask** allows approved edits inside the selected project while still asking before requested operations.
- **Workspace write + Auto-allow** removes approval prompts for the turn. Use this only in a project you trust.
- **Full access + Auto-allow** removes the workspace boundary as well. It is intentionally an explicit, unsafe choice and may still be refused by account or school policy.

The **Reasoning effort** setting was previously missing from this harness. It is now sent to Codex for every new turn when the selected model advertises that effort. Higher effort can improve hard coding or planning tasks but costs more time and quota. **Detailed reasoning summaries** show server-provided activity summaries; private chain-of-thought is never exposed. The service-tier control affects speed or availability when the model advertises tiers. Tool use remains controlled by the official Codex App Server and the selected policy.

The composer can sit at the bottom, left, or right of the conversation. Drag the panel separators to resize the projects, composer, and activity areas. Only the message and activity panes scroll; the desktop shell stays fixed. Selecting a saved project changes the active workspace without disconnecting Codex. Use the folder arrow on a project card to open it in Windows Explorer, and use the upper-right button to connect or disconnect Codex.

The app keeps configuration, logs, WebView2 profile data, and Codex account state under `data/` beside the executable. `data/codex-home/` may contain sensitive authentication state. Sign out before sharing the folder or leaving it on a shared computer. The app never bundles developer credentials, project folders, or login tokens.

## What the portable folder contains

```text
RustPortableHarness/
  RustPortableHarness.exe
  codex.exe                         # official Codex CLI 0.163.0 x64
  codex-resources/                  # official runtime helpers when supplied
  data/                             # created on first launch
  projects/                         # empty convenience folder
  README-PORTABLE.md
  THIRD-PARTY-LICENSES.txt
```

The folder is relocatable. Paths are anchored to the executable location, while the selected project remains at its existing absolute path. Moving a project requires reconnecting it from the project list.

## Troubleshooting

- **WebView2 Runtime missing:** install or request the approved Microsoft Edge WebView2 Runtime for the computer. The portable app does not install a runtime. A fixed-runtime fallback is not included until it can be tested as a supported Tauri distribution.
- A separate native Rust GUI (for example, egui) could reuse the backend in a future release for computers without WebView2. It is not included in this MVP.
- **Codex missing or incompatible:** restore the official `codex.exe` from the release ZIP. Settings shows the validated version, path, and SHA-256. This build accepts `codex-cli 0.163.0` only because its protocol schemas were generated from that binary.
- **Authentication or network error:** check the account, network, proxy, and school policy. No API key is saved by the harness. Restart Codex after changing network conditions.
- **Project cannot be opened:** choose a folder you can read and write where needed. The harness never copies the project into `data/`.
- **Permission request is unfamiliar:** decline it or stop the turn. The client never auto-approves.
- **Portable data is read-only:** move the folder to a writable location you own. No machine-wide registry or PATH changes are made.

The release does not require PowerShell, Node.js, Python, Rust, npm, or an installed Codex command on the target computer. Those are build-time tools. At runtime the app starts the bundled `codex.exe` directly and opens project folders through Windows Explorer. See [docs/SCHOOL-DEPLOYMENT.md](docs/SCHOOL-DEPLOYMENT.md) for the remaining school-machine dependencies and failure modes.

## Security and school computers

Codex actions can read or change project files and, when explicitly approved, run commands according to the Codex sandbox. Network-enabled agent actions may send source material to remote services. Use only projects and accounts allowed by your school's policy. Do not copy `data/codex-home/` between computers.

See [docs/SECURITY.md](docs/SECURITY.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), and [docs/PROTOCOL-VERSION.md](docs/PROTOCOL-VERSION.md).
