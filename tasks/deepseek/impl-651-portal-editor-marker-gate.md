# Goal
Stop the portal converter from deleting visible geometry that only looks like an editor marker by name: drop a mesh's editor-marker shapes only when the NIF also declares itself a marker (the BSXFlags marker bit), exactly main's rule (track: Portal; approved by the user 2026-10-07).

# Context
- Branch: the portal demo branch (Rust). Bug: `drop_editor_marker_geometry` in `crates/converter/src/mesh.rs` (~line 810) removes every static mesh whose name `is_editor_marker_shape(...)` matches, by name only. Main's rule (and docs/lighting-l1-compatibility.md:19) requires the name AND the BSXFlags marker bit (bit 5): see `declares_editor_marker(nif: &NifFile)` in `crates/converter/src/material.rs` (~line 242) and its use near material.rs ~1080.
- Effect: about 38 visible effect models (FXDrips01, FXDrips01_More, ...) become empty GLBs with `bounds_valid = 0` and do not draw; 454 reference rows carry placeholder ±64 bounds (Markers 141, Furniture 112, Clutter 72, Effects 38). Analysis: `local/agents/runs/analysis-899-p1-statics-bounds-axes.1/report.md` (read it).
- Origin: portal's own commit 40b22fae (`git show 40b22fae`) - read it to see why the drop was added and keep that intent for real markers.
- Main keeps flagged markers in the GLB with an excluded-geometry material but still counts them in its bounds (a main bug, tabled). Portal keeps dropping real flagged markers; keep the code shaped so it can converge with main later: reuse `declares_editor_marker` (make it `pub(crate)` if needed), do not invent a new name list.

# Scope
You own only `crates/converter/src/mesh.rs` and `crates/converter/src/material.rs` (the latter only to share `declares_editor_marker`). Change nothing else. Never use git stash.

Build rule (machine-wide): every cargo command goes through the build gate with the owner set:
`BUILD_GATE_OWNER=portal python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo ...`
(it waits for a slot, RAM and holds; just wait).

# Done when
- The drop happens only for a NIF that declares the marker bit; an unflagged model keeps its shapes (and so gets real bounds).
- Tests (converter, beside the existing editor-marker tests): (1) an FXDrips01-like model - a shape named like a marker, NO BSX marker bit - keeps its geometry and has valid bounds; (2) a real flagged marker (name + bit) has the shape dropped as before; (3) a flagged NIF whose other shapes are ordinary keeps those.
- `cargo fmt --all -- --check`, `cargo clippy -p converter --all-targets --all-features -- -D warnings`, `cargo nextest run -p converter --all-targets` pass (through the gate). Check `df -h /c` first; below 15 GB, stop and report.

# Report
What changed (file:line), how the BSX flag reaches `drop_editor_marker_geometry`, tests added, check results, and the foundations check: (1) no shortcut (main's rule, not a list); (2) right layer; (3) data-driven (the NIF's own flag, any mod's models); (4) moddable (a mod's model with a marker-like name but no flag is kept); (5) aligned (correct visuals on portal). Answer: what shortcut does this take, and what would break for a mod or another worldspace?
