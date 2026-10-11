# Goal
Fix the confirmed pre-open review findings on open draft PR #99 (the depth prepass tests vertex alpha like the main pass). Track: Phase 2.

# Context
- Worktree `D:/Mudcrab/worktrees/p2-prepass`, branch `phase2/prepass-vertex-alpha-v2` (checked out at the PR's current upstream head 54eb94b3; do not switch). Use the worktree's own `target/`. This PR is already open upstream (#99); you make LOCAL commits on top of its head. Never push. Never run engine.exe.
- Reviews to act on (read them): `C:/Users/Brannock/.claude-deepseek/agents/runs/review-670-prestage-open-99.1/report.md` and `C:/Users/Brannock/.claude-deepseek/agents/runs/review-670-mimo.1/report.md`. The PR text the reviewers read is `C:/Modding/Mudcrab/mudcrab/local/review/pr-open-99/pr-text.md` (do not edit it; list the PR-text corrections you recommend in your report instead).
- Verify every finding against the code before fixing; skip any that is wrong and say why.

# Fixes
1. The patched WGSL is only string-matched; nothing compiles it. Add a test that composes or at least parses the patched prepass functions shader with naga / naga_oil as Bevy does (the crates are already in the dependency tree through Bevy; no new direct dependency unless unavoidable: if it needs one, stop and report instead), or, if that is not practical, an app-level test that the Shader asset is replaced and a check that the prepass VertexOutput declares `color` under `VERTEX_COLORS` and `output_color` is `vec4<f32>` in Bevy's source.
2. `#ifdef VERTEX_COLORS` is spliced mid-line (indented) while `#endif` is at column 0: make both directives start at column 0.
3. The test that reads Bevy's source from the local cargo registry panics when the file is missing or `CARGO_HOME`/`USERPROFILE`/`HOME` are unset: skip cleanly in those cases (print why).
4. If the embedded shader path is never found, `done` is never set and nothing reports it: after a bounded number of frames, log an error once and stop trying.
5. `shaders.get_mut` failure loses the patch silently: log an error.
6. Assert in the test that the inserted multiply comes after the line that assigns the textured base colour to `output_color`.

# Scope
The files the findings name, and their tests. No new dependencies.

# Done when
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run --workspace` pass (the test `app::tests::streaming_fixture_loads_its_interior_by_id...` is load-sensitive on main; if only it fails, rerun it alone and report).
- Commits in upstream style (`fix(engine): ...`), trailer `Co-Authored-By: DeepSeek V4.1 <noreply@deepseek.com>`, files staged by name. No push, no amend.

# Report
Commit ids; each finding: fixed / skipped (why); the PR-text corrections you recommend, as exact old -> new sentences; test count. Under 400 words.
