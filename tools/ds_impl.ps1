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
    [switch]$NoSeed                       # start the task's target/ empty instead of copying the lead's
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$agent = Join-Path $HOME '.claude\skills\deepseek-agents\ds-agent.ps1'
$implRoot = Join-Path $root 'local\impl'
$stateDir = Join-Path $root 'local\agents'
$pilotLog = Join-Path $stateDir 'pilot.csv'

# Each checkout builds into its own target/ (git-ignored inside the worktree). A shared target
# directory is NOT safe: `cargo test -p engine` writes the same test-binary path from every
# checkout, so parallel workers ran each other's binaries (found by impl-007, 2026-09-22). To avoid
# a cold build per task, the new target/ is seeded with a copy of the lead's target/debug (without
# incremental/, which is only valid for the checkout that wrote it); measured by the DeepSeek Workers
# harness on this project: 6.6 GB in ~6 s, after which a test build recompiles only the workspace
# crates. Ported from that harness's ds_impl.ps1 (2026-09-22).
Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
$cargoBin = Join-Path $HOME '.cargo\bin'
if ((Test-Path $cargoBin) -and ($env:Path -notlike "*$cargoBin*")) { $env:Path = "$cargoBin;$env:Path" }
function Set-TaskTarget([string]$Worktree, [switch]$Seed) {
    $target = Join-Path $Worktree 'target'
    $env:CARGO_TARGET_DIR = $target
    $leadDebug = Join-Path $root 'target\debug'
    if ($Seed -and -not $NoSeed -and (Test-Path $leadDebug) -and -not (Test-Path (Join-Path $target 'debug'))) {
        $t0 = Get-Date
        & robocopy.exe $leadDebug (Join-Path $target 'debug') /E /MT:16 /XD incremental /NFL /NDL /NJH /NJS /NP | Out-Null
        # robocopy exits 0-7 on success, 8+ on failure.
        if ($LASTEXITCODE -ge 8) { "[ds-impl] seeding target/ failed (robocopy $LASTEXITCODE); the first build will be cold" }
        else { "[ds-impl] seeded target/debug from the lead's build in $([int]((Get-Date) - $t0).TotalSeconds) s" }
        $global:LASTEXITCODE = 0
    }
}

function Fail([string]$m) { [Console]::Error.WriteLine("[ds-impl] $m"); exit 2 }
function Split-List([string]$v) {
    if (-not $v) { return @() }
    @($v -split ',(?![^()]*\))' | ForEach-Object { $_.Trim() } | Where-Object { $_ })
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
    $copied = @()
    foreach ($file in $info.files) {
        $from = Join-Path $path $file
        if (-not (Test-Path -LiteralPath $from)) { "[ds-impl] $file was not written, skipped"; continue }
        $to = Join-Path $root $file
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $to) | Out-Null
        Copy-Item -LiteralPath $from -Destination $to -Force
        $copied += $file
    }
    "[ds-impl] copied into master: $($copied -join ', ')"
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
    $started = Get-Date
    $resultPath = Join-Path $work '.ds-result.json'
    $out = if (Test-Path $resultPath) { Get-Content $resultPath -Raw } else { '' }
    $status = if ($out -match 'status=(\S+)') { $Matches[1] } else { 'unrecorded' }
    "[ds-impl] post-run checks for $Name on $($baseCommit.Substring(0,7))"
    Set-TaskTarget $work
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

$baseCommit = (& git -C $root rev-parse $Base).Trim()
$work = Join-Path $implRoot $Name

if ($DryRun) {
    "task:     $Name"
    "base:     $baseCommit"
    "worktree: $work"
    "owned:    $($owned -join ', ')"
    "accept:   $($checks -join ' | ')"
    exit 0
}

if (Test-Path $work) { Fail "$Name already exists: review it, then -Integrate or -Discard it" }
New-Item -ItemType Directory -Force -Path $implRoot | Out-Null
& git -C $root worktree add --detach $work $baseCommit | Out-Null
if (-not (Test-Path $work)) { Fail 'git worktree add failed' }

