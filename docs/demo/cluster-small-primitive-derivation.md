# Wider cluster coverage: proposed dataset build

Status: DS1 review fixes pass automated acceptance on 2026-10-20. After replacing the
provisional admission estimate with measured ratios and a write-time floor guard, the exact
authorized 1,024-triangle terrain derive exited 0 in 274.78750640002545 seconds. Overlay
`7be1674902d8a47d`, label `default`, is published under the supplied base. Dust reports 3.0 GiB
(3,256,340,788 apparent bytes); the guard's lowest sample was 17,176,412,160 free bytes,
above its 10 GiB floor. Terrain sources matched; the verifier reports two extensions and
zero problems. The earlier admission refusal is historical. Commands and evidence are in
[DS1 evidence](../evidence/ds1-overlay-2026-10-20.md). GPU readiness and 120 fps acceptance
remain unverified here; the lead runs the engine checks after merging. See
[cycle34 evidence](../evidence/cluster-fallback-cycle34-2026-10-18.md) for the earlier baseline.

The current cluster set excludes otherwise eligible primitives below 1,024
triangles. The existing threshold option can admit these primitives without
changing builder quality, source geometry, material guards or runtime handoff
proof. This is a coverage change; its performance must still be measured.

## Layout: the base stays whole, the clusters are an overlay

`converter derive-clusters` no longer copies the base. It reads the base read-only and writes one overlay
folder under the base's `derived/` folder (design: DS1, `D:/Mudcrab/review/drafts/DS1-DESIGN.md`):

```
<base>/                                   never written by derive-clusters
  skyrim_world.db, textures/, meshes/, lod/, conversion-manifest.json, ...
  derived/
    clusters-<settings-id>/               one overlay per build-settings variant
      overlay.json                        kind, label, settings id, base core identity, base fingerprint,
                                          builder identity, created, OWNER text
      clusters.db                         cluster tables only (cluster_meshes, cluster_rejects,
                                          cluster_reference_coverage, cluster_instance_map, terrain tables)
      clusters/                           payload chunks
      cluster-manifest.json               built_from = the base core identity
      cluster-terrain-manifest.json
```

- `<settings-id>` is a short hex hash of the resolved build settings (triangle threshold, terrain,
  include-trees and the other settings that change the output; `--jobs` is not one of them) plus the builder
  identity, computed by one function in the converter. Read it from the overlay folder name or
  `overlay.json`; do not compute it by hand. Two settings variants of one base give two folders.
  `overlay.json` records the full `build_settings` object, including converter version and builder
  identity. The id hashes that object's canonical JSON; the reader checks their consistency and
  reconstructs a rebuild command with the recorded threshold, terrain/tree flags and label.
- `--label` (default `default`) is the human name in `overlay.json`. The engine selects the overlay by
  label, settings id or path. A label is unique within one base. Reusing it with different settings
  refuses unless `--replace` is passed; replacement retires the previous overlay with that label only
  after the new overlay publishes. Use distinct labels to retain several settings variants.
- The base fingerprint is the base's core identity plus the hash of its `conversion-manifest.json`. The
  engine refuses an overlay whose recorded identity or fingerprint differs from the base it is started with,
  and refuses an old full-copy dataset (cluster tables inside `skyrim_world.db`); rebuild with
  `converter derive-clusters <base>`.
- A failed run removes only the `derived/.staging-*` folder it created; `--keep-failed-staging` keeps it for
  inspection. An existing overlay with the same settings id is replaced only with `--replace`.
- Recorded asset failures are retained core limitations: derivation prints their count and records
  `base_failure_count` in `overlay.json`. The engine's existing incomplete-assets flag still controls
  whether the base may run; an overlay does not repair its missing assets.
- The engine retains a binding receipt for the launch. Later terrain and world connections reuse it,
  still check their base connection's core identity, and still attach `clusters.db` read-only. The
  engine may create the sibling bookkeeping file `derived/clusters-<settings-id>.lock`; no database,
  payload or manifest is written by a reader.

Full conversions with `--lod-mode cluster` or `auto` use this same writer after publishing the base.
The base contains no cluster tables or payloads. Both modes derive offline even when the recorded
adapter probe reports missing runtime features. If derivation fails, the base stays valid and the
command exits non-zero with the original cause and an exact `converter derive-clusters` retry.
Terrain inputs move into an owned sibling `<base>.terrain-sources-*/vfs` folder before derivation.
Success removes it; failure retains it and names it in the retry command. Stage counters
remain in the conversion report. Use standalone `cluster-stage --bench-baked` for the measurement
comparison; `--cluster-bench-baked` is refused in full conversion.

