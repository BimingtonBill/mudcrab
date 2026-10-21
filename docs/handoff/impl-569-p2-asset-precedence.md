# impl-569-p2-asset-precedence

Track: Phase 2. Branch `impl/569-asset-precedence`, cut from `main`.

## Review follow-up (host discovery moved out of the library)

The first pass had `pipeline.rs` itself read `%LOCALAPPDATA%`/`%USERPROFILE%`
and probe `Documents/...`, so `AssetPipeline::run_async` behaved differently
per machine, and any test that built a `PipelineConfig` directly (most of
them) could pick up whatever real `Plugins.txt`/`Skyrim.ini` happened to sit
on the machine running `cargo test` — which is a real, not hypothetical,
problem: this dev box has both. The review correctly called the warning
gates I'd added a way of hiding that, not fixing it. This section documents
the fix; "What changed" and everything below it describes the current state.

**Moved to `main.rs` (the only place that reads the host now):**

- `default_documents_dir()` (`%USERPROFILE%\Documents`) and
  `default_plugins_txt_candidate()` (`%LOCALAPPDATA%\Skyrim Special
  Edition\Plugins.txt`) — both env-var reads, both now private functions in
  `main.rs`.
- `resolve_default_path(explicit, candidates) -> Option<PathBuf>`: picks
  `explicit` if given, else the first `candidates` entry that exists as a
  file. Pure given its inputs (no env reads of its own), so it's tested with
  temp files, not the real host. `main()` calls it twice — once for
  `plugins_file` (a single candidate), once for `skyrim_ini` (the
  `archive_order::default_ini_candidates` list) — and assigns the result
  straight into `PipelineConfig`.
- New `--plugins-file <path>` CLI option (this brief's main.rs scope note
  said "the new option only", meaning `--skyrim-ini`; the review's design
  requires `main.rs` to resolve `plugins_file` too, so `--plugins-file` is
  now a real flag, superseding that note).

**Removed from the library** (`archive_order.rs`/`pipeline.rs`): every
`env::var_os` call, `resolve_ini_archive_list`, `IniArchiveList`, and
`plugin_paths`'s `plugins_txt_candidate` parameter. `archive_order.rs` keeps
only pure, given-paths logic: `STOCK_ARCHIVE_LIST`, `parse_archive_list`,
`default_ini_candidates` (path-joining only, `documents_dir` is a parameter,
not an env read), and `sort_archives_by_load_order`.

**`pipeline.rs` now uses only `PipelineConfig`:**
- `plugin_paths(config, files)`: `config.plugins_file` if set (read via
  `read_plugins_txt`, no second-guessing); else alphabetical file-scan
  fallback.
- `ini_archive_list(config)`: `config.skyrim_ini` if set (read + parsed via
  `parse_archive_list`); else `STOCK_ARCHIVE_LIST` with a warning.

Tests that exercise these now build `PipelineConfig` and set
`plugins_file`/`skyrim_ini` to temp-file paths directly — none of them touch
`%LOCALAPPDATA%`, `Documents`, or the game folder.

**Which warning gates I kept, which I dropped, and why** (the review asked
for this explicitly):

- **Dropped:** "an existing-but-empty `Plugins.txt` is treated as missing
  and falls through to the file scan." This was a first-pass workaround for
  exactly the contamination the review flagged (this machine's real,
  near-empty `Plugins.txt`) — it has no justification once the pipeline only
  ever sees a config value the caller already resolved. Now: `config.
  plugins_file = Some(path)` is read as-is, full stop; a configured file
  that lists nothing active really does yield zero plugins. Test:
  `pipeline::tests::a_configured_plugins_file_with_no_active_plugins_yields_no_plugins`.
