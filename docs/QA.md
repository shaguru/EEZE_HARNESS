# Verification record

Environment: Windows x64, WebView2 Runtime 155.0.4283.62, Rust 1.99.1 MSVC, Node 22.12.0, npm 11.1.0, Codex CLI 0.163.0.

Automated checks completed:

* `npm run build` — passed, including TypeScript type checking and Vite production assets.
* `cargo check --manifest-path src-tauri/Cargo.toml` — passed.
* `cargo test --manifest-path src-tauri/Cargo.toml` — 15 passed (plus one separately invoked real-binary test): framing, Unicode, size limits, out-of-order responses, duplicate IDs, cancellation cleanup, initialization ordering, path/config safety, state resync, approval safety, and thread filtering.
* `node scripts/protocol-smoke.mjs` — passed handshake, model/list (8 models), thread/start, thread/list, thread/read, thread/turns/list against a clean CODEX_HOME. Authentication was absent, so the server correctly returned an auth error rather than generating an assistant delta.
* `npm audit --omit=dev --audit-level=high` — passed with bundled Monaco 0.56.0 and a patched DOMPurify override (3.4.17). Re-run after dependency updates.
* `cargo test --manifest-path src-tauri/Cargo.toml real_companion_handshake -- --ignored --nocapture` — passed through the actual Rust child launcher, supported policy arguments, initialization sequence, model listing, and containment-job shutdown.
* A fresh ZIP was extracted to the Desktop and launched with the process working directory set to `%TEMP%`. This caught and corrected the missing Tauri `custom-protocol` feature. The corrected UI loads from the EXE's bundled assets at `tauri.localhost`; the legacy `untrusted` startup config was also caught and corrected.
* `node tests/ui.mjs` — five fixture checks passed in headless Edge at 1366×768: raw HTML does not execute, external links require an explicit action, locally bundled Monaco loads the authoritative patch, approval is not automatic, and decline preserves the request key.
* Windows host token integrity was Medium (unelevated). PE import inspection found OS DLL imports only for the harness and Codex; no Node/Python/Rust runtime DLLs or VC redistributable installer are required by those imports. The host has developer tools, so a separate clean machine remains necessary to verify a truly toolchain-free target environment.
* `node scripts/live-acceptance.mjs <portable-codex.exe> <isolated-portable-CODEX_HOME>` — passed with the signed-in test copy: streamed `PORTABLE_RECOVERY_OK`, killed and restarted app-server, resumed the same thread and recovered history, then requested a write in read-only mode. Codex refused the write and no file appeared. A second turn requested a harmless escalated command; Codex emitted a real `item/commandExecution/requestApproval`, the probe declined it, and Codex reported it stopped without running anything else.

Still required before claiming full release acceptance:

* Verify the accept branch of a real command approval and a live file-change approval against an account-enabled project. The command decline branch passed; later acceptance probes timed out before any request was emitted. The file-change response mapping and UI are covered by fixtures, but an actual file-change approval was not observed.
* Repeat the ZIP test on a standard non-admin account with WebView2 absent/present cases.
* Validate school proxy/AppLocker policy and any fixed-runtime fallback separately. The fallback is intentionally not claimed by this release.
