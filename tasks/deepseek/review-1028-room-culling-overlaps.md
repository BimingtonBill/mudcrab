# Goal
Review the overlap-edge commits on the local branch `phase2/room-culling` (Mudcrab, Rust/Bevy 0.19, upstream issue #188, engine half: interior room-and-portal culling, off by default). Decide whether they are correct and conservative, meaning culling never hides something visible, and whether they introduce new problems.

# Context
- Your working folder: `delta.diff` (fdf2a9b1 to 9d30957e, see `commits.txt`), and `before/` and `after/` copies of `crates/engine/src/room_culling.rs`.
- `reviews/context.md` says why each commit exists.
- Read code only from the folder. The worktree D:/Mudcrab/worktrees/p2-roomcull is readable if you need other files.

# What to check
1. **Separating-axis test (`rooms_overlap`).**
   - The axes are correct (3+3 face axes and 9 cross products).
   - Degenerate cross products are handled.
   - The tolerance is applied once along each axis.
   - Non-finite input is handled.
2. **Thick-portal narrowing.**
   - Is region ∩ rect(A) ∩ rect(B) really a superset of what can be seen through A∩B?
   - The projection of a box straddling the near plane.
   - A box containing the camera projects to the full screen.
   - The widening cap still only widens.
   - No infinite walks: cycles through overlap edges and links.
3. **Loader.**
   - Overlap edges are kept separate from XLRM links.
   - Pairs that are already linked are not doubled.
   - The rotation comes from the shared conversion.
4. **Cost.** O(rooms²) per cell load, and the per-frame walk cost with many overlap edges (Ansilvund has ~28 pairs; SwindlersDen ~57 without other edges).
5. **Tests.**
   - They assert the behaviour.
   - No test was weakened: three fixtures were re-spaced for the 8-unit tolerance. Check that the assertions are unchanged.

# Scope
Read only. Change nothing.

# Report
- A verdict line: ready / small fixes / fix first.
- Findings as `file:line - issue - why - fix`. At most 8.
- What you verified.
- Under 500 words.
