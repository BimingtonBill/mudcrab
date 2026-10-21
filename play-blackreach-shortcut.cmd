@echo off
rem Mudcrab: start directly in Blackreach (skips the four doors).
cd /d "%~dp0"
start "" "target\release\engine.exe" --assets "C:\Modding\Mudcrab\SkyrimConverted" --demo blackreach --walk
