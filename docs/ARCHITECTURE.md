# Architecture

The browser UI never receives a raw process handle, shell bridge, filesystem bridge, or API key. Tauri commands are narrow domain operations and the Rust process owns the child transport.

```mermaid
sequenceDiagram
  participant UI as React UI
  participant T as Tauri commands
  participant R as Rpc runtime
  participant C as codex.exe app-server
  UI->>T: connect()
  T->>R: spawn beside executable + CODEX_HOME=data/codex-home
  R->>C: initialize (one JSON object per line)
  C-->>R: initialize result
  R->>C: initialized notification
  R->>C: model/list, account/read
  R-->>UI: version, models, account, lifecycle
  UI->>T: open_thread / send_turn
  T->>R: validated request
  R->>C: thread/start or thread/resume; turn/start
  C-->>R: turn/item/delta notifications
  R-->>UI: snapshot generation event
  C->>R: server request (approval)
  R-->>UI: pending approval with exact params
  UI->>T: decide(key, decision)
  T->>R: schema-defined JSON-RPC response
  R->>C: response with matching request id
```

`Runtime` is the authoritative in-memory snapshot. It stores bounded visible items, history cursors, active turn state, model/account data, diffs supplied by Codex, and pending approvals. UI remounts request `snapshot()` and subscribe only to a generation-change event. A stale child generation cannot mutate a replacement connection.

The child process has independent stdout, stderr, and lifecycle tasks. Stdout is JSONL and has an 8 MiB frame limit. Stderr is consumed for back-pressure but never forwarded as protocol or rendered as agent output. Requests have monotonically increasing IDs, a timeout, and a pending map. All writes go through one Tokio channel.

Shutdown cancels pending requests, drops the child containment job, and stops accepting UI operations. Unexpected exit sets a failed lifecycle and clears approvals; the user must explicitly reconnect. The client never replays an in-flight command.
