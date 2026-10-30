param(
  [string]$CodexExecutable = "",
  # Optional: official claude.exe (Claude Code 2.1+) to include so students can use Claude without installing anything.
  [string]$ClaudeExecutable = "",
  # Optional: an extracted PortableGit folder (contains bin\bash.exe). Gives Claude a Bash shell on PCs without Git.
  [string]$PortableGit = "",
  # Optional: a "tools" folder prepared with Programming tools in the app (Python, Rust, Git,
  # PowerShell, Node.js). Bundling it saves every student the downloads.
  [string]$ToolsFrom = "",
  [string]$OutputRoot = "dist-portable",
  [switch]$SkipBuild
)
$ErrorActionPreference = "Stop"
function Require-Command($Name) { if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) { throw "Required developer tool missing: $Name" } }
Require-Command node; Require-Command npm; Require-Command cargo
if ([string]::IsNullOrWhiteSpace($CodexExecutable)) { $CodexExecutable = (Get-Command codex -ErrorAction Stop).Source; if ($CodexExecutable.EndsWith('.ps1')) { $CodexExecutable = Join-Path (Split-Path $CodexExecutable) 'node_modules/@openai/codex/node_modules/@openai/codex-win32-x64/vendor/x86_64-pc-windows-msvc/bin/codex.exe' } }
$CodexExecutable = (Resolve-Path $CodexExecutable).Path
$version = (& $CodexExecutable --version).Trim(); if ($version -ne 'codex-cli 0.163.0') { throw "Expected codex-cli 0.163.0, got $version" }
if ($ClaudeExecutable) {
  $ClaudeExecutable = (Resolve-Path $ClaudeExecutable).Path
  $env:DISABLE_AUTOUPDATER = '1'
  $claudeVersion = (& $ClaudeExecutable --version).Trim()
  if ($claudeVersion -notmatch '^(\d+)\.(\d+)\.\d+ \(Claude Code\)') { throw "Not the official Claude Code program: $claudeVersion" }
  if ([int]$Matches[1] -lt 2 -or ([int]$Matches[1] -eq 2 -and [int]$Matches[2] -lt 1)) { throw "Claude Code 2.1 or newer is required, got $claudeVersion" }
}
if ($ToolsFrom) {
  $ToolsFrom = (Resolve-Path $ToolsFrom).Path
  if (-not (Test-Path (Join-Path $ToolsFrom "*"))) { throw "Tools folder is empty: $ToolsFrom" }
}
if ($PortableGit) {
  $PortableGit = (Resolve-Path $PortableGit).Path
  if (-not (Test-Path (Join-Path $PortableGit 'bin/bash.exe'))) { throw "PortableGit folder must contain bin\bash.exe" }
}
$null=New-Item -ItemType Directory -Force $OutputRoot
$output=(Resolve-Path $OutputRoot).Path
$stage = Join-Path $output ('staging/' + [Guid]::NewGuid().ToString('N') + '/RustPortableHarness')
$null=New-Item -ItemType Directory -Force $stage
if (-not $SkipBuild) { npm ci; if($LASTEXITCODE){throw 'npm ci failed'}; npm run build; if($LASTEXITCODE){throw 'Frontend build failed'}; cargo build --release --manifest-path src-tauri/Cargo.toml; if($LASTEXITCODE){throw 'Rust build failed'} }
$app = Resolve-Path 'src-tauri/target/release/RustPortableHarness.exe'; Copy-Item $app $stage
Copy-Item $CodexExecutable (Join-Path $stage 'codex.exe')
$targetVendor = Split-Path (Split-Path $CodexExecutable); $resources = Join-Path $targetVendor 'codex-resources'; if (Test-Path $resources) { Copy-Item $resources (Join-Path $stage 'codex-resources') -Recurse }
$codexPath = Join-Path $targetVendor 'codex-path'; if (Test-Path $codexPath) { Copy-Item $codexPath (Join-Path $stage 'codex-path') -Recurse }
if ($ClaudeExecutable) {
  Copy-Item $ClaudeExecutable (Join-Path $stage 'claude.exe')
  $claudeHash = (Get-FileHash (Join-Path $stage 'claude.exe') -Algorithm SHA256).Hash; Set-Content (Join-Path $stage 'claude.exe.sha256') "$claudeHash  claude.exe"
}
if ($ToolsFrom) { Copy-Item $ToolsFrom (Join-Path $stage 'tools') -Recurse }
if ($PortableGit -and -not (Test-Path (Join-Path $stage 'tools/git'))) { $null = New-Item -ItemType Directory -Force (Join-Path $stage 'tools'); Copy-Item $PortableGit (Join-Path $stage 'tools/git') -Recurse }
foreach ($d in @('data','data/codex-home','data/claude-home','data/accounts','data/logs','data/temp','projects')) { New-Item -ItemType Directory -Force (Join-Path $stage $d) | Out-Null }
Copy-Item README-PORTABLE.md,START-HERE.txt,THIRD-PARTY-LICENSES.txt,LICENSE -Destination $stage
Copy-Item docs -Destination (Join-Path $stage 'docs') -Recurse
$codeModeHost = Join-Path (Split-Path $CodexExecutable) 'codex-code-mode-host.exe'; if (Test-Path $codeModeHost) { Copy-Item $codeModeHost -Destination $stage }
$packageManifest = Join-Path $targetVendor 'codex-package.json'; if (Test-Path $packageManifest) { Copy-Item $packageManifest -Destination $stage }
$hash = (Get-FileHash (Join-Path $stage 'codex.exe') -Algorithm SHA256).Hash; Set-Content (Join-Path $stage 'codex.exe.sha256') "$hash  codex.exe"
& ./scripts/verify-release.ps1 -ReleaseRoot $stage
$zip = Join-Path (Resolve-Path $OutputRoot).Path 'RustPortableHarness-win-x64-portable.zip'; if (Test-Path $zip) { Remove-Item $zip -Force }; Compress-Archive -Path $stage -DestinationPath $zip -CompressionLevel Optimal
$zipHash=(Get-FileHash $zip -Algorithm SHA256).Hash; Set-Content ($zip + '.sha256') "$zipHash  RustPortableHarness-win-x64-portable.zip"; Write-Host "Portable release: $zip"; Write-Host "Stage: $stage"; Write-Host "Size: $([math]::Round((Get-Item $zip).Length / 1MB, 2)) MiB"; Write-Host "Codex SHA-256: $hash"
if ($ClaudeExecutable) { Write-Host "Claude: $claudeVersion (SHA-256 $claudeHash)" } else { Write-Host 'Claude: not included (students can still choose claude.exe in Settings > Advanced)' }
if ($ToolsFrom) { Write-Host "Programming tools: included from $ToolsFrom" }
if ($PortableGit) { Write-Host 'PortableGit: included (tools\git)' }