A later base conversion refuses a destination containing overlays or a live derive staging run.
Bookkeeping files left by a failed first derive do not require `--discard-derived`. The guard runs after
interrupted-publication recovery. `--discard-derived` authorizes retirement of overlays only when the
new base publishes successfully: a failed reconversion preserves the old base and its overlays.
Do not discard overlays merely to change cluster settings: derive another variant or use `--replace`.

## Inputs and destination

- Base (source): `C:/Modding/Mudcrab/SkyrimConverted-one-set`.
- Base core identity:
  `e81cef8960eed6c02c80540e804e7ac35ae54ca17c0e704998586b163340dc6d`.
- Raw DDS root: `D:/Mudcrab/SkyrimConverted-vg0a-timers/vfs`.
- New output: `C:/Modding/Mudcrab/SkyrimConverted-one-set/derived/clusters-<settings-id>/` with label
  `default` (the settings id is printed by the converter and stored in `overlay.json`). There is no
  new dataset folder and no base copy.
- The earlier plan used the full-copy destination
  `D:/Mudcrab/SkyrimConverted-vg7-showcase-20261018-small-primitives`; DS1 does not write that destination.
  The existing certified full-copy dataset is retained as measurement evidence; the engine refuses
  its old layout. Only the user may authorize its retirement.
- The base's core files are not modified: hash all of them before and after (see Acceptance). Only the new
  `derived/` folder appears inside the base.

The read-only cycle36 inventory finds no cluster store in the source, equal core
and LOD identities, and all 65 DDS contents equal to both the LOD source recipe
and conversion manifest. This is a snapshot, not guarded certification. The
derivation must repeat the existing full preflight and hash checks under its
source/shared and destination/exclusive AssetLocks.

Keep STAT and TREE scope, no added child-world scope, no area limit, and terrain.
Use the staged publication path: the converter builds under `derived/.staging-<pid>-<time>/` and renames it
to `derived/clusters-<settings-id>/` on success. Finalize the whole object-plus-terrain overlay before
recording terrain's final overlay hash dependency; both manifests keep `built_from` equal to the base core
identity. The base is read through a read-only connection and never copied, backed up or linked, so its core
logical contents, warnings, foreign manifest entries and unknown manifest fields are unchanged by
construction. Existing core incompleteness stays visible.

## Prerequisites

1. Synthetic threshold 1,024 versus 1 tests now pass through the actual stage,
   authority, base finalization and engine reader. Include one-triangle and
   18-triangle meshes; verify decoded leaf triangle multiplicity and winding.
   BLEND, override and missing-model refusals remain intact. Engine all-target
   nextest passes 2,033/2,033; fmt and all-feature clippy pass (cycle36c).
   The runtime-ineligible Complete fallback correction also passes engine
   nextest 2,044/2,044, fmt and all-feature clippy (cycle37b). Its real-loader
   billboard fixture and tangent-cache delivery proof are documented in
   [cycle37 evidence](../evidence/cluster-source-fallback-cycle37-2026-10-18.md).
   GPU, visual, far-shadow and performance acceptance remain open.
2. Converter prepared from checked integration source
   `ed8a712c4f5d062ef9eebe07623b37644be52f6e`: gated quick-profile build passed
   (cycle38, 29.33 seconds). The saved executable is
   `C:/Modding/Mudcrab/builds/vg7-small-primitive-converter-cycle38/converter.exe`,
   SHA256 `39b84c635fe8dce27e5fbcd594c6c37fd230305665df2f89b62a16964b1e9d31`.
   Its owned folder contains `identity.json`, the successful build log and
   `derive-clusters --help` output (exit 0). This preparation ran no derivation.
   That executable predates the overlay layout: it takes `<source> <new-output>` and copies the base. Do
   not run it for this build. Prepare and record a new immutable converter from the checked overlay
   integration head (`derive-clusters <base> [--label L] ...`, no output path), with its own
   `identity.json`, build log, hash and `--help` output, and use that one. Recheck the executable hash
   before using it; do not substitute an old image.
3. The derivation's destination is a new folder inside the source's `derived/` (the overlay layout, DS1,
   decided 2026-10-20), not a copy of the base. No confirmation step is part of this procedure; the
   capacity and execution gates below still apply.
