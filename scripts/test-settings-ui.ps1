param(
    [string]$ExePath = "$PSScriptRoot\..\target\release\openvoice-desktop.exe"
)

$ErrorActionPreference = "Stop"
if (-not (Test-Path $ExePath)) { throw "Missing executable: $ExePath" }

$process = Start-Process -FilePath $ExePath -ArgumentList "settings" -PassThru
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        Start-Sleep -Milliseconds 250
        $process.Refresh()
    } while ($process.MainWindowHandle -eq 0 -and [DateTime]::UtcNow -lt $deadline)

    if ($process.MainWindowHandle -eq 0) { throw "Settings window did not appear" }
    if ($process.MainWindowTitle -notlike "*OpenVoiceDesktop*") {
        throw "Unexpected settings window title: $($process.MainWindowTitle)"
    }

    $second = Start-Process -FilePath $ExePath -ArgumentList "settings" -PassThru
    $second.WaitForExit(5000) | Out-Null
    $process.Refresh()
    if ($process.HasExited) { throw "Second launch replaced existing settings instance" }
    Write-Host "Windows settings smoke: PASS"
} finally {
    if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
}
