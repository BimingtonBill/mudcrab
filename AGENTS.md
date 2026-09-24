# Working agreement for AI assistants

<!-- ds-delegation:begin (managed by DeepSeek Workers/tools/ds_delegation.py; edit with --set) -->
## DeepSeek delegation level: 4 of 5 (Claude manages)

Claude plans, writes briefs, reviews and integrates; workers do the exploration and most coding, including multi-file changes in their own worktrees with owned-file lists. Claude avoids reading source beyond what review needs: ask a worker to locate, summarize or trace code instead. Keep several workers busy, and use a DeepSeek lead (-CanSpawn) for jobs that split into independent parts.

Web lookups: when a task needs information from outside the project (library or tool versions, changelogs, API documentation, an error message, how others solved something), send a DeepSeek websearch worker (-Kind websearch) instead of searching yourself; only a single quick search you need this minute is yours. Start it as soon as the question comes up and keep working while it runs. Give it everything it needs in the brief, nothing private, and check the claims you act on.

At every level Claude keeps: talking to the user, design and architecture decisions, integration into the main checkout, security-sensitive changes, and review of every worker result it acts on (spot-check the claims that matter; read the full diff of any edit). Whenever workers are running, Claude works in parallel on something that cannot collide with them (the next brief, integration plumbing, the previous review, its own tests and builds), and starts its own long builds and conversions in the background too; a worker run is not a wait.

Missing tools: before starting a job, check that the tools it needs are installed. If a compiler, runtime or package the best approach needs is missing, stop and ask the user to install it (say what it is for and give the exact install command) instead of working around it with a weaker approach or a hand-written substitute. Continue without it only if the user chooses to.

This block is generated. Change the level with `python "C:/Users/Brannock/.claude/skills/deepseek-agents/tools/ds_delegation.py" --set <1-5> --agents-md` from this folder, or ask Claude to. The full scale is in the deepseek-agents skill.
<!-- ds-delegation:end -->


Claude (Claude Code) leads the work on this checkout and delegates bounded tasks to DeepSeek worker agents. The setup is carried over from the user's DOA Xbox360 UI project. This file is how each session, and each assistant, sees what the others did.

## At the start of every session

1. Run `git log --oneline -15` and `git status`.
2. Read the **Handoff log** below, newest entry first.
3. If `git status` shows uncommitted changes you did not make, stop and ask the user before touching them. Another session or a worker may be mid-task. `tools/ds_status.ps1` shows whether a worker is running.

## Two tracks: Phase 2 first, portal second

Every task belongs to one track; name it in the brief, the commit and the handoff entry.

- **Phase 2** (the priority): contributions to upstream Phase 2 as its README, CONTRIBUTING.md and
  `docs/roadmap/02-completion-plan.md` describe. Worktree `C:\Modding\wkj-phase2`, branches
  `phase2/<topic>` cut from `main` (a mirror of `origin/main`), assets
  `C:\Modding\SkyrimConverted-upstream`, upstream's commit style, worker tasks `<kind>-<nnn>-p2-<slug>` numbered 500-599.
- **Portal**: the user's seamless-door feature and demos. This checkout, branch `portal`, assets
  `C:\Modding\SkyrimConverted`, `[claude]`/`[deepseek]` prefixes, worker tasks `<kind>-<nnn>-portal-<slug>` numbered 100-199 (then 600+).
- Changes flow one way: `main` and `phase2/*` merge into `portal`, never the reverse. A Phase 2 bug
  found on the portal is fixed on a `phase2/*` branch first.

Which work belongs to which track, the PR rules and the reasons: `docs/pr/two-tracks.md`. Read it
before starting any `phase2/*` branch or moving work between tracks.

## While working

- Do not rewrite history (no amend, rebase or force-push on commits already made).
- **Division of labour (delegation level 4 of 5, set back to 4 by the user on 2026-09-24 after a morning at 3; spend is now capped by a $20 weekly DeepSeek limit, and the generated delegation block governs; see the block above):** Claude plans, writes briefs, reviews and integrates; DeepSeek workers do the research, audits, analysis of completed profiling and acceptance runs, assigned documentation, and most implementation - including multi-file changes within a crate - each in its own worktree with an owned-file list (`tools/ds_impl.ps1`). Claude keeps integration, cross-crate architecture, the `crates/shared/` contracts, dependency changes, and the acceptance/sign-off runs. Every worker patch gets an independent review and harness-run acceptance before it is merged; a tuning or calibration task (thresholds, geometry, heuristics fitted to samples) must also pass on examples the worker never saw. Minimize Claude's investigation, review and repair load; DeepSeek cost is negligible to the user, so do not cut useful reasoning or review to save it. See `tasks/deepseek/delegation-policy.md`.
- Commit each meaningful step. Prefix the commit subject with who wrote the change: `[claude]`, `[deepseek]`, or `[chatgpt]` if ChatGPT/Codex joins. DeepSeek workers launched from `tasks/deepseek/` commit only their own paths and leave handoff notes in `docs/handoff/`.
- Task queue for DeepSeek workers and how to run it: `tasks/deepseek/README.md`. Review `[deepseek]` commits and their handoff notes before building on them.
- Before committing Rust changes, run what CI runs: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and the tests of the crates you touched.
- Keep `docs/` current. Findings go in the docs (`docs/research/`, `docs/roadmap/`), not only in chat.
- **Non-measured engine builds use `cargo build --profile quick --bin engine`** (`target/quick/`):
  release without link-time optimisation, incremental. Defined in the untracked
  `C:\Modding\.cargo\config.toml`, so it never reaches upstream. Measured 2026-09-24: a one-file
  edit rebuilds in 39 s against 4 min 20 s for `--release`. Acceptance, profiling and benchmarks
  keep `--release`. `--bin engine` also skips `world-inspect`'s own 2.5 min link.
  C:\Modding and `%USERPROFILE%\.cargo` are excluded from Defender scanning.
