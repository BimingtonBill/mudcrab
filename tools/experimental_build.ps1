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
   A merge of the same ref onto the same tree as in the previous build reuses that build's result.
3. Applies the patches in tools/experimental/fixups/*.patch (merge fallout, in name order):
   pre-portal-*.patch before the portal merge, the rest after it.
4. Builds the playable engine and the converter: cargo build --profile quick --bin engine --bin converter
   (own build folder; quick = release without LTO).

Play it with tools/experimental_play.ps1.

.EXAMPLE
powershell -NoProfile -ExecutionPolicy Bypass -File tools/experimental_build.ps1
#>
param(
    [string]$Branch = "experimental/$(Get-Date -Format yyyy-MM-dd)",
    [string]$Worktree = "D:/Modding-overflow/wt-experimental",
    [string]$TargetDir = "D:/Modding-overflow/wt-targets/experimental",
    [switch]$Resume,          # continue merging from the current branch head instead of resetting
    [switch]$SkipOnConflict,  # skip a PR whose conflicts have no recorded resolution
    [switch]$NoPortal,        # main + PRs only
    [switch]$NoBuild,
    [string]$PortalRef = "portal",
    [string]$ResolveFrom,     # a previous build whose merges are reused when unchanged (default: the branch before reset)
    [switch]$NoFetch          # reuse refs/exp/pr<N> as they are (replays the exact heads of the last run)
)
$ErrorActionPreference = "Continue"  # git writes progress to stderr; failures are checked by exit code
$repo = Split-Path -Parent $PSScriptRoot

# Merge order: dependencies first (#118 before #126/#125, #123 before #124, #141 before #142).
# #105 (LOD) goes early: it is the largest and merges clean onto main. #119 merged into main 2026-10-04.
$prs = @(99, 105, 114, 118, 126, 125, 123, 124, 128, 121, 122, 136, 137, 139, 141, 142, 152)

# Left out on purpose, with the reason (update when a PR merges, closes or is fixed):
$skipped = [ordered]@{
    127 = "Mudcrab rename: a mechanical text rename that conflicts with everything; lands after the others"
}

function Invoke-Git { param([Parameter(ValueFromRemainingArguments)]$GitArgs) & git -C $Worktree @GitArgs; if ($LASTEXITCODE -ne 0) { throw "git $GitArgs failed ($LASTEXITCODE)" } }

if (-not $NoFetch) {
    Write-Host "== fetch origin and PR heads"
    & git -C $repo fetch -q origin
    foreach ($n in $prs) { & git -C $repo fetch -q origin "+pull/$n/head:refs/exp/pr$n" }
}

if (-not (Test-Path $Worktree)) { & git -C $repo worktree add -q $Worktree -B $Branch origin/main }
$dirty = & git -C $Worktree status --porcelain --untracked-files=no
if ($dirty) { throw "worktree $Worktree has uncommitted changes; finish or remove them first" }
& git -C $Worktree checkout -q -B $Branch
if (-not $Resume) {
    # Keep the previous build reachable: its merges are reused below when nothing changed.
    & git -C $Worktree update-ref refs/exp/previous HEAD
    Invoke-Git reset -q --hard origin/main
}
if (-not $ResolveFrom) { $ResolveFrom = "refs/exp/previous" }

# Merge commits of the previous build, keyed by "<first parent tree> <second parent>": a merge of the
# same ref onto the same tree reuses that commit's tree, including hand fixes inside the merge.
$known = @{}
& git -C $Worktree log --merges --format="%T %P" $ResolveFrom 2>$null | ForEach-Object {
    $f = $_ -split ' '
    if ($f.Count -ge 3) { $known["$(& git -C $Worktree rev-parse "$($f[1])^{tree}") $($f[2])"] = $f[0] }
}

function Invoke-Fixups([string]$Stage) {
    $all = Get-ChildItem (Join-Path $PSScriptRoot "experimental/fixups") -Filter *.patch -ErrorAction SilentlyContinue | Sort-Object Name
    foreach ($p in $all) {
        if (($Stage -eq "pre-portal") -ne $p.Name.StartsWith("pre-portal-")) { continue }
        & git -C $Worktree apply --ignore-whitespace --reverse --check $p.FullName *> $null
        if ($LASTEXITCODE -eq 0) { Write-Host "   fixup $($p.Name) already in"; continue }
        Invoke-Git apply --ignore-whitespace --3way $p.FullName
        Invoke-Git commit -q -m "[claude] experimental: $($p.BaseName)"
        Write-Host "OK fixup $($p.Name)"
    }
}

$skippedNow = @()
$merges = @($prs | ForEach-Object { @{ Name = "PR #$_"; Ref = "refs/exp/pr$_" } })
if (-not $NoPortal) { $merges += @{ Name = "portal"; Ref = $PortalRef } }

foreach ($m in $merges) {
    if ($m.Name -eq "portal") { Invoke-Fixups "pre-portal" }
    $sha = (& git -C $Worktree rev-parse --short $m.Ref)
    $full = (& git -C $Worktree rev-parse $m.Ref)
    & git -C $Worktree merge-base --is-ancestor $m.Ref HEAD
    if ($LASTEXITCODE -eq 0) { Write-Host "   $($m.Name) ($sha) already in"; continue }
    $msg = "Merge $($m.Name) ($sha) into experimental"
    $reuse = $known["$(& git -C $Worktree rev-parse 'HEAD^{tree}') $full"]
    & git -C $Worktree -c rerere.enabled=true -c rerere.autoupdate=true merge --no-ff --no-commit $m.Ref *> $null
    if ($reuse) {
        Invoke-Git read-tree --reset -u $reuse
        Invoke-Git commit -q -m $msg
        Write-Host "OK $($m.Name) ($sha), same resolution as the previous build"
        continue
    }
    $unresolved = @(& git -C $Worktree diff --name-only --diff-filter=U)
    if ($unresolved.Count -eq 0) {
        & git -C $Worktree rev-parse -q --verify MERGE_HEAD *> $null
        if ($LASTEXITCODE -eq 0) { Invoke-Git -c rerere.enabled=true commit -q -m $msg }
        Write-Host "OK $($m.Name) ($sha)"
        continue
    }
    if ($SkipOnConflict -and $m.Name -ne "portal") {
        Invoke-Git merge --abort
        $skippedNow += "$($m.Name) ($sha): unrecorded conflicts in $($unresolved -join ', ')"
        Write-Host "SKIP $($m.Name): unrecorded conflicts"
        continue
    }
    Write-Host "STOP $($m.Name) ($sha): resolve these, git add, git -c rerere.enabled=true commit -m `"$msg`", then rerun with -Resume:"
    $unresolved | ForEach-Object { Write-Host "   $_" }
    exit 2
}
if ($NoPortal) { Invoke-Fixups "pre-portal" } else { Invoke-Fixups "post-portal" }

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
