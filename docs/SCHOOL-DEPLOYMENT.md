# School desktop deployment

The portable harness is designed to run from a user-owned Desktop folder. It does not require PowerShell, Node.js, Python, Rust, npm, an installer, registry changes, administrator elevation, or a machine-wide PATH entry. The runtime launches the bundled `codex.exe` directly.

The school computer still needs:

- a compatible Microsoft Edge WebView2 Runtime;
- permission to launch the portable executable and write `data/` beside it;
- network access to the Codex service, its sign-in page, and any configured proxy or certificate chain;
- an eligible Codex account and school permission to use an AI coding agent with the selected project;
- read/write access to the project folder when Workspace write is selected.

PowerShell is not a harness dependency. An agent command that explicitly asks for `powershell.exe`, `pwsh`, or another unavailable developer tool can still fail because Codex executes the command on the school machine. Ask Codex to use an available `cmd.exe` or an installed project tool instead, or decline the command. The harness never installs a shell or bypasses AppLocker, endpoint protection, proxy rules, or school policy.

Common locked-down-machine failures include a missing WebView2 runtime, blocked unsigned or portable executables, a read-only Desktop or redirected profile, blocked browser sign-in redirects, proxy or TLS inspection errors, disabled child processes, antivirus quarantine of the bundled executable, project folders outside the user's access, and policy restrictions on sending source files to a remote model service. The Diagnostics panel reports the selected binary, portable paths, protocol version, sandbox, and approval policy without including account tokens.

Before classroom use, test the exact release ZIP on a representative non-admin account. Sign in only on a computer approved for that account, and sign out or use **Wipe portable account & history** before leaving a shared computer. `data/codex-home/` may contain sensitive authentication state.
