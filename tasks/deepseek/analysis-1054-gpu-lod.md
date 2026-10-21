# Goal
Answer: can the converter's terrain LOD stage use the GPU, and how, without lowering quality? The stage
takes ~30 min quiet: per chunk it bakes a 1024x1024 albedo atlas (up to six LAND layers blended in linear
light, VCLR tint) and encodes it to UASTC KTX2 with three mips on the CPU (basisu, one thread per call).
Chef's upstream PR #167 already removes a duplicate encode and adds chunk reuse; this is about what remains.

# Context
- Stage analysis: `local/agents/runs/analysis-1052-lod-stage-time.1/report.md` (read first).
- PR #167 source (the code any GPU path would build on): `C:/Modding/Mudcrab/worktrees/review-pr167`
  (`crates/converter/src/lod*`, `terrain*`, `albedo*`, `texture.rs`).
- Existing GPU UASTC encoder used by the texture stage: `crates/converter/src/texture_gpu/` (mod.rs, `uastc_encode.wgsl`,
  `run_batcher`, `takes()`), in both this folder and the PR checkout. Find its quality checks, tests,
  fallbacks, and how its output compares with basisu (any parity or PSNR tests, docs, research notes:
  `rg -i "texture_gpu|gpu uastc|uastc_encode" local/research/INDEX.md docs`).

# Answer
1. Atlas encode on the GPU: can `encode_rgba_mips` route through texture_gpu (in-memory RGBA, not a file
   path)? What changes (API, batching across chunks, threading with rayon, device sharing with the texture
   stage)? Is its UASTC quality proven equal to basisu at the level used, and if not, what parity test is
   needed (decode, PSNR/FLIP per chunk, worst case)?
2. Atlas bake on the GPU: is the blend (six layers, bilinear opacity, linear-light, tint, sRGB) a good compute
   shader? Data upload cost (source DDS, per-cell grids), exactness (float rules) against the CPU result, and
   whether bake + encode could stay on the GPU without a readback in between.
3. Expected gain, from counts in the report (4,639 chunks), rough and labelled as estimates.
4. Fallback: machines without a usable adapter, CI, determinism and the build identity (GPU vs CPU bytes
   must not create two "valid" generations silently).
5. Foundations check (no shortcuts, right layer, flexible, moddable, aligned) and ownership: LOD is Chef's
   area (PR #167); texture_gpu is whose? (git log). Recommend: what to propose to Chef, in what order, after #167.

# Scope
Read only. Change nothing. Cite path:line for every claim; label estimates.
