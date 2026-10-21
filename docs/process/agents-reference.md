# AGENTS.md reference sections

Full text of sections moved out of `AGENTS.md` on 2026-10-16 to keep the always-loaded file short. Verbatim, unchanged. `AGENTS.md` keeps a short form and a pointer for each. Earlier full text: `agents-full-2026-10-11.md`.

## Crediting mod authors (the user, 2026-10-13)
Applies whenever work reimplements, replaces, converts or builds on a mod, a mod framework, SKSE or an SKSE plugin (modlist support, Luau API replacements, MCM, animation or physics runtimes, scanners):
- Write fresh code from documented behaviour. Never copy another author's code unless its licence allows it and the user agrees to those terms.
- Name the original mod and author, with link and licence, in the code comment, the PR text, the research note and `docs/CREDITS-mod-authors.md` (create it on first use, one row per mod: name, author, URL, licence, what Mudcrab took or replaced).
- Respect permission terms: "no conversion", "no redistribution" and "ask first" mean no. Never ship mod files; convert only the user's own install.
- Before replacing a key plugin, the user decides whether to contact its author. Draft the message; do not send it (publishing rule).
- Any scanner or note that lists mods carries a licence and a source-available column.

## Research index (the user, 2026-10-13)
Two indexes, one search: `rg -i "<term>" local/research/INDEX*.md`. Vanilla Skyrim facts go in `INDEX-skyrim.md` (by subsystem, with evidence tier and coverage); our decisions, measurements and plans go in `INDEX-mudcrab.md`; a note that does both gets a row in each. Every new research note gets its row when it is filed. A worker's Skyrim note is checked by a Claude review (about ten load-bearing claims, corrections marked in the note) before it counts as blueprint.

## Look vanilla records up first (2026-10-14)
Vanilla Skyrim record data is a text dump, one YAML file per record: `D:/Mudcrab/skyrim-records/<Plugin>.esm/<Type>/<EditorID> - <FormID>_<Plugin>.esm.yaml` (Spriggit, the five official masters; see `dump-status.txt` there). It sits outside every worktree on purpose.
- `python tools/skyrim_rec.py index` (every record type, coverage), `sheet <SIG>` (one page: vanilla fields with counts and ranges, what the converter reads with `file:line`, schema subrecord), `find <text> [--type Doors] [--plugin Skyrim]`, then `show <EditorID|FormID>` (an Update.esm override prints beside the original). Content search: `rg -i "<term>" D:/Mudcrab/skyrim-records`. A one-line NOTE on stderr means the sheets lag the converter or the dump: run `python tools/skyrim_rec.py refresh` (about 5 minutes). If you changed converter record code, run it before you close the thread. An edit hook reminds you once per session when you touch `crates/converter/src/esm` or `records`. The engine column covers table columns only; a `?` or `read partially` in a sheet means unknown, not absent.
- Before changing how a record or field is read, look it up and cite its EditorID or FormID. A guess about vanilla data is a shortcut.
- It shows what is stored, not what the engine does with it: xEdit, UESP and reference captures stay the sources for meaning and runtime behaviour. Rebuild with `python tools/skyrim_rec.py dump` (about 25 min; needs the .NET 10 SDK and `C:/Tools/Spriggit`).

## Tier 3 (disassembly, Ghidra, decompile) is gated (the user, 2026-10-15)
Before any Ghidra, decompile, disassembly or trace of a Skyrim binary, or writing from one, read the quick card in `docs/process/clean-room-tier3.md` (draft, no lawyer review yet). Tier 3 stays blocked until the user approves the exact question in `docs/provenance/<topic>.md`. Writers of Mudcrab code never see dirty-side output; specs pass `python tools/spec_lint.py <spec>` first. Never touch copy protection.
