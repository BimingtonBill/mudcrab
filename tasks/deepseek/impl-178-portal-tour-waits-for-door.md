# impl-178 - The tour's walk-through presses E once and waits for the door to open before walking

Owned files: crates/engine/src/demo_tour.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user watched the automated door tour (2026-09-25) and asked whether this is intended: the player appears in front of a door, waits, "smushes up against it for a bit", then teleports back and walks smoothly through the opening door. It is not intended. The tour log shows it on the first door of every run: `stage 0: the walk from 60 units in front of the door is going nowhere (grounded=true); walking in from 320 units instead`.

## Why it happens

In `Phase::WalkThrough` (`crates/engine/src/demo_tour.rs`, around lines 1300-1310) the tour presses `E` (released and re-pressed every frame while the door is not open) **and holds `W` at the same time**. The door is solid until its swing is complete, so from the 60-unit standoff the player walks into the swinging leaf and makes no progress; after `WALK_STUCK_SECONDS` (2.5 s) the standoff ladder (`WALK_STANDOFFS = [60, 320, 160]`) moves the player back to 320, by which time the door is open, and that walk succeeds. The standoff ladder is meant for spots that are inside rock or over a drop, not for this.

Also new since impl-176 (merged just before this task): `E` on an **open** door now **closes** it, and `E` on a load door whose far side is still loading holds the opening, with a second `E` **cancelling** that hold. So pressing `E` every frame now alternates hold/cancel on a loading door, and must never be pressed on a door that is already open (for example a door the crossing opened at the arrival).

## What to build

In the walk-through, do what a player does:
1. Stand at the standoff (as now). If the door is closed and not an auto-load door, press `E` **once** (a fresh press, as the code already arranges with release+press) as soon as the door is in `E` range, and do not press it again while the door is `Closed` but held for loading, `Opening` or `Open`.
2. Do **not** hold `W` until the door is fully open (`Open { .. }`; use the door state the tour already reads, `door_is_open` / the `DoorState`). While waiting, the stuck/fall timers (`walk_stuck`, `walk_fell`) must not advance for "no progress", since not moving is expected. Give the wait its own time limit (for example 10 s, a constant with a doc comment); past it, FAIL the stage with a line saying the door never opened.
3. Then hold `W` and walk through as now (capture frames, standoff ladder only for genuinely unwalkable spots).
4. An auto-load door (`door.auto_load`) needs no `E`: walk as now.
5. If the door is already open when the walk-through starts (the crossing can open a door at the arrival), do not press `E` at all; just walk.

Keep the log lines' style; add one line when the door is seen fully open ("stage N: door XXXXXXXX open after S s; walking in").

## Checks

- Unit tests where the existing `demo_tour.rs` tests make it possible (pure helpers are fine: e.g. a function deciding "press E / wait / walk" from the door state and whether it was already pressed).
- `cargo build --profile quick --bin engine`, then the short tour:
  `target/quick/engine.exe --assets C:\Modding\Mudcrab\SkyrimConverted --demo riverwood --walk --demo-tour local/t178 --tour-doors 2`
  It must end `tour PASSED after 2 crossings (short tour)`, and `local/t178/tour.txt` must contain **no** "going nowhere" line. Also run the Alftand short tour (`--demo alftand --walk --demo-tour local/t178a --tour-doors 2`) and report its last line. Read only `local/t178*` under `local/`.

## Report

Files changed and why; the wait limit you chose; test names; both tours' last lines and whether any "going nowhere" line remains.
