# Goal
Short design analysis (no code): could the converter merge the object-LOD meshes of each LOD chunk (per material) so a loaded LOD chunk is a handful of entities instead of about 900? Phase 1 track; Team Lead's question from Phase 2's door-crossing trace.

# Context
- Trace: `local/research/engine-foundations/analysis-1130-a14-crossing-trace-2026-10-09.md`. At one door crossing, `plan_lod_chunks`' command flush despawns 158,292 entities from LOD chunk roots in one frame (about 600 ms traced, 150-190 ms untraced), each firing 4 observers; about 900 entities per LOD chunk.
- Converter LOD stage: `crates/converter/src/lod/` (terrain LOD #105 and whatever object LOD it writes). Output in the converted data: `lod/<worldspace>/...` payloads (8.3 GB, 4,639 chunks in `lod_chunks`, tiers 4/8/16; `local/research/fun/p1-vgl-lod-baseline-2026-10-09.md`). The converted data also holds 10,782 baked object-LOD GLBs under `meshes/terrain/<world>/objects/` (15.0M triangles; `local/research/fun/p1-vg1-mesh-census-2026-10-09.md`).
- Engine side: `crates/engine/src/streaming/lod.rs` (`plan_lod_chunks`, spawn and despawn of chunk roots and children).
- Read in this checkout (branch portal): `C:/Modding/Mudcrab/mudcrab`. Converted data, read only: `C:/Modding/Mudcrab/SkyrimConverted-portal-rooms` (`skyrim_world.db` tables `lod*`, folder `lod/`).
- Upstream: #106 (Chef's static-object chunks) and #203 (cluster LOD idea). Read them with `gh pr view <n> --repo Mudcrab-Team/mudcrab --json title,body,files,state` / `gh issue view 203 --repo Mudcrab-Team/mudcrab --json title,body,comments` (try both pr and issue for each number).

# Answer
1. **What one LOD chunk spawns today**: which entities per chunk (terrain mesh, object-LOD instances, per-primitive children, lights?), where in lod.rs, and the measured or computed count per chunk for a few real chunks (pick 3 Tamriel chunks of different tiers from the DB and payloads). Is the ~900 mostly object-LOD instances, per-primitive mesh children, or something else?
2. **What merging per chunk per material would cut**: entity count per chunk before/after (estimate from the data), and therefore the despawn and spawn cost (scale the trace's per-entity observer cost). Also draw calls and culling granularity lost (one merged mesh per material per chunk cannot be culled per object).
3. **Converter cost**: where in the LOD stage the merge would go, what it needs (vertex transforms per instance, per-material batching, index widths, bounds), extra conversion time (rough), and data size (instances are references today vs merged geometry duplicated per chunk: estimate bytes for the same 3 chunks and extrapolate to 4,639). Does it change #185 cell keys / per-chunk content hashes?
4. **Conflicts**: with #106 (what does it chunk, and would a per-chunk merge overlap, replace or sit on top of it?) and #203 (cluster LOD per mesh: a merged per-chunk mesh is not per-mesh, so does it fight a later cluster path, or is it a stopgap that cluster LOD replaces?).
5. **Alternatives worth one line each**: engine-side (despawn spread over frames, despawn without the 4 observers, one parent with GPU instancing) vs converter merge. Facts only; the choice is Team Lead's and the user's.
6. Foundations check: what shortcut would each option take, and what would break for a mod or another worldspace?

# Done when
A note `local/research/fun/p1-lod-chunk-merge-analysis-2026-10-09.md` (under 900 words, cited file:line, numbers marked measured or estimated) and one INDEX row in `local/research/INDEX.md`. No code changes, no cargo, no engine runs.

# Report (under 300 words)
The answers to 1-5 in one line each, and the note's path.
