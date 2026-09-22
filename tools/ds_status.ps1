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
# Where this runs. From the DeepSeek Workers harness (launcher\ds-agent.ps1 beside tools\): the project is
# DS_PROJECT or the current folder, and the launcher is the harness's. Copied into a project's tools\: the
# project is DS_PROJECT or the folder above tools\, and the launcher is the installed skill's.
$here = Split-Path -Parent $PSScriptRoot
$skillDir = Join-Path $HOME '.claude\skills\deepseek-agents'
$inHarness = Test-Path (Join-Path $here 'launcher\ds-agent.ps1')
# Installed as part of the skill, the launcher sits beside tools\ instead: act on the current folder too.
$inSkill = -not $inHarness -and (Test-Path (Join-Path $here 'ds-agent.ps1'))
$root = if ($env:DS_PROJECT) { $env:DS_PROJECT } elseif ($inHarness -or $inSkill) { (Get-Location).Path } else { $here }
$agent = if ($inHarness) { Join-Path $here 'launcher\ds-agent.ps1' } elseif ($inSkill) { Join-Path $here 'ds-agent.ps1' } else { Join-Path $skillDir 'ds-agent.ps1' }
$harness = if ($inHarness -or $inSkill) { $here } else { $skillDir }
# The launcher's state folder: DS_STATE_DIR, the project's stateDir, local\agents when the project has
# local\, else the shared ~\.claude-deepseek\agents.
$stateDir = $null
if ($env:DS_STATE_DIR) { $stateDir = $env:DS_STATE_DIR }
if (-not $stateDir -and (Test-Path (Join-Path $root '.deepseek-agents.json'))) {
    try { $cfg = Get-Content (Join-Path $root '.deepseek-agents.json') -Raw | ConvertFrom-Json; if ($cfg.stateDir) { $stateDir = if ([IO.Path]::IsPathRooted($cfg.stateDir)) { $cfg.stateDir } else { Join-Path $root $cfg.stateDir } } } catch { }
}
if (-not $stateDir) { $stateDir = if (Test-Path (Join-Path $root 'local')) { Join-Path $root 'local\agents' } else { Join-Path $HOME '.claude-deepseek\agents' } }
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
