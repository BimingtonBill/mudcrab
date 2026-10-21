# Starts one of the walkable demos against a converted asset folder: the Alftand -> Blackreach
# descent, or the Riverwood village.
#
#   pwsh -File tools/demo/play-demo.ps1 -Assets "<converted>" [-Start alftand|blackreach|riverwood]
#   -Cluster on adds a cluster overlay from <converted>/derived/ and the cluster terrain (the base
#   folder is read as it is, never written); -PortalDoors on crosses load doors seamlessly. Both
#   default to off (no cluster overlay, load screens), so every option is stated explicitly.
#   -ClusterOverlay <label|settings-id|path> picks the overlay; without it the engine takes the
#   only overlay and refuses if there are several. -FarTerrainShadows (with -Cluster on) adds
#   distant terrain sun shadows. Build an overlay with: converter derive-clusters <converted> ...
#   (docs/demo/cluster-small-primitive-derivation.md).
#
# The engine binary is looked up relative to this repository, so the script works from a checkout
# anywhere. Build it first ("cargo build --release -p engine --bin engine"), or let
# tools/demo/setup-demo.ps1 do the whole setup. See docs/demo/README.md.
#
# The engine runs in the foreground, so its log stays in this console: that is what you read when
# something does not look right. Close the window (or Ctrl+C) to stop it.

