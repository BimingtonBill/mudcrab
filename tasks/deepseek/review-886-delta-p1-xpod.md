# Goal
Review one fix commit on a local branch before it goes upstream (converter half of Mudcrab-Team/mudcrab#188, interior room graph). Be strict. Decide whether it is correct and whether anything that used to work now fails.

# Context
- Delta: `local/review/pr-room-graph/delta-xpod.diff` (commit c3802c19 on 0f2645b0). Read code only from `local/review/pr-room-graph/head/`; the checkout under `crates/` is a different branch.
- Record reference: xEdit 4.1.5f defs, `Core/wbDefinitionsTES5.pas`, REFR: `wbArray(XPOD, 'Portal Data', wbStruct('References', [wbFormIDCk('Origin', [REFR, NULL]), wbFormIDCk('Destination', [REFR, NULL])]))`. So XPOD is an array of 8-byte pairs. Every XPOD in the official masters is 8 bytes.
- What changed:
  - The XPOD remap is now `Array { stride: 4 }`.
  - `room_portals` has one row per pair, `PRIMARY KEY (ref_id, pair_index)`.
  - An older-shaped `room_portals` is dropped and recreated in `create_tables`.
  - Two new counters, `portals_with_extra_pairs` and `portals_with_trailing_xpod_bytes`.
  - Spec and design note updated.
- The author ran fmt, workspace clippy -D warnings and nextest on converter and dummy-content: 607 passed.

# Check, with diff-line evidence
1. An 8-byte XPOD gives exactly the same remap and rows as before.
2. The `Array` behaviour on odd lengths matches the existing convention for other Array tags.
3. The table migration: is it safe on every path that calls `create_tables` (the movement annotator's subset export, reused databases, a fresh database)? Could it drop data that isn't derived?
4. The counters and the snapshot are consistent with the code.
5. Would each new test fail if its fix were reverted?
6. Do the spec and the code agree?

# Scope
Read only.

# Report
Verdict line: ready / small fixes / fix first. Then findings as `file:line - issue - why - fix`, at most 5, each marked blocker / should fix / nit. Under 300 words.
