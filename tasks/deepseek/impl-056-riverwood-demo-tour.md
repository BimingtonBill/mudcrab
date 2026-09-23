# impl-056 - Make the scripted demo tour follow the demo the run started in

## Why

`--demo-tour <dir>` is the self-checking run that proves a demo works without a person at the
keyboard: it walks the route, photographs every place and door, crosses each door, and exits with
a log. It is hard-wired to Alftand: `crates/engine/src/demo_tour.rs:37` is
`pub const ALFTAND_ROUTE: [u32; 4]`, and `crates/engine/src/app.rs:1831,1836` hard-code the door
count and the "find the Alftand entrance" objective text.

A second demo now exists. `crates/engine/src/config.rs` has a `DemoStart` entry `"riverwood"`
(Tamriel, the Helgen road south-west of the village). Its route is **four houses, entered and left
again**, which is eight door crossings rather than Alftand's four one-way ones.

Read `docs/design/riverwood-demo.md` first - sections 2 and 3 are the measured data this task
uses. Do not re-derive it.

## What to build

**1. A route per demo.** Replace the bare `ALFTAND_ROUTE` constant with a named-route lookup that
the tour resolves from the demo the run started in. Keep this exact interface, because the lead's
integration and the reviewer depend on it:

```rust
/// A scripted route: the load doors to cross, in order, and the objective line shown on screen.
pub struct DemoRoute {
    pub name: &'static str,
    pub doors: &'static [u32],
    pub objective: &'static str,
}

/// The route for a `--demo <name>` run; `None` when the name has no scripted route.
pub fn route_for_demo(name: Option<&str>) -> Option<&'static DemoRoute>;

/// Kept so existing callers and tests still resolve.
pub const ALFTAND_ROUTE: [u32; 4] = [0x0001_5D48, 0x0009_2809, 0x0009_256A, 0x0006_998D];
```

`EngineConfig` does not currently keep the `--demo` name it resolved (`config.rs:269` area applies
`DemoStart` and discards the string). Add a `pub demo: Option<String>` field to `EngineConfig`,
set where `--demo` is parsed, defaulted to `None`. That field is yours; it is the only change you
make in `config.rs` and you must not touch the `DemoStart::named` match arms.

**2. The Riverwood route.** Eight doors, verified against `door_links` in the converted database
(`C:/Modding/SkyrimConverted/skyrim_world.db`) - each pair is reciprocal, so the tour enters a
house and comes back out before walking to the next:

| # | door ref | what it does |
|---|---|---|
| 1 | `0x0001_CBB0` | into Sven's House (`0001CB84`) |
| 2 | `0x0001_CBAF` | back out to Tamriel |
| 3 | `0x0001_341F` | into the Riverwood Trader (`000133C9`) |
| 4 | `0x0001_33E9` | back out |
| 5 | `0x0001_3420` | into Alvor and Sigrid's House (`000133C8`) |
| 6 | `0x0001_33FB` | back out |
| 7 | `0x0001_3424` | into the Sleeping Giant Inn (`000133C6`) |
| 8 | `0x0001_3419` | back out |

Objective line for it: one sentence naming the four houses and the `E` prompt, in the register of
the existing Alftand one.

**3. The objective counter.** `app.rs`'s `DEMO_ROUTE_DOORS` and the `Text::new(...)` in
`spawn_demo_objective` must come from the resolved route instead of being hard-coded. When the run
has no route (no `--demo`, or an unknown name), keep exactly today's behaviour: the Alftand text
and a count of 4. Change nothing else in `app.rs` - it is a shared file and the lead is editing
other parts of it.

**4. A returning route is not a descending one.** Alftand's route never revisits a place. In
Riverwood the tour leaves a house by a door that stands in the cell it just arrived in. Read
`run_demo_tour`'s phase machine (`demo_tour.rs:299` onward) and check that `FindDoor` can find a
door in the space the tour is currently in, that the screenshot and log naming does not collide
when the tour is in Tamriel four separate times, and that the walk-through capture directories are
unique per crossing. Fix what is actually broken; do not restructure the machine.

## Owned files

- `crates/engine/src/demo_tour.rs`
- `crates/engine/src/config.rs` - **only** the new `demo` field and the line that sets it
- `crates/engine/src/app.rs` - **only** `DEMO_ROUTE_DOORS` and the body of `spawn_demo_objective`
- `docs/handoff/impl-056-riverwood-demo-tour.md` - your notes

## Tests to write

In `demo_tour.rs`'s test module:

- `route_for_demo` returns the Riverwood route for `Some("riverwood")`, the Alftand one for
  `Some("alftand")` and `Some("blackreach")`, and `None` for `None` and for an unknown name.
- The Riverwood route's doors are eight, and they form four reciprocal pairs: door *2k+1*'s
  destination is door *2k+2* and back. Assert this against a table of the pairs written into the
  test, not against the database (the test must run in CI, which has no game data).
- Every route's doors are distinct FormIDs.
- The objective text of a route mentions as many crossings as the route has doors, or - if that is
  brittle to write - simply that each route has a non-empty objective and that `DEMO_ROUTE_DOORS`
  for a route equals its door count.

## Acceptance

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p engine
```

## Out of scope

Do not run the engine (no game data is guaranteed in your checkout), do not add a launcher script,
do not touch `player.rs`, `portal.rs`, `transition.rs` or `door_animation.rs`, and do not change
what the tour photographs or how it writes its contact sheets.
