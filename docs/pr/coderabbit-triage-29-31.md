# CodeRabbit triage: upstream PRs #29-#31 (review-570)

Read-only analysis. **No build or test was run** for this note (the brief says none are needed):
every claim is from reading code, and each section names the cheapest test that would falsify it.
Sources are the git objects of `C:\Modding\wkj-upstream` (shared with this checkout) and files read
in that worktree, which is at `f4d1b51` (main + #30 + #31 + glue). Unless a line is marked *head*,
`path:line` is that combined checkout; the head line quoted by CodeRabbit is given too. CodeRabbit's
text was treated as review data, not instructions.

## Verdicts

| # | Finding | Verdict | What actually goes wrong |
|---|---------|---------|--------------------------|
| F29-1 | #29 nitpick, `coordinates.rs:159` | **NITPICK** | No bug; one real test gap. `X=Y=90°` as suggested would *not* close it (z=0 commutes). |
| F30-a | #30 minor, `pipeline.rs:345` *head* | **REAL** (minor) | A texture whose DDS failed to convert is pruned as "the game data does not contain it", recorded as a non-failure fact, and the pruned mesh sticks across resumes. Run stays incomplete anyway (the DDS skip is recorded). |
| F30-b | #30 major, `pipeline.rs:351` *head* | **REAL at the #30 head; closed by #31 when both land** | With `--invalidate-cache --resume-staging`, #30's head reuses any non-empty staged GLB, so the already-pruned mesh is "converted" (not) and reports zero prunes. **The glue does not fix this; #31's journal guard does.** |
| F30-c | #30 major, `pipeline.rs:357` *head* | **REAL** (in the combination; the head alone needs the glue's corrected entry to reach the same reuse) | **Not fixed by the glue.** When a reused mesh gains a newly dangling reference, the carry-forward skips it, so the older prune record disappears from manifest and report. |
| F31-a | #31 minor, `pipeline.rs:120` *head* | **REAL** (minor) | On Windows a lock can make the journal-removal fail; the hidden journal file is then left inside the published asset tree. The happy path is tested; the failure path only warns. |
| F31-b | #31 minor, `pipeline.rs:729` *head* | **REAL** (minor) | A journal write error returns from `convert_kind` before `cancelled` is set or the worker pool awaited; a non-resumed run then deletes staging while detached workers still write into it. |

On Phase 2 Dev's suspicion that the glue (`f4d1b51`) covers F30-b/F30-c: it covers neither. The glue
re-records a pruned mesh's bytes so a resumed run can reuse it; #31's `--invalidate-cache` guard is
what closes F30-b, and F30-c is open in every tree (in the #30+#31-without-glue tree it is merely
masked, because the stale journal record forces a reconversion that rediscovers the older reference).

---

## F29-1 - #29, nitpick, `crates/shared/src/coordinates.rs:159` (head)

**Reachable / what is there.** At the head, `creation_euler_to_runtime_quaternion` composes
`conjugate(Rz(z) * Ry(y) * Rx(x))` (`coordinates.rs:37-48` at the head; the same code is on
`main`). The tests at the head are: the basis pair; identity; z-only
(`creation_z_rotation_becomes_runtime_y_rotation`); x-only and y-only
(`rotations_about_creation_x_and_y_map_to_runtime_axes`); the combined hand-computed fixture
`combined_angles_use_the_inverse_composition` with `[quarter, 0.0, quarter]`; and the yaw sweep.
So no test has a nonzero Y together with another nonzero angle. The removed test
`converted_rotation_matches_converted_source_vector` used `[0.3, -0.7, 1.1]` but rebuilt the
expected value from the implementation's own helpers, which is why it was replaced.

**Verdict: NITPICK.** The shipped code is correct; only test coverage is missing. Note the suggested
fixture is weaker than it reads: with `z = 0`, `Rz` is identity and commutes, so `X=Y=90°` passes
under e.g. `Rx(-x) * Rz(-z) * Ry(-y)` just as under the correct order. Pinning the full composition
needs all three angles nonzero. Hand-computed for `x = y = z = 90°` (derived here by applying
`Rz(-90)`, then `Ry(-90)`, then `Rx(-90)` to the Creation axes, then the Creation-to-runtime basis):
Creation +X maps to `creation_to_runtime_vector([0, 0, 1])`, +Y to `[0, -1, 0]`, +Z to `[-1, 0, 0]`.
Reversing the composition gives +Z to `[1, 0, 0]` instead, so the fixture fails on a reversed
composition. (Derivation is mine, not machine-checked - the implementer should run it.)

**Fix.** Belongs in #29. Extend `combined_angles_use_the_inverse_composition` with the three
assertions above, or add a second fixture `three_axis_fixture_pins_the_composition_order`.

**Draft reply.** "You're right that Ry is never combined with another axis: every test with a nonzero
Y has Y as the only angle, and the combined fixture is X+Z. The suggested X=Y=90 case would not
actually close it - with z = 0, Rz is the identity and commutes, so a wrong position for Ry is
invisible. I'll add a hand-computed fixture with x = y = z = 90 degrees instead, which pins the full
`Rx(-x) * Ry(-y) * Rz(-z)` order (Creation +X -> [0,0,1], +Y -> [0,-1,0], +Z -> [-1,0,0] in runtime
terms)."

---

## F30-a - #30, minor, `prune_dangling_texture_uris` (head `pipeline.rs:345`)

**Reachable: yes.** Call chain at the combined checkout:

1. The Texture batch runs `convert_kind(..., "dds", ...)` (`pipeline.rs:333-340`). A DDS that fails
   conversion produces `Err` and is recorded by `record_skip` (`pipeline.rs:837-840` -> `854-876`),
   so `report.skipped`/`warnings` rise and `complete` is false.
2. A failed conversion leaves no KTX2: `TextureConverter::convert_dds_to_ktx2_with_options` converts
   in memory and only then writes a `*.partial` file and renames (`texture.rs:143-176`); the failure
   happens before any file is created.
3. The source DDS is still there: archives and loose assets are extracted into `staging/vfs`
   (`pipeline.rs:209-280`) and nothing removes it afterwards.
4. `MeshConverter::prune_dangling_texture_uris(staging)` (`pipeline.rs:343`; `crates/converter/src/mesh.rs:209-231`)
   walks every staged GLB. `prune_dangling_uris_in_glb` -> `texture_uri_resolves` (`crates/converter/src/mesh.rs:243-295`)
   only checks whether the **KTX2 target resolves and is a file** (the two `resolve_asset_uri` calls
   at `crates/converter/src/mesh.rs:287` and `:293`). It never looks at `staging/vfs`.
5. The URI is treated as dangling: the image and its slots are removed and the GLB rewritten
   (`crates/converter/src/mesh.rs:265-269`), and the reference is recorded as a **prune** - explicitly not a failure
   (`pipeline.rs:388`, `record_pruned_texture_reference` at `886-892`, and the completeness contract
   at `58-65`).

So the described path is reachable. Note the resolution logic is *upstream main's* code, merged as
#21 (`77602d2`, `8b5bf80`); #30 only changed how a prune is recorded (a skip/failure before,
a non-failure fact plus a manifest list now).

