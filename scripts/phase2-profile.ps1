[CmdletBinding()]
param(
    [ValidateSet("all", "synthetic", "rural", "dense", "dense-legacy", "water", "stress", "stability", "portal-door-tour", "portal-heavy", "lod-far", "lod-far-ring-off", "lod-flythrough")]
    [string]$Scenario = "all",
    [string]$Assets,
    [string]$Worldspace = "0x3c",
    [int]$RuralGridX = 0,
    [int]$RuralGridY = 0,
    # Dense is Riverwood (about 2,900 references within two cells). `dense-legacy` is the place
    # `dense` used to be (18,-5, about 1,530), kept for one release to compare older baselines.
    [int]$DenseGridX = 3,
    [int]$DenseGridY = -12,
    [int]$DenseLegacyGridX = 18,
    [int]$DenseLegacyGridY = -5,
    [int]$WaterGridX = 0,
    [int]$WaterGridY = 0,
    [int]$Repetitions = 3,
    [int]$DurationSeconds = 120,
    [int]$StabilitySeconds = 1800,
    # The portal-door-tour window: long enough for the scripted Riverwood tour (Sven's House in and out,
    # `--tour-doors 2`) to finish its two crossings: it settles the world for about 10 s, crosses into the
    # house at about 20 s and back out at about 30 s (quick build, 2026-10-06 dry run), so 40 s ends a few
    # seconds after the second crossing. The tour repeats so the app is still running when the window ends
    # and writes its report.
    [int]$PortalTourSeconds = 40,
    [string]$Baseline = "profiling-baseline.json",
    [switch]$UpdateBaseline,
    [switch]$Quick,
    [switch]$SkipBuild,
    # Import the functions below and stop, instead of running a campaign: phase2-baseline.ps1
    # dot-sources this file with -AsLibrary so both scripts invoke the engine the same way.
    [switch]$AsLibrary
)

$ErrorActionPreference = "Stop"
# `ConvertFrom-Json -AsHashtable` (used for report and baseline files) arrived in PowerShell 6, so
# under Windows PowerShell 5.1 the failure would surface as a puzzling "parameter not found".
if ($PSVersionTable.PSVersion.Major -lt 7) {
    throw "Phase 2 campaign scripts need PowerShell 7 (pwsh): run again as 'pwsh -File scripts/$(Split-Path -Leaf $PSCommandPath) ...'"
}
$script:repository = Split-Path -Parent $PSScriptRoot
# The report format this script reads back. The engine writes it into every benchmark report; a
# `frame-metrics.json` left by another build is not accepted as this invocation's report.
$script:benchmarkReportFormatVersion = 10

function Get-ProfilingRepository {
    return $script:repository
}

