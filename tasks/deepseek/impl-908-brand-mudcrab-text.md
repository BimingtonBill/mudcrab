# impl-908-brand-mudcrab-text (cross-track, upstream PR prepared by Phase 3 Dev for Upstream Dev)

**Worker:** Claude subagent ("Claude impl #908"). **Worktree:** `C:\Modding\wkj-brand`, branch
`brand/mudcrab-text` (from `main` 6f583cd). **Target dir:** `CARGO_TARGET_DIR=D:/Mudcrab/brand-target`.

## The decision (made upstream; the user chose the scope)

The project is being renamed from **OpenSkyrim** to **Mudcrab**. This PR changes **visible text only**:
what a person reads. It must not change any name that files, settings or code depend on, so nothing
breaks and no conversion has to be redone.

## Change ("OpenSkyrim" / "Open Skyrim" / "OPENSKYRIM" as the project's name -> "Mudcrab" / "MUDCRAB")

- Window titles, UI text (e.g. the launcher's `OPENSKYRIM` header, "OpenSkyrim Launcher & Mod Manager",
  "PLAY OPENSKYRIM"), console/log messages, CLI `--help`/usage text, error messages.
- Documentation prose: `README*`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `docs/**` (specs, roadmap,
  ADRs), `.github/ISSUE_TEMPLATE/*`.
- Code comments and doc comments that name the project.
- `Cargo.toml` `description`-style text fields (not names, not URLs).
- Test assertions that check a changed visible string (update them with the string).

## Keep exactly as they are

- Environment variables: every `OPENSKYRIM_*` (e.g. `OPENSKYRIM_SKYRIM_DATA`, `OPENSKYRIM_CONVERTED_DIR`,
  the `OPENSKYRIM_*_FIXTURE` test variables). Where docs mention them, keep the variable name and
  rename only the surrounding prose.
- glTF extension / extras names in converted data: `OPEN_SKYRIM_material`, `OPEN_SKYRIM_material_animation`,
  `openSkyrim.*` node extras, and anything else written into converted files.
- Crate, package, module, binary, type and function names; file and folder names (e.g. `openskyrim.cfg`,
  `openskyrim.d`, `openskyrim_scripting`, `openskyrim_converter`), log/temp file names.
- URLs and repository names (e.g. `.../OpenSkyrim.git`): they point at real places.
- `vendor/**`: never edit vendored code.
- `LICENSE-MIT` / other licence text: do NOT change; list it in the handoff for the maintainer
  (a copyright-holder name is a legal choice).
- `.gitignore` patterns.
- Historical records where the old name is the fact (e.g. "renamed from OpenSkyrim", changelog-like
  history): keep, or add "(now Mudcrab)" if it reads better.

When unsure whether a string is a name code depends on, keep it and list it in the handoff.

## Method

`git grep -n -i -E "open ?skyrim|open_skyrim"` over the tree; go file by file. Keep the diff minimal:
no reflowing of unrelated lines, no other edits. Afterwards grep again and list every remaining match
with the reason it was kept (group by reason).

## Checks (D: target dir)

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
```

## Commit

One commit, upstream style: `docs: rename the project to Mudcrab in user-facing text` (or `chore:`),
body: what changed, what was deliberately kept (env vars, data-format names, crate/file names, URLs,
licence). No internal references or machine paths. No push. Handoff:
`C:\Modding\Mudcrab\mudcrab\docs\handoff\impl-908-brand-mudcrab-text.md` (not committed) with the
kept-matches list and the licence note.