4. Recover sufficient disk space without deleting unknown folders or evidence.
   The overlay is written inside the base's drive (C: for `SkyrimConverted-one-set`), not D:, and holds
   cluster tables and payload only, with no ~27 GB core copy. The converter refuses before any work when
   free space on the `derived/` volume is below its admission figure, and names the volume, the estimate,
   the needed and the free bytes. Review round 2 replaces the provisional multipliers with ratios
   from the lead's measured same-core 1,024-triangle terrain derive on 2026-10-20. The
   [evidence](../evidence/ds1-overlay-2026-10-20.md) records the source inventory and precision:

   ```
   object_payloads = ceil(mesh bytes * 2,908,039,936 / 2,570,110,144)
   tables = ceil(base database bytes * 563,449,856 / 2,173,202,432)
   terrain_budget = ceil(cell-cache bytes * 165,344,400 / 488,986,104) only with --terrain, otherwise 0
   estimate = object_payloads + tables + terrain_budget + 1 MiB manifests
   if overlays of the same base (same base fingerprint) already exist under derived/:
       hint = actual overlay bytes
       add terrain_budget to a hint only if --terrain is requested and that overlay has no terrain
       estimate = largest hint
   needed = estimate + 10 GiB retained free-space floor
   ```

   This prediction is only a fast-fail hint. The authoritative write-time guard samples free space
   per payload/manifest file and per 8 MiB SQLite write budget, including database, journal and
   shared-memory growth. It stops before an allocation would cross the unchanged 10 GiB floor,
   names the volume, needed and free bytes, and logs the lowest observed free bytes on exit.
   Failure removes only that run's staging; `--keep-failed-staging` retains it. One synchronized
   guard covers both workers, so they cannot spend the same sampled headroom. The guard does
   not reserve space against unrelated processes between samples.

   The database ratio covers measured cluster table growth, not a copied database: the base
   stays attached read-only. The table ratio includes terrain rows even for object-only builds
   because table growth was not separated by extension. Take `needed` from the printed admission line,
   and recheck with `python tools/disk_guard.py --drive C: --need <GiB>`. The earlier full-copy budget was 40 GiB growth
   on D: (at least 50 GiB with the floor); cycle38 repeated its refusal with only 15.1 GiB free on D.
   That figure no longer applies to the destination. The later approved cleanup was rejected by automatic approval review
   before removal. Hash-checked evidence preservation temporarily uses about
   0.9 GiB, leaving about 14.3 GiB free (cycle41). Do not start a build or
   derivation below the applicable gate floor. Compression of this thread's own
   retained incremental cache preserved all files but did not resolve D capacity.
5. Serialize with other GPU/build users: the converter performs a device
   capability probe and substantial CPU/I/O work. Record an honest end-by if a
   conversion hold is used; release it when the process ends. No FPS measurement
   is made during derivation.

The bounded command after these prerequisites is:

```powershell
# The overlay-capable converter recorded under prerequisite 2, not the cycle38 image.
$checkedConverter = '<recorded immutable overlay-capable converter.exe>'
& $checkedConverter derive-clusters `
  C:/Modding/Mudcrab/SkyrimConverted-one-set `
  --label default `
  --terrain --terrain-sources D:/Mudcrab/SkyrimConverted-vg0a-timers/vfs `
  --include-trees --jobs 2 --triangle-threshold 1
```

