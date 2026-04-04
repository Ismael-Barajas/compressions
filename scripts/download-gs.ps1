# Download Ghostscript binaries for Windows.
# Places them in src-tauri/binaries/ with Tauri target-triple naming.
# Requires 7-Zip to extract the NSIS installer.

$ErrorActionPreference = "Stop"

$GsVersion = "10.07.0"
$GsVersionNoDots = "10070"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$BinDir = Join-Path $ScriptDir "..\src-tauri\binaries"
New-Item -ItemType Directory -Force -Path $BinDir | Out-Null

$Target = "x86_64-pc-windows-msvc"
$Url = "https://github.com/ArtifexSoftware/ghostpdl-downloads/releases/download/gs$GsVersionNoDots/gs${GsVersionNoDots}w64.exe"
$TempDir = Join-Path $env:TEMP "gs_extract"
$TempInstaller = Join-Path $env:TEMP "gs_installer.exe"

Write-Host "Downloading Ghostscript $GsVersion for Windows..."
Invoke-WebRequest -Uri $Url -OutFile $TempInstaller

# Find 7-Zip
$SevenZip = $null
foreach ($path in @(
    "C:\Program Files\7-Zip\7z.exe",
    "C:\Program Files (x86)\7-Zip\7z.exe",
    (Get-Command 7z -ErrorAction SilentlyContinue).Source
)) {
    if ($path -and (Test-Path $path)) {
        $SevenZip = $path
        break
    }
}

if (-not $SevenZip) {
    Write-Error "7-Zip is required to extract the Ghostscript installer. Install with: choco install 7zip"
    Remove-Item -Force $TempInstaller
    exit 1
}

Write-Host "Extracting installer with 7-Zip..."
if (Test-Path $TempDir) { Remove-Item -Recurse -Force $TempDir }
& $SevenZip x "-o$TempDir" $TempInstaller | Out-Null

# Find the binaries
$GsExe = Get-ChildItem -Path $TempDir -Recurse -Filter "gswin64c.exe" | Select-Object -First 1
$GsDll = Get-ChildItem -Path $TempDir -Recurse -Filter "gsdll64.dll" | Select-Object -First 1

if (-not $GsExe) {
    Write-Error "Could not find gswin64c.exe in extracted installer"
    Remove-Item -Recurse -Force $TempDir, $TempInstaller
    exit 1
}

Copy-Item $GsExe.FullName (Join-Path $BinDir "gs-$Target.exe")
Write-Host "Installed gs-$Target.exe"

if ($GsDll) {
    Copy-Item $GsDll.FullName (Join-Path $BinDir "gsdll64.dll")
    Write-Host "Installed gsdll64.dll (required alongside gs.exe)"
}

# Copy Ghostscript resource files (fonts, init scripts, ICC profiles)
$GsResDir = Join-Path $BinDir "gs-res"
$ResourceDir = Get-ChildItem -Path $TempDir -Recurse -Directory -Filter "Resource" | Where-Object { $_.FullName -match "gs" } | Select-Object -First 1
if ($ResourceDir) {
    $GsShareParent = $ResourceDir.Parent.FullName
    if (Test-Path $GsResDir) { Remove-Item -Recurse -Force $GsResDir }
    New-Item -ItemType Directory -Force -Path $GsResDir | Out-Null
    Copy-Item -Recurse (Join-Path $GsShareParent "Resource") $GsResDir
    if (Test-Path (Join-Path $GsShareParent "lib")) {
        Copy-Item -Recurse (Join-Path $GsShareParent "lib") $GsResDir
    }
    if (Test-Path (Join-Path $GsShareParent "iccprofiles")) {
        Copy-Item -Recurse (Join-Path $GsShareParent "iccprofiles") $GsResDir
    }
    Write-Host "Installed Ghostscript resources to $GsResDir"
}

Remove-Item -Recurse -Force $TempDir, $TempInstaller

Write-Host ""
Write-Host "Ghostscript binaries installed to $BinDir"
Get-ChildItem $BinDir -Filter "gs*"