# Where the release engine binary is. `CARGO_TARGET_DIR`, when the caller set it, wins over the
# repository's own `target` directory (the worktree convention on this machine).
function Get-EngineBinaryPath {
    $targetRoot = $script:repository + "\target"
    if ($env:CARGO_TARGET_DIR) {
        $targetRoot = if ([IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) {
            $env:CARGO_TARGET_DIR
        } else {
            Join-Path $script:repository $env:CARGO_TARGET_DIR
        }
    }
    return Join-Path $targetRoot "release\engine.exe"
}

# The hardware label a run identifies itself by: CPU, GPU and driver, reduced to one token so it
# can be passed on the command line and compared between runs of the same machine.
function Get-BenchmarkHardware {
    $cpu = (Get-CimInstance Win32_Processor | Select-Object -First 1 -ExpandProperty Name) -replace '[^a-zA-Z0-9]+', '-'
    $gpuInfo = Get-CimInstance Win32_VideoController | Select-Object -First 1
    $gpu = $gpuInfo.Name -replace '[^a-zA-Z0-9]+', '-'
    $driver = $gpuInfo.DriverVersion -replace '[^a-zA-Z0-9.]+', '-'
    return "$cpu-$gpu-driver-$driver".Trim('-')
}

# Builds the release engine once. `-Jobs 1` keeps the campaign's single-job build (the profiling
# run wants the machine quiet, not hot); the baseline command builds with cargo's default.
function Invoke-EngineReleaseBuild {
    param([int]$Jobs = 0)
    $arguments = @("build", "--release", "--bin", "engine")
    if ($Jobs -gt 0) { $arguments += "-j$Jobs" }
    Push-Location $script:repository
    try {
        # Cargo writes its progress to stderr, which PowerShell 5.1 would turn into an error record
        # under this script's `$ErrorActionPreference = "Stop"`; the preference is relaxed for the
        # call so a build cannot abort on its own progress lines.
        $previous = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        try {
            & cargo @arguments
            $exitCode = $LASTEXITCODE
        } finally {
            $ErrorActionPreference = $previous
        }
        if ($exitCode -ne 0) { throw "Release build failed with exit code $exitCode" }
    } finally {
        Pop-Location
    }
}

# What this campaign is: the commit it was built from and whether the worktree was dirty.
#
# Git writes line-ending and progress warnings to stderr, which PowerShell 5.1 turns into an error
# record under this script's `$ErrorActionPreference = "Stop"`, so the preference is relaxed around
# the two calls (a warning is not a failed command).
function Get-BenchmarkRunIdentity {
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $commit = (& git -C $script:repository rev-parse --short=12 HEAD 2>$null)
        if (-not $commit) { $commit = "unknown" }
        & git -C $script:repository diff --quiet --ignore-submodules HEAD 2>$null
        $dirty = $LASTEXITCODE -ne 0
    } finally {
        $ErrorActionPreference = $previous
    }
    return [ordered]@{ commit = $commit; dirty = $dirty }
}

# The converted asset set a world scenario loads, checked for the four files every one of them
# needs, or $null when no -Assets was given. A relative path is taken from the repository root, the
# directory both campaign scripts run from.
function Resolve-ConvertedAssetSet {
    param([string]$Assets)
    if (-not $Assets) { return $null }
    $candidate = if ([IO.Path]::IsPathRooted($Assets)) { $Assets } else { Join-Path $script:repository $Assets }
    $resolved = (Resolve-Path -LiteralPath $candidate).Path
    foreach ($required in @("conversion-manifest.json", "integration-report.json", "skyrim_world.db", "cell_cache.rkyv")) {
        if (-not (Test-Path -LiteralPath (Join-Path $resolved $required) -PathType Leaf)) {
            throw "Converted asset set is missing $required"
        }
    }
    return $resolved
}

# How long one window of a scene runs: `stability` is the soak, the door tour is as long as its
# tour, and every other scene takes the common window.
function Get-SceneDuration {
    param([string]$Name, [int]$DurationSeconds, [int]$StabilitySeconds, [int]$PortalTourSeconds)
    if ($Name -eq "stability") { return $StabilitySeconds }
    if ($Name -eq "portal-door-tour") { return $PortalTourSeconds }
    return $DurationSeconds
}

# The portal scenes (this branch only): they run the portal's engine against the portal's converted
# assets (-Assets, the same parameter as every world scene) and are defined here, in the one place
# every scene is, so the baseline and the profile campaign time them the same way. Each scene's
# pose is written in its comment below.
#
# The terrain ring: a benchmark run turns the ring off unless `--terrain-radius` names it
# (config.rs), so every portal scene says which it wants. Every portal scene keeps portal's
# default ring (since 28ce63dc the exterior fog ends at the LOD horizon, so the ring no longer hides
# the native terrain LOD), except `lod-far-ring-off`, the comparison: the same pose with the ring
# off, so the two windows show what the ring costs at the LOD vantage.
$script:portalTerrainRadius = 8

