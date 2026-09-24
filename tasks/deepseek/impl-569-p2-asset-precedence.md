# Goal
Track: Phase 2 (for upstream). The converter resolves archives and loose files in the same order
Skyrim does, so a mod's loose files and later archives replace what they should, and nothing else.

# Context
- Your checkout is cut from `main`. Use upstream's commit style (`fix(converter): ...`), with no
  `[deepseek]` prefix.
- Skyrim resolves an asset path through four tiers, lowest to highest priority:
  1. the hard-coded base archives;
  2. archives listed in `Skyrim.ini` (`[Archive] sResourceArchiveList`, then
     `sResourceArchiveList2`, comma-separated, in order);
  3. archives named after plugins, in plugin load order (`<plugin>.bsa`, `<plugin> - Textures.bsa`,
     and so on);
  4. loose files in `Data/`.
  A later tier overrides an earlier one at the same path, and within a tier a later archive
  overrides an earlier one.
- What the converter does today (from a read-only audit of `crates/converter/src/pipeline.rs`;
  confirm each point on `main` first, since line numbers may differ):
  - Archives are every `.bsa`/`.ba2` found anywhere under the data folder, ordered by
    `sort_archives_by_load_order`: an archive whose stem matches a plugin gets that plugin's
    index. An archive matching no plugin gets `usize::MAX`, so it sorts **last** and overrides
    every plugin archive, the reverse of tier 2 under tier 3. No ini file is read.
  - Plugin load order falls back to **alphabetical** filename order for everything but the five
    official masters, unless `--plugins-file` is given.
  - `overlay_loose_assets` lays loose files over the extracted vfs, but only for the extensions
    `dds, nif, btr, bto, pex`. (The LOD branch adds `lod, lst, btt`; leave that to it.)
- The fix:
  1. Tier 2: read `sResourceArchiveList` and `sResourceArchiveList2` from `Skyrim.ini`. The file
     sits beside the game's executable in the parent of `Data/` for a stock install, or in the
     user's `Documents/My Games/Skyrim Special Edition/`. Accept an explicit `--skyrim-ini <path>`
     option and try those two places otherwise. Order the listed archives by the list, below
     every plugin archive. When no ini is found, fall back to the stock SE list, and say so in
     the report and a warning.
  2. An archive that matches neither the ini list nor a plugin sorts **first** (lowest), with a
     warning naming it, instead of last.
  3. Without `--plugins-file`, read `%LOCALAPPDATA%/Skyrim Special Edition/Plugins.txt` when it
     exists (active plugins, `*` prefix), before falling back to alphabetical order with a
     warning. Keep `--plugins-file` as the override.
  4. Leave the loose-file extension list alone except for a comment naming the tiers.
- Keep behaviour identical for a stock Skyrim SE install with no mods: the same archives, in an
  order that yields the same winning files. Say how you checked that.

# Scope
Owned files: `crates/converter/src/pipeline.rs`, `crates/converter/src/config.rs`,
`crates/converter/src/main.rs` (the new option only), a new `crates/converter/src/archive_order.rs`
if the logic is large enough to deserve one, tests next to them, and
`docs/handoff/impl-569-p2-asset-precedence.md`. No engine changes, no schema bump, no new
dependencies (parse the ini by hand; it is a flat key/value file).

# Done when
- Tests that fail on the old code and pass on yours:
  1. An ini-listed archive loses to a plugin archive at the same path, and two ini archives resolve
     in list order.
  2. An archive matching neither sorts lowest and warns.
  3. A `Plugins.txt` fixture sets the load order (and with `*` markers only the active plugins
     count), and `--plugins-file` still overrides it.
  4. With no ini and no Plugins.txt, the order is the stock fallback, with a warning.
- `cargo fmt --all -- --check`, `cargo clippy -p converter --all-targets -- -D warnings` and
  `cargo test -p converter` pass.

# Report
- For each point: holds on `main` or not, what you changed, and the test's name.
- How you established that a stock install resolves to the same files.
