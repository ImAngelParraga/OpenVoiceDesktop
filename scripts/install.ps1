param(
    [string]$Version = "latest",
    [string]$InstallDir = "$env:LOCALAPPDATA\OpenVoiceDesktop",
    [string]$SourceDir = (Get-Location).Path,
    [switch]$BuildFromSource,
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
$AssetName = "openvoice-desktop-windows-x86_64.zip"

function Test-Command {
    param([string]$Name)
    return $null -ne (Get-Command $Name -ErrorAction SilentlyContinue)
}

function Get-GitHubHeaders {
    $Token = $env:GH_TOKEN
    if (-not $Token) {
        $Token = $env:GITHUB_TOKEN
    }
    if ($Token) {
        return @{ Authorization = "Bearer $Token"; Accept = "application/vnd.github+json" }
    }
    return @{ Accept = "application/vnd.github+json" }
}

function Install-RustIfMissing {
    if (Test-Command cargo) {
        return
    }

    if (-not (Test-Command winget)) {
        throw "cargo is not installed. Install Rust from https://rustup.rs/, then rerun this script."
    }

    Write-Host "Installing Rust via winget..."
    winget install --id Rustlang.Rustup -e --source winget
    $CargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
    if (Test-Path $CargoBin) {
        $env:Path = "$CargoBin;$env:Path"
    }
    if (-not (Test-Command cargo)) {
        throw "Rust was installed, but cargo is not available in this PowerShell session. Open a new PowerShell window and rerun this script."
    }
}

function Get-Release {
    $Headers = Get-GitHubHeaders
    if ($Version -eq "latest") {
        return Invoke-RestMethod -Headers $Headers -Uri "https://api.github.com/repos/$Repo/releases/latest"
    }
    return Invoke-RestMethod -Headers $Headers -Uri "https://api.github.com/repos/$Repo/releases/tags/$Version"
}

function Save-ReleaseAsset {
    param([string]$Destination)

    if (Test-Command gh) {
        gh auth status *> $null
        $GhAuthenticated = $LASTEXITCODE -eq 0
    } else {
        $GhAuthenticated = $false
    }

    if ($GhAuthenticated) {
        if ($Version -eq "latest") {
            gh release download --repo $Repo --pattern $AssetName --dir (Split-Path $Destination) --clobber
        } else {
            gh release download $Version --repo $Repo --pattern $AssetName --dir (Split-Path $Destination) --clobber
        }
        $Downloaded = Join-Path (Split-Path $Destination) $AssetName
        if (Test-Path $Downloaded) {
            if ($Downloaded -ne $Destination) {
                Move-Item -Path $Downloaded -Destination $Destination -Force
            }
            return $true
        }
    }

    $Release = Get-Release
    $Asset = $Release.assets | Where-Object { $_.name -eq $AssetName } | Select-Object -First 1
    if (-not $Asset) {
        throw "Release does not contain $AssetName"
    }

    $Headers = Get-GitHubHeaders
    $Headers["Accept"] = "application/octet-stream"
    Write-Host "Downloading $($Asset.url)"
    Invoke-WebRequest -Headers $Headers -Uri $Asset.url -OutFile $Destination
    return $true
}

function Install-FromSource {
    $CargoToml = Join-Path $SourceDir "Cargo.toml"
    if (-not (Test-Path $CargoToml)) {
        throw "Source checkout not found at $SourceDir. Clone the repo or run with -SourceDir <path>."
    }

    Install-RustIfMissing
    Push-Location $SourceDir
    try {
        cargo build --release
        $BuiltExe = Join-Path $SourceDir "target\release\$ExeName"
        if (-not (Test-Path $BuiltExe)) {
            throw "Build completed but $BuiltExe was not found."
        }
        New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
        Copy-Item -Path $BuiltExe -Destination (Join-Path $InstallDir $ExeName) -Force
    } finally {
        Pop-Location
    }
}

function Install-FromRelease {
    $TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("openvoice-desktop-" + [Guid]::NewGuid())
    $ZipPath = Join-Path $TempDir $AssetName
    New-Item -ItemType Directory -Force -Path $TempDir | Out-Null
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

    try {
        Save-ReleaseAsset -Destination $ZipPath | Out-Null
        Expand-Archive -Path $ZipPath -DestinationPath $TempDir -Force

        $DownloadedExe = Get-ChildItem -Path $TempDir -Recurse -Filter $ExeName | Select-Object -First 1
        if (-not $DownloadedExe) {
            throw "Downloaded archive did not contain $ExeName"
        }

        Copy-Item -Path $DownloadedExe.FullName -Destination (Join-Path $InstallDir $ExeName) -Force
    } finally {
        Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
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
    $ExePath = Join-Path $InstallDir $ExeName
    $Shortcut.TargetPath = "powershell.exe"
    $Shortcut.Arguments = "-NoProfile -WindowStyle Hidden -Command `"Start-Process -FilePath '$ExePath' -ArgumentList 'daemon' -WindowStyle Hidden`""
    $Shortcut.WorkingDirectory = $InstallDir
    $Shortcut.Description = "OpenVoiceDesktop tray app"
    $Shortcut.Save()
    Write-Host "Created startup shortcut: $ShortcutPath"
}

if ($BuildFromSource) {
    Install-FromSource
} else {
    try {
        Install-FromRelease
    } catch {
        $CargoToml = Join-Path $SourceDir "Cargo.toml"
        if (Test-Path $CargoToml) {
            Write-Warning "Release install failed: $($_.Exception.Message)"
            Write-Host "Falling back to source build from $SourceDir"
            Install-FromSource
        } else {
            throw
        }
    }
}

Write-Config
New-StartupShortcut

if ($Start) {
    Start-Process -FilePath (Join-Path $InstallDir $ExeName) -ArgumentList "daemon" -WindowStyle Hidden
}

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
