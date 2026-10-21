# Goal
Track: Phase 2 (for upstream). A design for giving the engine a Skyrim-like sky, as a series of small
upstream PRs, from existing research. Write the design; change no code.

# Context
- The research: `local/research/sky.md` (how vanilla builds the sky: the Atmosphere.nif dome with
  vertex-colour blend masks, stars, moons, sun, cloud layers; WTHR/CLMT records with real values;
  the sky-to-fog blend; a Bevy 0.19 mapping). Read it in full; it is the main input.
- Upstream code is on the `main` branch: read files with `git show main:<path>`, search with
  `git grep <pattern> main -- <paths>`. The engine is Bevy 0.19 (`crates/engine`); the world database
  is SQLite written by `crates/converter/src/esm/exporter.rs` (schema contract
  `docs/specs/converters/db-schema.md`, version constant in `crates/shared/src/lib.rs`).
- Today the engine draws no sky: a flat clear colour. Upstream has no weather records in the database.
- Upstream conventions: `CONTRIBUTING.md`, `docs/roadmap/02-completion-plan.md` (read on `main`). Any
  converter output or database change needs a schema bump. PRs must be testable without game data
  (the `dummy-content` fixture generator, `crates/dummy-content`).

# Scope
Read-only. Write nothing except your report.

# Done when
The design answers every question below with evidence (`branch:path:line`, or a section of
`local/research/sky.md`), and marks what is inferred rather than checked.

# Report
1. What the first, smallest useful sky is (e.g. a gradient dome from one weather's colours at a fixed
   time of day), and what it leaves out.
2. Converter side: which records and fields (WTHR colours by time of day, CLMT, WRLD's climate link),
   the table(s) to add, the schema bump, and the dummy-content additions for tests.
3. Engine side: how to draw it in Bevy 0.19 (a mesh dome vs a skybox vs a background shader), how it
   meets the existing distance fog at the horizon, and which files change.
4. A PR series, smallest first: files, one sentence each, dependencies, how each is tested without
   Skyrim, risk.
5. Open questions for the lead, and anything in `local/research/sky.md` you found doubtful.
