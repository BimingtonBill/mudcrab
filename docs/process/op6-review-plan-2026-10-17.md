# OP6 review and completion plan — 2026-10-17

Review baseline: `opt/one-data-set` at `2a513b07`. The initial review was documentation only;
the user subsequently authorized implementation and OP6's cluster-showcase contract ownership.

## Continuation and coordination

- Implemented material capability gating before loader registration; unusable extensions cannot
  provide blend/falloff/animation payloads or install material-animation systems/resources. Core
  material handling is preserved. Independent read-only review found no defect.
- Implemented hash-only refresh under exclusive lock, preserving provenance and foreign JSON,
  rejecting stale/unknown ownership/version, duplicate IDs and missing/unlisted partial payloads.
  Independent review accepted after fixes; thirteen refresh regressions cover those boundaries,
  including rejection of a non-object manifest before the no-op path.
- Strengthened real-set hash verification to compare the whole extension list, under a shared lock.
- Reviewed implementation head `cdc2e5516f3cd1cb21281a0036d674daa7ee1c28` passes fmt,
  all-target/all-feature workspace clippy with `-D warnings`, workspace nextest (2997 passed,
  47 existing skips), workspace doc checks, 14 Python hash tests and the ignored real-set
  Rust/Python complete-entry check. All OP6-owned validation jobs finished; VG7 notified to
  release the scheduling pause. H18's converter-only fixture proof at this head passes all three:
  default 214 equal, interior/lights 195 each, against main `27d48513`; report
  `C:/Modding/Mudcrab/parity/runs/2026-10-17-014311-convert/report.md`. Four declared extensions
  account for additions. These fixtures do not exercise material payloads; real-set hash agreement
  proves integrity, not material core parity, so OP13 remains an acceptance limit.
- The complete workspace run exposed an existing shot-test diagnostic expectation affected by
  `float_roundtrip`: `1e40` now fails during JSON decoding rather than explicit finite validation.
  H18 agreed to accept either rejection message while retaining the same literal and `unwrap_err`.
- Cluster adoption contract: `docs/specs/converters/cluster-extension-adoption.md`. Reserved wrapper
  IDs `mudcrab.clusters` v1 and `mudcrab.cluster_terrain` v1 are distinct from binary format v3.
  VG8 confirmed whole-store ownership avoids splitting shared geometry tables; producer/reader
  validation and final manifests still need implementation before regeneration.
- Capability facts remain separate from user selections. OP15 tracks functional development UI
  availability reasons. Skyrim UI integration is outside this task and is not a showcase gate.
- VG7 coordinates shared integration. A read-only check found `dev-integration` clean at `e739c90e`,
  with its four room-culling commits still needing disposition. OP6 has neither claimed that
  checkout nor merged VG7. OP4 has since claimed presets and OP15's functional development UI;
  OP10 remains queued and OP5 still has no switch. These remain visual-acceptance dependencies.

## Evidence and scope

The one-set approach remains necessary and aligned with the approved policy: a main-compatible core, additive versioned payloads, and one asset folder usable by both engines. Loading compatibility is demonstrated; visual parity is not.

Read: the approved OP6 design (`C:/Modding/Mudcrab/mudcrab/local/research/op6-one-data-set-design-2026-10-16.md`), its schema inventory, policy sections 0.1/4.4, tracked one-tree policy, foundations plan, handoff, and parity reports. Local notes are absent from this migrated worktree; the originals remain in the main checkout.

- Converter report `C:/Modding/Mudcrab/parity/runs/2026-10-16-203747-convert/report.md`: **3/3 fixtures pass**, comparing `7297c417` with main `27d48513`. This predates the latest float-parsing/refresh commit. The fixture hash test explicitly says these meshes contain neither material extension; it cannot prove real-material core parity.
- Shot report `C:/Modding/Mudcrab/parity/runs/2026-10-17-001842-run/report.md`: **0/6 pass**. Both engines load and settle using `C:/Modding/Mudcrab/SkyrimConverted-one-set`. Almost every pixel differs. Ours is scratch `bb57b889`, reference `1fbfa215` on main `27d48513`; several phase branches are explicitly omitted. This establishes common-data loading, not an OP6-only visual regression. No per-option attribution has been measured.
- Fresh verification: `python -B -m unittest discover -s tools -p test_extension_hash.py`: **14 pass**. Earlier Rust checks are historical evidence, not rerun in this review.
- GitHub read-only check: main still `27d48513`; open overlap includes schema policy #113, projections #269 / PRs #270–271, settings #226 / PRs #229 and #262, lighting #219/#257, and merged terrain #235/#236. Recheck these before implementation/integration; coordinate with their owners rather than duplicating work. No publication is authorized by this plan.

