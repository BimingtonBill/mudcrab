<#
.SYNOPSIS
Rebuilds the internal experimental demo branch: upstream main + every open PR + the portal on top.
LOCAL ONLY. Never push this branch anywhere (publishing rule, AGENTS.md).

.DESCRIPTION
1. Fetches origin and every PR head below into refs/exp/pr<N>.
2. Resets the branch in its own worktree to origin/main and merges the PRs in the order below,
   then the `portal` branch. Conflicts that were resolved once are replayed by git rerere
   (resolutions live in .git/rr-cache). A conflict with no recorded resolution stops the script
   with the files listed; resolve, `git add`, `git -c rerere.enabled=true commit --no-edit`, then
   rerun with -Resume. -SkipOnConflict aborts and skips such a PR instead (listed at the end).
3. Applies the patches in tools/experimental/fixups/*.patch (merge fallout, in name order).
4. Builds the playable engine and the converter: cargo build --profile quick --bin engine --bin converter
   (own build folder; quick = release without LTO).

Play it with tools/experimental_play.ps1.

.EXAMPLE
powershell -NoProfile -ExecutionPolicy Bypass -File tools/experimental_build.ps1
#>
param(
    [string]$Branch = "experimental/2026-10-03",
    [string]$Worktree = "D:/Modding-overflow/wt-experimental",
    [string]$TargetDir = "D:/Modding-overflow/wt-targets/experimental",
    [switch]$Resume,          # continue merging from the current branch head instead of resetting
    [switch]$SkipOnConflict,  # skip a PR whose conflicts have no recorded resolution
    [switch]$NoPortal,        # main + PRs only
    [switch]$NoBuild
)
$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot

# Merge order: dependencies first (#119 before #114, #118 before #126/#125, #123 before #124).
$prs = @(99, 119, 114, 118, 126, 125, 123, 124, 128, 121, 122, 136, 137, 139, 141)

# Left out on purpose, with the reason (update when a PR merges, closes or is fixed):
$skipped = [ordered]@{
    105 = "LOD (Chef): conflicts with main itself in 10 files (converter cache/schema, database, launcher); its owner rebases"
    127 = "Mudcrab rename: a mechanical text rename that conflicts with everything; lands after the others"
}

function Git { param([Parameter(ValueFromRemainingArguments)]$GitArgs) & git -C $Worktree @GitArgs; if ($LASTEXITCODE -ne 0) { throw "git $GitArgs failed ($LASTEXITCODE)" } }

Write-Host "== fetch origin and PR heads"
& git -C $repo fetch -q origin
foreach ($n in $prs) { & git -C $repo fetch -q origin "+pull/$n/head:refs/exp/pr$n" }

if (-not (Test-Path $Worktree)) { & git -C $repo worktree add -q $Worktree -B $Branch origin/main }
$dirty = & git -C $Worktree status --porcelain --untracked-files=no
if ($dirty) { throw "worktree $Worktree has uncommitted changes; finish or remove them first" }
& git -C $Worktree checkout -q $Branch
if (-not $Resume) { Git reset -q --hard origin/main }

$skippedNow = @()
$merges = @($prs | ForEach-Object { @{ Name = "PR #$_"; Ref = "refs/exp/pr$_" } })
if (-not $NoPortal) { $merges += @{ Name = "portal"; Ref = "portal" } }

foreach ($m in $merges) {
    $sha = (& git -C $Worktree rev-parse --short $m.Ref)
    & git -C $Worktree merge-base --is-ancestor $m.Ref HEAD
    if ($LASTEXITCODE -eq 0) { Write-Host "   $($m.Name) ($sha) already in"; continue }
    $msg = "Merge $($m.Name) ($sha) into experimental"
    & git -C $Worktree -c rerere.enabled=true -c rerere.autoupdate=true merge --no-ff -q -m $msg $m.Ref 2>&1 | Out-Null
    $unresolved = @(& git -C $Worktree diff --name-only --diff-filter=U)
    if ($LASTEXITCODE -eq 0 -and $unresolved.Count -eq 0) {
        # rerere may have resolved everything but left the merge uncommitted.
        & git -C $Worktree rev-parse -q --verify MERGE_HEAD *> $null
        if ($LASTEXITCODE -eq 0) { Git -c rerere.enabled=true commit -q --no-edit }
        Write-Host "OK $($m.Name) ($sha)"
        continue
    }
    if ($SkipOnConflict -and $m.Name -ne "portal") {
        Git merge --abort
        $skippedNow += "$($m.Name) ($sha): unrecorded conflicts in $($unresolved -join ', ')"
        Write-Host "SKIP $($m.Name): unrecorded conflicts"
        continue
    }
    Write-Host "STOP $($m.Name) ($sha): resolve these, git add, git -c rerere.enabled=true commit --no-edit, then rerun with -Resume:"
    $unresolved | ForEach-Object { Write-Host "   $_" }
    exit 2
}

$fixups = Get-ChildItem (Join-Path $PSScriptRoot "experimental/fixups") -Filter *.patch -ErrorAction SilentlyContinue | Sort-Object Name
foreach ($p in $fixups) {
    & git -C $Worktree apply --reverse --check $p.FullName *> $null
    if ($LASTEXITCODE -eq 0) { Write-Host "   fixup $($p.Name) already in"; continue }
    Git apply --3way $p.FullName
    Git commit -q -m "[claude] experimental: $($p.BaseName)"
    Write-Host "OK fixup $($p.Name)"
}

Write-Host "== left out:"
$skipped.GetEnumerator() | ForEach-Object { Write-Host "   #$($_.Key): $($_.Value)" }
$skippedNow | ForEach-Object { Write-Host "   $_" }
Write-Host "== head: $(& git -C $Worktree log --oneline -1)"

if (-not $NoBuild) {
    $env:CARGO_TARGET_DIR = $TargetDir
    Push-Location $Worktree
    try { cargo build --profile quick --bin engine --bin converter; if ($LASTEXITCODE -ne 0) { throw "build failed" } } finally { Pop-Location }
    Write-Host "== built $TargetDir/quick/engine.exe and converter.exe; convert with tools/experimental_convert.cmd, play with tools/experimental_play.ps1"
}
