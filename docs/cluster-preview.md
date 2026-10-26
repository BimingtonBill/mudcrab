# Cluster geometry preview

This branch is a working preview of Mudcrab's clustered geometry path: every static object and
terrain is converted into GPU clusters (meshopt) and drawn at full detail at every distance, with no
object LOD, billboards or stand-ins. It is the integration branch the cluster work is developed on,
shared so others can build on it before the reviewed PR stack lands. Expect it to be rebased.

## Try it

1. Build: `cargo build --release -p launcher -p converter -p engine`.
2. In the launcher, tick **Clustered geometry** before converting, then convert. Without the
   launcher: `converter ... --lod-mode cluster` (see `converter --help`). A dataset converted without
   clusters must be reconverted.
3. Press Play. The launcher passes `--clusters <package> --cluster-terrain` to the engine when the
   converted output has a cluster package. The loading screen waits until clusters are ready.

## Where the code is

| Area | Path |
|---|---|
| Cluster builder (meshopt) | `crates/meshopt-cluster` |
| Payload format | `docs/adr/0013-cluster-lod-payload-format.md`, `crates/shared` |
| Conversion | `crates/converter` (`--lod-mode cluster`, cluster stage) |
| Launcher option | `crates/launcher/src/conversion` |
| Engine rendering, culling, ownership, streaming | `crates/engine/src/cluster`, `crates/engine/src/streaming` |
| Bevy renderer changes | `vendor/bevy_render-0.19.0`, `vendor/bevy_pbr-0.19.0`, `vendor/bevy_core_pipeline-0.19.0` |
| Design notes and measurements | `docs/design/cluster-*.md`, `docs/evidence/` |

## Known limits

- Performance: standing views run at about 120-127 fps on an RTX 3090 (target 140). Flying still
  hitches at cell crossings (about 40 frames over 33 ms per test route); work on this is ongoing.
- At a render-origin rebase (cell crossing) TAA history is rejected for a few frames; a fix is in
  progress.
- Snow-covered rocks render flat white or grey: a known snow-shader issue outside the cluster work.
- Apple Silicon and other GPUs are untested.
- Some development-only verification code (`gpu-reuse-verify` feature, `tools/gpu-reuse-verify`) is
  included; the PR stack leaves it out.
- The branch is behind `main`; it is not a release.
