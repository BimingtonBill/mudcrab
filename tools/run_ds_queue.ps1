# Run DeepSeek briefs one after another, unattended.
#   run_ds_queue.ps1 -Briefs t02-nif-coverage,t03-pex-opcodes [-WaitFor t01-phase2-signoff-gaps]
# -WaitFor: wait until no running process has these brief names on its command
# line (tasks that edit the same files must not overlap).
# Each brief's first line holds its allowed tools. A worker that stops at the
# turn limit is resumed once to finish its handoff and commit. The queue status
# in tasks/deepseek/README.md is updated and committed after each brief.
param(
    [Parameter(Mandatory)][string[]]$Briefs,
    [string[]]$WaitFor = @(),
    [int]$MaxTurns = 400
)
$ErrorActionPreference = 'Continue'
# powershell -File passes "a,b" as one string.
$Briefs = @($Briefs -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ })
$WaitFor = @($WaitFor -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ })
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
$readme = Join-Path $root 'tasks\deepseek\README.md'

function Set-Status([string]$brief, [string]$status) {
    $text = [IO.File]::ReadAllText($readme)
    $pattern = '(\| `' + [regex]::Escape($brief) + '\.md` \| [^|]+\| )[^|]*(\|)'
    $text = [regex]::Replace($text, $pattern, { param($m) $m.Groups[1].Value + $status + ' ' + $m.Groups[2].Value })
    [IO.File]::WriteAllText($readme, $text)
    & git -C $root commit -q -m "[deepseek] Task queue: $brief $status" -- tasks/deepseek/README.md 2>$null
}

foreach ($name in $WaitFor) {
    Write-Output "[queue] waiting for $name"
    while (Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -and $_.CommandLine -like "*$name*" -and $_.ProcessId -ne $PID -and $_.CommandLine -notlike '*run_ds_queue*' }) {
        Start-Sleep -Seconds 60
    }
}

foreach ($brief in $Briefs) {
    $file = "tasks/deepseek/$brief.md"
    # A brief may carry its own tools line; without one the launcher's project config supplies the baseline.
    $first = Get-Content -LiteralPath (Join-Path $root $file) -TotalCount 1
    $extra = @()
    if ($first -match '`([^`]+)`') { $extra = @('-AllowTools', $Matches[1]) }
    else { Write-Output "[queue] ${brief}: no tools line, using the .deepseek-agents.json baseline" }
    Set-Status $brief 'launched (queue)'
    Write-Output "[queue] start $brief $(Get-Date -Format HH:mm)"
    $before = (& git -C $root rev-parse HEAD).Trim()
    $out = & powershell -NoProfile -ExecutionPolicy Bypass -File $agent -TaskFile $file -Dir $root -Mode edit -Effort max -MaxTurns $MaxTurns -TimeoutMinutes 150 -Label $brief @extra 2>&1 | Out-String
    $footer = ($out -split "`n" | Where-Object { $_ -match 'session=\S+ status=' } | Select-Object -Last 1)
    Write-Output "[queue] $brief $footer"
    # A worker that runs out of turns is told to finish; give it two goes before giving up.
    $resumes = 0
    while ($resumes -lt 2 -and $footer -match 'session=(\S+) status=error\(error_max_turns\)') {
        $resumes++
        $out = & powershell -NoProfile -ExecutionPolicy Bypass -File $agent -Resume $Matches[1] -Task 'You hit the turn limit. Finish now: complete your handoff note, commit your work with git commit -- <your paths>, then write your final report.' -Dir $root -Mode edit -Effort max -MaxTurns 60 -Label "$brief-resume$resumes" @extra 2>&1 | Out-String
        $footer = ($out -split "`n" | Where-Object { $_ -match 'session=\S+ status=' } | Select-Object -Last 1)
        Write-Output "[queue] $brief resume $resumes`: $footer"
    }
    # Check the citations in whatever the worker committed before anyone builds on them.
    $notes = & git -C $root diff --name-only $before HEAD 2>$null | Where-Object { $_ -like '*.md' }
    if ($notes) {
        $report = & python (Join-Path $PSScriptRoot 'check_citations.py') --quiet @notes 2>&1 | Out-String
        if ($report.Trim()) { Write-Output "[queue] $brief citations:`r`n$($report.TrimEnd())" }
        else { Write-Output "[queue] $brief citations: all resolve" }
    }
    if ($footer -match 'status=ok') { Set-Status $brief "done, needs review (docs/handoff/$brief.md)" }
    else { Set-Status $brief "stopped: $($footer.Trim()) - check docs/handoff/$brief.md" }
}
Write-Output '[queue] finished'