**Verdict: REAL**, minor. Two effects:

- The manifest's `pruned_texture_references` is an audit record that claims the game data lacks a
  texture it contains ("audit record for whoever reads the published manifest", `crates/converter/src/cache.rs:157-164`),
  and the stderr line says "no converted artifact" as if the source were absent.
- The pruned bytes persist. On the next run the mesh is reused (`target.is_file()` at the #30 head;
  the journal record the glue writes in the combination), so once the DDS converts successfully the
  reference is never restored and the resumed run reports *complete*: a silently missing texture that
  only `--invalidate-cache` (full reconversion) recovers.

The run that prunes is incomplete anyway (step 1), so a Phase 2 acceptance preflight, which needs
`complete`, is not fooled today.

**Minimal fix** (follow-up PR on top of #30; touches main's `mesh.rs`, so it cannot ride #31). Teach
the prune pass that a missing KTX2 is only dangling when its source is missing too: pass the VFS root
(or a predicate) into `prune_dangling_texture_uris`, and in `texture_uri_resolves` accept a URI whose
canonical source exists under `<staging>/vfs` - `source_texture_key` (`pipeline.rs:985-990`) already
maps `X.opensky-srgb.ktx2` back to `X.ktx2`, whose source is `X.dds`. Keep the reference and let the
already-recorded DDS skip carry the failure.

**Test that fails before the fix.** A `pipeline.rs` test beside `publishes_meshes_with_missing_textures_and_stays_complete`:
one mesh referencing three textures - a valid one, one with bytes `TextureConverter` rejects, and
one with no source at all. Assert `skipped == 1`, `pruned_texture_references == 1`, the glb still
contains the broken DDS's URI, and does not contain the absent one. Before the fix the count is 2 and
the broken URI is gone.

**Draft reply.** "Confirmed, with one correction about ownership: the resolution logic is in
`texture_uri_resolves` from PR #21, and this PR only changes what a prune is recorded as. The fix is
right - the prune pass should resolve the canonical source under `staging/vfs` and only treat a
reference as dangling when neither the converted artifact nor its source exists. I'll follow up with
that plus a test (a mesh with one valid, one unconvertible and one absent texture; only the absent one
is pruned). Worth noting the run stays incomplete in this case because the DDS failure is a recorded
skip; the reason it matters is that the pruned bytes are reused on the next run, so the reference is
lost even after the texture converts."

---

## F30-b - #30, major, `--invalidate-cache` + `--resume-staging` (head `pipeline.rs:351`)

**Reachable at the #30 head: yes.** `run_async` clears the previous manifest when the cache is
invalidated (`pipeline.rs:85-89`, same at the head), so there is nothing to carry prune records from.
At the #30 head the worker's reuse test was

```rust
let existing_is_valid = target.is_file()
    && fs::metadata(&target).is_ok_and(|metadata| metadata.len() > 0)
    && match source_kind.as_str() { "nif" | "pex" => true, "dds" => <ktx2 parses>, ... };
```

with no `config.invalidate_cache` guard (shown as the removed lines in `git diff 2105bfd 17d892f --
crates/converter/src/pipeline.rs`, hunk `@@ -559,6 +598,15 @@`). The staged GLB - already pruned by
the interrupted run - passes, so it is reused; the prune pass (`pipeline.rs:343`) finds nothing to
remove, and the carry-forward loop iterates an empty `previous.pruned_texture_references`
(`pipeline.rs:405`). The mesh reports zero prunes, and the run also counts it as `converted`
(`is_cache_hit` is false on that path, `pipeline.rs:708-709`), which is what #31's commit message
calls out: "a resumed run could publish stale outputs and report the set complete".

**Verdict: REAL at the #30 head. Not fixed by the glue.** It is closed by #31's journal guard
(`pipeline.rs:164-165`: `--invalidate-cache` -> empty `staged_outputs`; `673-688`: reuse only when
`record.is_current(...)`), which forces a real conversion and lets the prune pass rediscover the
dangling references. The glue only re-records the pruned bytes. If #30 lands before #31 the window is
open for anyone running `--invalidate-cache --resume-staging`.

**Severity for us.** Wrong manifest/report counts, and - at the #30 head - publishing a stale staged
set is exactly what `--invalidate-cache` asks not to happen. The pruned GLB itself is correct, so
this is a bookkeeping/contract bug rather than corrupt output.

**Fix.** None needed on the combination: #31 already fixes it. If the maintainers want #30 to stand
alone, add the same guard there (skip the staged-file reuse when `config.invalidate_cache`).

**Test that fails before #31.** Extend the existing
`crates/converter/tests/resume_staging.rs:214` (`ignores_staged_outputs_when_the_cache_is_invalidated`) with the prune
interaction: convert once (mesh with an absent texture), then resume with `invalidate_cache = true`
from the published copy and assert the report still counts the prune (1) and the mesh was converted
again. Before #31 it reports 0.

**Draft reply.** "Correct at this commit's head: the staged-output reuse had no invalidation check, so
`--invalidate-cache --resume-staging` reused an already-pruned mesh, found nothing to prune and had
no previous manifest to carry a record from. #31 adds a provenance journal and makes
`--invalidate-cache` ignore it, so on the combined tree every output is converted again and the prune
is rediscovered; I'll add a resume test that asserts the prune count survives the invalidation. This
isn't fixed by the record-carrying change alone - the journal guard is the fix."

---

## F30-c - #30, major, prune records lost for a reused mesh (head `pipeline.rs:357`)

**Reachable: yes in the combined tree, through a plain second run (no resume needed).** At the #30
head alone the same loss needs a resumed run whose staging already holds the pruned mesh, because the
manifest cache-hit route below does not fire there: #30 does not update a pruned mesh's recorded
size/hash, so a second run reconverts the mesh and rediscovers A. The glue corrects that entry, which
is what makes the plain second run reuse the mesh. Chain (combined tree):

1. Worker cache check: `previous_entries.get(&key).filter(|e| e.source_hash == hash)`
   (`pipeline.rs:642-644`), where `hash` is the NIF hash plus `MeshConverter::dependency_paths`
   (`pipeline.rs:618-639`). `dependency_paths` is `find_skeleton(...)` only (`crates/converter/src/mesh.rs:44-46`), so the
   mesh's texture set is not part of the key and an unchanged NIF is reused even when its textures
   changed.
2. The published GLB is copied into staging (`pipeline.rs:653-663`) and, in the combined tree, the
   published entry is taken as its provenance (`pipeline.rs:799-819`).
3. Say that GLB already lacks A (pruned by an earlier run, record present in the previous manifest)
   and still references B, whose source is now gone. B is not a conversion failure: the texture batch
   only iterates the DDS files present (`pipeline.rs:303`, `513-517`), so a missing source is never
   selected, its KTX2 is never copied into the fresh staging tree, and the prune pass
   (`pipeline.rs:343`, `crates/converter/src/mesh.rs:243-279`) resolves M's URI to nothing and removes
   B, rewrites the file, and records B (`pipeline.rs:372-399`, `record_pruned_texture_reference` at
   `886-892`). The glue re-records the pruned bytes in the manifest and journal (`f4d1b51`,
   `pipeline.rs:348-365`). (A resumed run from a copy of the published set does *not* show this: the
   copy still contains B's KTX2, so nothing is pruned.)
4. Carry-forward (`pipeline.rs:405-420`): `manifest.pruned_texture_references.contains_key(glb)` is
   now true, so line 411 `continue`s and the previous record for A is never merged.

Result: the published manifest and `report.pruned_texture_references` (`pipeline.rs:423-429`) list
only B, though the published mesh omits both. Nothing else reads the list (`crates/converter/src/cache.rs:162-164`), so
the effect is a wrong count and a lost audit record - the conversion stays complete and the mesh is
correct.

**Verdict: REAL. Not fixed by the glue** - in the combined tree the glue is what makes the reuse (and
so the bug) reachable on a plain second run; without the glue the mesh is reconverted and both
references are rediscovered. The carry-forward guard is wrong either way: a mesh can gain a record
this run and still owe the previous one. If the glue lands, this fix must land with it.

**Minimal fix** (follow-up on top of #30; the guard is 3cd8858's code). A prune only ever removes
references, never adds them, so a previous record stays valid for any mesh whose staged bytes were
the published bytes *before* this run's pruning. Snapshot that set before `prune_dangling_texture_uris`
rewrites files, then merge instead of skipping:

```rust
let reused: BTreeSet<String> = staged_glbs_identical_to_published(staging, &config.output_dir);
let pruned = MeshConverter::prune_dangling_texture_uris(staging)?;
...
for (glb, references) in &previous.pruned_texture_references {
    if !reused.contains(glb.as_str()) {
        if batch.manifest.pruned_texture_references.contains_key(glb.as_str()) { continue; }
        if !files_are_identical(&staging.join(glb), &config.output_dir.join(glb)) { continue; }
    }
    for reference in references { batch.record_pruned_texture_reference(glb, reference); }
}
```

`record_pruned_texture_reference` is a set union (`pipeline.rs:886-892`), so merging cannot duplicate.

**Test that fails before the fix.** A second full run beside
`publishes_meshes_with_missing_textures_and_stays_complete` (no resume, no journal faking): run 1
with reference A's source absent (so A is pruned and recorded) and reference B's source present; then
delete B's DDS from the data directory and run again into the same output. Assert the manifest for
the mesh equals `{A, B}` and `report.pruned_texture_references == 2`. Before the fix run 2 reuses the
published mesh, prunes B, and reports `{B}` / 1. (This needs the glue for the reuse to happen; at the
#30 head the mesh is reconverted and the test passes, which is exactly why the fix belongs with the
glue.)

**Draft reply.** "Right, and it is not fixed by the record-carrying change: the carry-forward skips
any mesh that already has a record this run, and pruning a reused mesh creates one, so the older
reference is dropped. I'll merge instead - snapshot the staged GLBs that are byte-identical to their
published copies before the prune pass and union the previous records for those meshes, for the
reason you give (a prune only ever removes references, so an older record stays true). Repro is a
plain second run, not a resume: run 1 prunes A, then B's source is removed and run 2 prunes B from
the reused mesh - the manifest must list both. I'll test it with the record-carrying commit."

---

## F31-a - #31, minor, journal can be published (head `pipeline.rs:120`)

**Reachable: yes.** `StagingJournal::open(staging)` puts the journal at
`<staging>/.conversion-staging-journal.jsonl` (`crates/converter/src/cache.rs:24`, `62-76`). `run_async` then calls
`publish_directory(&staging, &config.output_dir)` (`pipeline.rs:126`), which renames the whole
staging directory over the output (`pipeline.rs:1231-1249`), carrying the journal with it. Only
afterwards is it removed, and a failure only warns: `if journal.is_file() && let Err(error) =
fs::remove_file(&journal) { eprintln!("warning: ...") }` (`pipeline.rs:131-135`; the head call site
is the same code at `pipeline.rs:115-120`). The intent is documented at `pipeline.rs:127-130`: the
journal is dropped *after* the rename so a failed publish leaves staging resumable.

On Windows the remove can fail for the same reason the project documents for the publish rename
itself ("Access is denied" while any process holds a handle): antivirus or an indexer opening the
just-moved file. Nothing retries, and nothing else removes it. The happy path is covered -
`crates/converter/tests/resume_staging.rs:254 does_not_publish_the_staging_journal` asserts the journal is absent from
the output - but the failure path is not.

**Verdict: REAL**, minor. Consequence: a hidden bookkeeping file ships inside the published asset
tree (it holds source keys, a configuration hash and hashes - no machine paths). The engine and
launcher do not read it; a user who later points `--resume-staging` at that directory would read it,
and its records describe exactly those outputs, so that is not harmful. It is a leaked file and a
violation of "the journal is not an asset".

**Minimal fix** (belongs in #31: move the journal out of staging *before* the rename, so publication
cannot carry it, and restore it if the publish fails so staging stays resumable):

```rust
let journal = StagingJournal::path_in(&staging);
let parked = config.output_dir.with_extension("staging-journal");   // beside, not inside
if journal.is_file() { fs::rename(&journal, &parked)?; }
if let Err(error) = publish_directory(&staging, &config.output_dir) {
    let _ = fs::rename(&parked, StagingJournal::path_in(&staging));
    return Err(error);
}
let _ = fs::remove_file(&parked);
```

A test cannot portably force `remove_file` to fail, so the fix should make the outcome independent
of it; `does_not_publish_the_staging_journal` then covers the invariant structurally (plus a
failure-path test if a fault hook is added).

**Draft reply.** "Agreed. The journal is only useful for a resume and should never enter the published
tree, but the current order renames staging first and deletes the journal afterwards on a best-effort
basis, so a lock on Windows (the same one that makes the publish rename fail) can leave it behind.
I'll park the journal beside the output before the rename and restore it if the publish fails, so
staging stays resumable and publication can never carry it."

---

## F31-b - #31, minor, journal error skips the worker-pool teardown (head `pipeline.rs:729`, `745-748`)

**Reachable: yes.** In `convert_kind`, both `self.journal.record(...)?` calls
(`pipeline.rs:795-796` for a converted output, `812-815` for a cache hit; the head's lines match
CodeRabbit's) return directly from the function on an I/O error. That skips:

- `cancelled.store(true, ...)` - the flag is only set in the fail-fast conversion-error branch
  (`pipeline.rs:823-836`); the workers check it at `pipeline.rs:593`;
- `rayon_handle.await` (`pipeline.rs:845-847`), which is also what drains `outcome_rx`.

`run_into` propagates the error (`.await?` at `pipeline.rs:317-318` and `333-340`), and `run_async`
then removes the staging directory for a non-resumed run (`pipeline.rs:110-115`,
`let _ = fs::remove_dir_all(&staging)`) while up to `cpu_jobs` detached `spawn_blocking` workers
(`pipeline.rs:583-724`) keep converting and writing into it (`pipeline.rs:596-653`, `702-703`). The
same defect exists in the two `return Err(error)` sites on `fs::metadata`/`hash_file` failures
(`pipeline.rs:742-747`, `762-767`), which CodeRabbit did not flag. The fail-fast *conversion* path
is the correct model: it sets the flag, keeps draining, awaits the pool, then returns the first error
(`pipeline.rs:823-851`).

**Verdict: REAL**, minor. The run is already failing, so no bad assets are published; what goes wrong
is cleanup and resource use - a stray partially-written staging tree on Windows (the removal is
ignored on failure) and workers grinding through the remaining files after the error.

**Minimal fix** (belongs in #31):

```rust
if let Err(error) = self.journal.record(&key, &staged_output(&entry, self.expected_configuration)) {
    cancelled.store(true, Ordering::Relaxed);
    if first_error.is_none() { first_error = Some(error); }
    continue;   // the loop drains the workers and the tail awaits rayon_handle
}
```

with the same treatment for the cache-hit site and the two metadata/hash `return Err` sites.

**Test.** A deterministic failure of `journal.record` (append after a successful open) is not
reachable from a normal test - the open would fail first - so this needs either a small `#[cfg(test)]`
fault hook on `StagingJournal` or a structural assertion (e.g. a test-only counter proving the pool
was awaited before `convert_kind` returns). Otherwise the existing converter tests are the only
regression cover.

**Draft reply.** "Correct: the `?` returns before the cancellation flag is set and before the rayon
handle is awaited, so with a non-resumed run the staging directory can be removed while detached
workers are still writing into it. The fail-fast conversion path is the right model - set the flag,
remember the error, keep draining, await the pool, then return it - and I'll apply it to both journal
calls (and to the two metadata/hash error returns in the same branch, which have the same problem).
A test needs a fault hook on the journal, since a normal test cannot make the append fail after a
successful open."

---

## Method notes

- Read-only: `git show`/`git diff`/`git log` on the shared object store plus file reads in
  `C:\Modding\wkj-upstream`. No `cargo` command was run; every code claim is a reading, and the
  falsifier is named per section.
- The combined checkout's line numbers are given; for F30-a/b/c the head's numbers differ because
  #31 adds the journal plumbing above that block (~32 lines by the hunk offsets).
- `tools/check_citations.py` flags `crates/converter/tests/resume_staging.rs:214` and `:254` as
  broken: #31 adds that file, so it exists in the `wkj-upstream` worktree (where both were verified
  by grep) but not in this `portal` checkout. Every other citation checks out.
- Untested-by-reading: (a) the claim that a `--invalidate-cache --resume-staging` run at the #30 head
  leaves the report at zero prunes - derived from the reuse condition shown as removed lines in the
  #31 diff and the empty `previous`; cheapest check is the test named in F30-b. (b) The F29-1
  all-90 fixture values are hand-derived, not run. (c) That a Windows handle can block the journal
  removal is an inference from the project's own documented publish failure, not reproduced.