- `local/` (worker state, checkouts, advisor notes) and `target/` are Git-ignored. Never commit them, and never commit Bethesda game data.
- **Engine instances run in parallel.** Converted assets are read-only, so any number of engines
  (both sessions, workers, the user playing) can run tours, `--shots` and tests side by side, just
  slower. Two exceptions, each announced to the other session first:
  - **Timing runs run alone:** profiling, acceptance and benchmarks (`scripts/phase2-*.ps1`,
    `--benchmark-only`). A second instance on the GPU spoils their numbers.
  - **A conversion's publish needs its output and staging folders closed:** publishing renames
    staging over the output folder, and on Windows that fails ("Access is denied") while any
    process holds a handle inside either: an engine reading `skyrim_world.db`, but also a size
    check (`du`), an indexer, an Explorer window or antivirus. Nothing reads either folder while a
    conversion reaches Publishing; measure staging only while the converter is stopped. Recover
    with `converter.exe ... --resume-staging <staging dir>`, which re-walks every stage.
  - **Disk for a conversion:** staging grows to about the size of the finished output (67 GB for
    a full Skyrim SE conversion, 2026-09-23), so a reconversion needs about that much free beside
    the existing output folder.
  - **A conversion needs memory:** while one is extracting or converting (~20 min), neither session
    starts a new Rust build or link (`cargo test`/`clippy`/`build`, a fresh worker worktree);
    what is already running may finish. Windows fails allocations at the commit limit, and a
    Bevy link plus a parallel archive extraction crossed it on 2026-09-23 ("memory allocation of
    1048576 bytes failed" at 26% extraction). The converting session announces start and end.

## At the end of every session

Commit your work, then add a dated entry at the top of the handoff log: what changed, what was tested, whether a worker is still running, and the next step.

## Handoff log

### 2026-09-24 (night) - Claude [portal] (the look gaps, worked with Phase 2 Dev; morning gallery)

**The user's asks:** a ranked list of how the demo's look differs from Skyrim
(`docs/research/look-gaps-2026-09-24.md`, 61 graded reference shots), then an overnight plan with
Phase 2 Dev working as one team, progress pictures, and pretty screenshots for the morning. Shared
log of the night: `local/overnight-2026-09-24.md`.

**Fixed on portal (each measured on the reference shots, tuning fitted on half and checked on the
other half):** dark city worldspaces take their parent's lighting (`7585b7d`); the sky fill is a
tenth of the sun, not half (`880a3f4`); shadows take 18 % of the sky's hue, no longer green
(`fe7d24e`); multi-pass snow materials are not drawn as a black layer - the Alftand ice blotches
(`aac5989`); effect shaders draw their greyscale texture through their palette (`b54dced`); glows
are capped under the tonemapper's white - Blackreach (`f4c40d7`); the city LAND edges past the weld
bound are drawn as authored (`80bf6de`, mirroring Phase 2's `4fe8f6e`). From Phase 2, cherry-picked
or merged: effect-shader emissive (`b8f152a`), default water height (Morthal's lake), refraction
planes excluded (the fire heat haze), the water look (white sheet gone), staging hard links for
reconversions, scripts reading the schema from source. Audit findings CC-01/02, RM-02, HT-01/02/04/05
closed.

**Data:** `C:\Modding\SkyrimConverted` is converter **schema 21** (complete, integration passed;
space_lighting also carries fog far colour, fog max and light fade). `engine-portal-fit.exe` is
schema 21. Tours on it: Riverwood 8/8, Alftand 4/4, 0 FAIL. Fallback copy `SkyrimConverted-s19`
(hard links; its vfs and ingestion cache removed after they filled C: at 20:40 - see memory
`reconversion-disk`). `target/debug` and `target/release` live on `D:\Modding-overflow` behind
junctions (the user allowed D:).

**Built, not switched on:** Skyrim's `1 - (d/r)^2` point-light falloff (`light_falloff.rs`, a
load-time patch of Bevy's attenuation line, QnA's research); `lights::SKYRIM_FALLOFF` is false
because alone it did not improve interiors, whose fog fills them. Next for interiors:
`local/reference/look-gaps/interior-lighting-design.md`.

**Open:** the fire cards still read as pale sheets (atlas animation, palette orientation); sky
(Phase 2); colour grading (QnA's `local/research/imagespace-colour-grading.md`: grade after the
tonemap, per weather and cell); distant land (Phase 2's LOD branches, not merged); a portal is not
drawn past ~600 units; a shots-run door once rendered closed (flaky).

**Morning gallery:** https://claude.ai/artifact/1QMmmU5jKwDhdnhKUynJpZ (private; JPEGs in
`local/gallery/final/`). **No workers running from this session.** Phase 2 Dev still has coders
and an audit-converter merge in flight; take their pre-resolved merge, not the branch.

### 2026-09-24 (overnight) - Claude [phase2] (conversion, audit, fixes, reference sheet, distant LOD)

**Upstream:** realfakenerd replied on #24 (2026-09-23 21:02): be patient, the roadmap is being
reworked, management changes and a Discord server are coming. Sends stay on hold. PR #23 got the
review fixes and a reply (sent with the user's OK); #25 has a local follow-up commit (roadmap doc,
comment, test) that is NOT pushed.

**Conversion:** `C:/Modding/SkyrimConverted-upstream` (schema 14, fresh, 20:20-07:14):
complete, 242,969 converted, 0 skipped, 511 textures pruned, integration report passed. Converter
built from `phase2/pruned-texture-complete`, so this is PR 3's real-install evidence.

**Audit and fixes:** lead-515 (12 reviewers) audited every branch
(`docs/pr/phase2-branch-audit.md`); impl-520..530 fixed every branch (I read each diff, fixed
several test/clippy slips myself). Integration branch `phase2/integration` passes all tests.

**Distant LOD:** step 1 `phase2/lod-mesh-containers`, step 2 `phase2/lod-database` (world DB
schema 4; review-535 found ~2,000 offset-grid blocks dropped, fixed), step 3
`phase2/lod-engine-terrain` (engine, `--lod`, origin-relative lookup; never run on a GPU and needs
a schema-4 database). Tree LOD decoded (`docs/research/tree-lod-layout.md`).

**Reference sheet:** `docs/research/upstream-reference-sheet-2026-09-24.md`, rendered with
`phase2/render-check` (engine-side fixes + local `--shots`). Biggest gaps: no trees/plants/Guardian
Stones and see-through Alftand ice (both fixed by the schema-15 branches, which need a schema-15
conversion: ~70 GB staging, disk is short), no sky/fog, dim light, no Blackreach glow.

**Other findings:** Dragonborn "missing" refs are a tree-LOD FormID-slot mismatch, not a bug;
the 278 "lost" records are deletions by later plugins (not a bug); `plugins.priority` is not the
FormID slot for plugins after the first light plugin (small upstream candidate).

**No workers running.** **Next:** decide with the user on disk for a schema-15 conversion (to see
trees and materials), whether to push the #25 follow-up, and what to do about the Discord.

### 2026-09-23 (night) - Claude [portal] (doorway alignment landed; PortalPlugin steps 1-5)

**Doorway alignment (the user's Sven's House report) is merged and confirmed by the user** on "a
bunch of random transitions" (`2b2687b`). impl-103 anchors the map on the two doorways; the lead's
review corrections are in (gate to the placed doorway centre, height cap = step height, destination
door hidden only outside the active space). Found only on the real assets, both fixed: an exterior
doorway's grid came from its *cell* (Riverwood's doors are in the persistent cell, grid 0,0, so
the wrong part of Skyrim streamed), and a crossing landed inside the closed far door (impl-152
opens it at the swap). Non-obvious transitions (ladders, trapdoors, cave mouths) keep the `XTEL`
landing and are **parked by the user** until they get their own design.

**PortalPlugin refactor steps 1-5 merged** (impl-153, `cdefe05`): one `add_plugins(PortalPlugin)`
in `run()`, `PortalOptions` in `config.rs`, `main.rs` byte-identical to `main`. The worker could not
run cargo (harness denied it); the lead fixed three tests and fmt. Both tours pass on the final
tree: Riverwood 8/8, Alftand 4/4, 0 panics. **Step 6** (atmosphere and terrain ring out of `app.rs`)
is next; it needs builds, so it waits for the end of Phase 2 Dev's conversion.

**Also:** `docs/roadmap/backlog-engine-and-portal.md` (every idea and note, split main engine /
portal; main-engine half sent to Phase 2 Dev); research-151 incremental conversion (handed to
Phase 2); reference sheets `local/reference/cmp-103/` and doorway shots
`local/reference/doorway-152/`. `target/release/engine.exe` in this checkout predates the refactor
merge: rebuild before the next play session.

**No workers running.** Phase 2 Dev's upstream conversion runs overnight: no builds, links or new
worktrees from this checkout until they announce the end. **Next:** step 6; then the backlog
(water white sheet first, once Phase 2 Dev says whether it is fork-only).

### 2026-09-23 (evening) - Claude [phase2] (Phase 2 track running; upstream sends on hold)

**Upstream:** issue #24 (lighting scope) and PR #25 (REFR rotation) opened; #23 still open. The user
put a **hold** on sending anything more until maintainers respond (memory + `docs/pr/two-tracks.md`).

**Ready and held** (all from `main`, CI-equivalent checks run where noted), in send order:
`phase2/pruned-texture-complete` (checks pass, body `pr-03`), `phase2/empty-model-bounds` (checks
pass, body `pr-04`), `phase2/shared-converter-schema` (checks pass), `phase2/material-publication`
(schema 15; after empty-model), `phase2/vertex-alpha-not-opacity` (after material),
`phase2/terrain-samplers-weights` and `phase2/terrain-seam-weld` (need a GPU run),
`phase2/sun-shadow-cascades` (needs a GPU run and a benchmark). Workers impl-083, 501, 502,
504-508 (DeepSeek) wrote them; Claude reviewed every diff. Audit: `docs/pr/phase2-fix-audit.md`.

**Coordination with Portal Dev:** rules in "Two tracks" and "While working" (parallel engines;
timing runs alone; no reader in a folder being published; no new builds during a conversion).
Portal reconverted `SkyrimConverted` to schema 19 (complete, 0 skipped).

**No workers running.** **Next:** the upstream-schema conversion into
`C:\Modding\SkyrimConverted-upstream` (about 70 GB; 117 GB free now) so the GPU-dependent branches
can be checked, announced to Portal Dev first; then GPU runs of the terrain, shadow and material
branches. Check #23/#24/#25 for feedback before proposing any send.

### 2026-09-23 (afternoon) - Claude [phase2] [portal] (the work split into two tracks)

**The user's ask:** keep Phase 2 contributions and the portal feature clearly separate, with
Phase 2 first and done as upstream's README describes. The user agreed to all of the plan's
decisions, including lighting going upstream (issue first) and offering the portal work later as a
Phase 4 proposal. The plan is `docs/pr/two-tracks.md`; the rules are the "Two tracks" section above.

**Done:** `deepseek-setup` renamed to `portal`; `main` fast-forwarded to `origin/main` (`2105bfd`)
and tracking it; worktree `C:\Modding\wkj-phase2` on `main`.

**Blocked on the user:** disk. `SkyrimConverted` is 73.8 GB, C: has 43 GB free, so the
upstream-schema conversion (`SkyrimConverted-upstream`) does not fit yet.

**Seen in passing, not mine:** `local/conversion-s18.out.log` ends `complete: false` with 511 skipped
inputs; `s18b` is later and small. The untracked `tasks/deepseek/*` briefs and `scratch-069/` belong
to another session. The eight "modified" engine files are line-ending noise (`git diff` is empty).

**Next:** Phase 2 - rebase PR 2 (rotation) onto `main` as `phase2/refr-rotation`; open the lighting
issue after the user confirms the draft in `two-tracks.md`. Portal - the `PortalPlugin` refactor
(`portal-` worker task).

### 2026-09-23 (day) - Claude (the Riverwood demo, and two rendering bugs it exposed)

**The user's ask:** "start on the riverwood demo", off research-055's data
(`docs/design/riverwood-demo.md`). Section 8 of that document is the part written after the demo
was built and run, and is the part to trust about behaviour.

**The demo works.** `--demo riverwood --walk` starts on the Helgen road south-west of the village;
`play-riverwood-demo.cmd` is beside the Blackreach launchers. `--demo-tour` is no longer
Alftand-only (impl-056): a route is keyed off the `--demo` name, and Riverwood's is eight doors -
four houses, each entered and left again. The passing run is `local/demo/riverwood2/tour.txt`:
**8 crossings of 8, all by walking, 0 dark frames in 2,056 captured frames.** The seamless
crossing now holds in daylight and in both directions, which the Alftand route could not show.
One tour bug fixed on the way: the walk-through's standoffs were 60 and 320 units, and inside
Alvor's House the far wall is nearer than 320; a third, 160, is tried last.

**Two rendering bugs, both dataset-wide, both found by looking at Riverwood:**

1. **The forest was invisible** (research-059, impl-063, `abaa14f`). Pines rendered as a spray of
   dots. My hypothesis - alpha-tested foliage falling through `render.rs`'s `AlphaMode::Blend`
   fallback - was **wrong**, and the worker falsified it properly: the trees convert to `MASK`
   with the NIF's own threshold. Bevy tests `vertex_color.a * texture.a` against the cutoff, and
   Skyrim's tree NIFs carry a `COLOR_0` whose alpha is a shader parameter, not opacity. Fixed
   engine-side at mesh load, no reconversion. Proof: `local/reference/_tree-before-after.png` -
   the canopies fill in and the house, thatch and wall in the same frame do not move. This also
   un-breaks `snow.wgsl`, which multiplies coverage by `in.color.a` under a comment claiming no
   converted mesh has vertex colours; 1,337 of them do.
2. **Everything read wet** (research-058, websearch-060, impl-062 `59f1f20`, impl-064 running).
   `material.rs` mapped Skyrim's Blinn-Phong glossiness *exponent* as a linear percentage. All
   59,268 published materials were wrong; 14,825 were clamped to Bevy's roughness floor. Now
   `(2/(n+2))^0.25`. The other half - the specular mask in the normal map's alpha, which the
   converter drops - is impl-064; websearch-060 settled that the mask scales intensity, not the
   exponent, so it can be a `specularTexture` pointing at the normal map with no derived textures.

**Converter schema 17 -> 18.** impl-064 rides the same bump, so one reconversion covers both. The
first reconversion was running when this was written (`local/conversion-s18.out.log`); a second is
needed after impl-064 lands. **Until a reconversion completes, the engine refuses the assets on
disk** - that is its schema check working, not a fault.

**Also landed:** impl-057, `crates/engine/src/model_animation.rs` - a model's own looping `Idle`
clip on every non-door reference. The mill wheel the Riverwood finale looks at now turns (16.67 s
a revolution, confirmed in the log). The snow catalogue read `material_object IS NOT NULL`, but
the exporter writes a plain 0 for a static with no `MATO`: 9,924 markers, fences and road chunks
against 2,506 real snow statics. Now `> 0`, with a test that fails on the old query (`ad7f138`).

**Reference shots.** `tools/reference/riverwood_shots.json`, twelve poses, ten against a UESP
reference in `local/reference/riverwood/`. All twelve render. research-055 computed the eight
exteriors at ground + 120 (eye height), but UESP's "place" screenshots are free-camera shots from
above the rooftops, so four of them could not frame their reference at any yaw. A sweep of
elevated candidates is `local/reference/rw_sweep4/`; the mill finale has been re-fitted from it.
The inn and the whole-village aerial still need one more pass - `local/reference/rw_sweep2.json`
is written and unrendered.

**Running when this was written:** impl-064 (the specular mask), doc-065 (the demo guide covering
both demos). **Next:** finish the reconversion, re-render and check the wet look is gone, land
impl-064, reconvert again, re-fit the inn and village poses, then a regression run of the Alftand
tour.


### 2026-09-23 (overnight) - Claude (visual gaps, animated doors and seamless crossing, demo for contributors, upstream PR)

**The user's five goals, and where each stands.**

1. **Visual gaps.** lead-026 (4 research + 3 websearch workers) produced `docs/research/visual-gaps-spec.md`. Landed:
   - `23adbb4` glow: HDR + bloom, `EMISSIVE_EXPOSURE = 100` applied only to materials the converter marked with an emissive strength (mushrooms, the Blackreach sun, flames), so Own_Emit surfaces (ice, snowy trees) keep Tamriel's daylight within 0.8%; `intensity_for_radius` capped at a 256-unit reference, which is bit-identical below radius 512 and removes Blackreach's orange ceiling. Claude's holdout: AlftandWorld 1.14x, Blackreach 1.26x, clip 0.66%.
   - `2b69616` empty marker models (Blackreach's two) no longer fail the bounds gate.
   - `26324d5` converter `space_lighting` + `matos` + `statics` DNAM columns, layouts settled from the real bytes (LGTM payload is DATA; inherit bit set = from template; WTHR FNAM interleaves day/night; MATO's last field is an integer flag).
   - `66ee786` engine per-space lighting: backdrop, fog, ambient hue and sun tint per space, destination atmosphere on the portal camera. Record colours are applied as **hue** with the calibrated brightness kept.
   - `cfe4893` directional snow (DNAM/MATO projected material), inert until the data exists.
   - Ice (environment maps) is still open, and so is the calibration of the last two, which needs the reconversion.
2. **Animated doors and seamless crossing.** `f6749e2` converter exports Open/Close clips (all 768 door NIFs decode; 171 animated). `50bc0f1` the door state machine (clips found by name; a leaf whose swing is under 45 degrees hides when open). `49f2df5` the seamless crossing: the player's pose is carried through the door instead of snapping. `b535a30` the two wired together, plus the review corrections. Claude's check on the integrated build: **walk tour 4/4 by walking, 0 errors, 0 dark frames in 811 captured frames**; the frames either side of a swap are the same view (`local/demo/walk-swap-fixed.jpg`). The route's Dwemer doors only twitch 5-9 degrees in the real data; their non-load twins swing 86-88. **Open question for the user:** keep the faithful twitch or borrow the twin's swing?
3. **Demo for contributors.** `cc53baf`: `docs/demo/README.md`, `tools/demo/setup-demo.ps1` (prereqs, build, Skyrim detection, conversion with resume, launcher) and `tools/demo/play-demo.ps1`. Claude ran both dry runs. Nothing published.
4. **The project as a whole.** Upstream `origin/main` (16 commits: `dummy-content`, ADRs, CI) merged locally (`8c21447`, one test-only conflict). `40e0eb5` extends `dummy-content` with an interior cell and a linked load-door pair, so upstream can test doors and portals without Skyrim. research-028: the streaming fixture never exits without a benchmark window - not a ring bug.
5. **Audit and PR design.** `docs/pr/upstream-pr-plan.md` (research-029): 9 PRs, fixes first, with exclusion and cleanup lists. Cleanups done: `410ce31` converter (ADR-0002 `OPENSKYRIM_SKYRIM_DATA` / `OPENSKYRIM_CONVERTED_DIR`, no machine paths, no private names), `62c0f7b` + `944250e` engine parts 1-2, part 3 running. Not done: re-cutting the commits into the PR series (the plan's step), which needs the user.

**Blocked on the user:** the reconversion (see the entry below). Until it runs, door clips, per-space lighting and snow have no data, and each is inert by design.

**Process notes.** Every worker patch got an independent DeepSeek review (review-035/040/041/042/045/049) or a full read by Claude; four rounds of corrections were sent back. Discard each ds_impl worktree right after integrating: they seed ~11 GB of `target/` each.

### 2026-09-22 (night) - Claude (rotation fix, distant terrain, closed doors)

- **User asks:** the Ruined Tower was rotated wrongly in the side-by-side comparison, so check rotation and placement. Also increase the render distance so distant terrain shows.
- **Rotation (`933b95f`, Claude, shared contract):** Creation `REFR` angles turn clockwise. Objects are now rotated by `Rx(-x) Ry(-y) Rz(-z)`, the inverse of the old conversion.
  - Yaw was proven from the door links: 88% of doors were consistent against 42% under the old convention.
  - Tilt and order were confirmed by research-023 (`22ad24c`): on 82,731 tilted references the median error against the terrain normal is 8.2°, and every alternative is 19-32°. Scale and position are applied correctly.
  - Objects and the arrival camera now share one convention.
- **Reference poses:** re-fitted by research-024 (`6804978`, `9d6d844`): 1 matched, 15 close, 10 area only. The Alftand exteriors are shot from the north side of the ravine. The Ruined Tower door faces east.
- **Closed doors (`f9c439e`, impl-022):** a load door draws closed unless the portal is rendering through it.
- **Distant terrain (`5211bad`, impl-021 plus a review round):** a terrain-only ring out to `--terrain-radius` (default 8), with distance fog and `NotShadowCaster` on the ring.
  - Frame rate on the Pale view: 180 FPS without the ring, about 97 FPS at radius 8 (shared GPU, rough).
  - Claude's change: benchmark runs keep the ring off unless `--terrain-radius` is given. A default benchmark passed with 0 commit-budget violations.
  - DeepSeek review-025 verdict: accept after fixes.
  - Unexplained: Claude's own `--streaming-fixture --terrain-radius 8` run did not exit within 400 s. The worker's fixture run validated, but the ring terrain was not exercised there. Check this before the Phase 2 runs.
- **Harness:** `tools/ds_impl.ps1` synced to skill v1.3 (`54645f9`).
- **Queued, not started:** impl-023 (`tasks/deepseek/impl-023-door-front-from-xtel.md`). The portal takes a door's front from the model axis, which is wrong for many door models (the tower door opens only from its back). The fix takes the front from the return link's `XTEL`.
- **Open rendering gaps:** glow/emissive, per-cell lighting templates and interior fog, the snow shader (roofs render bronze), overcast sky and weather, ice reading as rock, `BlackPlane01` voids, two Blackreach models that fail validation.
- **No workers running.**

### 2026-09-22 (late) - Claude (reference screenshots and matched renders)

- **User ask:** collect Skyrim screenshots of the demo's locations, then render the same compositions in OpenSkyrim for comparison.
- **References:** 26 UESP screenshots (CC BY-SA; download approved by the user) in `local/reference/uesp/` (git-ignored, `SOURCES.txt`).
- **Built:** design `docs/design/reference-shots.md` (the shots-file contract). impl-017 `--shots <file> [--shots-out <dir>]` renders exact poses once streaming settles (`5e53712`, DeepSeek, verified at runtime by Claude). research-018 wrote the first poses, `tools/compare_shots.py` and the mapping/fitting scripts (`adbea02`, `4050696`).
- **First renders:** `local/reference/cmp1/`. The Ruined Tower matches; most other poses are rough (walls, and some pitch signs flipped).
- **Found:** the Alftand01 arrival is washed out to white at eye height. The suspect is the stopgap camera lantern (3e9) in `update_atmosphere`. It showed once crossings landed at eye height (`4b1dd7a`).
- **Done:** research-020 refined all 26 poses by rendering: 1 matched, 12 close, 13 area only (`036efbe`; compare-sheet fix `64129bb`). impl-019 calibrated the lighting (`efa5351`):
  - the lantern is removed;
  - Bevy's 4π on point lights was missing from `lights.rs`;
  - the ambient levels and colours were re-measured.
  Claude's holdout check on the refined poses: Alftand02 is 1.8x too dark (it was 8x), AlftandWorld 1.2x, Blackreach 1.5x too bright; Tamriel is unchanged. `.deepseek-agents.json` now has `implAllowTools` with `cargo build` and the release engine.
- **Renders:** refined poses with the new lighting are in `local/reference/cmp-lit/` (with `_sheet.jpg`). Measure with `python tools/research/exposure_stats.py --shots <file> --render <dir>`.
- **Rendering gaps the comparison found** (in `docs/research/reference-shot-poses.md`):
  - no emissive/glow (Blackreach);
  - no per-cell lighting templates (`XCLL`/`LTMP`) or fog, so the lighting is flat;
  - ice reads as rock;
  - no snow shader or weather;
  - black voids (`BlackPlane01`?);
  - two Blackreach models fail validation (`WispAmbush`, `FrostSpiderAmbush01`);
  - one 4334-radius `FalmerCityLight02NS` floods the Blackreach ceiling orange.
- **Next:** the user picks the order. Glow first is recommended.

### 2026-09-22 (day) - Claude (terrain textures, see-through meshes, open doors with portals)

- **User asks:** work on terrain and terrain textures; doors not rendered (or open) so you can see into the next interior; fix objects that render transparent or misplaced.
- **Merged (DeepSeek, reviewed and integrated by Claude):** impl-009 portals + open load doors + pre-streamed cells kept out of the active space (`f219a05`); impl-010 terrain - root cause of the stretched textures was the ClampToEdge sampler on tiled layers, now Repeat; bilinear layer weights in the material uniform (`0ece570`); impl-012 converter - vertex alpha no longer means blend (2,632 materials were see-through), blend factors kept, EditorMarker shapes dropped (`b4ce0d9`). research-011 (`bc54a17`) diagnosed the transparency/misplacement classes.
- **Claude fixes found only at runtime:** portal startup ordering panic; WGSL u32->f32 in terrain.wgsl; portal quad was a floor-oriented `Plane3d`; every crossing arrived facing the return door (`arrival_camera_rotation`, `a49b4df`). Verified with `--demo-tour`: 4/4 crossings, walk test, and a doorway screenshot showing AlftandWorld through the Alftand02 door (`local/demo/portal5/02-door.png`); the tour now saves the portal camera's own image per door.
- **Harness:** `claude.exe` lives under the MSIX package folder, so launches need the updated skill (fixed by the DeepSeek Workers session); `tools/ds_impl.ps1` skips acceptance when no worker ran and seeds each task's `target/` from the lead's debug build (ported from the harness). Pending: sync the other tools with the harness at a clean point.
- **Later the same day:** schema-15 assets published (see-through ice/rock and editor markers gone, verified on screen); impl-013 glows/terrain bases/labels (`8d4160b`); tools synced with the harness (`c17fc48`); impl-014 converter `lights` table + `references.radius_override`, DB schema v5 (`5cd1bf4`); impl-015 engine point lights with a nearest-64 budget (`2acd409`); impl-016 auto-load doors cross on contact (`83ec4cd`). All DeepSeek, all accepted without correction.
- **Claude runtime fixes:** `ds_impl -Integrate` now stamps copied files (a 1 s "build" had reused a stale binary); lights never rendered - model-less LIGH references lacked `Visibility` (B0004) - and were ~100x too dim for the camera exposure (`EXPOSURE_CALIBRATION = 50` in `lights.rs`, the brightness knob); `update_atmosphere` never applied when starting inside Blackreach (`188e184`).
- **Data state:** `C:\Modding\SkyrimConverted` is converter schema 15 / DB schema 5 (504 lights, 15,167 lit references, 315 auto-load doors). A publish can fail with "Access is denied" if anything holds `skyrim_world.db` open (a worker script did); `converter.exe ... --resume-staging <staging dir>` republishes.
- **Verified:** `--demo-tour` 4/4 crossings + walk test on the final build; screenshots in `local/demo/lights3/`. The user is running agentic tests on this PC, so no FPS/timing numbers from today are valid.
- **Evening fix (Claude):** the black Alftand01 -> Alftand02 (and Tamriel -> Alftand01) portals were a heading-sign bug, not rendering: a Creation `XTEL` heading turns *clockwise* (forward = `(sin z, cos z)`; every route link puts the arrival in front of its destination door along that), while `arrival_camera_rotation` reused the counter-clockwise object convention plus a half turn, which only happened to fit AlftandWorld. Now `Quat::from_rotation_y(-z)`, with a regression test built from the four real links. Crossings also land the camera at eye height (XTEL is the feet point; fly mode and the tour had sat on the floor). Tour 4/4, screenshots `local/demo/portalfix2/`. Open question: objects still use the counter-clockwise `Rz` convention and look right on screen; not re-verified against the clockwise heading finding.
- **Known gaps:** lights overexpose right next to a source (inverse-square; tune `EXPOSURE_CALIBRATION` or add a tonemapping/exposure pass); GPU clustering ignores render layers, so a portal destination's lights can faintly reach the active space (impl-015's note).
- **Next:** the user's look at the lit demo; then the Phase 2 shakedown and visual review (no GPU-heavy runs while the user's agentic tests run).

### 2026-09-22 - Claude (overnight: walkable Alftand -> Blackreach demo; Phase 2 sign-off paused)

- **The user's goal for the night:** "get this cool Blackreach demo up and running, extra points if it's interactable and I can walk through the level". Done: `play-blackreach-demo.cmd` (or `target\release\engine.exe --assets C:\Modding\SkyrimConverted --demo alftand --walk`) starts on foot at the Alftand exit; four load doors (E) lead down to Blackreach with no loading screen. `--demo blackreach` (or `play-blackreach-shortcut.cmd`) starts inside Blackreach.
- **Verified:** `--demo-tour <dir>` walks the route by itself, screenshots every place and door, then holds W in Blackreach. Final run on `3de1505`: 4/4 crossings, walk test 600 units grounded, 0 errors, 170-180 FPS. Screenshots: `local/demo/final/`.
- **Built by DeepSeek workers, reviewed and integrated by Claude:** impl-005 (converter: `XTEL` -> `door_links`, DOOR/ACTI/FLOR/CONT/TREE/LIGH models, schema v4), impl-006 (engine: `ActiveCell`, interiors, rebase guard, `TransitionPlugin` pre-streams and crosses), impl-007 (`PlayerPlugin`: mouse look, walking via Bevy mesh ray casting, E to open, F to fly), impl-008 (weld vanilla one-point terrain seams instead of dropping cells; no water plane on dry cells). All four accepted without correction. Claude: design + `doors.rs` contract, CLI (`--walk`, `--demo`, `--start-position/--start-yaw`, `--demo-tour`), app wiring, demo tour, atmosphere (sky / underground ambient / lantern), objective text, launchers.
- **Data:** `C:\Modding\SkyrimConverted` reconverted at schema v4 (complete, integration passed, 2204 door links all resolved; the four route doors match research t02 exactly).
- **Harness fix:** `tools/ds_impl.ps1` now gives every checkout its own `target/`; a shared one let parallel engine workers run each other's test binaries (found by impl-007).
- **Known gaps:** the terrain layer blending is still blocky (research-004's Defect A, fix recipe in `docs/research/terrain-blending-seams.md`, not yet implemented); `LIGH` references are not real lights (interiors use ambient + a camera lantern, Blackreach reads dark blue); load doors are opened with E only (Skyrim's auto-load doors fire on contact); no skinned meshes, NPCs or physics (Phase 4).
- **Phase 2 sign-off (paused for the demo):** shakedown on real assets passed every gate except the visual review; the terrain fixes above are what the visual review needs. Test locations chosen from data: rural (0,0), dense (4,-21), water (21,-3). Next: implement the blending fix, rerun the shakedown, user reviews screenshots, then two full campaigns.

### 2026-09-21 - Claude (clone, toolchain, DeepSeek harness)

- **Cloned** `realfakenerd/OpenSkyrim` at `8919096` into `C:\Modding\OpenSkyrimProject`.
- **Toolchain installed** on this machine: Visual Studio 2022 Build Tools (C++ workload) and Rust stable 1.98.1 (MSVC) via rustup. CMake and Ninja were already present.
- **DeepSeek harness** copied from the DOA Xbox360 UI project and adapted: `.deepseek-agents.json`, `tasks/deepseek/` (context, cookbook, delegation policy, reviewer, run analyst, advisors, self-test, implementation template, result schema), `tools/ds_impl.ps1` (isolated worktree per implementation task, sharing `target/`), `tools/ds_status.ps1`, `tools/run_ds_queue.ps1`, `tools/ds_report.py`, `tools/check_worker_result.py` (now recognises `cargo` commands), `tools/check_citations.py`, and their tests (`python -m unittest discover -s tests`).
- **Verified:** `cargo check --workspace` passes (5 min cold, only a future-incompat warning from the `proc-macro-error2` dependency). The harness self-test passed 6/6 (`local/agents/runs.csv`).
- **Skyrim SE** is installed at `C:\Games\Steam\steamapps\common\Skyrim Special Edition`. Workers can read it (in `readOnlyDirs` and described in `tasks/deepseek/context.md`) but never write to it.
- **Private project:** the user wants none of this public. Commit locally only, never push, never open PRs against the upstream repository.
- **Next:** run the harness self-test (`tasks/deepseek/selftest.md`), then choose the first tasks.
