# Z-SHOT — Copyright (C) 2026 R. Trunk
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Builds the release exe and the installer: installer\Output\Z-SHOT-Setup-<version>.exe
# Needs Rust (cargo) and Inno Setup 7 (or 6).
param([switch]$SkipCargo)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$app = Join-Path $root 'app'

$version = (Select-String -Path (Join-Path $app 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
Write-Host "Z-SHOT $version"

if (-not $SkipCargo) {
    Push-Location $app
    try {
        cargo build --release
        if ($LASTEXITCODE -ne 0) { throw "cargo build --release fehlgeschlagen" }
    } finally { Pop-Location }
}
$exe = Join-Path $app 'target\release\z-shot.exe'
if (-not (Test-Path $exe)) { throw "Nicht gefunden: $exe" }

# The icon is rendered by build.rs into the newest build output directory.
$ico = Get-ChildItem (Join-Path $app 'target\release\build') -Recurse -Filter 'z-shot.ico' -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1

$iscc = @(
    "$env:ProgramFiles\Inno Setup 7\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:LOCALAPPDATA\Programs\Inno Setup 7\ISCC.exe",
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw "Inno Setup (ISCC.exe) nicht gefunden" }

$defs = @("/DAppVersion=$version", "/DAppExe=$exe")
if ($ico) { $defs += "/DAppIcon=$($ico.FullName)" }
& $iscc /Q @defs (Join-Path $PSScriptRoot 'z-shot.iss')
if ($LASTEXITCODE -ne 0) { throw "ISCC fehlgeschlagen ($LASTEXITCODE)" }

$out = Join-Path $PSScriptRoot "Output\Z-SHOT-Setup-$version.exe"
Write-Host "Fertig: $out"
