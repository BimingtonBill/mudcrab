# Goal
Produce a file-organisation and naming plan for the project's loose files (the user, 2026-10-07: "do an
organisation sweep and clean up ... organize files and naming ... take no shortcuts"). Read-only: you
propose, Upstream Dev applies. Track: Upstream Dev (housekeeping).

# Context
- `C:/Modding/Mudcrab/mudcrab/local/` (git-ignored, shared by all dev sessions) has about 192 top-level
  entries: many conversion/demo logs (`conversion-s18.out.log` ... `conversion-s28.*`,
  `demo-user*.out/err`, `conversion-p2night*`), old task folders (`t210`, `t218`, `t221`, `t232`, ...),
  handovers, scripts, an old `engine-schema17.exe`, and folders that ARE in active use (`agents`, `team`,
  `research`, `teach`, `pr-drafts`, `plans`, `review`, `reference`, `perf-baseline`, `disk`, `impl`).
- Loose files also sit at the roots of `C:/Modding` (e.g. `cfg_ours.rs`, `cfg_theirs.rs`,
  `config_firstpass.rs`, `render_firstpass.rs`, `fix_review645.py`, `resolve_hunks.py`, `showhunks.py`,
  `launch_coder_d.sh`, `SkyrimConverted-conversion-report.json`) and `D:/Mudcrab` (convert-*.cmd,
  *.exit, *-report.json, s15-*, p1-census-*, wt-702-*.log, engine-before-water.exe, converter-s15.exe ...).
- Rules (AGENTS.md "Disk"): unknown means keep; only owners delete, with the user's yes; never touch repo
  checkouts' tracked files, `SkyrimConverted*`, the game install, the user's tools (Wabbajack, Modlists,
  Bethini Pie). Moving a file can break a reference: notes, the research index, handoff logs and memory
  cite paths.

# Scope
- Read only. Run: `ls`, `rg`, `git log`, `git status` (to list, date and
  cross-reference files). Do not move, rename or delete anything.
- For every top-level entry of `local/`, and every loose FILE at the roots of `C:/Modding` and
  `D:/Mudcrab` (not the folders there, those are handled separately), find:
  last modified date, size, likely owner (session/track) and purpose, and every reference to its path in:
  `local/research/INDEX.md`, `local/team/*.md`, `docs/**`, `AGENTS.md`, `tasks/**`, `scripts/**`, `tools/**`,
  `local/plans/**`, `local/pr-drafts/**`, `local/teach/**`, and the memory folder (-AddDir).
- Propose a target layout for `local/` (for example `local/logs/conversions/`, `local/logs/demo-runs/`,
  `local/archive/2026-09/` for old task folders, `local/handover/` for handovers, `local/bin/` for old
  binaries), and a naming convention for new files and folders (owner/track, purpose, date where useful),
  consistent with what is already used (`impl-NNN-p2-slug`, `local/team/YYYY-MM-DD.md`).
- Entries that are in active use or referenced from current docs stay where they are unless the plan also
  lists every reference to update.

# Done when
- Every entry is classified: keep in place / move to <path> (with references to update) / dead candidate
  (owner must confirm) / unknown.
- A machine-readable plan exists.

# Report
- Put the plan in the report as one TSV block with columns:
  path, kind, size, modified, owner_guess, purpose, references (paths:line, or none), action, target, notes.
  (You cannot create files; Upstream Dev saves the TSV.)
- In the report: the proposed layout and naming convention, counts per action, and the 15 most
  important decisions for the owners.
