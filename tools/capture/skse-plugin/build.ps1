# Configure and build MudcrabCapture with the MSVC Build Tools, vcpkg and a CommonLibSSE-NG checkout.
# Usage: powershell -File build.ps1 [-Install]   (-Install copies the DLL into the MO2 mod folder)
param(
    [string]$CommonLib = 'C:\Tools\CommonLibSSE-NG',
    [string]$Vcpkg = 'C:\Tools\vcpkg',
    [string]$Mods = 'C:\Modding\MO2-capture\mods',
    [switch]$Install
)
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$vcvars = 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat'
$env:PATH += ';C:\Program Files (x86)\Microsoft Visual Studio\Installer'
# vcvars sets VCPKG_ROOT to Visual Studio's own copy, so ours is set after it, in the same cmd.
$cmd = "`"$vcvars`" >nul && set VCPKG_ROOT=$Vcpkg&& set COMMONLIBSSE_PATH=$CommonLib&& cd /d `"$here`" && cmake --preset release && cmake --build build"
cmd /c $cmd
if ($LASTEXITCODE) { throw "build failed ($LASTEXITCODE)" }
$dll = Get-ChildItem "$here\build" -Filter MudcrabCapture.dll -Recurse | Select-Object -First 1
Write-Output "built $($dll.FullName)"
if ($Install) {
    $dest = "$Mods\MudcrabCapture\SKSE\Plugins"
    New-Item -ItemType Directory -Force "$dest\MudcrabCapture" | Out-Null
    Copy-Item $dll.FullName $dest -Force
    Write-Output "installed to $dest"
}
