# Protocol version

This release is generated and tested against:

* Codex CLI: `codex-cli 0.163.0`
* Windows package: `@openai/codex` `0.163.0-win32-x64`
* Release binary SHA-256: `2692C8C7BB2CF8B0AEFE638741FC90236F4B824C59578CE84994F9DE58200993`
* Official package source: https://github.com/openai/codex

The checked-in bindings are generated with:

```powershell
codex.exe app-server generate-json-schema --out .\generated\json-schema
codex.exe app-server generate-ts --out .\generated\ts
```

The harness uses raw `serde_json::Value` at the transport boundary and validates the operations it sends. This keeps unknown future fields from crashing the client while `validate_binary` prevents silently mixing schemas from another release.

Compatibility finding: `untrusted` remains in the generated `AskForApproval` type, but this exact executable rejects it as a startup config value. The harness therefore uses the supported `on-request` mode with `read-only` by default and explicit `approvalsReviewer: user`. A Rust integration smoke test now exercises the actual launcher arguments, handshake, model list, and Job Object shutdown.

Official package provenance: npm tarball `https://registry.npmjs.org/@openai/codex/-/codex-0.163.0-win32-x64.tgz`, integrity `sha512-QQGQ/q9dCJ5C5jqhgFrwycpWG3szIyFNThMiKl6hYtu6N/eM3Qd7StesgnCFAfzVED8IkE3N3oH71szzcludqw==`. The installed binary was independently compared with this freshly downloaded package.

Implemented and exercised operations include `initialize`, `initialized`, `model/list`, `account/read`, `account/login/start`, `account/login/cancel`, `account/logout`, `thread/start`, `thread/resume`, `thread/read`, `thread/list`, `thread/turns/list`, `thread/archive`, `turn/start`, `turn/interrupt`, `windowsSandbox/setupStart`, and approval responses for `item/commandExecution/requestApproval`, `item/fileChange/requestApproval`, `execCommandApproval`, `applyPatchApproval`, and `item/permissions/requestApproval`.

The UI handles `turn/started`, `turn/completed`, `item/started`, `item/completed`, `item/agentMessage/delta`, command output deltas, file-change patch updates, turn diffs, account updates, login completion, warnings, and errors. Unsupported server requests receive a capability error or remain visible for safe cancellation; they are never silently accepted.