## Findings

1. **High: material extension status does not control its consumers.** `engine/src/extensions.rs` reports missing/stale/unsupported material data, but `render.rs:1568` reads blend factors directly, and `render.rs:1763` reads animation data directly. The material feature is explicitly notice-only. A stale or future-version payload can still change rendering after a warning says it is off. Carry validated capability into the loader and suppress only extension-derived inputs and systems when unusable, preserving core material handling.
2. **High: refresh can erase provenance without regeneration.** `converter/src/main.rs:655` invokes `build_entries`, which stamps every recognized payload with the current `lod_build` identity. Thus a previously stale payload can become present merely by refreshing its hash. The command also bypasses the normal asset-lock path. Preserve provenance unless rebuilding establishes it; lock the data set exclusively before reading and writing. Preserve unknown extension entries rather than dropping them from a newer producer. Test these cases before recommending refresh as a general repair command.
3. **Acceptance gap: data availability enables behavior.** `open_features` opens snow, lighting and door catalogs whenever their manifests are usable. Availability is necessary but does not implement the approved vanilla-default option policy. Keep data capabilities distinct from option selection and coordinate behavior switches with OP3/OP4/OP5/OP10. Declaring vanilla record data in a manifest does not prove its current renderer matches vanilla.
4. **User-visible reasons remain incomplete.** `ExtensionNotices` is stored and logged but has no settings consumer. Connect it to the shared settings inventory, including unsupported versions and stale data, before claiming the menu contract complete.
5. **Proof and documentation need updating.** The tracked handoff still lists real conversion and fixture comparison as future work. The extension spec omits terrain files from the LOD ownership table and describes the old material-listing rule. The shot report still lists private schema numbering as an active divergence although this branch retired it. The real-data Rust hash test uses `zip` without asserting equal entry sets; strengthen it so missing entries cannot escape that comparison.

Known core mesh differences remain OP13: environment-cube encoding, effect-shader emissive/base-colour output, and unverified vertex/primitive output. Do not hide these as owned extensions or declare fixture coverage sufficient.

## Ordered plan and stop conditions

1. **Repair OP6 validity handling.** Fix material consumers and provenance-safe refresh in bounded patches, independently review implementation, and add regression cases for missing/stale/unsupported material entries, stale refresh, concurrent writer exclusion, and unknown extensions. Done: invalid extension payloads cannot affect rendering and refreshing hashes cannot make stale data current.
2. **Close converter proof at the final head.** Rerun default/interior/lights comparison against pinned main, plus representative licensed local material cases covering OP13's known differences. Compare core database schema/rows, mesh binary chunks, core JSON, texture bytes and URIs; exclude only declared owned payloads. Strengthen exact extension-set assertions and verify Rust/Python hashes on the existing one-set folder. Done: fixtures pass at the current commit; remaining real-mesh differences have explicit owners and block broad byte-parity claims.
3. **Finish option and menu integration with owners.** Ask the existing OP3/OP4/OP5/OP10 threads before editing shared files; connect notices through the settings inventory. Supply a validated capability separately from the vanilla-default option state. Done: all-off behavior is selectable and disabled features do not install their systems or load their payloads. Keep parity fixes intended upstream on phase branches.
4. **Repeat visual acceptance when switches exist.** Pin and record both engine heads, phase inclusions/omissions, flags and data identity; refresh the noise floor, run six poses as TEST, and run the required portal-heavy GPU check before dev integration. Use per-option isolation only to resolve a remaining concrete failure. Done: all six satisfy the project's parity criterion, with excluded-reference uncertainty resolved or explicitly accepted. A loaded world is not a passing parity result.
5. **Validate and integrate locally.** Run gated fmt, workspace clippy with all targets/features, workspace nextest and doc tests after code fixes. Review the full final diff, refresh the handoff/design/status and upstream-note draft, then integrate only through `D:/Mudcrab/merge/dev-integration` once acceptance holds. Keep the thread open until follow-up is complete. Publishing and old-set deletion still require the user's per-item yes.

No new conversion is planned: reuse the existing one-set folder for checks. No timing run is justified for this review; the unresolved questions are correctness, capability gating and visual equivalence. If later changes alter render cost, read the profiling workflow and use TEST captures before considering a separately justified TIMING run. No lower settings, arbitrary IDs, worldspace exceptions or relaxed parity thresholds are proposed.

Hive join was attempted but refused because `CLAUDE_CODE_SESSION_ID` is unset in this Codex session; no successful presence registration is claimed.