# The engine arguments that make up one portal scene beyond the benchmark and profile arguments
# every scene shares. `$RunDirectory` is where the run writes (the tour's frames go under it).
function Get-PortalSceneArguments {
    param([string]$Name, [string]$RunDirectory, [switch]$StandingStill)
    switch ($Name) {
        # The demo door tour: Riverwood's route, the first two doors - Sven's House in, then the door
        # inside it back out (`--tour-doors 2`; the demo starts on the Helgen road at 19600,-46650,
        # facing the village). The tour walks to each door, waits for its destination to stream in,
        # crosses it and photographs both sides, so the window holds the approach, the doorway with
        # the portal drawing the room beyond it, the swap and the way back out. `--tour-repeat 3`
        # makes the tour outlast the window: the benchmark writes its report at the window's end,
        # which the tour finishing first would pre-empt. The tour's own screenshots are part of the
        # load it measures. A benchmark run keeps the scripted camera (it does not walk), so the
        # crossings are the scripted ones.
        "portal-door-tour" {
            return @(
                "--demo", "riverwood",
                "--terrain-radius", "$($script:portalTerrainRadius)",
                "--demo-tour", (Join-Path $RunDirectory "tour"),
                "--tour-doors", "2", "--tour-repeat", "3"
            )
        }
        # A standing view with one open portal view on screen: the camera 350 units in front of Sven's
        # House door (0001CBB0, Tamriel; eye 20558.4,-46062.6,-2.1, yaw 161.5, pitch 5.2, hfov 75),
        # the first shot of tools/reference/doorway_shots.json at the baseline's 1600x900. A shots
        # run that is also a benchmark opens the door, waits for the portal to draw the room beyond
        # it and then holds the view for the window instead of photographing it.
        "portal-heavy" {
            return @(
                "--terrain-radius", "$($script:portalTerrainRadius)",
                "--shots", (Join-Path (Get-ProfilingRepository) "tools\reference\baseline_portal_heavy.json"),
                "--shots-out", (Join-Path $RunDirectory "shots")
            )
        }
        # A high vantage above Riverwood looking north up the valley, where native terrain LOD (#105)
        # reaches the horizon: eye 20500,-45500,6000 in Tamriel, yaw 0, pitch 8, hfov 90 (the a-north
        # shot of local/reference/lod-check/lod-check.json, held for the window).
        "lod-far" {
            return @(
                "--terrain-radius", "$($script:portalTerrainRadius)",
                "--shots", (Join-Path (Get-ProfilingRepository) "tools\reference\baseline_lod_far.json"),
                "--shots-out", (Join-Path $RunDirectory "shots")
            )
        }
        # The ring-off comparison: lod-far's pose and shot with no `--terrain-radius`, so the
        # benchmark's default turns the terrain ring off (terrain only within the streamed cells).
        "lod-far-ring-off" {
            return @(
                "--shots", (Join-Path (Get-ProfilingRepository) "tools\reference\baseline_lod_far.json"),
                "--shots-out", (Join-Path $RunDirectory "shots")
            )
        }
        # A scripted fly-through from LOD into full detail: from the same vantage (20500,-45500,6000,
        # yaw 0: north, level), the camera flies north at 1500 units a second over the full-detail
        # grid and the LOD terrain beyond it; the auto-flight turns back after four cells, so the
        # window crosses between the two again and again. The standing-still window of the same
        # scene is the start pose with no flight.
        "lod-flythrough" {
            $arguments = @(
                "--terrain-radius", "$($script:portalTerrainRadius)",
                "--start-position", "20500", "-45500", "6000", "--start-yaw", "0"
            )
            if (-not $StandingStill) { $arguments += @("--auto-fly-speed", "1500") }
            return $arguments
        }
    }
}

