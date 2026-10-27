param([Parameter(Mandatory=$true)][string]$ReleaseRoot)
$ErrorActionPreference='Stop'
$root=(Resolve-Path -LiteralPath $ReleaseRoot).Path
foreach($name in @('RustPortableHarness.exe','codex.exe','README-PORTABLE.md','START-HERE.txt','THIRD-PARTY-LICENSES.txt','docs/licenses/DEPENDENCY-LICENSES.txt')) {
    if(-not (Test-Path -LiteralPath (Join-Path $root $name))) { throw "Release file missing: $name" }
}
$exes=@('RustPortableHarness.exe','codex.exe'); if(Test-Path -LiteralPath (Join-Path $root 'claude.exe')) { $exes += 'claude.exe' }
foreach($name in $exes) {
    $pe=[IO.File]::OpenRead((Join-Path $root $name))
    try {
        $reader=[IO.BinaryReader]::new($pe)
        if($reader.ReadUInt16() -ne 0x5a4d) { throw "$name is not a PE executable" }
        $null=$pe.Seek(0x3c,[IO.SeekOrigin]::Begin);$offset=$reader.ReadUInt32()
        $null=$pe.Seek($offset,[IO.SeekOrigin]::Begin)
        if($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne 0x8664) { throw "$name is not Windows x64" }
    } finally { $pe.Dispose() }
}
$exe=Join-Path $root 'codex.exe'
if((& $exe --version).Trim() -ne 'codex-cli 0.163.0') { throw 'Codex version mismatch' }
$expected='2692C8C7BB2CF8B0AEFE638741FC90236F4B824C59578CE84994F9DE58200993'
if((Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash -ne $expected) { throw 'Codex SHA-256 differs from the verified official 0.163.0 package' }
if(Test-Path (Join-Path $root 'data')) {
    $files=@(Get-ChildItem -LiteralPath (Join-Path $root 'data') -Recurse -File -Force)
    if($files.Count -gt 0) { throw 'Release staging data must be blank; refusing to package account or app state' }
}
foreach($d in @('data','data/codex-home','data/claude-home','data/accounts','data/logs','data/temp','projects')) { $null=New-Item -ItemType Directory -Force (Join-Path $root $d) }
$probe=Join-Path $root 'data/.verify-write'
Set-Content -LiteralPath $probe 'ok';Remove-Item -LiteralPath $probe -Force
Write-Host "Release verified: $root"
Write-Host 'Windows x64 executables; official Codex hash/version match; portable data is blank and writable.'
$claude=Join-Path $root 'claude.exe'
if(Test-Path -LiteralPath $claude) { $env:DISABLE_AUTOUPDATER='1'; $cv=(& $claude --version).Trim(); if($cv -notmatch '\(Claude Code\)$') { throw "claude.exe is not Claude Code: $cv" }; Write-Host "Claude Code included: $cv" } else { Write-Host 'Claude Code: not included' }
if(Test-Path -LiteralPath (Join-Path $root 'tools/PortableGit/bin/bash.exe')) { Write-Host 'PortableGit: included' }
Write-Host 'WebView2: required at runtime'
