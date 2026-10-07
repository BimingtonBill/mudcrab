# Nightly copy of local/'s text folders to D: (the user, 2026-10-02).
#
# local/ is git-ignored, so PR drafts, review copies, team logs, research and PR evidence have no other
# copy. Each run writes a dated snapshot under D:\Mudcrab\local-backup\<yyyy-MM-dd> and keeps
# the newest 14; it only ever removes its own dated snapshot folders. It protects against an accidental
# delete or `git clean`, not against the PC failing.

$ErrorActionPreference = 'Stop'
$source = 'C:\Modding\MudcrabProject\local'
$backupRoot = 'D:\Mudcrab\local-backup'
$folders = @('pr-drafts', 'review', 'team', 'research', 'pr-evidence')
$keep = 14

New-Item -ItemType Directory -Force -Path $backupRoot | Out-Null
$ownerFile = Join-Path $backupRoot 'OWNER.txt'
if (-not (Test-Path -LiteralPath $ownerFile)) {
    @(
        'owner: Upstream Dev'
        'purpose: nightly snapshots of local/ text folders (tools/backup_local.ps1); keeps the newest 14'
        'class: B'
        'keep-until: while-open'
        'links: no'
        "created: $(Get-Date -Format 'yyyy-MM-dd')"
    ) | Set-Content -LiteralPath $ownerFile -Encoding utf8
}

$snapshot = Join-Path $backupRoot (Get-Date -Format 'yyyy-MM-dd')
$failed = @()
foreach ($folder in $folders) {
    $from = Join-Path $source $folder
    if (-not (Test-Path -LiteralPath $from)) { continue }
    # /E copies subfolders; /XJ never follows junctions; no /MIR or /PURGE, so nothing is deleted.
    robocopy $from (Join-Path $snapshot $folder) /E /XJ /R:1 /W:1 /NFL /NDL /NP /NJH /NJS | Out-Null
    if ($LASTEXITCODE -ge 8) { $failed += "$folder (robocopy exit $LASTEXITCODE)" }
}
# The loose notes at local/'s top level (handover briefs and similar).
robocopy $source $snapshot *.md /R:1 /W:1 /NFL /NDL /NP /NJH /NJS | Out-Null
if ($LASTEXITCODE -ge 8) { $failed += "top-level notes (robocopy exit $LASTEXITCODE)" }

# Keep the newest $keep dated snapshots; only folders named like a date are ever removed.
$dated = Get-ChildItem -LiteralPath $backupRoot -Directory |
    Where-Object { $_.Name -match '^\d{4}-\d{2}-\d{2}$' } |
    Sort-Object Name -Descending
foreach ($old in ($dated | Select-Object -Skip $keep)) {
    $isLink = ($old.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0
    if (-not $isLink) { Remove-Item -LiteralPath $old.FullName -Recurse -Force }
}

$log = Join-Path $backupRoot 'backup.log'
$line = "$(Get-Date -Format 'yyyy-MM-dd HH:mm') snapshot $snapshot " + $(if ($failed) { "FAILED: $($failed -join ', ')" } else { 'ok' })
Add-Content -LiteralPath $log -Value $line
if ($failed) { exit 1 }
