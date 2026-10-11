# Goal
The launcher refuses an Output folder that overlaps the Skyrim Data folder (is it, sits inside it, or holds it) at the
moment it is chosen, not only when a run starts, using the converter's own rule. Track: Phase 3 (held locally).

# Context
- Crate `crates/launcher`. The converter's run already refuses this: `PipelineConfig::validate` compares resolved
  paths of the output (and its ingestion cache) with the Data folder. Phase 1 made that rule public as
  `pub fn converter::check_outside_data(data_dir: &Path, output_dir: &Path) -> color_eyre::Result<()>` (re-exported
  from `converter`; it checks the output and its default cache `<output>.assets-cache`; `data_dir` must exist because
  it is canonicalised, so a missing Data folder returns an error), which `validate` now calls, so launcher and converter share one rule.
- `crates/launcher/src/conversion/mod.rs`: `output_is_safe_target(output: &Path) -> Result<(), String>` words
  `converter::check_output_dir`'s answer for the notice pane. It is called from
  `crates/launcher/src/conversion/panel.rs`: `accept_dropped_folder` (~line 511, the Output drop) and
  `click_controls` (~line 444, every Start / Start over / Resume press).
- `GamePathConfig { skyrim_data_path: Option<PathBuf>, converted_assets_path: PathBuf }` holds the two folders.
  A dropped folder that holds `Skyrim.esm` (or an install root with a Data subfolder) becomes the Data folder
  (`game_detection::data_dir_from_drop`); any other folder is offered as the Output.
- Decisions already made:
  1. Extend the launcher's check to take the Data folder: e.g. `output_problem(data: Option<&Path>, output: &Path)
     -> Result<(), String>` (or add a `data` argument to `output_is_safe_target`), which runs the converter's overlap
     helper when `data` is `Some`, then `check_output_dir` as today. One function, used at both call sites.
     Notice wording (plain, like the others): "<output> overlaps the Skyrim Data folder <data>; choose a folder
     outside it." Use the converter's message if it already reads like that.
  2. Output drop: refused with that notice, the Output row keeps its old value (same as the existing refusal).
  3. Start / Start over / Resume press: refused with that notice, no input pushed (same as the existing refusal).
  4. Data drop: the Data folder IS taken (it is the game), but if the current Output now overlaps it, add the notice
     so the player picks another Output; the press check then blocks Start until they do.
  5. If the overlap helper returns an I/O error (unreadable path), treat it like the existing Unreadable case: refuse
     with a notice that names the path.
- Update `docs/specs/modding/launcher.md` where the Output rules are listed (the "Output" row and "The Output folder
  is replaced" section): add the Data-folder rule.

# Scope
Owned files: crates/launcher/src/conversion/mod.rs, crates/launcher/src/conversion/panel.rs, docs/specs/modding/launcher.md
- Change only the owned files. No Cargo.toml changes, no converter changes. No git commands.

# Done when
Acceptance: cargo nextest run -p launcher
- Also run `cargo fmt -p launcher -- --check` and `cargo clippy -p launcher --all-targets -- -D warnings`.
- Tests (in the existing test modules, reusing their helpers such as `input_app`, `press`, `notices`, `data_folder`,
  `temp_dir`):
  - the launcher check refuses an output equal to Data, inside Data (a not-yet-existing folder inside it too), and a
    folder holding Data; accepts a sibling folder; with `data = None` it behaves as before;
  - dropping an empty folder that sits inside the Data folder leaves the Output unchanged, with the notice;
  - with Output inside Data, Start and Resume push no input and show the notice;
  - dropping a Data folder that holds the current Output keeps the new Data and adds the notice.
- Existing tests still pass.

# Report
- What you changed and any decision that wasn't obvious.
- The commands you ran and what they printed (short).
