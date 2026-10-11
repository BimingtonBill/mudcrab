# Goal
Prove upstream issue #188's engine half (`--room-culling`) on real data: rendering interiors with room culling on must give the same image as with it off. Measure how much it hides.

# Context
- Two local branches:
  - `phase2/room-culling` (fdf2a9b1): stacked on phase1/room-graph d4f8381d; adds `--room-culling`, which culls the main camera's VisibleEntities by room and portal.
  - `phase2/interior-shots` (01b2a732): stacked on PR #128 386267dc; lets `--shots` render poses inside interior cells.
- Neither branch takes the other. You test them together in a THROWAWAY branch:
  - Create worktree D:/Mudcrab/worktrees/p2-roomcull-image on a new local branch `test/room-culling-image`, from phase2/interior-shots, then `git merge --no-ff phase2/room-culling`.
  - Resolve conflicts keeping both sides' behaviour, and list each conflict and how you resolved it.
  - This branch is never sent anywhere.
  - Create OWNER.txt in the worktree first: owner Phase 2 Dev, purpose throwaway #188 image test, class A, keep-until date:2026-10-09, links no.
- Build folder: D:/Mudcrab/builds/p2-intshots (already built for interior-shots; pass `--target-dir`). Use `--profile quick --bin engine` for the engine.
- Data: D:/Mudcrab/SkyrimConverted-p1rooms (read-only).
  - It holds world DB schema 7 with the §15 room tables (spec: docs/specs/converters/db-schema.md §15 on the room-culling branch).
  - D:/Mudcrab/p1rooms-room-cells.tsv lists the 292 interiors with rooms.
  - Query the DB read-only with Python sqlite3 to place cameras.
- An earlier proof shots file is in D:/Mudcrab/scratch/p2-intshots-proof/proof_shots.json. The format is in docs/specs/engine/reference-shots.md.

# Work
1. **Shots file:** about 12 interior poses. Write it in D:/Mudcrab/scratch/p2-roomcull-image/ (create OWNER.txt there first; class A, keep-until date:2026-10-09).
   - Cells: BleakFallsBarrow01 0x000371DE, Ansilvund01 0x000152C0, WhiterunDragonsreach 0x000165A3, NightcallerTemple01 0x0009AA4A.
   - Three poses per cell:
     - (a) in the middle of a room, looking at a doorway that leads on;
     - (b) standing in a doorway;
     - (c) looking at a wall with rooms behind it.
   - Plus one interior without rooms as a control (e.g. RiverwoodSleepingGiantInn; find its id in `cells`).
2. **Render each pose twice** with the same flags (`--lights` included), once without and once with `--room-culling`.
3. **Compare.**
   - Per pose: count differing pixels (threshold per channel, e.g. > 8/255), plus the maximum difference. Use Python with Pillow and numpy.
   - Expected: identical or near-identical. Any real difference means culling hid something visible: report the pose and save a diff image.
4. **Measure what culling hides:** the main camera's Mesh3d visible count with culling off and on per pose. Add a debug log line in room_culling.rs on the throwaway branch only, or use an existing counter.

# Rules
- Never use `git stash`: the stash list is shared by all worktrees.
- Commit only on the throwaway branch, never on the two source branches. No push.
- Run `df -h /d` before each build. Below 15 GB free, stop and report.
- Run engines in the foreground, one at a time, and kill only your own engine PIDs. No timing or benchmark runs.
- Run no background hold, wait or sleep commands.
- If the merge needs more than small conflict fixes, stop and report instead of rewriting either branch.

# Report (under 500 words)
- The merge conflicts and how you resolved them.
- The shots file path.
- A per-pose table: cell, pose kind, differing pixels, max diff, visible meshes off/on.
- Diff image paths for any pose that differs.
- Your conclusion: is `--room-culling` image-safe on these interiors? If not, the likely cause (portal clipping, the open-distance margin, axis or rotation mapping, membership).
