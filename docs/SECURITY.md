# Security model

RustPortableHarness keeps the official agent programs as the policy boundary. The harness doesn't call
an LLM API itself, scrape terminal output, grant access on its own, or expose an HTTP server.

## Both agents

* Each account has its own home folder inside `data/` (`CODEX_HOME` for Codex, `CLAUDE_CONFIG_DIR` for
  Claude). These variables are set only for the child process, never for the user environment.
* The child working directory is the canonical project the user selected. Project files are never
  copied into portable data. Picking a whole drive, the profile, Desktop or Documents asks for
  confirmation.
* Child processes run hidden with piped stdio, inside a kill-on-close Job Object, and with `TEMP`/`TMP`
  pointing to `data/temp`.
* API keys and session variables inherited from the parent (`OPENAI_API_KEY`, `CODEX_API_KEY`, and every
  `ANTHROPIC*` / `CLAUDE*` variable) are removed so the portable sign-in is always the one used.
* The proxy comes from the environment or the per-user Windows proxy setting (read only), or from
  Advanced settings. `localhost` is always excluded so sign-in callbacks never go through a proxy. An
  optional school CA certificate is passed as `CODEX_CA_CERTIFICATE` / `NODE_EXTRA_CA_CERTS`.
* First-run permission mode is **Ask before changes** (read-only sandbox + on-request approval).
  **Work without asking** and **Full access** are explicit choices with a warning, and can't be changed
  in the middle of a turn.
* Approval requests show a plain-language summary plus the exact parameters. Only schema-defined
  responses are sent, correlated by request ID.
* Markdown is rendered with raw HTML disabled; links open only after confirmation and only for `https`.
* Protocol frames and streamed content are bounded (8 MiB frames, capped item text and output).
* Diagnostics never contain tokens, emails or chat contents.

## Programming tools

* Tools are downloaded only from their official release pages (astral-sh/uv,
  astral-sh/python-build-standalone, static.rust-lang.org, brechtsanders/winlibs_mingw, git-for-windows,
  PowerShell, nodejs.org) at pinned versions, and each file must match a pinned SHA-256 before it is used.
  Archives are unpacked with path checks (no `..` or absolute paths). Python is unpacked by the app itself
  (not `uv python install`), so nothing is written to the registry or the user profile.
* All tool state lives in the app folder: `CARGO_HOME`, `RUSTUP_HOME`, `UV_*` (Python installs,
  bin and cache), `PIP_CACHE_DIR`, `npm_config_cache`/`prefix`, and `GIT_CONFIG_GLOBAL`.
  rustup runs with `--no-modify-path`.
* The tools environment trusts the Windows certificate store (`UV_SYSTEM_CERTS`, `NODE_USE_SYSTEM_CA`;
  curl and Git use Schannel) and sets `PSExecutionPolicyPreference=RemoteSigned` for the app's own
  processes only. Group Policy settings still win.
* Codex is started with `shell_environment_policy.inherit="all"` so its commands see these settings;
  its default secret-name filter still applies. Both AIs get a short note listing the tools
  (`$CODEX_HOME/AGENTS.md` managed block, Claude `--append-system-prompt`).
* Run and Terminal open a normal console in the project folder. Project templates write only inside
  the new project folder.
* The default permission is now workspace-write + on-request ("Edit files in this project"), because
  Codex 0.163 refuses every write in read-only mode instead of asking. Configs still on the old
  read-only default are upgraded once, with a notice; a later read-only choice is kept.

## Claude specifics

* The harness runs `claude.exe -p --input-format stream-json --output-format stream-json
  --permission-prompt-tool stdio`. Claude Code decides which actions need permission and asks the
  harness through its control channel; the harness answers allow/deny only for that request ID.
* Permission modes: *Ask before changes* → `default`; *Edit files* / *Work without asking* →
  `acceptEdits`. In *Work without asking* the harness allows each request itself, as the user chose.
  `bypassPermissions` is never used.
* `--setting-sources user` by default, so a project's `.claude/settings*.json` (which can define hooks
  that run commands) is ignored unless a teacher enables project settings in Advanced.
* Auto-update, telemetry and error reporting are turned off for the portable child.
* Sign-in uses the official `claude auth login` flow; sign-out uses `claude auth logout`, which clears the
  sign-in wherever Claude keeps it (file or Windows Credential Manager).
* stderr is kept only as a short tail, shown when Claude exits unexpectedly, and never logged.

## Accounts

Multiple accounts are separate sign-ins, switched by the user. The harness never rotates accounts
automatically or combines their limits. Sharing logins and evading usage limits break OpenAI's and
Anthropic's terms.

The network and account policies of the host computer still apply. A portable EXE does not establish
permission to use an AI service or to send a school project's files to one.
