# build.ps1 — Qeynos suite build + validate + install gate (Windows PowerShell 5.1).
# No `&&`, no ternary. Fails with a nonzero exit on any gate failure, naming the gate.
#
#   powershell -ExecutionPolicy Bypass -File build.ps1 _template
#   powershell -ExecutionPolicy Bypass -File build.ps1 -All
#
# Per crate: build (release) -> test (release) -> bundle (.clap + .vst3) ->
# clap-validator on .clap -> pluginval --strictness-level 8 on .vst3 ->
# install .clap to the per-user CLAP dir, .vst3 to the admin VST3 junction if present,
# and copy both into dist/.

param([string]$Crate, [switch]$All)

$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
if (-not $env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR = 'C:\qvs-target' }

# The rustup windows-gnu toolchain ships dlltool but no assembler, so raw-dylib
# import-library generation (windows-sys, parking_lot_core) fails. Portable
# MinGW-w64 binutils (as/dlltool/ld) from tools/bin/mingw64 fixes it. Must be on
# PATH for every build. See STATUS.md / CHECKPOINTS.md.
$mingwBin = Join-Path $PSScriptRoot 'tools\bin\mingw64\bin'
if (Test-Path $mingwBin) { $env:Path = "$mingwBin;$env:Path" }

$repo = $PSScriptRoot
$targetDir = $env:CARGO_TARGET_DIR
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
$clapValidator = Join-Path $repo 'tools\bin\clap-validator.exe'
$pluginval = Join-Path $repo 'tools\bin\pluginval.exe'

function Fail($gate, $message) {
    Write-Host ''
    Write-Host "==================== GATE FAILED: $gate ===================="
    Write-Host $message
    exit 1
}

function Invoke-Step($gate, $exe, $stepArgs) {
    Write-Host ''
    Write-Host "---- [$gate] $exe $($stepArgs -join ' ')"
    $output = & $exe @stepArgs 2>&1 | Out-String
    Write-Host $output
    if ($LASTEXITCODE -ne 0) { Fail $gate $output }
    return $output
}

# ---- Resolve the crate list -------------------------------------------------
$crates = @()
if ($All) {
    $pluginsDir = Join-Path $repo 'plugins'
    $crates = Get-ChildItem -Path $pluginsDir -Directory | ForEach-Object { $_.Name }
} elseif ($Crate) {
    $crates = @($Crate)
} else {
    Fail 'args' 'Specify a crate name (e.g. build.ps1 _template) or -All.'
}

# ---- Install destinations ---------------------------------------------------
$clapInstallDir = Join-Path $env:LOCALAPPDATA 'Programs\Common\CLAP\Qeynos'
$vst3InstallDir = 'C:\Program Files\Common Files\VST3\Qeynos'
$distClap = Join-Path $repo 'dist\clap'
$distVst3 = Join-Path $repo 'dist\vst3'
New-Item -ItemType Directory -Force -Path $clapInstallDir | Out-Null
New-Item -ItemType Directory -Force -Path $distClap | Out-Null
New-Item -ItemType Directory -Force -Path $distVst3 | Out-Null

function Test-Writable($dir) {
    if (-not (Test-Path $dir)) { return $false }
    $probe = Join-Path $dir ('.qvs-write-test-' + [System.Guid]::NewGuid().ToString('N'))
    try {
        Set-Content -Path $probe -Value 'x' -ErrorAction Stop
        Remove-Item -Path $probe -Force -ErrorAction SilentlyContinue
        return $true
    } catch {
        return $false
    }
}

foreach ($c in $crates) {
    Write-Host ''
    Write-Host "############################################################"
    Write-Host "## Building crate: $c"
    Write-Host "############################################################"

    Invoke-Step "build:$c"  $cargo @('build', '--release', '-p', $c)  | Out-Null
    Invoke-Step "test:$c"   $cargo @('test', '--release', '-p', $c)   | Out-Null
    Invoke-Step "bundle:$c" $cargo @('xtask', 'bundle', $c, '--release') | Out-Null

    $clapBundle = Join-Path $targetDir "bundled\$c.clap"
    $vst3Bundle = Join-Path $targetDir "bundled\$c.vst3"

    if (-not (Test-Path $clapBundle)) { Fail "bundle:$c" "Expected CLAP bundle not found: $clapBundle" }
    if (-not (Test-Path $vst3Bundle)) { Fail "bundle:$c" "Expected VST3 bundle not found: $vst3Bundle" }

    Invoke-Step "clap-validator:$c" $clapValidator @('validate', $clapBundle) | Out-Null
    Invoke-Step "pluginval:$c" $pluginval @(
        '--strictness-level', '8',
        '--skip-gui-tests',
        '--timeout-ms', '120000',
        '--validate', $vst3Bundle
    ) | Out-Null

    # ---- Install + dist copies ---------------------------------------------
    Copy-Item -Path $clapBundle -Destination $clapInstallDir -Recurse -Force
    Write-Host "Installed CLAP -> $clapInstallDir"

    Copy-Item -Path $clapBundle -Destination $distClap -Recurse -Force
    Copy-Item -Path $vst3Bundle -Destination $distVst3 -Recurse -Force
    Write-Host "Copied bundles -> dist/"

    if (Test-Writable $vst3InstallDir) {
        Copy-Item -Path $vst3Bundle -Destination $vst3InstallDir -Recurse -Force
        Write-Host "Installed VST3 -> $vst3InstallDir"
    } else {
        Write-Host "Skipped VST3 install (junction $vst3InstallDir absent or not writable)."
    }

    Write-Host ''
    Write-Host "==================== GREEN: $c passed all gates ===================="
}

exit 0
