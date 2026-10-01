# Goal
Track: portal. Keep every load doorway walkable once the player walks on rapier colliders. Vanilla
Skyrim closes the space behind a load door with authored collision (nobody walks through a load door;
the game teleports), but the portal's seamless crossing walks the player through the doorway. Today
the Alftand short door tour fails at stage 1: door `0x00092809` (`DwemerLargeDoorLoad01`, interior
Alftand01) opens, but the authored collision mesh of the facade `0x00092808`
(`Dungeons\Dwemer\Facades\DweFacadeHallSmExBg01.nif`, 16 units from the door) closes the doorway, so
"the player cannot walk in from any of the standoffs". Its render mesh has a door-shaped hole; its
authored collision does not.

# Context
- Your checkout is branch `portal-merge-main` (base b3de61a8). Commit prefix `[deepseek]`.
- `crates/engine/src/player.rs`: `WorldProbe` (rapier world half + mesh half for door leaves,
  `door_mesh_is_solid`, which also checks `in_active_space`), `RapierProbe`, `walk_step`, the door
  systems. `crates/engine/src/doors.rs`: `LoadDoor` (on the door reference's root entity, whose
  `GlobalTransform` is its placement), `DoorState` (`is_open`), `DoorLeaf`.
  `crates/engine/src/world/components.rs`: `InstanceBounds` / `ExpectedModelBounds` (check which one a
  spawned door reference carries and in which space).
- The mesh-only probe never had this problem because it ray-casts render meshes.

# Design (fixed)
1. In the composite probe, a hit from the **world (rapier) half** whose hit point lies inside the
   doorway volume of an **open** load door of the player's own space is ignored. The doorway volume is
   the door's own model bounds in world space (its oriented box from the reference's transform and
   the converted model bounds), grown by a small margin (say 32 units horizontally, not downward), and
   extended along the door's outward axis enough to cover the backing geometry right behind it (the
   facade here is 16 units away; choose and justify the depth, e.g. 64 units each side).
2. Ground support is unchanged: downward rays still stand on world colliders inside a doorway (a
   doorway's floor must hold the player). Only non-downward (wall) hits are ignored.
3. A closed door still blocks, through the mesh half (unchanged). Doors of other spaces are irrelevant
   (they are not in the active space).
4. No change for runs without world colliders (mesh probe).
5. Precompute the doorway volumes once per frame in the walk system from a query of `LoadDoor` +
   `DoorState` + `GlobalTransform` + bounds; keep the per-ray cost to a short list check.

# Scope
Owned files: crates/engine/src/player.rs, docs/handoff/impl-281-portal-walkable-doorways.md
Everything else read-only. Tests only; no engine window or GPU. The handoff note is written, not
committed.

Acceptance: cargo fmt --all -- --check, cargo clippy -p engine --all-targets -- -D warnings, cargo nextest run -p engine

# Done when
- Acceptance passes; existing player/portal tests unchanged.
- Tests (headless, `PhysicsCorePlugin`, as the existing `world_probe_tests`): a static collider wall
  across an open load door's doorway does not block a walk through it; the same wall blocks when the
  door is closed (its leaf blocks too); the same wall away from any doorway still blocks; the floor
  collider inside the doorway still supports the player; a doorway of a door in another space carves
  nothing.

# Report
Commits, the doorway volume's construction (which bounds component, axes, margins and why), tests.
