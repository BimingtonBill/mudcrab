# Goal
Review a local branch for upstream issue #187 (Mudcrab, Rust/Bevy 0.19): a `ViewPolicy` layer that bounds the cost of secondary cameras (the water reflection camera first) and refreshes them after discontinuities. Decide whether it is correct, faithful to its brief, and safe to build on. Also find what used to work and now fails.

# Context
- Your working folder is the review folder:
  - `delta.diff` is the change from upstream main 2c11897b to the branch head 774aac9b; `commits.txt` lists the commits.
  - `before/` and `after/` hold the old and new copy of every touched file. Read code only from there.
- In `reviews/`:
  - `brief.md` is the coder's brief;
  - `design.md` is the design study it implements (commits 1 and 3 of its plan);
  - `coder-report.md` is the coder's own report: what it claims, and what it skipped.
- Bevy 0.19 sources, for checking claims about `Camera::is_active`, render targets, `TransformHelper`, `Projection::compute_frustum` and schedule sets: C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/.

# What to check
1. **Behaviour today.**
   - With the default (baseline) preset, the water reflection looks and costs exactly as before: every frame, same size, same camera order.
   - Nothing about the main camera changed.
2. **The policy.**
   - `apply_view_policies` is the only writer of `is_active` for managed cameras.
   - Every-other-frame is relative to the last admission.
   - The staleness deadline and demand reopening force a render.
   - Without demand, the camera deactivates and keeps its image.
   - The image handle stays the same across frames; it is resized only when the policy changes.
3. **Invalidation.**
   - The origin, `CameraSpace`, projection/target size, water height and accumulated pose jumps are compared against the last *scheduled* pose.
   - A dirty view without demand stays dirty.
   - `InvalidateSecondaryViews` works, and the fixture sends it after the teleport pose is committed.
4. **Schedule.**
   - The systems run in PostUpdate before transform propagation and camera updates, and use current transforms.
   - No query conflicts and no one-frame lag.
5. **Generic.** `view_policy.rs` knows nothing about water (portal cameras will use it next).
6. **Tests.** They exercise the real systems, not copies of the logic, and no existing test was weakened or deleted.

# Scope
Read only. Change nothing.

# Report
- A verdict line: ready / small fixes / fix first.
- Findings, most severe first, as `file:line - issue - why it matters - fix`. At most 10.
- What you verified as correct, in short.
- What you could not check.
- Under 700 words.
