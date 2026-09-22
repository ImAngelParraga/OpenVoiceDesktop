param(
    [Parameter(Mandatory = $true)]
    [string]$ExePath,
    [string]$ReportPath = "$PSScriptRoot\..\target\windows-runtime-smoke.md"
)

$ErrorActionPreference = "Stop"
if (-not (Test-Path -LiteralPath $ExePath -PathType Leaf)) {
    throw "Missing executable: $ExePath"
}

$cases = @(
    @{
        Id = "W01"; Title = "Recording overlay"
        Steps = "With Settings closed, start recording. Save overlay visibility, top/bottom position, compact/expanded style, and theme one at a time. Repeat with Settings open and on a secondary monitor with different DPI. Confirm one popup and each new setting on next recording."
    },
    @{
        Id = "W02"; Title = "Shortcuts and push-to-talk"
        Steps = "Change toggle, cancel, and language shortcuts; Save and use each without restart. Try a chord owned by another app: Save must reject it and old chord must still work. Confirm Escape works in other apps while idle. Enable push-to-talk and confirm recording stops exactly once on release."
    },
    @{
        Id = "W03"; Title = "Busy status, cancel, and pending Save"
        Steps = "Use a slow ASR endpoint. Stop recording, query 'openvoice-desktop status' during upload, then cancel from tray or shortcut. Confirm no text inserts. During another slow upload, Save a setting and confirm pending status followed by application after idle."
    },
    @{
        Id = "W04"; Title = "Recording output reduction"
        Steps = "With audible playback active, Save reduced output mode and a lower volume; start/stop recording and confirm playback reduces then restores. Repeat with muted mode, default output, and separately routed headset output. Record any routing mismatch or failed restore."
    },
    @{
        Id = "W05"; Title = "Save runtime truth"
        Steps = "Save one runtime field, confirm it takes effect without restart and only that field is reported. Stop resident daemon via 'openvoice-desktop shutdown' while Settings remains open; change a field and Save. Confirm daemon starts, 'openvoice-desktop status' succeeds, or Save reports startup failure rather than applied."
    },
    @{
        Id = "W06"; Title = "Save feedback"
        Steps = "Save without edits, then Save one changed field. Confirm 'No changes' versus one-field result. Success text must clear after about six seconds; error text must stay visible. Expand details for a pending/unavailable field and confirm layout remains bounded."
    },
    @{
        Id = "W07"; Title = "Theme and language"
        Steps = "Change theme and interface language before Save. Confirm immediate appearance/navigation change, Settings title, and persistence after Save and reopen. Note that field labels and detailed messages remain English by design."
    },
    @{
        Id = "W08"; Title = "Controls, validation, and labels"
        Steps = "Enter invalid numeric value and confirm inline error blocks Save. Clear previously set ASR language and confirm auto-detect. Check log-level dropdown, live log updates, microphone fallback note, and Start hidden/unavailable labels."
    },
    @{
        Id = "W09"; Title = "Audio-device refresh"
        Steps = "Connect/disconnect a microphone and output device. Refresh list repeatedly without restarting Settings; check new/default devices. Select a device, disconnect it, refresh, and confirm missing choice plus fallback warning remain visible."
    },
    @{
        Id = "W10"; Title = "COM cleanup under audio failures"
        Steps = "While recording output reduction is active, disconnect/reconnect playback devices across repeated recording attempts. Confirm app survives, output restores when possible, and process handle count does not grow steadily. Note any audio error."
    }
)

$results = @()
foreach ($case in $cases) {
    Write-Host "`n$($case.Id): $($case.Title)"
    Write-Host $case.Steps
    do {
        $answer = (Read-Host "Result: P=pass, F=fail, N=not run").Trim().ToUpperInvariant()
    } while ($answer -notin @("P", "F", "N"))
    $observation = (Read-Host "Observation (optional; do not include tokens or transcript text)").Trim()
    $status = switch ($answer) { "P" { "PASS" }; "F" { "FAIL" }; default { "NOT RUN" } }
    $results += [pscustomobject]@{
        Id = $case.Id
        Title = $case.Title
        Steps = $case.Steps
        Status = $status
        Observation = $observation
    }
}

$resolvedExe = (Resolve-Path -LiteralPath $ExePath).Path
$report = @(
    "# Windows runtime smoke",
    "",
    "Date (UTC): $([DateTime]::UtcNow.ToString('yyyy-MM-dd HH:mm:ss'))",
    "Executable: $resolvedExe",
    "Windows: $([System.Environment]::OSVersion.VersionString)",
    "",
    "Native results below are operator observations, not automated assertions.",
    ""
)
foreach ($result in $results) {
    $report += "## $($result.Id) — $($result.Title): $($result.Status)"
    $report += ""
    $report += "Reproduction: $($result.Steps)"
    $report += ""
    $report += "Observation: $($result.Observation)"
    $report += ""
}
$reportDirectory = Split-Path -Parent $ReportPath
if ($reportDirectory) {
    New-Item -ItemType Directory -Force -Path $reportDirectory | Out-Null
}
$report | Set-Content -LiteralPath $ReportPath -Encoding UTF8
Write-Host "`nReport: $ReportPath"
if ($results.Status -contains "FAIL" -or $results.Status -contains "NOT RUN") {
    Write-Host "Windows runtime smoke: INCOMPLETE OR FAILED"
    exit 1
}
Write-Host "Windows runtime smoke: PASS"
exit 0
