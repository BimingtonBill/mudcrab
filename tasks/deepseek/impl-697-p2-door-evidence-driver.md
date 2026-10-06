# Goal
Track: Phase 2, LOCAL EVIDENCE ONLY (never pushed, never part of a PR). A scripted driver that exercises the real
door crossing of branch `phase2/door-preload` on real data: stand where a player stands, face a load door, press
E through the real input path, save the frames, cross back the same way, exit. The lead turns the frames into a
clip and reads the crossing's own INFO lines for timing.

# Context
- Base `af797aeb` (branch phase2/door-preload). Make a LOCAL branch: `git -C C:/Modding/MudcrabProject
  worktree add -b local/door-evidence D:/Modding-overflow/wt-697 af797aeb`; target
  `CARGO_TARGET_DIR=D:/Modding-overflow/wt-targets/door-evidence` (keep it: the lead runs the binary).
- Crossing code: `crates/engine/src/door_crossing.rs` (activation = E with the camera ray against the door's
  bounds within 200 units; fade; INFO line `door crossing: fade-in starts` with `milliseconds=`),
  `physics.rs` `TeleportPlayer`, doors: `crates/engine/src/doors.rs` (`LoadDoor`).
- Data: `D:/Modding-overflow/p2main-doors` (main conversion + door_links). Sven's house, Riverwood:
  exterior door 0x1CBB0 at Creation (20669.5, -46394.5, -122.1), grid (5,-12), worldspace 60 (0x3C); its link
  arrives inside at (-510.2,-198.9,-16.0) in interior cell 117636 RiverwoodSvensHouse. The inside door 0x1CBAF
  at (-511.5,-254.9,-16.0); its link arrives outside at (20644.0,-46318.4,-121.9), yaw -0.483 rad.

# Do (all in one new file `crates/engine/src/door_evidence.rs` plus a registration line; keep it small)
1. Enabled only by env var `MUDCRAB_DOOR_EVIDENCE=<out dir>`; otherwise does nothing.
2. Steps: wait until the start area is streamed and nothing pending (StreamingMetrics); place the player
   (TeleportPlayer, feet) at the outside arrival point of 0x1CBAF's link, turned to face door 0x1CBB0 (yaw from
   the vector to the door); wait ~1 s; press E by writing the real `ButtonInput<KeyCode>` press for one frame
   (the cursor-captured gate in door_crossing.rs must pass: set what it reads, say how); save one PNG per frame
   (Bevy `Screenshot::primary_window()`, numbered) from 0.5 s before E until 1.5 s after the crossing's fade-in
   completes; then inside, face door 0x1CBAF from where the crossing placed the player and do the same back
   out; then exit the app (AppExit). Log one INFO line per step.
3. Fail loudly (ERROR + exit non-zero) if a door is not found within 30 s or a crossing does not complete in 15 s.
4. Build `cargo build --profile quick --bin engine` in your target dir; do not run the engine window (the lead
   does). fmt only; no need for clippy/tests beyond `cargo check`.
Commit on the local branch with subject `local: door crossing evidence driver (never push)`.

Rules: never git push; never touch the phase2/door-preload branch itself; no background hold/wait/sleep
commands (foreground builds, timeout up to 600000 ms); never kill engine.exe by name.

# Report
Commit hash, the exact command line to run it (start flags for grid (5,-12) worldspace 0x3C with `--assets
D:/Modding-overflow/p2main-doors`, the env var, the binary path), what it logs, any assumption about the
cursor-capture gate.
