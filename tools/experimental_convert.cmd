@echo off
rem Converts Skyrim SE into the experimental demo's own output with the experimental branch's converter.
rem LOCAL ONLY. Run in its own console: start "" tools\experimental_convert.cmd
rem Needs ~70 GB free on D: (output ~12 GB, staging ~12 GB, cache ~47 GB). Never du these folders while it runs.
title Portal Dev: conversion SkyrimConverted-experimental (experimental/2026-10-03 converter)
set OUT=D:\Modding-overflow\SkyrimConverted-experimental
set EXITFILE=D:\Modding-overflow\convert-experimental.exit
del /q %EXITFILE% 2>nul
D:\Modding-overflow\wt-targets\experimental\quick\converter.exe "C:\Games\Steam\steamapps\common\Skyrim Special Edition\Data" %OUT% --texture-encoder gpu --report-json D:\Modding-overflow\convert-experimental-report.json
echo %ERRORLEVEL% > %EXITFILE%
