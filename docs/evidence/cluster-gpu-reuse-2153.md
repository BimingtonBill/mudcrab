# #2153 quiet GPU attribution

All values are milliseconds per complete render frame in a counter-certified 30-second post-ready tail. One release executable, source pin, main28 dataset/package, pose, RTX 3090/Vulkan driver 616.92, 1600x900, AutoNoVsync and vanilla-high settings. Each run passed the quiet guard with no intruders and complete inventory of 112,278 entities and 35,713 meshes. First-after-build warm-up was discarded.

| Pass | Both forced | Cull reuse, shadows forced | Combined exact reuse |
|---|---:|---:|---:|
| main_opaque_pass_3d | 2.440562 | 2.447436 | 2.425710 |
| early prepass | 2.062960 | 2.068149 | 2.072686 |
| shadow_directional_light_0_cascade_3_early | 0.615805 | 0.603368 | 0.605111 |
| shadow_directional_light_0_cascade_2_early | 0.349001 | 0.347626 | 0.000000 |
| cluster_cull | 0.308587 | 0.000000 | 0.000000 |
| cluster_instance_cull | 0.305231 | 0.000000 | 0.000000 |
| main_transparent_pass_3d | 0.295529 | 0.288662 | 0.278019 |
| ssao | 0.206803 | 0.207900 | 0.219982 |
| shadow_directional_light_0_cascade_1_early | 0.141838 | 0.141725 | 0.000000 |
| bin_unpacking | 0.106565 | 0.100256 | 0.105315 |
| bloom | 0.094199 | 0.093749 | 0.110658 |
| clustering | 0.054548 | 0.068762 | 0.069256 |
| cluster_draw_scatter | 0.073526 | 0.000000 | 0.000000 |
| cluster_draw_tile_prefix | 0.066406 | 0.000000 | 0.000000 |
| shadow_directional_light_0_cascade_0_early | 0.055284 | 0.055800 | 0.000000 |
| taa | 0.052548 | 0.053280 | 0.054314 |
| cluster_draw_large_scatter | 0.052752 | 0.000000 | 0.000000 |
| cluster_instance_prefix | 0.034613 | 0.000000 | 0.000000 |
| cluster_instance_scatter | 0.034436 | 0.000000 | 0.000000 |
| early_mesh_preprocessing | 0.023858 | 0.017417 | 0.017603 |
| late_mesh_preprocessing | 0.017131 | 0.016984 | 0.016972 |
| tonemapping | 0.017130 | 0.017116 | 0.017084 |
| cluster_draw_group_prefix | 0.011779 | 0.000000 | 0.000000 |
| upscaling | 0.011433 | 0.011290 | 0.011413 |
| late prepass | 0.008629 | 0.010466 | 0.008640 |
| ui | 0.003042 | 0.003083 | 0.003074 |
| **Recorded sum** | **7.444194** | **6.553069** | **6.015837** |

The incremental differences are **0.891125 ms** with shadows still forced, then **0.537232 ms** with cull reuse held constant. Total observed difference: **1.428357 ms**. This one three-configuration sequence supports inference, not a repeated-run performance acceptance result. Forcing cull recomputation also changes shadow dependencies, so a cluster-forced-only run would not isolate shadow retention.

The combined recorded sum is 1.124163 ms below the 7.14 ms target and 2.314163 ms below 8.33 ms. A pass sum is not GPU critical-path frame time or end-to-end FPS. Legacy verdict performance failures remain retained. No unprofiled 120/140 FPS claim follows.

Zeros require complete covering dispatch/per-view counter intervals. All eight stages have zero dispatches in normal settled mode. Cascades 0–2 have positive reuse with zero raster decisions; cascade 3 renders because WaterMaterial lacks a shadow dependency declaration and has writable bound textures. No declaration, texture exemption, settings reduction or content removal was introduced.

The historical #2152 total 7.671804 ms is a different retained binary/run and is not used to attribute this same-binary difference.

Raw runs: `D:/Mudcrab/builds/cluster-gpu-reuse-target/capture-2153-r4/{before-both-01,before-shadow-01,after-01}`. Raw Tracy traces, exports, guards, counters, metadata, benchmarks and verdicts remain there. The rejected earlier R3 capture has a disturbed dust.exe guard and is excluded.

## Source and GPU verification

The retained R3 source digest is `8b64135706fea19e712789a543ba7f5b9f3f518061dd1b58005447a52735db23`.
The release executable SHA-256 is `732c9d7c2861c71518c5766f5de6288bdf4a5563184be65a2938f7ea6339a16c`.
All 892 Rust, WGSL and Cargo files still match the source snapshot used for the
following checks. Only documentation and vendor-local Git whitespace policy changed afterwards.
The JSON receipt retains log hashes and per-run input identities.

- Workspace strict all-target/all-feature Clippy: passed.
- Workspace all-target nextest: 5,583 passed, 65 skipped.
- Vendored PBR tests: 68 passed.
- Workspace doctests: 18 passed, 15 ignored.
- Workspace fmt, modified PBR style2021 fmt and whitespace checks: passed.
- Capture/controller CPU checks: 123 passed, including all 111 inherited checks.
- Two explicitly executed Vulkan cull fixtures: passed on RTX 3090/616.92.
  Sixteen production/shader/fixture/lock hashes match their retained proof.

