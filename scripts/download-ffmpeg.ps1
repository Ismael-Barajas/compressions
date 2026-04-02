# Download static FFmpeg and ffprobe binaries for Windows.
# Places them in src-tauri/binaries/ with Tauri target-triple naming.

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$BinDir = Join-Path $ScriptDir "..\src-tauri\binaries"
New-Item -ItemType Directory -Force -Path $BinDir | Out-Null

$Target = "x86_64-pc-windows-msvc"
$Url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip"
$TempZip = Join-Path $env:TEMP "ffmpeg-win.zip"
$TempDir = Join-Path $env:TEMP "ffmpeg_win_extract"

Write-Host "Downloading FFmpeg for Windows..."
Invoke-WebRequest -Uri $Url -OutFile $TempZip

Write-Host "Extracting..."
if (Test-Path $TempDir) { Remove-Item -Recurse -Force $TempDir }
Expand-Archive -Path $TempZip -DestinationPath $TempDir

$ExtractDir = Get-ChildItem -Path $TempDir -Directory | Where-Object { $_.Name -match "ffmpeg-" } | Select-Object -First 1
$FfmpegSrc = Join-Path $ExtractDir.FullName "bin\ffmpeg.exe"
$FfprobeSrc = Join-Path $ExtractDir.FullName "bin\ffprobe.exe"

Copy-Item $FfmpegSrc (Join-Path $BinDir "ffmpeg-$Target.exe")
Copy-Item $FfprobeSrc (Join-Path $BinDir "ffprobe-$Target.exe")

Remove-Item -Recurse -Force $TempZip, $TempDir

Write-Host "FFmpeg binaries installed to $BinDir"
Get-ChildItem $BinDir