# The engine's arguments for one window of one scenario: the scenario's own definition (fixture
# or world, its grid, its camera movement) and the output paths this run writes.
#
# `-StandingStill` is the same scenario with the camera left in place: it drops the
# `--auto-fly-speed` a moving scenario would add, so the phase2-baseline command can measure the
# same scene once standing still and once streaming.
function Get-ScenarioArguments {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string]$RunDirectory,
        [Parameter(Mandatory)][string]$Commit,
        [Parameter(Mandatory)][string]$Hardware,
        [string]$RunId = "run-1",
        [string]$RunLabel,
        [switch]$DirtyWorktree,
        [switch]$StandingStill,
        [int]$DurationSeconds = 120,
        [int]$StabilitySeconds = 1800,
        [int]$PortalTourSeconds = 40,
        [string]$ResolvedAssets,
        [string]$Worldspace = "0x3c",
        [int]$RuralGridX = 0,
        [int]$RuralGridY = 0,
        [int]$DenseGridX = 3,
        [int]$DenseGridY = -12,
        [int]$DenseLegacyGridX = 18,
        [int]$DenseLegacyGridY = -5,
        [int]$WaterGridX = 0,
        [int]$WaterGridY = 0
    )
    $arguments = @(
        "--benchmark-duration", "$(Get-SceneDuration -Name $Name -DurationSeconds $DurationSeconds -StabilitySeconds $StabilitySeconds -PortalTourSeconds $PortalTourSeconds)",
        "--benchmark-output", (Join-Path $RunDirectory "acceptance.json"),
        "--profile-output", $RunDirectory,
        "--profile-scenario", $Name,
        "--profile-run-id", $RunId,
        "--profile-commit", $Commit,
        "--profile-hardware", $Hardware,
        "--accept-min-fps", "0",
        "--accept-p95-ms", "1000000",
        "--accept-max-memory-growth-gib", "1000000"
    )
    if ($RunLabel) { $arguments += @("--run-label", $RunLabel) }
    if ($DirtyWorktree) { $arguments += "--profile-dirty-worktree" }
    if ($Name -eq "synthetic") {
        $arguments += @("--benchmark-only", "--synthetic-instances", "250000")
    } else {
        $arguments += @("--assets", $ResolvedAssets, "--worldspace", $Worldspace)
        if ($Name -in @("portal-door-tour", "portal-heavy", "lod-far", "lod-far-ring-off", "lod-flythrough")) {
            $arguments += Get-PortalSceneArguments -Name $Name -RunDirectory $RunDirectory -StandingStill:$StandingStill
        } elseif ($Name -eq "water") {
            $arguments += @("--grid-x", "$WaterGridX", "--grid-y", "$WaterGridY")
        } elseif ($Name -eq "dense-legacy") {
            # The old dense place (18,-5), kept for one release so older baselines can be compared.
            $arguments += @("--grid-x", "$DenseLegacyGridX", "--grid-y", "$DenseLegacyGridY")
        } elseif ($Name -in @("dense", "stability")) {
            $arguments += @("--grid-x", "$DenseGridX", "--grid-y", "$DenseGridY")
        } else {
            $arguments += @("--grid-x", "$RuralGridX", "--grid-y", "$RuralGridY")
        }
        if ($Name -eq "stress") {
            $arguments += @("--stream-radius", "3")
            if (-not $StandingStill) { $arguments += @("--auto-fly-speed", "5000") }
        }
        if ($Name -eq "stability" -and -not $StandingStill) {
            $arguments += @("--auto-fly-speed", "900")
        }
    }
    return $arguments
}

# The value of `--<Name>` in an engine argument list, or `$null` when the switch is not there.
function Get-ArgumentValue {
    param([string[]]$Arguments, [string]$Name)
    for ($index = 0; $index -lt ($Arguments.Count - 1); $index++) {
        if ($Arguments[$index] -eq $Name) { return $Arguments[$index + 1] }
    }
    return $null
}

function Get-Median {
    param([double[]]$Values)
    $ordered = @($Values | Sort-Object)
    $middle = [int][Math]::Floor($ordered.Count / 2)
    if ($ordered.Count % 2) { return $ordered[$middle] }
    return ($ordered[$middle - 1] + $ordered[$middle]) / 2.0
}

