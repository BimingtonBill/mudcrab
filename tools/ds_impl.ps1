# Run one bounded DeepSeek implementation task in an isolated checkout.
#
#   tools/ds_impl.ps1 -Brief tasks/deepseek/i01-esm-record-parser.md
#   tools/ds_impl.ps1 -Brief ... -Base 8919096 -Accept "cargo test -p converter"
#   tools/ds_impl.ps1 -List | -Integrate i01-result-validator | -Discard i01-result-validator
#
# What it does, in order:
#   1. records the base commit and makes a git worktree under local/impl/<name> from it;
#   2. writes a task-scoped .deepseek-agents.json into that worktree: the shared read-only
#      folders and protections stay, minus the paths this task owns;
#   3. launches the worker there with the shared launcher and the result schema;
#   4. checks what it touched against the files the brief assigns (scope check);
#   5. runs the acceptance commands in the worktree and records their exit codes;
#   6. appends a row to local/agents/pilot.csv for the delegation record.
#
# The worktree is left in place for review. Nothing is committed to master by the worker:
# the lead integrates with -Integrate after reading the diff.
[CmdletBinding()]
param(
    [string]$Brief,
    [string]$Base = 'HEAD',
    [string]$Name,
    [string]$Files,                       # comma-separated paths the task owns
    [string]$Accept,                      # comma-separated commands, run in the worktree
    [int]$MaxTurns = 300,
    [int]$TimeoutMinutes = 90,
    [string]$Effort = 'max',
    [switch]$List,
    [string]$Integrate,
    [string]$Discard,
    [string]$Post,                        # re-run the checks for a finished checkout, no worker
    [switch]$DryRun,
    [switch]$NoSeed,                      # Rust: start the task's target/ empty instead of copying the lead's
    [string]$WebDomains,                  # comma-separated domains the worker may fetch; its edits stay in the worktree for review
    [switch]$Force                        # with -Integrate: copy even a file master has changed since the worker's base
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
$implRoot = Join-Path $root 'local\impl'
# The launcher's state folder, for the pilot log: the rule lives in launcher/ds-state.ps1, found the way
# $agent above is (harness, installed skill, or the skill's own folder).
$stateScript = if ($inHarness) { Join-Path $here 'launcher\ds-state.ps1' } elseif ($inSkill) { Join-Path $here 'ds-state.ps1' } else { Join-Path $skillDir 'ds-state.ps1' }
$stateDir = & $stateScript -Dir $root
$pilotLog = Join-Path $stateDir 'pilot.csv'

# The acceptance commands run in this process, which inherited its PATH from whatever started it. Add the
# saved Machine and User PATH entries it lacks, as the launcher does for the worker, so a tool installed
# after the Claude app started (rustup) is found here too. Without this, stress test 2's three Phase B
# tasks all got "acceptance FAIL" from `cargo: CommandNotFound` on good work (H13).
$pathEntries = @($env:Path -split ';' | Where-Object { $_ })
foreach ($scope in 'Machine', 'User') {
    foreach ($entry in ([string][Environment]::GetEnvironmentVariable('Path', $scope) -split ';')) {
        if (-not $entry) { continue }
        $expanded = [Environment]::ExpandEnvironmentVariables($entry).TrimEnd('\')
        if (-not ($pathEntries | Where-Object { $_.TrimEnd('\') -ieq $expanded })) { $pathEntries += $expanded }
    }
}
$env:Path = $pathEntries -join ';'

# Each Rust checkout builds into its own target/ (inside the worktree, so -Discard removes it). A shared
# target/ is NOT safe: Cargo gives a workspace crate's test binary the same name in every checkout, so
# parallel workers overwrote and ran each other's binaries (OpenSkyrim impl-006/impl-007, 2026-09-22).
# To avoid a cold build per task, the new target/ is seeded with a copy of the lead's built profiles
# (without incremental/, which is only valid for the checkout that wrote it): on OpenSkyrim, 6.6 GB
# copied in 6 s, and the test build then recompiled only the workspace's own two crates (17 s).
# The crate may sit in a subfolder (restir-water keeps it in gpu/, H2): the first Cargo.toml found at the
# root or one level down is the crate, and the target/ goes beside it, where cargo would put it anyway.
Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
$crateRel = $null
if (Test-Path (Join-Path $root 'Cargo.toml')) { $crateRel = '' }
else {
    $sub = Get-ChildItem $root -Directory -ErrorAction SilentlyContinue | Where-Object { $_.Name -notin @('local', 'target', '.git') -and (Test-Path (Join-Path $_.FullName 'Cargo.toml')) } | Select-Object -First 1
    if ($sub) { $crateRel = $sub.Name }
}
$isCargo = $null -ne $crateRel
function Set-TaskTarget([string]$Worktree, [switch]$Seed) {
    if (-not $isCargo) { return }
    $crateDir = if ($crateRel) { Join-Path $Worktree $crateRel } else { $Worktree }
    $target = Join-Path $crateDir 'target'
    $env:CARGO_TARGET_DIR = $target
    $leadTarget = Join-Path $(if ($crateRel) { Join-Path $root $crateRel } else { $root }) 'target'
    if (-not $Seed -or $NoSeed -or -not (Test-Path $leadTarget)) { return }
    # Every profile the lead has built (debug, release, ...), so a release acceptance is warm too (H3).
    foreach ($profile in Get-ChildItem $leadTarget -Directory | Where-Object { $_.Name -ne 'tmp' -and -not (Test-Path (Join-Path $target $_.Name)) }) {
        # A profile cargo is building right now holds its .cargo-lock: its files are half-written, so a copy
        # would be no use and robocopy would hit locked files. Leave that profile cold.
        $cargoLock = Join-Path $profile.FullName '.cargo-lock'
        if (Test-Path -LiteralPath $cargoLock) {
            $busy = $false
            try { $held = [IO.File]::Open($cargoLock, 'Open', 'ReadWrite', 'None'); $held.Close() } catch { $busy = $true }
            if ($busy) { "[ds-impl] not seeding target/$($profile.Name): a cargo build is writing it now; that build will be cold"; continue }
        }
        $t0 = Get-Date
        # /R:0 /W:0: a locked file is skipped, not retried. robocopy's defaults (/R:1000000 /W:30) turned one
        # file held by the lead's own build into a 50-minute stall (OpenSkyrim impl-166, 2026-09-24). The seed
        # is only a warm cache: cargo rebuilds whatever is missing.
        $out = @(& robocopy.exe $profile.FullName (Join-Path $target $profile.Name) /E /MT:16 /R:0 /W:0 /XD incremental /NFL /NDL /NJH /NJS /NP)
        $code = $LASTEXITCODE
        $skipped = @($out | Where-Object { $_ -match 'ERROR \d+ \(0x' }).Count   # one such line per file not copied
        $secs = [int]((Get-Date) - $t0).TotalSeconds
        # robocopy: 0-7 copied, 8 some files could not be copied, 16 nothing could be done.
        if ($code -ge 16) { "[ds-impl] seeding target/$($profile.Name) failed (robocopy $code); that build will be cold" }
        elseif ($code -ge 8) { "[ds-impl] seeded target/$($profile.Name) in $secs s, skipping $skipped file(s) that were in use; cargo rebuilds those" }
        else { "[ds-impl] seeded target/$($profile.Name) from the lead's build in $secs s" }
        $global:LASTEXITCODE = 0
    }
}

function Fail([string]$m) { [Console]::Error.WriteLine("[ds-impl] $m"); exit 2 }
function Split-List([string]$v) {
    if (-not $v) { return @() }
    @($v -split ',(?![^()]*\))' | ForEach-Object { $_.Trim() } | Where-Object { $_ })
}
$pilotHeader = 'started,task,type,base,status,scope_ok,checks_passed,checks_total,seconds,outcome,reviewer,lead_minutes,notes'
# Appends $Row to pilot.csv, keeping the header and every other row. With -Dedupe, any earlier row
# for $Task is dropped first, so a re-run of -Post for the same task replaces its old row instead of
# piling up near-duplicates that differ only by timestamp/seconds. Written atomically: temp file, then
# a rename over the target, so a reader never sees a half-written file.
function Write-PilotRow([string]$Row, [string]$Task, [switch]$Dedupe) {
    New-Item -ItemType Directory -Force -Path $stateDir | Out-Null
    $lines = if (Test-Path -LiteralPath $pilotLog) {
        @([IO.File]::ReadAllText($pilotLog) -split "`r?`n" | Where-Object { $_ -ne '' })
    } else { @() }
    $header = if ($lines.Count -gt 0) { $lines[0] } else { $pilotHeader }
    $dataLines = if ($lines.Count -gt 1) { $lines[1..($lines.Count - 1)] } else { @() }
    if ($Dedupe -and $Task) {
        $dataLines = @($dataLines | Where-Object { ($_ -split ',')[1] -ne $Task })
    }
    $dataLines += $Row
    $content = (@($header) + $dataLines -join "`r`n") + "`r`n"
    $tmp = "$pilotLog.tmp$PID"
    [IO.File]::WriteAllText($tmp, $content, (New-Object System.Text.UTF8Encoding $false))
    Move-Item -LiteralPath $tmp -Destination $pilotLog -Force
}

if ($List) {
    if (-not (Test-Path $implRoot)) { 'no implementation tasks'; exit 0 }
    Get-ChildItem $implRoot -Directory | ForEach-Object {
        $meta = Join-Path $_.FullName '.ds-impl.json'
        $info = if (Test-Path $meta) { Get-Content $meta -Raw | ConvertFrom-Json } else { $null }
        $dirty = @(& git -C $_.FullName status --porcelain 2>$null) -join "`n"
        [pscustomobject]@{
            Task = $_.Name
            Base = if ($info) { $info.base.Substring(0, 7) } else { '?' }
            Files = if ($info) { ($info.files -join ' ') } else { '' }
            Changed = @($dirty -split "`n" | Where-Object { $_ }).Count
        }
    } | Format-Table -AutoSize | Out-String -Width 160
    exit 0
}

if ($Discard) {
    $path = Join-Path $implRoot $Discard
    if (-not (Test-Path $path)) { Fail "no such task: $Discard" }
    & git -C $root worktree remove --force $path
    "[ds-impl] discarded $Discard"
    exit 0
}

if ($Integrate) {
    $path = Join-Path $implRoot $Integrate
    $meta = Join-Path $path '.ds-impl.json'
    if (-not (Test-Path $meta)) { Fail "no such task: $Integrate" }
    $info = Get-Content $meta -Raw | ConvertFrom-Json

    # Completeness gate: what is already known about this task on disk, printed before anything is
    # copied (docs/todo.md #13). Only a failed acceptance blocks (pass -Force to override); the rest
    # is information, since a missing review or missing docs is not proof either way.
    $pilotRows = @()
    if (Test-Path -LiteralPath $pilotLog) {
        $pilotRows = @(Import-Csv -LiteralPath $pilotLog | Where-Object { $_.task -eq $Integrate })
    }
    $lastRow = if ($pilotRows.Count -gt 0) { $pilotRows[$pilotRows.Count - 1] } else { $null }
    if ($lastRow) {
        "[ds-impl] check: acceptance - $($lastRow.checks_passed)/$($lastRow.checks_total) checks passed, outcome=$($lastRow.outcome)"
        $scopeNote = if ($lastRow.scope_ok -eq 'True') { 'ok' } elseif ($lastRow.scope_ok -eq 'False') { 'NOT ok' } else { 'not recorded' }
        "[ds-impl] check: scope - $scopeNote"
    } else {
        "[ds-impl] check: acceptance - no pilot.csv row found for $Integrate; nothing recorded to check"
    }
    $acceptanceFailed = $lastRow -and ($lastRow.outcome -in @('failed', 'launch-failed'))
    if ($acceptanceFailed -and -not $Force) {
        Fail "$Integrate's last recorded acceptance did not pass (outcome=$($lastRow.outcome)); refusing to integrate broken work. Fix it and re-run -Post $Integrate, or pass -Force to copy it anyway: -Integrate $Integrate -Force"
    }
    # Every coder ds_impl runs leaves a row, so none means its acceptance was never checked here. Don't let
    # that pass as clean (OpenSkyrim audit finding HT-20260924-04): check it first, or say so with -Force.
    if (-not $lastRow -and -not $Force) {
        Fail "no acceptance is recorded for $Integrate, so nothing says it works. Run its checks first (-Post $Integrate), or pass -Force to copy it anyway: -Integrate $Integrate -Force"
    }

    $reviewName = $Integrate -replace '^impl-', ''
    $reviewRoot = Join-Path $stateDir 'runs'
    $reviewReport = $null
    if (Test-Path -LiteralPath $reviewRoot) {
        $reviewReport = Get-ChildItem $reviewRoot -Directory -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -eq "review-$reviewName" -or $_.Name -like "review-$reviewName.*" } |
            Sort-Object LastWriteTime -Descending |
            ForEach-Object { Join-Path $_.FullName 'report.md' } |
            Where-Object { Test-Path -LiteralPath $_ } |
            Select-Object -First 1
    }
    if ($reviewReport) {
        # Prefer an inline "Verdict: accept" style line over a bare "## Verdict" heading, which
        # carries no verdict itself (the templates put the actual word on the line below it).
        $verdictLines = @(Get-Content -LiteralPath $reviewReport | Where-Object { $_ -match 'Verdict' })
        $verdictLine = ($verdictLines | Where-Object { $_ -match 'Verdict\s*[:\-]\s*\S' } | Select-Object -First 1)
        if (-not $verdictLine -and $verdictLines.Count -gt 0) { $verdictLine = $verdictLines[0] }
        if ($verdictLine) { "[ds-impl] check: review - $($verdictLine.ToString().Trim())" }
        else { "[ds-impl] check: review - report at $reviewReport has no line mentioning Verdict; read it yourself" }
    } else {
        "[ds-impl] check: review - no review worker ran; review the diff yourself"
    }

    $pitfalls = Join-Path $stateDir 'memory\pitfalls.md'
    if (Test-Path -LiteralPath $pitfalls) { "[ds-impl] check: pitfalls - check the diff against $pitfalls" }

    $docFiles = @($info.files | Where-Object { $_ -match '\.md$' })
    if ($docFiles) { "[ds-impl] check: docs - $($docFiles -join ', ')" }
    else { "[ds-impl] check: docs - none of the owned files are docs; user-facing changes may still need docs" }

    $copied = @()
    $moved = @()
    foreach ($file in $info.files) {
        $from = Join-Path $path $file
        if (-not (Test-Path -LiteralPath $from)) { "[ds-impl] $file was not written, skipped"; continue }
        $to = Join-Path $root $file
        # The worker started from $info.base. If the lead has changed this file since, copying the whole
        # file over silently throws those changes away (OpenSkyrim, 2026-09-23: a DemoStart entry and a
        # plugin registration, both found afterwards by grep). Leave such a file alone and say so; the
        # worker's own change can be applied as a patch instead. This covers every file in the task's
        # list, not only the ones its brief calls owned: both losses above were one-line changes to
        # files the worker did not own, which is why nobody was watching them.
        if (-not $Force -and (Test-Path -LiteralPath $to)) {
            $sinceBase = @(& git -C $root diff --name-only "$($info.base)" -- $file 2>$null)
            # If git can't compare (the base commit is gone after a rebase), treat the file as changed rather
            # than overwrite master's version unchecked (review-049).
            if ($LASTEXITCODE -ne 0 -or $sinceBase) { $moved += $file; $global:LASTEXITCODE = 0; continue }
        }
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $to) | Out-Null
        Copy-Item -LiteralPath $from -Destination $to -Force
        # Copy-Item keeps the worker's write time. If the lead built after the worker wrote the file,
        # cargo sees the integrated source as older than the build and reuses the stale binary
        # (OpenSkyrim impl-013: a release build "finished" in 1 s after 3 engine files changed).
        (Get-Item -LiteralPath $to).LastWriteTime = Get-Date
        $copied += $file
    }
    if ($copied) { "[ds-impl] copied into master: $($copied -join ', ')" }
    foreach ($file in $moved) {
        "[ds-impl] NOT copied, changed in master since the worker's base ($($info.base.Substring(0,7))): $file"
        "[ds-impl]   apply the worker's own change instead:"
        "[ds-impl]     git -C '$path' diff $($info.base) -- $file > patch.diff; git -C '$root' apply patch.diff"
        "[ds-impl]   or re-run the task from the current base, or pass -Force to overwrite master's version"
    }
    '[ds-impl] review with git diff, run the tests, then commit with an explicit path list.'
    exit 0
}

if ($Post) {
    # The worker has already run (or its runner died): check and record what is in the checkout.
    $work = Join-Path $implRoot $Post
    $metaPath = Join-Path $work '.ds-impl.json'
    if (-not (Test-Path $metaPath)) { Fail "no such task: $Post" }
    $info = Get-Content $metaPath -Raw | ConvertFrom-Json
    $Name = $info.task; $owned = @($info.files); $checks = @($info.accept); $baseCommit = $info.base
    Set-TaskTarget $work
    $started = Get-Date
    $resultPath = Join-Path $work '.ds-result.json'
    $out = if (Test-Path $resultPath) { Get-Content $resultPath -Raw } else { '' }
    $status = if ($out -match 'status=(\S+)') { $Matches[1] } else { 'unrecorded' }
    "[ds-impl] post-run checks for $Name on $($baseCommit.Substring(0,7))"
}
else {

if (-not $Brief) { Fail 'pass -Brief <file>, or -List / -Post / -Integrate / -Discard <task>' }
$briefPath = if ([IO.Path]::IsPathRooted($Brief)) { $Brief } else { Join-Path $root $Brief }
if (-not (Test-Path -LiteralPath $briefPath)) { Fail "brief not found: $Brief" }
if (-not $Name) { $Name = [IO.Path]::GetFileNameWithoutExtension($briefPath) }

# The brief is the contract: it may declare its own owned files and acceptance commands.
$briefText = [IO.File]::ReadAllText($briefPath)
if (-not $Files -and $briefText -match '(?m)^\s*Owned files:\s*`?([^`\r\n]+)`?\s*$') { $Files = $Matches[1] }
if (-not $Accept -and $briefText -match '(?m)^\s*Acceptance:\s*`?([^`\r\n]+)`?\s*$') { $Accept = $Matches[1] }
$owned = Split-List $Files
$checks = Split-List $Accept
if (-not $owned) { Fail 'the task owns no files: add "Owned files: a,b" to the brief or pass -Files' }
# -Integrate copies owned files into the project, so they must stay inside it.
$badPath = @($owned | Where-Object { [IO.Path]::IsPathRooted($_) -or ($_ -replace '\\', '/') -match '(^|/)\.\.(/|$)' -or ($_ -replace '\\', '/') -match '^\.git(/|$)' })
if ($badPath) { Fail "owned files must be paths inside the project: $($badPath -join ', ')" }

# Started by a DeepSeek lead (DS_RUN_ID is set) rather than by Claude: the brief was written by DeepSeek.
# Its acceptance commands run here, outside the workers' permission system, so they must be plain test
# commands; and it may not claim files the project protects, which would lift that protection in the worktree.
$fromLead = [bool]$env:DS_RUN_ID
if ($fromLead) {
    $leadConfigPath = Join-Path $root '.deepseek-agents.json'
    $leadConfig = if (Test-Path $leadConfigPath) { Get-Content $leadConfigPath -Raw | ConvertFrom-Json } else { $null }
    $prefixes = if ($leadConfig -and $leadConfig.leadAcceptance) { @($leadConfig.leadAcceptance) }
                else { @('cargo test', 'cargo check', 'cargo clippy', 'cargo build', 'cargo fmt --check', 'python -m unittest',
                         'python -m pytest', 'py -m unittest', 'py -m pytest', 'npm test', 'npm run test', 'pnpm test',
                         'yarn test', 'go test', 'go vet', 'dotnet test', 'dotnet build') }
    foreach ($check in $checks) {
        $okPrefix = $prefixes | Where-Object { $check -eq $_ -or $check.StartsWith("$_ ") }
        if (-not $okPrefix -or $check -match '[;&|<>`$\r\n]') {
            Fail "a lead's acceptance command must be a plain test command starting with one of: $($prefixes -join ', ') (no command separators, pipes, redirects or variables). Refused: $check"
        }
    }
    # The project's denyEdit protects the main checkout from direct edits, which a worktree coder never
    # makes; its work reaches the checkout only through Claude's -Integrate. So a lead may give a coder
    # protected source (OpenSkyrim's crates/**: four leads landed no code on 2026-09-23 because this rule
    # used to cover denyEdit too). The files that govern workers themselves stay Claude's.
    foreach ($file in $owned) {
        $f = $file -replace '\\', '/'
        foreach ($rule in @('.deepseek-agents.json', 'AGENTS.md', 'CLAUDE.md', '.claude/**', 'tools/ds_*', 'tools/run_ds_queue.ps1')) {
            if (-not $rule) { continue }
            $pattern = '^' + [regex]::Escape(($rule -replace '\\', '/')).Replace('\*\*', '.*').Replace('\*', '[^/]*') + '$'
            if ($f -match $pattern) { Fail "a lead may not give a coder a protected file ($file matches $rule); ask Claude" }
        }
    }
}

$baseCommit = (& git -C $root rev-parse $Base).Trim()
$work = Join-Path $implRoot $Name

# Size check before any money is spent. How big the owned files are predicts a coder's length far better
# than how many there are or how long the brief is: over 38 OpenSkyrim coders (2026-09), under 4,000
# owned lines took a median of about 95 steps, 4,000-8,000 took 124 (69% over 100), and over 8,000 took
# 186 (94% over 100). Every step re-reads the context, so those long runs cost the most.
$ownedLines = 0
foreach ($file in $owned) {
    $full = Join-Path $root $file
    if (Test-Path -LiteralPath $full -PathType Leaf) { $ownedLines += @([IO.File]::ReadAllLines($full)).Count }
}
$sizeNote = if ($ownedLines -gt 8000) {
    "$ownedLines lines in owned files: coders this size took a median of 186 steps (94% went over 100). Split the task, or name the functions or line ranges to change so the coder reads only those."
} elseif ($ownedLines -gt 4000) {
    "$ownedLines lines in owned files: coders this size took a median of 124 steps (69% went over 100). Consider splitting it, or point the brief at the exact functions to change."
} else { $null }
if ($sizeNote) { "[ds-impl] size: $sizeNote" }

if ($DryRun) {
    "project:  $root$(if ($inHarness) { ' (run from the harness)' } elseif ($inSkill) { ' (run from the installed skill)' } else { ' (tools in the project)' })"
    "launcher: $agent"
    "task:     $Name"
    "base:     $baseCommit"
    "worktree: $work"
    "owned:    $($owned -join ', ')"
    "accept:   $($checks -join ' | ')"
    "size:     $ownedLines lines in owned files$(if (-not $sizeNote) { ' (small enough)' })"
    exit 0
}

if (Test-Path $work) { Fail "$Name already exists: review it, then -Integrate or -Discard it" }
New-Item -ItemType Directory -Force -Path $implRoot | Out-Null
& git -C $root worktree add --detach $work $baseCommit | Out-Null
if (-not (Test-Path $work)) { Fail 'git worktree add failed' }

# Task-scoped permissions: the shared protections, minus the paths this task owns.
# A project without .deepseek-agents.json has no shared protections to carry over (the launcher treats
# the file as optional too); this used to crash here, after the worktree was already made.
$configPath = Join-Path $root '.deepseek-agents.json'
$config = if (Test-Path $configPath) { Get-Content $configPath -Raw | ConvertFrom-Json } else { [pscustomobject]@{} }
$keptDenies = @()
foreach ($rule in $config.denyEdit) {
    $pattern = '^' + [regex]::Escape($rule).Replace('\*\*', '.*').Replace('\*', '[^/]*') + '$'
    $ownedHere = @($owned | Where-Object { $_ -replace '\\', '/' -match $pattern })
    if (-not $ownedHere) { $keptDenies += $rule }
}
$scoped = [ordered]@{
    # A worktree has no local/ (it is git-ignored), so the lead's data folders are added
    # read-only: run artifacts, reference screenshots, converted asset samples. Not local/
    # itself - the checkouts live under local/impl, and a deny rule there would forbid the
    # worker to write its own files - and not local/agents, which is worker bookkeeping.
    readOnlyDirs = @($config.readOnlyDirs | Where-Object { $_ }) + @(
        Get-ChildItem (Join-Path $root 'local') -Directory -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -notin @('impl', 'agents') } | ForEach-Object { $_.FullName })
    denyEdit     = $keptDenies
    # A project may set implAllowTools in .deepseek-agents.json; otherwise python, plus cargo for Rust projects.
    # implAllowTools replaces the default set; the project's own allowTools are always kept too (H4).
    allowTools   = ((@([string]$config.allowTools) + @($(if ($config.implAllowTools) { [string]$config.implAllowTools }
                     elseif ($isCargo) { 'Bash(cargo check *),Bash(cargo test *),Bash(cargo clippy *),Bash(cargo fmt *),Bash(cargo tree *),Bash(python *),Bash(python -m unittest *)' }
                     else { 'Bash(python *),Bash(python -m unittest *)' }))) | Where-Object { $_ }) -join ','
    stateDir     = $stateDir
    defaults     = [ordered]@{ mode = 'edit'; effort = $Effort; maxTurns = $MaxTurns; timeoutMinutes = $TimeoutMinutes }
}
[IO.File]::WriteAllText((Join-Path $work '.deepseek-agents.json'),
    ($scoped | ConvertTo-Json -Depth 6), (New-Object System.Text.UTF8Encoding $false))
[IO.File]::WriteAllText((Join-Path $work '.ds-impl.json'),
    ([ordered]@{ task = $Name; base = $baseCommit; files = $owned; accept = $checks; brief = $Brief } |
        ConvertTo-Json -Depth 4), (New-Object System.Text.UTF8Encoding $false))
Set-TaskTarget $work -Seed

$started = Get-Date
"[ds-impl] $Name on $($baseCommit.Substring(0,7)) in $work"
$schema = Join-Path $root 'tasks\deepseek\result-schema.json'
if (-not (Test-Path $schema)) { $schema = Join-Path $harness 'templates\result-schema.json' }
# The worker writes a harmless notice to stderr. Merging that into the pipeline under
# ErrorActionPreference Stop turns it into a terminating error and kills this runner while
# the worker carries on, so let stderr flow to ours and keep the preference relaxed here.
$webArgs = @(if ($WebDomains) { '-WebDomains', $WebDomains })
$previousPreference = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
try {
    $out = & powershell -NoProfile -ExecutionPolicy Bypass -File $agent -TaskFile $briefPath -Dir $work `
        -Label $Name -Schema $schema -MaxTurns $MaxTurns -TimeoutMinutes $TimeoutMinutes -Effort $Effort @webArgs | Out-String
} finally { $ErrorActionPreference = $previousPreference }
$out.TrimEnd()
[IO.File]::WriteAllText((Join-Path $work '.ds-result.json'), $out, (New-Object System.Text.UTF8Encoding $false))
$footer = ($out -split "`n" | Where-Object { $_ -match 'session=\S+ status=' } | Select-Object -Last 1)
$status = if ($footer -match 'status=(\S+)') { $Matches[1] } else { 'unknown' }

# No footer means the worker never ran (the launcher failed: no claude.exe, no key, a bad flag). Running
# the acceptance builds then only costs minutes and records a misleading FAIL against an untouched
# checkout (reported by the OpenSkyrim session, 2026-09-22), so stop and say what happened.
if (-not $footer) {
    $row = '{0},{1},implementation,{2},no-worker,,0,{3},{4},launch-failed,,,the launcher returned no worker result; acceptance skipped' -f `
        $started.ToString('s'), $Name, $baseCommit.Substring(0, 7), $checks.Count, [int]((Get-Date) - $started).TotalSeconds
    Write-PilotRow -Row $row -Task $Name -Dedupe:([bool]$Post)
    "[ds-impl] ${Name}: the worker did not run (no result from the launcher; its errors are above). Acceptance skipped."
    "[ds-impl] fix the launch problem, then -Discard $Name and run the task again."
    exit 1
}

}   # end of the launch path

# Scope check: everything the worker touched, against what the brief assigned.
# --untracked-files=all: plain --porcelain collapses a wholly-new directory (e.g. a fixtures
# folder) into one line ("tests/fixtures/"), which never matches an exact owned file path and
# false-positives every task whose owned files live in a new subdirectory. This worktree is one
# task's small checkout, not the full repo, so listing all files here is cheap.
$touched = @(& git -C $work status --porcelain --untracked-files=all | ForEach-Object { ($_ -replace '^..\s+', '').Trim() } |
    Where-Object { $_ -and $_ -notmatch '^\.(deepseek-agents|ds-impl|ds-result|ds-review)\.json$' -and $_ -notmatch '(^|/)target/' })
$outside = @($touched | Where-Object { $owned -notcontains $_ })
if ($outside) { "[ds-impl] OUT OF SCOPE: $($outside -join ', ')" } else { "[ds-impl] scope ok ($($touched.Count) file(s): $($touched -join ', '))" }

# Acceptance: the brief's own commands, run in the worktree.
$results = @()
foreach ($check in $checks) {
    $command = "Set-Location '$work'; $check"
    $ErrorActionPreference = 'Continue'   # unittest reports on stderr; that is not a failure here
    $output = & powershell -NoProfile -ExecutionPolicy Bypass -Command $command 2>&1 | Out-String
    $code = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    $results += [pscustomobject]@{ Check = $check; Exit = $code }
    $tail = ($output -split "`n" | Where-Object { $_.Trim() } | Select-Object -Last 3) -join ' / '
    "[ds-impl] accept($code): $check -- $tail"
}
$accepted = ($results.Count -gt 0 -and -not ($results | Where-Object { $_.Exit -ne 0 }) -and -not $outside)

$row = '{0},{1},implementation,{2},{3},{4},{5},{6},{7},{8},,,' -f $started.ToString('s'), $Name, $baseCommit.Substring(0, 7),
    $status, (-not $outside), @($results | Where-Object { $_.Exit -eq 0 }).Count, $results.Count,
    [int]((Get-Date) - $started).TotalSeconds, $(if ($accepted) { 'pending-review' } else { 'failed' })
# -Post re-runs these checks against an already-recorded task with no new worker run: replace that
# task's earlier row instead of adding a duplicate that differs only by timestamp/seconds (docs/todo.md #4).
Write-PilotRow -Row $row -Task $Name -Dedupe:([bool]$Post)

"[ds-impl] ${Name}: worker $status, acceptance $(if ($accepted) { 'PASS' } else { 'FAIL' }). Review: git -C '$work' diff"
if (-not $accepted) { exit 1 }

# Keep the tracks busy: the next coder can start (this one's build is done), and a read-only review of
# this one runs beside it, since it builds nothing. OpenSkyrim ran 78 coders and 8 reviews in three days
# (2026-09-22..24), with one worker at a time for 59% of the time workers ran.
$reviewLabel = 'review-' + ($Name -replace '^impl-', '')
$reviewTask = "Review coder task $Name. Its brief: $briefPath. Its work is in the git worktree $work; the changed files are $($touched -join ', '). Check that the change does what the brief asks and nothing more, stays in its owned files, and is covered by its tests, and look for what the acceptance commands would miss. Read-only: report findings with file:line and a verdict (accept, fix first, or reject); do not fix anything."
"[ds-impl] next, in one message: start the next coder now, and this DeepSeek review of $Name beside it (read-only, so build limits don't apply):"
"  powershell -NoProfile -ExecutionPolicy Bypass -File `"$agent`" -Kind review -Mode read -Effort high -Dir `"$root`" -Label $reviewLabel -Task `"$reviewTask`""
"[ds-impl] integrate $Name after reading the review; meanwhile do your own part (the next brief, integration wiring, checks of earlier work)."
