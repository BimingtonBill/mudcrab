# impl-226 - Why an open door still costs ~1.6 ms a frame when its doorway is not drawn

Owned files: crates/engine/src/portal.rs,crates/engine/src/lights.rs,crates/engine/src/transition.rs (only what the fix needs)

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. `local/research/portal-bench-2026-09-26.md` (release, alone, 8 dense doors): with the door open but the portal camera OFF (open-behind, open-occluded: portal_active 0%), frames cost +1.6-1.8 ms, GPU +0.3 ms and CPU +0.17 ms over the same door closed. Candidates: the destination's cells stay resident with shadow casters that the MAIN sun's cascades still draw (are layer-2 meshes excluded from the main sun's shadow views? impl-177/191 set shadow-view layers), the destination's point lights still enabled (impl-200's 16-light share while no portal draws?), the portal sun still casting (its cascades exist while the camera is inactive?), the mirror leaf and hidden far door, extraction cost of the resident destination, impl-225's 2 s destination hold.
Find which, with numbers: use `--portal-bench tools/bench/portal_bench_doors.json` (quick build is fine for attribution; say so) and Bevy's per-pass GPU diagnostics, toggling each candidate. Fix what should not cost anything while the doorway is not drawn (e.g. disable the portal sun's shadows and the destination lights while the portal camera is inactive, keep the cells resident). Keep the first-open hitch fix (impl-202) and impl-225's hold behaviour.
Checks: unit tests for any rule; before/after bench (quick) per state; short tour passes; shadows after a quick round trip still hold. Report the attribution table and the fix.
