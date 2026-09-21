# What the DeepSeek workers are doing, and how to stop one.
#   tools/ds_status.ps1                 list the running workers and the last runs
#   tools/ds_status.ps1 -Stop t33-auto-switch
#   tools/ds_status.ps1 -Runs 20        more history
#
# A worker outlives the PowerShell process that launched it, so this reads the state files
# ds-agent.ps1 leaves in local/agents/ rather than looking for launchers. A worker whose
# transcript has not grown for a few minutes is stuck or thinking; the column says which.
param(
    [string]$Stop,
    [int]$Runs = 8,
    [int]$StallMinutes = 5
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$stateDir = Join-Path $root 'local\agents'
if (-not (Test-Path -LiteralPath $stateDir)) { Write-Output "no worker state yet ($stateDir)"; exit 0 }

function Get-States {
    Get-ChildItem -LiteralPath $stateDir -Filter '*.json' -ErrorAction SilentlyContinue | ForEach-Object {
        try { Get-Content -LiteralPath $_.FullName -Raw | ConvertFrom-Json } catch { $null }
    } | Where-Object { $_ }
}

if ($Stop) {
    $state = Get-States | Where-Object { $_.label -eq $Stop }
    if (-not $state) { Write-Output "no running worker labelled '$Stop'"; exit 1 }
    foreach ($one in @($state)) {
        $alive = Get-Process -Id $one.pid -ErrorAction SilentlyContinue
        if ($alive) {
            & taskkill.exe /PID $one.pid /T /F | Out-Null
            Write-Output "stopped $($one.label) (pid $($one.pid)). Check git log for commits it made before it died."
        } else {
            Write-Output "$($one.label) (pid $($one.pid)) was already gone"
        }
        Remove-Item -LiteralPath (Join-Path $stateDir "$($one.label).json") -ErrorAction SilentlyContinue
    }
    exit 0
}

$now = Get-Date
$rows = foreach ($state in Get-States) {
    $alive = [bool](Get-Process -Id $state.pid -ErrorAction SilentlyContinue)
    $elapsed = if ($state.started) { $now - [datetime]$state.started } else { $null }
    $lastWrite = $null
    if ($state.transcript -and (Test-Path -LiteralPath $state.transcript)) {
        $lastWrite = (Get-Item -LiteralPath $state.transcript).LastWriteTime
    }
    $quiet = if ($lastWrite) { [int]($now - $lastWrite).TotalMinutes } else { $null }
    [pscustomobject]@{
        Label   = $state.label
        Pid     = $state.pid
        State   = if (-not $alive) { 'GONE (stale state file)' } elseif ($null -eq $quiet) { 'starting' }
                  elseif ($quiet -ge $StallMinutes) { "quiet ${quiet}m - check it" } else { 'working' }
        Running = if ($elapsed) { '{0:hh\:mm\:ss}' -f $elapsed } else { '' }
        Brief   = $state.brief
    }
}
if ($rows) { $rows | Format-Table -AutoSize | Out-String -Width 160 } else { Write-Output 'no workers running' }

$log = Join-Path $stateDir 'runs.csv'
if (Test-Path -LiteralPath $log) {
    Write-Output "last $Runs runs:"
    Import-Csv -LiteralPath $log | Select-Object -Last $Runs |
        Select-Object started, label, status, turns, tokens_in, tokens_out, seconds, denied |
        Format-Table -AutoSize | Out-String -Width 160
}
