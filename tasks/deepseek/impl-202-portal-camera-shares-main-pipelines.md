# impl-202 - Opening a door for the first time compiles no new pipelines

Owned files: crates/engine/src/portal.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. After impl-188/189 (same file). Read `local/research/portal-first-open-hitch-2026-09-25.md` (research-197) first; it traces the hitch from source and lists the instruments (section 5).

research-197: the portal camera is a non-HDR view, so its mesh pipeline key differs from the main camera's (`TONEMAP_IN_SHADER | TONEMAP_METHOD_NONE | DEBAND_DITHER`), and every material variant a destination shows is specialized and compiled on the first open. Proposed fix: give the portal camera `Hdr` (keep `Tonemapping::None`, `DebandDither::Disabled`) so its key matches the main camera's and the main view's already-compiled pipelines are reused.

## What to do
1. **Measure first** (research-197 section 5): frame times and the pipeline cache's waiting/created counts over the first 60 frames after the first door open and after a second open, on the current code. Log them (temporary logging is fine; keep one info line per open if cheap).
2. Apply the `Hdr` change; check the portal target format and the doorway quad's sampling still give the same image (the main view tonemaps the quad; the portal image must stay linear and must not be tonemapped twice). Compare door shots before/after.
3. Measure again. If the key still differs (research-197 lists what else could: MSAA, depth prepass flags, occlusion culling), find which bit and say so. If `Hdr` is not enough or changes the image, implement research-197's fallback (a bounded warm-up render at a tiny viewport while a door is within prestream reach, same key) instead, and report the cost.

## Checks
- Unit tests where logic allows (the camera bundle's components).
- Short tour passes; door shots before/after look the same; the first-open numbers before/after. Read only `local/t202*` and the report under `local/`.

## Report
The before/after first-open numbers, what differed in the key, the change, test names, the tour's last line.
