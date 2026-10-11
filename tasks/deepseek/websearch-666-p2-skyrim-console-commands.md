# Goal
Produce a cited reference of Skyrim (Special Edition) console commands, so the engine's developer console uses only Skyrim's real command names, short forms and syntax. Track: Phase 2 (upstream issue #88).

# Context
- The engine's console currently offers invented commands: `help`, `clear`, `noclip`, `collision` (shows collision outlines, today's F3 key), `tankard` (spawns a test physics object, T key) and `grab` (E key). These must become real Skyrim commands, or stay keys only when Skyrim has no equivalent.
- Likely mappings to verify (do not assume): ToggleCollision / tcl (toggle player collision = noclip); ToggleCollisionGeometry / tcg or whatever Skyrim really uses to show collision geometry (check whether it is TCG, or "ToggleCollisionGeometry" exists only in Fallout/Oblivion; if Skyrim has none, say so); Help <filter> [form type] as Skyrim's help with filter; player.placeatme <FormID> [count] for spawning an object; whether Skyrim has any console command to clear the console output (for example "cls" exists?); grabbing objects has no console command (verify).
- Also list the commands a later slice would likely implement, with exact syntax: coc (CenterOnCell), cow (CenterOnWorld), tgm, tfc (with the optional 1), tm, tg, tll, tai, tcai, player.setav / setav, player.additem, prid, moveto, setpos / getpos, setangle, kill, resurrect, disable / enable, markfordelete, unlock, lock, fov, sucsm, sgtm, qqq, save / load (console forms), tdetect, tmm, showinventory / inv, set timescale, togglesky? only real ones.

# Web questions
- UESP: https://en.uesp.net/wiki/Skyrim:Console (and subpages), the Creation Kit wiki console command pages (https://ck.uesp.net/wiki/Category:Console_Commands or similar). For each command: long name, short name(s), exact syntax and arguments, what it does, whether it needs a selected reference (prid / click). Note where sources disagree.

# Scope
Web reading only; change nothing in any repository.

# Done when
Every command named above is either listed with long name, short form, syntax, effect and a source link, or marked "not a Skyrim command" with the source that shows it.

# Report
A Markdown document: (1) a table "Engine action -> Skyrim command (long / short), syntax, source, or 'key only: no Skyrim command'" for help, clear, noclip, collision outlines, spawn object, grab; (2) a table of later-slice commands with the same columns; (3) notes on disagreements. Every row cites a URL. Under 1,200 words.