function Get-RegressionComparison {
    param([string]$Metric, [double]$Old, [double]$New)
    $absoluteDelta = if ($Metric -eq "average_fps") { $Old - $New } else { $New - $Old }
    $regression = $absoluteDelta / $Old * 100.0
    $noiseFloor = switch ($Metric) {
        "average_fps" { 5.0 }
        { $_ -in @("frame_ms_p95", "frame_ms_p99") } { 1.5 }
        { $_ -in @("peak_process_memory_gib", "process_memory_growth_gib") } { 0.05 }
        default { 0.0 }
    }
    $materialRegression = $absoluteDelta -gt $noiseFloor
    $status = if ($materialRegression -and $regression -gt 10.0) {
        "fail"
    } elseif ($materialRegression -and $regression -gt 5.0) {
        "warn"
    } else {
        "pass"
    }
    return [ordered]@{
        regression_percent = $regression
        regression_absolute = $absoluteDelta
        noise_floor = $noiseFloor
        status = $status
    }
}

# Runs one window of one scenario and returns its report (`frame-metrics.json`, the JSON the engine
# also writes as `acceptance.json`). Throws when the engine exits non-zero, and when the report it
# reads back cannot be shown to have been written by this invocation: a successful exit with a stale
# or absent report would otherwise count an old run (or nothing) as a fresh repeat.
function Invoke-EngineWindow {
    param(
        [Parameter(Mandatory)][string]$Engine,
        [Parameter(Mandatory)][string[]]$Arguments,
        [Parameter(Mandatory)][string]$RunDirectory,
        [Parameter(Mandatory)][string]$Label,
        [switch]$AsHashtable,
        # The engine exits non-zero when one of its own pass gates failed (the commit budget, a
        # streaming invariant) even though it wrote a complete report. The profiling campaign treats
        # that as a failed run; the baseline measures and does not judge, so it reads the report
        # anyway and warns which gates failed.
        [switch]$AllowGateFailure
    )
    New-Item -ItemType Directory -Force -Path $RunDirectory | Out-Null
    $reportPath = Join-Path $RunDirectory "frame-metrics.json"
    $metadataPath = Join-Path $RunDirectory "metadata.json"
    # What this invocation asked the engine to run, so the bundle read back can be identified as this
    # run's rather than an earlier one's.
    $expectedRunId = Get-ArgumentValue $Arguments "--profile-run-id"
    $expectedScenario = Get-ArgumentValue $Arguments "--profile-scenario"
    $startedUnixMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    # The engine logs to stderr; under `$ErrorActionPreference = "Stop"` PowerShell 5.1 would treat
    # the first log line as an error, so the preference is relaxed for the run. The engine runs from
    # the repository root: its default assets directory, ini files and screenshot paths are resolved
    # relative to the working directory, so a direct campaign must not depend on where it was started.
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    Push-Location $script:repository
    try {
        & $Engine @Arguments
        $exitCode = $LASTEXITCODE
    } finally {
        Pop-Location
        $ErrorActionPreference = $previous
    }
    if ($exitCode -ne 0 -and -not $AllowGateFailure) { throw "$Label failed with exit code $exitCode" }
    if (-not (Test-Path -LiteralPath $reportPath -PathType Leaf)) {
        if ($exitCode -ne 0) { throw "$Label failed with exit code $exitCode and wrote no report in $RunDirectory" }
        throw "$Label exited 0 but wrote no report in $RunDirectory`: the repeat measured nothing"
    }
    $report = Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json -AsHashtable
    if ($report.format_version -ne $script:benchmarkReportFormatVersion) {
        throw "$Label wrote report format $($report.format_version) in $RunDirectory, but this script reads format $($script:benchmarkReportFormatVersion)"
    }
    if ([long]$report.generated_unix_ms -lt $startedUnixMs) {
        throw "$Label read a stale report from $RunDirectory (written at $($report.generated_unix_ms) ms, this run started at $startedUnixMs ms)"
    }
    if ([long]$report.frames -le 0 -or [double]$report.elapsed_seconds -le 0) {
        throw "$Label reported no measured frames in $RunDirectory (frames $($report.frames), elapsed $($report.elapsed_seconds) s), so the repeat did not complete"
    }
    if (Test-Path -LiteralPath $metadataPath -PathType Leaf) {
        $metadata = Get-Content -LiteralPath $metadataPath -Raw | ConvertFrom-Json -AsHashtable
        if ($expectedRunId -and $metadata.run_id -ne $expectedRunId) {
            throw "$Label read a bundle in $RunDirectory belonging to run '$($metadata.run_id)', not '$expectedRunId'"
        }
        if ($expectedScenario -and $metadata.scenario -ne $expectedScenario) {
            throw "$Label read a bundle in $RunDirectory belonging to scenario '$($metadata.scenario)', not '$expectedScenario'"
        }
    } elseif ($expectedRunId) {
        throw "$Label exited 0 but wrote no profiling bundle in $RunDirectory (no metadata.json), so the repeat cannot be identified"
    }
    if ($exitCode -ne 0) {
        # Only a report that says its own gates failed excuses the exit code: a crash after an older
        # report, or an exit for any other reason, was refused above or fails the freshness checks.
        if ($report.passed -ne $false) { throw "$Label failed with exit code $exitCode" }
        $failedGates = @($report.thresholds.GetEnumerator() | Where-Object { $_.Value -is [bool] -and -not $_.Value } | ForEach-Object { $_.Key })
        Write-Warning "$Label exited $exitCode because its own gates failed ($($failedGates -join ', ')): the report in $RunDirectory is kept and measured"
    }
    if ($AsHashtable) { return $report }
    return (Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json)
}

