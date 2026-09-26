# Phase 2: Core Engine Runtime & Vercidium Renderer (`engine`)

> **Status: Acceptance pending.** Runtime, real-asset closure, HZB/indirect-renderer conformance and automated gates are implemented. The three-repetition target-hardware campaign, approved baseline, and signed visual review remain required before completion.

> **Goal:** Build the core Bevy 0.19+ engine runtime to achieve zero-loading-screen spatial streaming and high-efficiency GPU instanced rendering for massive Skyrim render distances.

---

## 🎯 Key Deliverables & Specifications

### 2.1 Bevy ECS Architecture & World Initialization

- Phase 2 workspace design (`launcher`, `converter`, `shared`, `engine`). The isolated `scripting` crate remains a Phase 3 deliverable.
- Bevy 0.19 ECS components (`FormId`, `CellRef`, `WorldTransform`, `MeshHandle`, `MaterialHandle`).

### 2.2 Multi-Threaded Cell & Spatial Streaming

- Asynchronous background frustum queries against `skyrim_world.db` using the **Hybrid Spatial Query Module** (normalized exterior R-Tree + $O(1)$ interior `cell_id` lookups).
- Zero-CPU heightmap loading via `mmap` zero-copy `rkyv` buffers (`cell_cache.rkyv`).
- Dynamic sub-millisecond cell load/unload pipeline across interior/exterior boundaries with zero loading screens.

### 2.3 Vercidium-Style GPU Instanced Indirect Renderer

- Bevy GPU preprocessing, batching and indirect draw buffers backed by `wgpu`.
- Batching hundreds of thousands of static world instances (foliage, trees, rocks, architecture) into GPU buffers.
- Frustum and HZB occlusion culling through Bevy 0.19's native render-world path.

### 2.4 Terrain & Water Shader Pipeline

- Multi-layer PBR terrain shader (up to 6 splat texture layers per land cell).
- Dynamic water surface rendering with planar reflections and flow maps.

---

## Implemented Runtime Design

- `shared` owns the versioned database/cache contract consumed by both converter and engine.
- `cell_cache.rkyv` v3 stores decoded 33×33 heights, packed normals, vertex colors, terrain layers, splat weights, and water metadata.
- A bounded background worker owns the read-only SQLite connection. The Bevy main thread only submits cell requests and commits a configurable number of completed payloads per frame.
- Exterior streaming uses cell-grid selection followed by normalized `exterior_spatial` R-Tree lookup. Interiors use the direct `cell_id` index.
- When an exterior grid contains both Skyrim's persistent reference cell and a terrain cell, the
  runtime selects the LAND-bearing cell for terrain while the spatial index still contributes the
  persistent references. This prevents the persistent `(0,0)` cell from masking the real landscape.
- Cell lifecycle states prevent duplicate work and use separate load/unload radii for hysteresis.
- World coordinates are represented as cell grid plus local position; render roots are rebased around the camera to preserve `f32` precision.
- Bevy 0.19 GPU preprocessing provides material/mesh batching and indirect draw commands. `DepthPrepass` and `OcclusionCulling` are active. Runtime proof records GPU preprocessing/culling state, indirect drawing, occlusion views, HZB views, indirect phase buffers, batch sets and proof frames.
- Terrain uses a PBR material extension with six KTX2 layers and vertex splat weights.
- Water uses animated flow normals, an offscreen reflected camera, Fresnel composition, and a separate render layer to prevent recursive reflection.
- The launcher starts the sibling engine binary and passes the canonical converted-assets path.

## Explicit phase boundary

Phase 2 renders the static world, terrain and water. Character skinning and animation, gameplay
particles/effects, and collision/physics are intentionally outside this phase: animation and
particles remain Phase 4 gameplay work, while collision coverage remains tracked with the Phase 1
asset pipeline and Phase 4 physics integration. Their absence must not be represented as a static
renderer capability or as a failure of the Phase 2 asset closure.

Runtime static-world discovery therefore excludes sky/weather geometry, editor marker meshes and
the `Effects`/`Markers` trees. Those records require their dedicated animation, particle or editor
semantics and must not be rendered as ordinary world statics by the Phase 2 loader.

## Running

```text
cargo run -p engine -- --assets modern_assets
```

Useful runtime options include `--worldspace`, `--grid-x`, `--grid-y`, and `--stream-radius`.

The asset-independent renderer benchmark is:

```text
cargo run -p engine -- --benchmark-only --synthetic-instances 250000 --benchmark-frames 600
```

The benchmark uses one mesh/material pair so Bevy's GPU preprocessing can exercise the indirect instancing and visibility path without redistributing Skyrim assets.

### Inspecting a running engine (Bevy Remote Protocol)

The engine can serve the [Bevy Remote Protocol](https://docs.rs/bevy_remote) so developers and
tools can query entities, components and resources while it runs. It is off by default: build with
the `remote` feature and start with `--remote`.

```text
cargo run -p engine --features remote -- --assets modern_assets --remote
```

`--remote-port <port>` picks another port (and implies `--remote`); the default is Bevy's 15702.
Bevy also serves the render world on 15703, which cannot be changed, so a second engine running
with `--remote` at the same time cannot serve its render world, even on another `--remote-port`. An engine built without the feature refuses `--remote` with an
error saying to rebuild with `--features remote`.

```text
curl -s -X POST http://127.0.0.1:15702 -H "Content-Type: application/json" -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"rpc.discover\"}"
curl -s -X POST http://127.0.0.1:15702 -H "Content-Type: application/json" -d "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"world.query\",\"params\":{\"data\":{\"components\":[\"engine::world::components::FormId\"]}}}"
```

Only reflected types are visible through the protocol: Bevy's own (`Transform`, `Name`, ...), the
engine's world components (`FormId`, `CellRef`, `WorldPosition`, `ExteriorCellGrid`, ...) and the
`StreamingMetrics` and `RenderOrigin` resources. On the portal branch also the `ActiveCell`
resource and the `LoadDoor`, `DoorState`, `DoorLeaf`, `StreamedCellKey`, `DistantTerrainRoot`,
`Player`, `PlayerMode` and `CrossingHeld` components (for example
`engine::doors::DoorState` in a `world.query`).

The server listens on 127.0.0.1 only and has **no authentication**: anything that can reach the
port can read and change the world. Use it for local development only.

## Integration and acceptance

The full asset-integrity, real-world, stress, stability, and performance procedure is documented in
[`02-integration-and-acceptance.md`](02-integration-and-acceptance.md). The PowerShell runner makes
all non-visual gates reproducible and emits JSON reports suitable for CI or release evidence.

The reproducible profiling campaign, regression policy, per-run bundles, and GPU counter capability
reporting are documented in [`02-profiling.md`](02-profiling.md).

Final release verdicts and their evidence package are documented in
[`02-acceptance.md`](02-acceptance.md).

## Compatibility

Phase 2 requires database schema version 4, converter manifest schema 14, and cell cache version 3.
Older or incomplete assets are rejected and must be reconverted through the launcher or converter CLI.