The allocation-local gate observed three expected RED failures before reuse,
then four GREEN tests. The vendor producer/empty-closure RED run observed nine
expected failures with 17 other tests passing. The final PBR suite covers the
corrections. Signed-zero joint comparison, fallback-image capability removal,
ordinary missing-draw lookup, typed diagnostic inventory, strict receipt joins
and snapshot-wrapper parsing retain focused RED/GREEN logs in the owned
target. The R4 README records the caller RED result; the parent reran all 123
GREEN checks. CPU-only gate mutation is distinguished
from producer routing and live native rendering in the invalidation ledger.

At complete standing content, native frame 1234 matched **39 exact raw pairs**:
five cull views, all four shadow maps and ten camera attachment pairs, totaling
1,571,992,744 bytes per lane. Complete indirect arguments/counts/visible records,
compacted instance lists and dispatch words were compared. Camera outputs cover
early/late depth, packed normal and motion, plus opaque/transparent depth and
linear HDR. Color is before temporal postprocessing; presented/TAA output is not
independently paired. Strict all-observed attachment coverage passed. The
standing reused cascades 0–2 were clear-only; populated retained-map proof comes
from the moving-run tail below.

## Moving-camera TEST and visible evidence

Accepted runs are `traversal-visible-normal-01` and
`traversal-visible-forced-01` under the same owned R4 capture directory and pin.
Both finished the full native 900-units/s, 45-second, 40,500-unit route with its
40-second tail. Each has five nonblack, frame-bound moving PNGs and settled-tail
PNGs. Representative normal and forced frames were inspected: mountains, city,
terrain and forest progress through the view without an observed stale-view
artifact. Three logged windows entirely during movement dispatch all eight
stages in each run. Sequential real-time routes are not pixel-aligned parity
captures. The official `verdict.py` reports **CONTENT: PASS** for both; streaming
failures, undrawn static geometry and runtime-refused undrawn geometry are zero.
Final changing-route inventories are 113,477 entities / 35,745 meshes. The
companion standing runs enforce the exact 112,278 / 35,713 baseline inventory.

The normal moving run used the opt-in verifier. After 20 seconds in the settled
tail and a fresh SceneState, frame **5587** compared **4/4 observed reused maps**
with zero byte differences. Cascade 3 contains **12,733 non-clear depth texels**;
this is populated retained-map proof, rather than equality of two empty maps.
Full caster/material/texture candidate manifests and refusal diagnostics remain
linked to the receipts. The other three maps are clear-only. The strict native
summary reports `MATCH_ALL_OBSERVED_SHADOW_BYTES`; no untested map is counted as
passing. This tail audit is not a byte audit of every moving frame.

The first settle-5 normal/forced routes completed and passed machine checks,
but their moving screenshots showed the mandatory startup cover. They are
excluded from visual acceptance. Logs prove that the old native route started
after CPU readiness while cluster drawability admission still held the cover;
it released only after motion stopped. The lead confirmed that traversal task
#2151 already fixes this on integration (`666d7ceca`, included in `ec2596e8d`).
No GPU reuse source was changed. The lead approved the existing settle-20 option
for this supplementary pair. Each accepted run additionally proves actual
cover release precedes Moving and every saved moving PNG is nonblack. The cover
was never disabled and no readiness criterion was weakened.

The first R3 quiet attempt was rejected because its guard observed `dust.exe`.
A pre-launch R4 TEST was refused by a foreign hold. Both failures remain retained.
The final quiet controls and supplementary movement pair all have quiet guards.
The lead authorized retaining one hold across remaining owner-guarded TESTs;
this was necessary because `build_gate.py test` refuses even the owner's hold.
Owned holds were cleared in `finally`, including intermediate handoff/re-arm,
and no compiling occurred in the owned capture window. `r4-retained-window.json`
and `r4-visible-window.json` preserve the exact protocol and cleanup.

## Limits and remaining integration work

The legacy performance verdict remains **FAIL**, and its native-route traversal
field remains **N/A**. The supplemental validator checks the native route
without rewriting benchmark fields. This GPU attribution result does not prove
sustained unprofiled 120/140 FPS. A portal-heavy integration run and Metal device
measurement remain required before Plus integration. Native equality proves one
standing extracted frame and one selected moving-run tail; it does not certify
every mod, portal view or possible animation. Individual dependency and routing
tests, physical cull fixtures and source tracing cover the invalidation matrix;
not every live producer was mutated during the native route. Raw escaped GPU
writes outside the accepted producer contract remain unsupported.

The current standing cascade 3 refuses WaterMaterial: its prepared material is
present, its shadow declaration is absent, and actual bound textures have
render/storage write capability. A static declaration alone would not waive
that texture refusal. No declaration or trust exemption was added. Stock caster
queue/visibility/preprocessing work still runs while a map is retained; this
change removes cluster selection and exact shadow map clear/raster work.

The source design, writer inventory and invalidation ledger are adjacent:
`../design/cluster-gpu-exact-reuse.md`,
`cluster-gpu-texture-writers-2153.txt`, and
`cluster-gpu-reuse-invalidations-2153.md`. Full hash-linked results are in
`cluster-gpu-reuse-2153.json`. Raw binary/trace/readback/PNG artifacts remain in
the owned target, not in the source repository. No push or PR was made.

The vendor-local `.gitattributes` preserves five untouched Bevy archive files
that contain upstream trailing whitespace or blank EOF lines. Their bytes remain
identical to the installed exact 0.19.0 package; local Rust/audit patches retain
normal whitespace checks. This metadata-only policy does not alter shader inputs.
