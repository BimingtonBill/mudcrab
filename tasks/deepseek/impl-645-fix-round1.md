Finish impl-645 (per-frame span series + lights/physics/shadow spans) in this same worktree. Your last run ended at the step limit with 2 compile errors (E0433, unresolved path) in the engine's lib test build; fix them first.

Build rule (machine-wide): run every cargo command through the build gate, e.g.
`python C:/Users/Brannock/AppData/Local/mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets --all-features -- -D warnings`
(it waits for a free slot and RAM; just wait).

Then check the brief's Done-when (C:\Modding\Mudcrab\mudcrab\tasks\deepseek\impl-645-portal-frame-series.md): the series row opened in `First` keyed by the timeline frame number, `frame-series.csv` written in the bundle, the bound, the tests; and the spans `lights/budget`, `shadow/relayer_mark` + gauge, `physics/step`. Add the column `portal/propagate` to the series (another coder added that span in portal.rs). Commit your work (`[deepseek] profiling: ...`, owned files only: profiling.rs, metrics.rs, lights.rs, physics.rs, shadow_layers.rs). Never use git stash.

Done when: `cargo fmt --all -- --check`, `cargo clippy -p engine --all-targets --all-features -- -D warnings`, `cargo nextest run -p engine --all-targets` pass (via the gate). Report: what you fixed, the CSV header, the frame alignment with the timeline and with `--benchmark-frame-times` rows (warm-up offset), tests, checks, and the foundations check.