- **Kept, reframed as a message-accuracy rule, not a hermeticity rule:** the
  alphabetical-plugin-order warning only fires for more than one plugin
  (`plugins.len() > 1`), and the ini-fallback warning only fires when
  `sort_archives_by_load_order` reports the ini tier was actually consulted
  (`used_ini_tier`). Both warnings *claim* something ("guessed a load
  order", "fell back to the stock list") that is simply false when there
  was nothing to guess (0-1 plugins — only one possible order exists either
  way) or nothing that used the fallback list (every archive matched a
  plugin, so which ini list backed tier 2 made no difference this run).
  These are about the warning text being true, independent of which host or
  test produced the input — with the host reads gone, they no longer serve
  double duty as a test-isolation hack. Concretely: the `launcher` crate
  (`crates/launcher/src/handlers.rs`) never sets `plugins_file`/`skyrim_ini`
  and, after this change, gets no host discovery of its own (that now lives
  only in `main.rs`), so every GUI-driven conversion hits both "unset"
  branches — without these two gates, ordinary single-plugin, no-mod
  conversions through the launcher would be marked incomplete on every run.
  Tests: `pipeline::tests::a_single_plugin_is_unambiguous_and_does_not_warn`,
  `archive_order::tests::orders_archives_by_plugin_load_order` /
  `a_stock_install_sorts_every_archive_under_skyrim_esm_unaffected_by_the_ini_tier`
  (`used_ini_tier == false`).

**Consequence worth flagging:** `crates/launcher/src/handlers.rs` now gets
*no* Skyrim.ini/Plugins.txt auto-discovery at all (it constructs
`PipelineConfig::new` directly, bypassing `main.rs`). That's an accepted
side effect of "move host discovery into the CLI layer", not something this
brief's scope covers fixing.

Reran `cargo fmt --all -- --check`, `cargo clippy -p converter --all-targets
-- -D warnings`, `cargo test -p converter` after this change — all still
pass; exact output in the task report.

## What changed (current state)

New module `crates/converter/src/archive_order.rs`:

- `STOCK_ARCHIVE_LIST`: the 18 archive names from Special Edition's shipped
  `Skyrim_Default.ini` (`[Archive] sResourceArchiveList` then
  `sResourceArchiveList2`). Verified byte-for-byte against
  `Skyrim_Default.ini` in a real install (see "How I checked" below); this is
  Bethesda's own default, identical on every SE install, not a value read
  from any particular machine.
- `parse_archive_list`: hand-rolled flat-ini parser for the `[Archive]`
  section only (no new dependency, per the brief).
- `default_ini_candidates`: the two stock `Skyrim.ini` locations (parent of
  `Data/`, and `Documents/My Games/Skyrim Special Edition/`) as a pure
  function of `data_dir` and a given `documents_dir` — no env reads; `main.rs`
  supplies `documents_dir` from `%USERPROFILE%`.
- `sort_archives_by_load_order`: takes the ini archive list as a third input
  and checks tier 3 (plugin match) before tier 2 (ini match) before tier 1
  (unmatched). Returns the tier-1 warnings plus whether the ini tier was
  actually consulted by anything (`used_ini_tier`).

`crates/converter/src/pipeline.rs`:

- `plugin_paths(config, files)`: config-only, see above.
- `ini_archive_list(config)`: config-only, see above.
- `run_into` calls both, gates the ini-fallback warning on `used_ini_tier`,
  and folds every returned warning into `report.warnings`.
- `overlay_loose_assets` (tier 4) got a doc comment naming the tiers; its
  extension list (`dds, nif, pex`) is untouched.

`crates/converter/src/config.rs`: new `PipelineConfig::skyrim_ini:
Option<PathBuf>` (mirrors `plugins_file`).

`crates/converter/src/main.rs`: new `--skyrim-ini <path>` and
`--plugins-file <path>` CLI options; both resolved against host-discovered
defaults via `resolve_default_path` before being written into
`PipelineConfig` (see "Review follow-up" above).

`crates/converter/src/lib.rs`: `pub mod archive_order;` — one line,
otherwise the new file isn't part of the crate. Not in the brief's file list
but unavoidable; nothing else in that file changed.

## Per point, holds-on-main or not, and the test

Context bullets (audited against this checkout's `main`, i.e.
`2105bfd`/upstream, before any edit):

- "Archives ... ordered by `sort_archives_by_load_order`: ... An archive
  matching no plugin gets `usize::MAX`, so it sorts last ... No ini file is
  read." **Holds verbatim.** Fixed by tier classification order
  plugin → ini → unmatched (unmatched now sorts *first*).
  Test: `archive_order::tests::an_archive_matching_neither_sorts_lowest_and_warns`.
- "Plugin load order falls back to alphabetical ... unless `--plugins-file`
  is given." **Held before this task** (as a `PipelineConfig` field with no
  CLI flag); now `--plugins-file` is a real CLI flag too (added in the
  review follow-up, see above).
