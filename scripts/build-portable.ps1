param([string]$CodexExecutable = "", [string]$OutputRoot = "dist-portable", [switch]$SkipBuild)
$ErrorActionPreference = "Stop"
function Require-Command($Name) { if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) { throw "Required developer tool missing: $Name" } }
Require-Command node; Require-Command npm; Require-Command cargo
if ([string]::IsNullOrWhiteSpace($CodexExecutable)) { $CodexExecutable = (Get-Command codex -ErrorAction Stop).Source; if ($CodexExecutable.EndsWith('.ps1')) { $CodexExecutable = Join-Path (Split-Path $CodexExecutable) 'node_modules/@openai/codex/node_modules/@openai/codex-win32-x64/vendor/x86_64-pc-windows-msvc/bin/codex.exe' } }
$CodexExecutable = (Resolve-Path $CodexExecutable).Path
$version = (& $CodexExecutable --version).Trim(); if ($version -ne 'codex-cli 0.163.0') { throw "Expected codex-cli 0.163.0, got $version" }
$null=New-Item -ItemType Directory -Force $OutputRoot
$output=(Resolve-Path $OutputRoot).Path
$stage = Join-Path $output ('staging/' + [Guid]::NewGuid().ToString('N') + '/RustPortableHarness')
$null=New-Item -ItemType Directory -Force $stage
if (-not $SkipBuild) { npm ci; if($LASTEXITCODE){throw 'npm ci failed'}; npm run build; if($LASTEXITCODE){throw 'Frontend build failed'}; cargo build --release --manifest-path src-tauri/Cargo.toml; if($LASTEXITCODE){throw 'Rust build failed'} }
$app = Resolve-Path 'src-tauri/target/release/RustPortableHarness.exe'; Copy-Item $app $stage
Copy-Item $CodexExecutable (Join-Path $stage 'codex.exe')
$targetVendor = Split-Path (Split-Path $CodexExecutable); $resources = Join-Path $targetVendor 'codex-resources'; if (Test-Path $resources) { Copy-Item $resources (Join-Path $stage 'codex-resources') -Recurse }
$codexPath = Join-Path $targetVendor 'codex-path'; if (Test-Path $codexPath) { Copy-Item $codexPath (Join-Path $stage 'codex-path') -Recurse }
foreach ($d in @('data','data/codex-home','data/logs','data/temp','projects')) { New-Item -ItemType Directory -Force (Join-Path $stage $d) | Out-Null }
Copy-Item README-PORTABLE.md,THIRD-PARTY-LICENSES.txt,LICENSE -Destination $stage
Copy-Item docs -Destination (Join-Path $stage 'docs') -Recurse
$codeModeHost = Join-Path (Split-Path $CodexExecutable) 'codex-code-mode-host.exe'; if (Test-Path $codeModeHost) { Copy-Item $codeModeHost -Destination $stage }
$packageManifest = Join-Path $targetVendor 'codex-package.json'; if (Test-Path $packageManifest) { Copy-Item $packageManifest -Destination $stage }
$hash = (Get-FileHash (Join-Path $stage 'codex.exe') -Algorithm SHA256).Hash; Set-Content (Join-Path $stage 'codex.exe.sha256') "$hash  codex.exe"
& ./scripts/verify-release.ps1 -ReleaseRoot $stage
$zip = Join-Path (Resolve-Path $OutputRoot).Path 'RustPortableHarness-win-x64-portable.zip'; if (Test-Path $zip) { Remove-Item $zip -Force }; Compress-Archive -Path $stage -DestinationPath $zip -CompressionLevel Optimal
$zipHash=(Get-FileHash $zip -Algorithm SHA256).Hash; Set-Content ($zip + '.sha256') "$zipHash  RustPortableHarness-win-x64-portable.zip"; Write-Host "Portable release: $zip"; Write-Host "Stage: $stage"; Write-Host "Size: $([math]::Round((Get-Item $zip).Length / 1MB, 2)) MiB"; Write-Host "Codex SHA-256: $hash"
