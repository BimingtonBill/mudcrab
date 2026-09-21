@echo off
rem OpenSkyrim: start directly in Blackreach (skips the four doors).
cd /d "%~dp0"
start "" "target\release\engine.exe" --assets "C:\Modding\SkyrimConverted" --demo blackreach --walk