[CmdletBinding()]
param(
    # The converted asset folder: the one holding skyrim_world.db (see tools/demo/setup-demo.ps1).
    [Parameter(Mandatory = $true)]
    [string]$Assets,
    # Engine executable; defaults to this checkout's target/release engine.
    [string]$Engine,
    # Cluster overlay (distant cluster LOD and terrain) from <Assets>/derived/clusters-<settings-id>/.
    # "on" adds --clusters [<ClusterOverlay>] and --cluster-terrain; "off" adds neither.
    [ValidateSet("on", "off")]
    [string]$Cluster = "off",
    # Which overlay under <Assets>/derived/: its label (overlay.json), settings id, folder name or a
    # path to the overlay folder. Empty selects the only overlay (the engine refuses if there are
    # several, so pass the label then; the converter's default label is "default").
    [string]$ClusterOverlay,
    # Door crossings. "on" maps to the engine's seamless portals (--load-doors seamless);
    # "off" maps to load screens (--load-doors load-screen).
    [ValidateSet("on", "off")]
    [string]$PortalDoors = "off",
    # Optional distant terrain sun shadows; requires -Cluster on.
    [switch]$FarTerrainShadows,
    # Explicit development use only: keeps existing incomplete-core warnings visible.
    [switch]$AllowIncompleteAssets,
    # Explicit demo presentation; the engine's global default remains unchanged.
    [ValidateSet("vanilla-low", "vanilla-medium", "vanilla-high", "vanilla-ultra", "showcase", "current", "bevy", "custom")]
    [string]$Graphics = "showcase",
    # Anti-aliasing on top of the preset. TAA hides the shimmer of thin foliage at a distance; it
    # needs the stable cluster draw order (fixed 2026-10-20), without which it smeared pine leaves.
    [ValidateSet("off", "fxaa", "smaa", "taa")]
    [string]$Aa = "taa",
    # Where to start: on foot outside the Alftand entrance in the Pale, straight inside
    # Blackreach, or on the Helgen road south-west of Riverwood.
    [ValidateSet("alftand", "blackreach", "riverwood")]
    [string]$Start = "alftand",
    # Optional Creation-unit position and heading, overriding the named demo's start pose.
    [ValidateCount(3, 3)]
    [double[]]$StartPosition,
    [double]$StartYaw,
    # Fly instead of walking (the old free camera: WASD, Space up, Shift down, Ctrl fast).
    [switch]$Fly,
    # Scripted run instead of an interactive one: walks the route, screenshots every place and door,
    # then holds W in Blackreach. Writes its log and images into this folder.
    [string]$Tour,
    # Render the camera poses in this JSON file to PNGs and exit (docs/design/reference-shots.md).
    [string]$Shots,
    # Where a -Shots run writes its images; defaults to a folder named after the shots file.
    [string]$ShotsOut,
    # Reach of the terrain-only ring, in cells beyond the full-detail grid (engine default 8).
    [int]$TerrainRadius = 0,
    # Print the command instead of running it.
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repository = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$windowsVariable = Get-Variable -Name IsWindows -ErrorAction SilentlyContinue
$onWindows = if ($windowsVariable) { [bool]$windowsVariable.Value } else { $true }

function Write-Note([string]$Text) {
    Write-Host "   $Text" -ForegroundColor Gray
}

function Resolve-RepoPath([string]$Path) {
    if ([IO.Path]::IsPathRooted($Path)) { return [IO.Path]::GetFullPath($Path) }
    return [IO.Path]::GetFullPath((Join-Path $repository $Path))
}

$engineRelative = if ($onWindows) { "target/release/engine.exe" } else { "target/release/engine" }
$Engine = if ($Engine) { Resolve-RepoPath $Engine } else { Join-Path $repository $engineRelative }
if (-not (Test-Path -LiteralPath $Engine -PathType Leaf)) {
    throw @"
The engine executable does not exist at '$Engine'.

Build it:      cargo build --release -p engine --bin engine
Or set up:     pwsh -File tools/demo/setup-demo.ps1 -SkyrimPath "<SkyrimSE>" -OutDir "<converted>"
Or select:    -Engine "<engine executable>"
"@
}

if ($PSBoundParameters.ContainsKey("StartYaw") -and -not $PSBoundParameters.ContainsKey("StartPosition")) {
    throw "-StartYaw requires -StartPosition."
}
foreach ($coordinate in @($StartPosition) + @($StartYaw)) {
    if ([double]::IsNaN($coordinate) -or [double]::IsInfinity($coordinate) -or [Math]::Abs($coordinate) -gt [single]::MaxValue) {
        throw "Start position and yaw must be finite numbers in the engine float range."
    }
}

$Assets = Resolve-RepoPath $Assets
if (-not (Test-Path -LiteralPath $Assets -PathType Container)) {
    throw "The asset folder '$Assets' does not exist."
}
if ($FarTerrainShadows -and $Cluster -ne "on") {
    throw "-FarTerrainShadows requires -Cluster on: distant terrain sun shadows need the cluster terrain."
}
if ($ClusterOverlay -and $Cluster -ne "on") {
    throw "-ClusterOverlay requires -Cluster on."
}
if ($Cluster -eq "on" -and -not (Test-Path -LiteralPath (Join-Path $Assets "skyrim_world.db") -PathType Leaf)) {
    throw "The base dataset '$Assets' has no skyrim_world.db."
}

# The overlays of a base live in <base>/derived/clusters-<settings-id>/ (overlay.json, clusters.db).
# This only checks that the selection can match something, for a clear message before the engine
# starts; the engine decides the binding to the base (core identity, fingerprint) and refuses a
# mismatch or an old full-copy dataset.
function Get-ClusterOverlayFolders([string]$Base) {
    $derived = Join-Path $Base "derived"
    if (-not (Test-Path -LiteralPath $derived -PathType Container)) { return @() }
    return @(Get-ChildItem -LiteralPath $derived -Directory |
        Where-Object { $_.Name -like "clusters-*" -and (Test-Path -LiteralPath (Join-Path $_.FullName "overlay.json") -PathType Leaf) })
}

function Get-ClusterOverlayLabel([string]$Folder) {
    try {
        $document = Get-Content -LiteralPath (Join-Path $Folder "overlay.json") -Raw | ConvertFrom-Json
        return [string]$document.label
    } catch {
        return ""
    }
}

function Get-ClusterOverlayNames($Overlays) {
    return ($Overlays | ForEach-Object { "$($_.Name) (label '$(Get-ClusterOverlayLabel $_.FullName)')" }) -join ", "
}

function Resolve-ClusterOverlaySelector([string]$Base, [string]$Selector) {
    $overlays = @(Get-ClusterOverlayFolders $Base)
    if ($overlays.Count -eq 0) {
        throw "'$Base' has no cluster overlay under derived/. Build one: converter derive-clusters `"$Base`" --terrain ... (docs/demo/cluster-small-primitive-derivation.md)."
    }
    if (-not $Selector) {
        if ($overlays.Count -gt 1) {
            throw "'$Base' has several cluster overlays: $(Get-ClusterOverlayNames $overlays). Choose one with -ClusterOverlay <label|settings-id>."
        }
        return ""
    }
    # A path to an overlay folder (as the engine takes it: relative to the repository or to the base)
    # passes through absolute; the engine checks that it belongs to the base.
    $candidates = @(Resolve-RepoPath $Selector)
    if (-not [IO.Path]::IsPathRooted($Selector)) { $candidates += Resolve-RepoPath (Join-Path $Base $Selector) }
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath (Join-Path $candidate "overlay.json") -PathType Leaf) { return $candidate }
    }
    $found = @($overlays | Where-Object {
        $_.Name -eq $Selector -or $_.Name -eq "clusters-$Selector" -or (Get-ClusterOverlayLabel $_.FullName) -eq $Selector
    })
    if ($found.Count -ne 1) {
        throw "-ClusterOverlay '$Selector' matches $($found.Count) cluster overlays under '$Base': $(Get-ClusterOverlayNames $overlays)."
    }
    return $Selector
}

$clusterSelector = ""
if ($Cluster -eq "on") {
    $clusterSelector = Resolve-ClusterOverlaySelector $Assets $ClusterOverlay
}
if (-not (Test-Path -LiteralPath (Join-Path $Assets "skyrim_world.db"))) {
    Write-Note "'$Assets' has no skyrim_world.db - it does not look like a converted asset folder."
    Write-Note "Convert yours with tools/demo/setup-demo.ps1."
}

$doorMode = if ($PortalDoors -eq "on") { "seamless" } else { "load-screen" }

$arguments = [System.Collections.Generic.List[string]]::new()
$arguments.Add("--assets"); $arguments.Add($Assets)
if ($AllowIncompleteAssets) {
    Write-Warning "Development dataset: existing core record warnings/incompleteness remain. This does not certify the core as complete."
    $arguments.Add("--allow-incomplete-assets")
}
$arguments.Add("--graphics"); $arguments.Add($Graphics)
$arguments.Add("--aa"); $arguments.Add($Aa)
$arguments.Add("--load-doors"); $arguments.Add($doorMode)
$arguments.Add("--demo"); $arguments.Add($Start)
if ($PSBoundParameters.ContainsKey("StartPosition")) {
    $arguments.Add("--start-position")
    foreach ($coordinate in $StartPosition) {
        $arguments.Add($coordinate.ToString("R", [Globalization.CultureInfo]::InvariantCulture))
    }
}
if ($PSBoundParameters.ContainsKey("StartYaw")) {
    $arguments.Add("--start-yaw")
    $arguments.Add($StartYaw.ToString("R", [Globalization.CultureInfo]::InvariantCulture))
}
# Walking is opt-in (--walk); without it the engine uses the free-flight camera, which is what -Fly
# asks for. There is no --fly flag.
if (-not $Fly) { $arguments.Add("--walk") }
# The demo front-loads (the user, 2026-10-18): the first load runs behind a black cover, and the
# exterior's distant LOD stays loaded while inside an interior, so coming back out reloads nothing.
$arguments.Add("--startup-cover")
$arguments.Add("--lod-keep-hidden-in-interiors")
if ($Cluster -eq "on") {
    # The overlay stays separate from the base: --assets (above) is the base, --clusters names the
    # overlay under its derived/ folder. A bare --clusters takes the only overlay.
    $arguments.Add("--clusters")
    if ($clusterSelector) { $arguments.Add($clusterSelector) }
    $arguments.Add("--cluster-terrain")
    # Every cluster batch loads behind the startup cover, not as it comes into view: a batch
    # arriving during play changes what the views select and voids a crossing's destination proof.
    $arguments.Add("--cluster-preload-batches")
    # The cluster owns the statics, trees and far terrain it covers at every distance (chosen at
    # launch): no per-frame handoff with the stock copies, no baked object LOD or tree billboards.
    $arguments.Add("--cluster-own-statics")
    if ($FarTerrainShadows) { $arguments.Add("--cluster-far-terrain-shadows") }
}
if ($Tour) { $arguments.Add("--demo-tour"); $arguments.Add((Resolve-RepoPath $Tour)) }
if ($Shots) { $arguments.Add("--shots"); $arguments.Add((Resolve-RepoPath $Shots)) }
if ($ShotsOut) { $arguments.Add("--shots-out"); $arguments.Add((Resolve-RepoPath $ShotsOut)) }
if ($TerrainRadius -gt 0) { $arguments.Add("--terrain-radius"); $arguments.Add("$TerrainRadius") }

$display = "& " + ((@($engine) + $arguments | ForEach-Object { "'" + $_.Replace("'", "''") + "'" }) -join " ")

$graphicsNote = if ($Graphics -eq "showcase") { " (script default; the engine's own default is vanilla-high)" } else { "" }
# Not $farTerrainShadows: PowerShell variable names are case-insensitive and the parameter
# variable would keep its [switch] type constraint and refuse the string assignment.
$farShadowsText = if ($FarTerrainShadows) { "on" } else { "off" }

Write-Note "engine:              $engine"
Write-Note "assets:              $Assets"
Write-Note "start:               $Start"
$overlayNote = if ($Cluster -ne "on") { "" } elseif ($clusterSelector) { " (overlay $clusterSelector)" } else { " (the only overlay under derived/)" }
Write-Note "cluster:             $Cluster$overlayNote"
Write-Note "portal doors:        $PortalDoors (--load-doors $doorMode)"
Write-Note "graphics preset:     $Graphics$graphicsNote"
Write-Note "anti-aliasing:       $Aa"
Write-Note "loading:             first load behind a cover; exterior distant LOD kept inside interiors"
Write-Note "far terrain shadows: $farShadowsText"
if ($Fly) {
    Write-Note "controls: legacy free camera, WASD move, Space up, Shift down, Ctrl fast."
} else {
    Write-Note "controls: mouse look (click the window first), WASD move, Shift run, Space jump,"
    if ($PortalDoors -eq "on") {
        Write-Note "          E open/close doors, then walk through seamlessly; F walk/fly, Esc release the mouse."
    } else {
        Write-Note "          E activate a load-screen door, F walk/fly, Esc release the mouse."
    }
}
Write-Note "graphics: G opens the development panel; arrows change settings, S saves."
Write-Host ""

if ($DryRun) {
    Write-Host "[dry-run] $display"
    return
}

Write-Host "+ $display"

& $engine @($arguments)
exit $LASTEXITCODE