# Task-scoped permissions: the shared protections, minus the paths this task owns.
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
    readOnlyDirs = @($config.readOnlyDirs) + @(
        Get-ChildItem (Join-Path $root 'local') -Directory -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -notin @('impl', 'agents') } | ForEach-Object { $_.FullName })
    denyEdit     = $keptDenies
    allowTools   = 'Bash(cargo check *),Bash(cargo test *),Bash(cargo clippy *),Bash(cargo fmt *),Bash(cargo tree *),Bash(python *),Bash(python -m unittest *)'
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
# The worker writes a harmless notice to stderr. Merging that into the pipeline under
# ErrorActionPreference Stop turns it into a terminating error and kills this runner while
# the worker carries on, so let stderr flow to ours and keep the preference relaxed here.
$previousPreference = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
try {
    $out = & powershell -NoProfile -ExecutionPolicy Bypass -File $agent -TaskFile $briefPath -Dir $work `
        -Label $Name -Schema $schema -MaxTurns $MaxTurns -TimeoutMinutes $TimeoutMinutes -Effort $Effort | Out-String
} finally { $ErrorActionPreference = $previousPreference }
$out.TrimEnd()
[IO.File]::WriteAllText((Join-Path $work '.ds-result.json'), $out, (New-Object System.Text.UTF8Encoding $false))
$footer = ($out -split "`n" | Where-Object { $_ -match 'session=\S+ status=' } | Select-Object -Last 1)
$status = if ($footer -match 'status=(\S+)') { $Matches[1] } else { 'unknown' }
if ($status -eq 'unknown') {
    # The launcher failed before a worker ran (no footer): there is nothing to check, and running the
    # acceptance builds would only spend minutes compiling the base commit and lock the checkout.
    New-Item -ItemType Directory -Force -Path $stateDir | Out-Null
    if (-not (Test-Path $pilotLog)) {
        [IO.File]::WriteAllText($pilotLog, "started,task,type,base,status,scope_ok,checks_passed,checks_total,seconds,outcome,reviewer,lead_minutes,notes`r`n",
            (New-Object System.Text.UTF8Encoding $false))
    }
    $row = '{0},{1},implementation,{2},no-worker,,0,{3},{4},launch-failed,,,the launcher returned no worker result; acceptance skipped' -f `
        $started.ToString('s'), $Name, $baseCommit.Substring(0, 7), $checks.Count, [int]((Get-Date) - $started).TotalSeconds
    [IO.File]::AppendAllText($pilotLog, $row + "`r`n", (New-Object System.Text.UTF8Encoding $false))
    "[ds-impl] ${Name}: the worker never ran - see the launcher output above. Discard with -Discard $Name."
    exit 1
}

}   # end of the launch path

# Scope check: everything the worker touched, against what the brief assigned.
# --untracked-files=all: plain --porcelain collapses a wholly-new directory (e.g. a fixtures
# folder) into one line ("tests/fixtures/"), which never matches an exact owned file path and
# false-positives every task whose owned files live in a new subdirectory. This worktree is one
# task's small checkout, not the full repo, so listing all files here is cheap.
$touched = @(& git -C $work status --porcelain --untracked-files=all | ForEach-Object { ($_ -replace '^..\s+', '').Trim() } |
    Where-Object { $_ -and $_ -notmatch '^\.(deepseek-agents|ds-impl|ds-result|ds-review)\.json$' -and $_ -notmatch '^target/' })
$outside = @($touched | Where-Object { $owned -notcontains $_ })
if ($outside) { "[ds-impl] OUT OF SCOPE: $($outside -join ', ')" } else { "[ds-impl] scope ok ($($touched.Count) file(s))" }

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

New-Item -ItemType Directory -Force -Path $stateDir | Out-Null
if (-not (Test-Path $pilotLog)) {
    [IO.File]::WriteAllText($pilotLog, "started,task,type,base,status,scope_ok,checks_passed,checks_total,seconds,outcome,reviewer,lead_minutes,notes`r`n",
        (New-Object System.Text.UTF8Encoding $false))
}
$row = '{0},{1},implementation,{2},{3},{4},{5},{6},{7},{8},,,' -f $started.ToString('s'), $Name, $baseCommit.Substring(0, 7),
    $status, (-not $outside), @($results | Where-Object { $_.Exit -eq 0 }).Count, $results.Count,
    [int]((Get-Date) - $started).TotalSeconds, $(if ($accepted) { 'pending-review' } else { 'failed' })
[IO.File]::AppendAllText($pilotLog, $row + "`r`n", (New-Object System.Text.UTF8Encoding $false))

"[ds-impl] ${Name}: worker $status, acceptance $(if ($accepted) { 'PASS' } else { 'FAIL' }). Review: git -C '$work' diff"
if (-not $accepted) { exit 1 }
