# Goal
Design the engine half of upstream issue #104 phase (a): placed movable objects (clutter) become dynamic Rapier bodies with their authored collision, resting on terrain and statics and colliding with the player, the debug tankards and each other. Read-only design with file:line citations, against upstream main, so a coder can implement it in one PR.

# Context
- Checkout to read: `D:/Mudcrab/worktrees/p2-console` is upstream main a896b903 plus an unrelated console commit; read `crates/engine/src/physics.rs`, `streaming.rs`, `world/database.rs`, `app.rs`. The converter half is on `D:/Mudcrab/worktrees/p1-physics` (branch phase1/physics-bodies 60379b3d): read `crates/shared/src/collision.rs` there for the new `bodies` data, and the data contract `C:/Modding/Mudcrab/mudcrab/local/research/p104-physics-body-contract.md`.
- Issue #104 (Chef's research), summary: Skyrim classifies a body as dynamic by motion system + quality + mass; clutter defaults friction 0.5, restitution 0.4, linear damping 0.0996, angular damping 0.0498; inertia carries length squared; a dynamic body needs convex colliders; grab and constraints are later phases. "V19 Rapier panic" is mentioned as blocking controller-to-body impulses: find what that refers to in physics.rs.
- Agreed engine-side rules: spawn a dynamic body only when the body's `kind` is "dynamic" AND the placed reference's base record type (`records.record_type`, `ReferenceRow::base_record_type`) is a movable type; everything else keeps today's fixed collider; never both for one reference. Non-convex shapes on a dynamic body get a convex hull of their vertices, counted and logged. Read the collision extras under both keys `openSkyrimCollision` and `mudcrabCollision`; ignore unknown body fields. Inertia arrives as a full symmetric 3x3; convert with parry's `MassProperties::with_inertia_matrix`. The stable object ID is the reference's load-order-resolved FormId.
- Bevy 0.19, bevy_rapier3d 0.36 (check the vendored/registry source for exact APIs).

# Questions
1. Today's path: where collision extras are read and colliders spawned for placed references (file:line), how they're parented under cell roots, and how the debug tankards are made dynamic (what to copy).
2. Origin rebasing: the engine rebases the world origin (`origin_rebases` metric). How do Rapier bodies survive a rebase today (static colliders, tankards)? What must a dynamic body do so a rebase doesn't teleport it or wake/launch it? Cite the rebase code.
3. Cell lifecycle: a dynamic body that rolls out of its cell, and cell unload/despawn (including the paced unloads): what happens to the body today if it's a child of the cell root, and what's the smallest correct rule for phase (a) (e.g. despawn with its cell, resting pose not persisted, since persistence is #90)?
4. Which base record types count as movable in Skyrim (MISC, WEAP, ARMO, BOOK, INGR, ALCH, KEYM, AMMO, SLGM, SCRL, ...)? Cite a source (UESP, xEdit or nif.xml-adjacent docs if available offline; otherwise say it needs a web check).
5. Rapier mapping: mass properties from mass + COM + tensor; damping; friction and restitution (combine rules vs Havok's); max linear and angular velocity (does bevy_rapier have them, or does it need a clamp system?); deactivator to sleep; CCD for small fast bodies; collision groups so dynamic clutter collides with terrain, statics, the player's controller and tankards.
6. Performance: how many dynamic bodies can a loaded cell grid hold (use the census: 1,911 dynamic of 11,976 bodies install-wide), and whether bodies should only be simulated near the player.
7. File plan and test plan in the style of the existing physics.rs tests (headless fixture app), including a fixture case the dummy-content crate would need (name it; Phase 1 owns dummy-content).

# Scope
Read only. Change nothing. Run only git read commands and file reads.

# Done when
Every question has an answer with file:line citations, or an explicit "needs a web check" with the exact question.

# Report
Summary first, then one section per question, then risks. Under 1,500 words.
