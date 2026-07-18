param(
    [Parameter(Mandatory = $true)]
    [string]$InstallerPath
)

$ErrorActionPreference = "Stop"
$ProductName = "OpenVoiceDesktop"
$ConfigDir = Join-Path $env:APPDATA "openvoice-desktop"
$ConfigPath = Join-Path $ConfigDir "config.toml"
$StartMenuShortcut = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\OpenVoiceDesktop\OpenVoiceDesktop.lnk"
$RunKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
$RunValueName = "OpenVoiceDesktop"

function Get-UninstallEntry {
    Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*" -ErrorAction SilentlyContinue |
        Where-Object DisplayName -eq $ProductName |
        Select-Object -First 1
}

function Invoke-Setup {
    $process = Start-Process -FilePath $InstallerPath -ArgumentList "/S" -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "installer exited with $($process.ExitCode)"
    }
}

Invoke-Setup
$entry = Get-UninstallEntry
if (-not $entry) { throw "uninstall registration was not created" }
if (-not $entry.InstallLocation) { throw "uninstall registration has no InstallLocation" }
$InstallLocation = $entry.InstallLocation.Trim('"')
$ExePath = Join-Path $InstallLocation "openvoice-desktop.exe"
if (-not (Test-Path $ExePath)) { throw "installed executable not found at $ExePath" }
if (-not (Test-Path $StartMenuShortcut)) { throw "Start menu shortcut was not created" }

New-Item -ItemType Directory -Force -Path $ConfigDir | Out-Null
"schema_version = 1`n" | Set-Content -Path $ConfigPath -Encoding ascii
$ConfigHash = (Get-FileHash $ConfigPath -Algorithm SHA256).Hash

Invoke-Setup
if ((Get-FileHash $ConfigPath -Algorithm SHA256).Hash -ne $ConfigHash) {
    throw "reinstall changed existing config"
}

schtasks.exe /Create /F /SC ONLOGON /TN OpenVoiceDesktop /TR 'cmd.exe /c exit 0' | Out-Null
if ($LASTEXITCODE -ne 0) { throw "could not create autostart smoke-test task" }
New-Item -Path $RunKey -Force | Out-Null
New-ItemProperty -Path $RunKey -Name $RunValueName -PropertyType String -Value 'cmd.exe /c exit 0' -Force | Out-Null

$entry = Get-UninstallEntry
$Uninstaller = $entry.UninstallString.Trim('"')
$process = Start-Process -FilePath $Uninstaller -ArgumentList "/S" -Wait -PassThru
if ($process.ExitCode -ne 0) { throw "uninstaller exited with $($process.ExitCode)" }
if (Test-Path $ExePath) { throw "uninstall left installed executable" }
if (Test-Path $StartMenuShortcut) { throw "uninstall left Start menu shortcut" }
if (-not (Test-Path $ConfigPath)) { throw "uninstall removed user config" }

schtasks.exe /Query /TN OpenVoiceDesktop *> $null
if ($LASTEXITCODE -eq 0) { throw "uninstall left autostart task" }
if (Get-ItemProperty -Path $RunKey -Name $RunValueName -ErrorAction SilentlyContinue) {
    throw "uninstall left current-user autostart registration"
}

Write-Host "Windows installer smoke: PASS"
exit 0