if ($AsLibrary) { return }

$engine = Get-EngineBinaryPath
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$hardware = Get-BenchmarkHardware
$campaign = Join-Path $script:repository "target\profiling\$stamp-$hardware"
$testTemp = Join-Path $script:repository "target\test-temp"
New-Item -ItemType Directory -Force -Path $campaign, $testTemp | Out-Null
$env:TEMP = $testTemp
$env:TMP = $testTemp

if ($Quick) {
    $DurationSeconds = 3
    $StabilitySeconds = 5
    $Repetitions = 1
}
if ($Repetitions -lt 1) { throw "Repetitions must be at least 1" }
$benchmarkPriority = [Diagnostics.ProcessPriorityClass]::AboveNormal
try {
    (Get-Process -Id $PID).PriorityClass = $benchmarkPriority
} catch {
    throw "Could not set the profiling runner priority to $benchmarkPriority`: $($_.Exception.Message)"
}

if (-not $SkipBuild -or -not (Test-Path -LiteralPath $engine -PathType Leaf)) {
    Invoke-EngineReleaseBuild -Jobs 1
}

$identity = Get-BenchmarkRunIdentity
$commit = $identity.commit
$dirty = $identity.dirty
$baselinePath = if ([IO.Path]::IsPathRooted($Baseline)) { $Baseline } else { Join-Path $script:repository $Baseline }
$baselineData = if (Test-Path -LiteralPath $baselinePath) {
    Get-Content -LiteralPath $baselinePath -Raw | ConvertFrom-Json -AsHashtable
} else { @{} }

$resolvedAssets = Resolve-ConvertedAssetSet -Assets $Assets

$requested = if ($Scenario -eq "all") {
    if ($resolvedAssets) { @("synthetic", "rural", "dense", "water", "stress", "stability") } else { @("synthetic") }
} else { @($Scenario) }
if ($requested.Where({ $_ -ne "synthetic" }).Count -gt 0 -and -not $resolvedAssets) {
    throw "Scenario '$Scenario' requires -Assets with a converted legal Skyrim asset set"
}

