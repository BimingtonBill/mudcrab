<#
.SYNOPSIS
Plays the internal experimental demo (main + open PRs + portal), built by tools/experimental_build.ps1.
LOCAL ONLY. Extra arguments go to the engine, e.g. `-- --demo alftand`.
#>
param(
    [string]$Exe = "D:/Modding-overflow/wt-targets/experimental/quick/engine.exe",
    [string]$Assets = "D:/Modding-overflow/SkyrimConverted-experimental",
    [string]$Demo = "riverwood",
    [Parameter(ValueFromRemainingArguments)]$EngineArgs
)
if (-not (Test-Path $Exe)) { throw "no build at $Exe; run tools/experimental_build.ps1 first" }
if (-not (Test-Path "$Assets/skyrim_world.db")) { throw "no converted data at $Assets; see the conversion line in the day log" }
& $Exe --assets $Assets --demo $Demo --walk --run-label "experimental demo" @EngineArgs
