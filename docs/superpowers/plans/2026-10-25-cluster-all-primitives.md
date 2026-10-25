# Cluster mode all-primitives implementation plan

**Goal:** Cluster every supported static primitive in forced cluster mode, expose undrawn refused references, and rebuild the affected dataset without lowering detail.

**Architecture:** Resolve the forced cluster conversion threshold to one at the existing conversion-options seam. Preserve configurable thresholds for auto/baked products. Keep the engine's stock eligibility unchanged and report refused static references for which no distant path exists.

**Specification:** Task #2144.2, implementation brief supplied in the task thread. Base: `phase2/cluster-integration`, `1332e0d87`.

## Constraints

- No baked stand-ins, billboards, geometry caps, or silent fallbacks.
- Preserve permanent material/state refusals and existing auto/baked behavior.
- Run Cargo through the build gate as `sol-2144-2`; use only the task's target directory.
- Use gate TEST mode for engine validation, without an exclusive hold.
- Keep 15 GB free on D:. Publish nothing; commit locally with the requested attribution.

## Work

1. Add a real conversion/derivation regression for small primitives and the recorded threshold. Observe red; resolve cluster conversion settings and retry commands consistently; observe green. Check explicit threshold overrides and unchanged auto/baked behavior.
2. Add engine tests and diagnostics for refused static references beyond the grid without a draw path. Count immutable static holes in `cluster/undrawn_static_geometry` with an ERROR, and runtime-refused STAT/TREE gaps in `cluster/undrawn_runtime_refused` with a WARN. Log both at first qualification with class counts. Preserve stock permission rules; record the runtime gap as a follow-up.
3. Copy the original package's terrain/tree settings and scheduling, recover hash-matched terrain sources if necessary, and derive into a new settings-addressed directory. Preserve the old package for the lead to retire after timing the replacement. Update the manifest selector before deriving so its fingerprint binds the correct package.
4. Verify instance and refusal counts, then run gated engine TEST qualification against the rebuilt package and main28. Record time and size without making performance claims.
5. Run formatting, strict converter/engine and workspace Clippy, converter/engine and workspace nextest, and workspace doctests. Review the final diff, record evidence in the handoff log, and commit only named paths locally.

## Review focus

Check explicit cluster threshold overrides, retry recipe identity, selector/fingerprint consistency, non-static references excluded from hole counts, and permanent stock refusals excluded from missing-path counts. Other worldspaces and modded sources must follow the same data-driven rules.
