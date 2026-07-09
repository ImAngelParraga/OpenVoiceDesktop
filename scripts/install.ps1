param(
    [string]$Version = "latest",
    [string]$InstallDir = "$env:LOCALAPPDATA\OpenVoiceDesktop",
    [switch]$NoStartup,
    [switch]$Start
)

$ErrorActionPreference = "Stop"
$Repo = "ImAngelParraga/OpenVoiceDesktop"
$ExeName = "openvoice-desktop.exe"
$ConfigDir = Join-Path $env:APPDATA "openvoice-desktop"
$ConfigPath = Join-Path $ConfigDir "config.toml"
$StartupDir = [Environment]::GetFolderPath("Startup")
$ShortcutPath = Join-Path $StartupDir "OpenVoiceDesktop.lnk"

function Get-Release {
    if ($Version -eq "latest") {
        return Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
    }
    return Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/tags/$Version"
}

function Write-Config {
    if (Test-Path $ConfigPath) {
        Write-Host "Keeping existing config: $ConfigPath"
        return
    }

    New-Item -ItemType Directory -Force -Path $ConfigDir | Out-Null
    @"
[asr]
# Any OpenAI-compatible transcription endpoint root.
# Requests are sent to {base_url}/v1/audio/transcriptions.
base_url = "http://127.0.0.1:9000"
model = "whisper-1"
# language = "en"
# prompt = "Optional vocabulary or formatting hint"

[audio]
sample_rate_hz = 16000
channels = 1

[behavior]
restore_clipboard = true
clipboard_restore_delay_ms = 250
"@ | Set-Content -Path $ConfigPath -Encoding UTF8
    Write-Host "Created config: $ConfigPath"
}

function New-StartupShortcut {
    if ($NoStartup) {
        return
    }

    $WshShell = New-Object -ComObject WScript.Shell
    $Shortcut = $WshShell.CreateShortcut($ShortcutPath)
    $Shortcut.TargetPath = Join-Path $InstallDir $ExeName
    $Shortcut.Arguments = "daemon"
    $Shortcut.WorkingDirectory = $InstallDir
    $Shortcut.Description = "OpenVoiceDesktop tray app"
    $Shortcut.Save()
    Write-Host "Created startup shortcut: $ShortcutPath"
}

$Release = Get-Release
$Asset = $Release.assets | Where-Object { $_.name -eq "openvoice-desktop-windows-x86_64.zip" } | Select-Object -First 1
if (-not $Asset) {
    throw "Release does not contain openvoice-desktop-windows-x86_64.zip"
}

$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("openvoice-desktop-" + [Guid]::NewGuid())
$ZipPath = Join-Path $TempDir $Asset.name
New-Item -ItemType Directory -Force -Path $TempDir | Out-Null
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

Write-Host "Downloading $($Asset.browser_download_url)"
Invoke-WebRequest -Uri $Asset.browser_download_url -OutFile $ZipPath
Expand-Archive -Path $ZipPath -DestinationPath $TempDir -Force

$DownloadedExe = Get-ChildItem -Path $TempDir -Recurse -Filter $ExeName | Select-Object -First 1
if (-not $DownloadedExe) {
    throw "Downloaded archive did not contain $ExeName"
}

Copy-Item -Path $DownloadedExe.FullName -Destination (Join-Path $InstallDir $ExeName) -Force
Write-Config
New-StartupShortcut

if ($Start) {
    Start-Process -FilePath (Join-Path $InstallDir $ExeName) -ArgumentList "daemon"
}

Remove-Item -Path $TempDir -Recurse -Force

Write-Host ""
Write-Host "OpenVoiceDesktop installed."
Write-Host "Binary: $(Join-Path $InstallDir $ExeName)"
Write-Host "Config: $ConfigPath"
Write-Host ""
Write-Host "Next steps:"
Write-Host "  1. Edit $ConfigPath and set your ASR base_url/model."
Write-Host "  2. Store your token in Windows Credential Manager:"
Write-Host '     cmdkey /generic:openvoice-desktop/asr-token /user:asr /pass:YOUR_TOKEN'
Write-Host "  3. Start OpenVoiceDesktop from the Start menu shortcut or run:"
Write-Host "     $(Join-Path $InstallDir $ExeName) daemon"
Write-Host ""
Write-Host "Windows V1 release binaries are unsigned. SmartScreen may show a warning."
