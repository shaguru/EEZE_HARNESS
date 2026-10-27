# Release 0.1.0 — portable MVP candidate

The source and independent Windows x64 ZIP have been built. The app was launched unelevated from an uninstalled Desktop folder with a different current working directory. Official Codex 0.163.0 schemas and binary provenance are recorded in PROTOCOL-VERSION.md.

Working capabilities include native project selection; portable configuration and Codex home; version/PE/hash validation; stdio initialization; dynamic model selection; browser/device sign-in; thread creation/list/read/resume/archive; streamed Markdown; command/tool activity; interrupt; pending approvals with exact request correlation; authoritative file/turn patches in a locally bundled Monaco viewer; themes and font size; sanitized diagnostics; and an explicitly confirmed portable account/history wipe.

Verified against the real binary: handshake, models, sign-in in a separate Desktop test copy, streamed responses, thread history, restart/resume, and command approval rejection. Fifteen Rust tests and five UI fixture checks pass. The release staging process rejects nonblank data and verifies the official binary's exact SHA-256.

This is not full acceptance of every requirement in the original specification. Approved command execution, a real file-change approval, a missing-WebView2 machine, and a separate locked-down school machine remain unverified. A subsequent live acceptance probe timed out before the model emitted an operation; the app's failure/recovery path remains explicit. No fixed WebView2 runtime ZIP, native egui alternative, workspace editor, rollback system, attachment workflow, multi-agent UI, or language localization is included.

Developer commands used: `npm ci`, `npm run build`, `cargo check`, `cargo test`, the explicitly invoked real companion handshake test, `cargo build --release`, `node scripts/protocol-smoke.mjs`, `node scripts/live-acceptance.mjs`, `node tests/ui.mjs`, `npm audit --omit=dev --audit-level=high`, and `scripts/build-portable.ps1` / `scripts/verify-release.ps1`.

The release ZIP is under `dist-portable/RustPortableHarness-win-x64-portable.zip`, with a sibling `.sha256` file. Keep the user's signed-in Desktop test folder separate; it is never copied into the release.