- "`overlay_loose_assets` ... only for the extensions `dds, nif, btr, bto,
  pex`. (The LOD branch adds `lod, lst, btt`.)" **Does not hold as
  described**: on this `main`, the list is `dds, nif, pex` only — no `btr`/
  `bto` (and no LOD extensions, consistent with the LOD branch not being
  merged here). Left the list untouched per the brief; added a doc comment
  naming the tiers instead of changing it.

The fix, in the brief's numbering:

1. Tier 2 (ini archive list). `archive_order::{STOCK_ARCHIVE_LIST,
   parse_archive_list, default_ini_candidates}` + `pipeline::ini_archive_list`
   + `--skyrim-ini`/discovery in `main.rs`.
   Tests: `archive_order::tests::ini_listed_archive_loses_to_a_plugin_archive_at_the_same_stem`,
   `two_ini_archives_resolve_in_list_order`,
   `default_ini_candidates_checks_the_data_parent_then_documents`,
   `parses_both_lists_in_order_from_the_archive_section_only`,
   `pipeline::tests::reads_the_configured_skyrim_ini_with_no_warning`.
2. Unmatched archive sorts lowest with a warning, not highest.
   Test: `archive_order::tests::an_archive_matching_neither_sorts_lowest_and_warns`.
3. `Plugins.txt`/`--plugins-file`: `main.rs` resolves which file (if any)
   applies; `pipeline::plugin_paths` reads whatever `config.plugins_file`
   says, "*" markers and all (via the existing `esm::read_plugins_txt`).
   Tests: `pipeline::tests::plugins_file_sets_the_load_order_from_active_plugins_only`
   (the "*" / active-only behaviour) and
   `main::tests::resolve_default_path_prefers_the_explicit_value` (the
   override).
4. No ini and no `Plugins.txt` configured → stock fallback with a warning.
   Tests: `pipeline::tests::falls_back_to_stock_archive_list_and_warns_when_skyrim_ini_is_unset`
   (ini half) and
   `pipeline::tests::falls_back_to_alphabetical_plugin_order_and_warns_when_plugins_file_is_unset`
   (plugin half).
5. Loose-file extension list left alone; tier comment added on
   `overlay_loose_assets`.

## How I checked a stock install resolves to the same files

Every Special Edition stock archive's file stem (`Skyrim - Misc`, `Skyrim -
Textures0`, ...) starts with the `Skyrim.esm` plugin's stem followed by a
separator, so it always matches the tier-3 (plugin) rule — checked first,
unchanged — before the ini tier or the unmatched tier (this change's actual
edits) are ever consulted. Locked in by
`archive_order::tests::a_stock_install_sorts_every_archive_under_skyrim_esm_unaffected_by_the_ini_tier`:
sorts all 18 real `STOCK_ARCHIVE_LIST` names plus `Skyrim.esm`, from a
deliberately reversed input, and asserts zero warnings, `used_ini_tier ==
false`, and the same order the old plugin-index+stem sort already produced.

On the plugin side, the alphabetical fallback's master ranking (`skyrim.esm
=0, update.esm=1, dawnguard.esm=2, hearthfires.esm=3, dragonborn.esm=4`,
unchanged) is Bethesda's own fixed master order — the same order a real
`Plugins.txt` for a mod-free install would list them in, so there is only
one possible ordering for the vanilla master set either way.

I also grounded the constants against a real (read-only) install at
`C:/Games/Steam/steamapps/common/Skyrim Special Edition/`:
`Skyrim_Default.ini`'s `[Archive]` section is exactly the 18-entry
`STOCK_ARCHIVE_LIST` above; there is no `Skyrim.ini` beside the executable in
a stock install (only `Skyrim_Default.ini`/`High.ini`/etc.), and the real,
active `Skyrim.ini` lives in `Documents/My Games/Skyrim Special Edition/` —
matching the two candidates `default_ini_candidates` checks. Did not run a
full conversion (out of scope).

## Checks run (after the review follow-up)

- `cargo fmt --all -- --check` — clean.
- `cargo clippy -p converter --all-targets -- -D warnings` — clean (only the
  pre-existing, unrelated `proc-macro-error2` future-incompat note).
- `cargo test -p converter` — all green: 113 lib tests (archive_order +
  pipeline + everything else), 8 `fixture_round_trip.rs` integration tests,
  plus the small `src/bin/*`/`main.rs` unit tests (main.rs's own binary test
  target now has 5, including the three new `resolve_default_path` tests).
  No test reads `%LOCALAPPDATA%`, `Documents`, or the real game folder.

## Left as-is / out of scope

- `crates/launcher/src/handlers.rs` — untouched; see "Consequence worth
  flagging" above (no more auto-discovery for the GUI path).
- The LOD branch's loose-file extensions (`lod, lst, btt`) and `btr`/`bto` —
  not present on this `main`, left for that branch as instructed.
