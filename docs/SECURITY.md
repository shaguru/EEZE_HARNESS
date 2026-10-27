# Security model

RustPortableHarness deliberately keeps the official Codex executable as the policy boundary. The harness does not call an LLM API, scrape terminal ANSI output, grant full access, or expose an HTTP server.

* `CODEX_HOME` is set only for the child and points to `data/codex-home/`. It is never added to the user environment.
* The child working directory is the canonical project selected by the user. Project files are not copied into portable data.
* First-run defaults are `read-only` sandbox and `on-request` approval policy, routed to the user. This binary rejects the legacy `untrusted` startup configuration even though its generated protocol enum retains the value. The UI offers `workspace-write`, `danger-full-access`, and `never` (auto-allow) only after an explicit preference save and shows an unsafe-mode warning. Read-only does not imply every read command needs approval; Codex decides which operations require a request.
* Server-originated requests are stored by JSON-RPC ID. The approval pane shows exact method and params. Only the response union generated for the shipped version is emitted.
* Markdown is rendered with raw HTML disabled; external links are shown in a confirmation dialog and only `https` URLs are opened.
* Protocol frames and streamed content are bounded. Agent output is displayed as text and is not interpreted as native commands or Tauri calls.
* Account files may be sensitive. They are never included in diagnostics, logs, source control, or the release ZIP. Use **Sign out** before sharing a folder.
* The process is started hidden on Windows, has piped stdio, and is contained in a kill-on-parent-close Job Object where Windows permits it. No registry, service, PATH, installer, or elevation is used.

The network and account policies of the host computer still apply. A portable EXE does not establish permission to use Codex or transmit source files from a school project.