`--jobs 2` is the user's limit for this machine; do not raise it. Output goes to
`C:/Modding/Mudcrab/SkyrimConverted-one-set/derived/clusters-<settings-id>/`. Optional flags:
`--replace` (replace an existing overlay with the same settings id; the old one is removed only after the new
one publishes, also retiring the previous holder of that label across settings ids) and
`--keep-failed-staging` (keep the failed run's staging folder for inspection).

`$checkedConverter` must name that recorded immutable executable. Do not run this
command while any prerequisite remains unmet.

The command above is the wider-coverage proposal, not the command executed for DS1 review.
Since label `default` now exists, that proposal needs a distinct label or explicit `--replace`.
The successful review run used threshold 1,024 without `--include-trees`. To rebuild that
published review variant with the current overlay-capable converter, through the conversion-class gate:

```powershell
converter.exe derive-clusters "C:/Modding/Mudcrab/SkyrimConverted-one-set" --triangle-threshold 1024 --label default --jobs 2 --terrain --terrain-sources "D:/Mudcrab/SkyrimConverted-vg0a-timers/vfs" --replace
```

To start the demo on the result:

```powershell
pwsh -File tools/demo/play-demo.ps1 -Assets C:/Modding/Mudcrab/SkyrimConverted-one-set -Cluster on -ClusterOverlay default
```

That passes `--assets <base> --clusters default --cluster-terrain ...` to the engine. `--clusters`
takes the label, the settings id or the overlay path; bare `--clusters` (play-demo without `-ClusterOverlay`)
takes the only overlay and is refused when the base has several. `--cluster-package` no longer exists.
The supplied base remains incomplete. Its direct engine command, for the lead's GPU checks only
(not executed during DS1 implementation), is:

```powershell
engine.exe --assets "C:/Modding/Mudcrab/SkyrimConverted-one-set" --clusters default --cluster-own-statics --cluster-terrain --allow-incomplete-assets
```

## Native LOD integration boundary

The baked object LOD owner supplied `phase2/baked-object-lod-engine` at
`6654e80cf0495822b2db2231ded76b266c4beb51` as the reusable local head and permits
adapting its modules without changing that owned branch. Its older scratch merge
is not cleared for dev: the portal-heavy GPU check remains undone. The historical
release measurements in `local/research/fun/n2-baked-object-lod-results-2026-10-13.md`
(including the October 14 addenda) describe that older tree and dataset, not this
cluster integration or a stable 120 fps result.

At integration head `2b18f2e1`, `world/object_lod.rs` contains the inventory reader,
but no engine consumer opens `ObjectLodSource`; `streaming/lod_objects.rs` and
`streaming/lod_trees.rs` are absent. Native rendering is therefore still an
integration gap, not a completed consequence of retaining the inventory reader.
Preserve the reusable head's arbitrary nesting-level validation, held-interior
lifecycle and structural pop counters when adapting those consumers.

The old `FullDetailReady` is not a valid render handoff certificate. Its
`update_full_detail_ready` accepts full-resident cells with zero pending main-world
markers, scans every six frames, and retains the set between scans; it has no
view/pass key, queued draw proof or explicit asset-failure check. Both near-cell
suppression and coarse/fine block replacement need current render proof before
that consumer is installed. The destination bridge supplies geometry/material/
pipeline/queued-membership checks, but its previous-frame reports do not alone
prove full-source completeness or same-frame replacement. Reuse the old selector,
payload/hash validation and loading lifecycle; adapt demand to `LodDemandViews`,
publish all roots through the existing combined role owner, and keep Pinned roots
hidden. Do not add a second independent portal crossing planner or install the
old tree plugin implicitly. This audit involved no Rust edits or new build.

Baked object blocks hand off per cell through `lodSegment` and full-detail
readiness; they cannot prove replacement of one rejected reference. Keep exact
per-reference ordinary fallback for the cluster path. Tree billboards need the
validated identity relation and same-frame view/pass proof before retirement;
the old consumer's cell-level handoff is not sufficient for cluster replacement.
The owner's singleton-segment tagging gap (N6) remains reported open. Only vanilla
data and object levels 4/8/16 were exercised there; neither mod compatibility nor
level-32 live behavior is established by those runs. These native-path gaps remain
part of all-off and mixed-path acceptance after the proposed dataset build.

Keep `mudcrab.lod_inventory` v1 and the agreed separate
`mudcrab.lod_tree_references` v1 contract (`7ae481df`), with the shared reader
revised at `beaa0b2d` still awaiting integration. The proposed inventory-v2 ordinal
column is not required by this reader: `rows_in_block` supplies the rendered
instances directly from validated companion rows, each with its own payload,
reference and continuous per-block ordinal. Equality with the old native table is
a multiset check with multiplicity, not a positional identity join. Identical
tuples remain distinct companion keys; the source parser attests their identity
assignment. Do not reconstruct that assignment from old rows, rowid or SELECT
order. The reader's duplicate-tuple tests are component evidence, not a completed
producer or renderer integration.

The separate extension owns its two relation/binding tables and carries the final
inventory hash in its binding row; no generic envelope field or inventory schema
change is needed. Retain resolved/disjoint/unknown states, supported parser/order
rules, per-world completeness scope and verified plugin-remap provenance. Missing
or unqualified relation keeps native billboards and disables TREE cluster
replacement for that domain, rather than permitting overlap. A future source
derive needs verified raw BTT inputs and matching plugin layout; absence of BTT
from a manifest inventory is not proof that the source files are unavailable.
Keep source discovery, locks (shared on the base, exclusive on the overlay path)
and staged publication as explicit producer gates; non-owned-table preservation
is replaced by the base staying byte-identical. Approval for the wider-coverage cluster derivation
does not by itself qualify unimplemented tree-relation data or its runtime proof.

## Acceptance after publication

Hash every file of the base before and after the run: the base must be byte-identical and only
`derived/` may be new. Recompute the overlay's extension hashes and their bindings to the
base core identity and base fingerprint in `overlay.json`:
`python tools/extension_hash.py --assets C:/Modding/Mudcrab/SkyrimConverted-one-set --overlay default`
(`--overlay` takes a label, settings id, folder name or path; exit 0 clean, 1 problems). Start the engine on the overlay, then on a
different base or changed manifest to see the refusal ("this cluster overlay was built for another
conversion of this base"). Inventory
Complete/Partial/Unknown/NotEligible references and remaining rejection reasons;
do not promise that the previous 153,971 wholly rejected references all become
Complete. Inspect actual payload counts and memory use before rendering.

Run the current ordinary-renderer door/far/repeated-pose route with fixed quality
and reach, reading all readiness failures and viewing captured images. Then test
walking and portal crossings in both directions, the all-off path and optional
shadow behavior. Activate the demo launcher only after visual and functional
gates pass. Quiet release-profile timing, discarded warm-up and guard verdicts
remain necessary for any stable 120 fps claim.
