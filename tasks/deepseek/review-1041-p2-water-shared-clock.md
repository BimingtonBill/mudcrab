# Goal
Review the water shared-clock commits on local branch `phase2/water-shared-clock` (Mudcrab, Rust/Bevy 0.19, wgpu 29). The change removes a per-frame write to every water material (which made Bevy re-prepare each water cell's bind group every frame) by having the water shader read Bevy's shared `globals.time`, for the 160 fps goal (6.25 ms frames).

# Context
- Checkout: C:/Modding/Mudcrab/worktrees/p2-water. Review `git diff f40ca1df..HEAD` (4 commits: 255ceabb, 868b816d, f94b4bff, c67d6f23).
- Files: crates/engine/src/shaders/water.wgsl (new `clock: vec4` field; `select(globals.time, water.clock.x, water.clock.y > 0.5)`), crates/engine/src/render.rs (`animate_water_materials` deleted; `WaterExtension::with_pinned_clock`; `WATER_CLOCK_WRAP_PERIOD` = 12 h set on `Time<Virtual>` in `VercidiumRendererPlugin::build`; reflection camera writes skipped when unchanged; tests), crates/engine/src/app.rs (terrain/water fixture pins its clock at 1.0), crates/engine/src/physics.rs (player body yaw written only when changed).
- Bevy 0.19 source is in the cargo registry (`%USERPROFILE%/.cargo/registry/src/*/bevy_*-0.19*`). Check there how `globals.time` is produced (bevy_render globals: which `Time`, wrapped or not), whether the generic `Time` copies `Time<Virtual>`'s wrap period, and whether `init_resource::<Time<Virtual>>` in TimePlugin would overwrite a resource the renderer plugin inserted first (plugin order).
- Known compromise stated by the coder: the wave phase is not periodic, so the waves jump once per wrap (now every 12 h instead of 1 h), and the wrap change is engine-wide.

# Scope
Read only. Change nothing.

# Done when
You have verified with file:line: the uniform layout in Rust matches the WGSL struct (field order, std140/encase alignment, the `#[uniform(100)]` binding); nothing else wrote or read the old `.w` time field (search the whole repo, docs and scripts included, also for the removed profiler key `render/water_animation`); the pinned fixture still renders the same still as before (the old code used 1.0 for the fixture too); pausing (virtual time paused) still freezes the waves as before; the `set_if_neq`/guarded writes cannot leave the reflection camera stale (first frame, surface leaving and entering view, exposure change); the tests assert real behaviour.

# Foundations check (answer explicitly)
What shortcut does this take, and what would break for a mod (a mod's own water type, many water cells, a mod reading the wrapped clock) or another worldspace? Is the 12 h engine-wide wrap acceptable, or is there a clean way to make the wave phase wrap-safe (for example a per-material phase origin, or a periodic formulation) without a per-frame material write?

# Report
Verdict (accept / fix needed), findings as path:line - issue - fix, what you verified, what you could not verify.