$results = @{}
$regressionFailed = $false
foreach ($name in $requested) {
    $runs = @()
    for ($run = 1; $run -le $Repetitions; $run++) {
        $runDirectory = Join-Path $campaign "$name\run-$run"
        $arguments = Get-ScenarioArguments -Name $name -RunDirectory $runDirectory -Commit $commit -Hardware $hardware `
            -RunId "run-$run" -DirtyWorktree:$dirty -DurationSeconds $DurationSeconds -StabilitySeconds $StabilitySeconds -PortalTourSeconds $PortalTourSeconds `
            -ResolvedAssets $resolvedAssets -Worldspace $Worldspace `
            -RuralGridX $RuralGridX -RuralGridY $RuralGridY -DenseGridX $DenseGridX -DenseGridY $DenseGridY `
            -DenseLegacyGridX $DenseLegacyGridX -DenseLegacyGridY $DenseLegacyGridY `
            -WaterGridX $WaterGridX -WaterGridY $WaterGridY
        $runs += Invoke-EngineWindow -Engine $engine -Arguments $arguments -RunDirectory $runDirectory -Label "Profiling run $name/run-$run"
    }

    $median = [ordered]@{
        average_fps = Get-Median @($runs.average_fps)
        frame_ms_p95 = Get-Median @($runs.frame_ms_p95)
        frame_ms_p99 = Get-Median @($runs.frame_ms_p99)
        peak_process_memory_gib = Get-Median @($runs.peak_process_memory_gib)
        process_memory_growth_gib = Get-Median @($runs.process_memory_growth_gib)
    }
    $comparison = [ordered]@{ scenario = $name; repetitions = $Repetitions; median = $median; regressions = @() }
    if ($baselineData.ContainsKey($name)) {
        foreach ($metric in @("average_fps", "frame_ms_p95", "frame_ms_p99", "peak_process_memory_gib", "process_memory_growth_gib")) {
            $old = [double]$baselineData[$name][$metric]
            $new = [double]$median[$metric]
            if ($old -le 0) { continue }
            $regression = Get-RegressionComparison -Metric $metric -Old $old -New $new
            if ($regression.status -eq "fail") { $regressionFailed = $true }
            $comparison.regressions += [ordered]@{
                metric = $metric; baseline = $old; current = $new
                regression_percent = $regression.regression_percent
                regression_absolute = $regression.regression_absolute
                noise_floor = $regression.noise_floor
                status = $regression.status
            }
        }
    }
    $results[$name] = $median
    $comparison | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $campaign "$name\comparison.json")
}

[ordered]@{ format_version = 1; generated_at = (Get-Date).ToString("o"); commit = $commit; dirty = $dirty; hardware = $hardware; scenarios = $results } |
    ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $campaign "campaign.json")
$summary = @(
    "# Phase 2 profiling campaign",
    "",
    "- Commit: $commit",
    "- Dirty worktree: $dirty",
    "- Hardware: $hardware",
    "- Repetitions: $Repetitions",
    "",
    "| Scenario | Median FPS | Frame P95 ms | Frame P99 ms | Peak GiB | Growth GiB |",
    "|---|---:|---:|---:|---:|---:|"
)
foreach ($name in $requested) {
    $value = $results[$name]
    $summary += "| $name | $([Math]::Round($value.average_fps, 2)) | $([Math]::Round($value.frame_ms_p95, 3)) | $([Math]::Round($value.frame_ms_p99, 3)) | $([Math]::Round($value.peak_process_memory_gib, 3)) | $([Math]::Round($value.process_memory_growth_gib, 3)) |"
}
$summary | Set-Content -LiteralPath (Join-Path $campaign "summary.md")
if ($UpdateBaseline) {
    $results | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $baselinePath
    Write-Host "Baseline updated: $baselinePath"
}
if ($regressionFailed) { throw "Profiling regression exceeded 10%. See $campaign" }
Write-Host "Profiling campaign complete: $campaign"
