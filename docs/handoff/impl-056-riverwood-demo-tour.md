# impl-056 — the scripted demo tour follows the demo the run started in

**Task:** the lead's brief `impl-056-riverwood-demo-tour` (no copy under `tasks/deepseek/` in this
worktree).

**Acceptance run here:** `cargo fmt --all -- --check` (no output, clean), `cargo clippy
--workspace --all-targets -- -D warnings` (finished, no lint), `cargo test -p engine` (278 passed,
0 failed in the lib, 3 in `world-inspect`, doctests unchanged). The five new tests are
`demo_tour::tests::*`; they were mutation-checked before the final run — changing Riverwood's
`0x0001_33E9` to `0x0001_33E8` and its objective's "Eight" to "Nine" each made the intended test
fail with the intended message, and both were reverted.

## What changed

### `crates/engine/src/demo_tour.rs`

- A route is data now: `DemoRoute { name, doors, objective }`, with two of them (`ALFTAND`,
  `RIVERWOOD`) in the private table `DEMO_ROUTES` and the brief's two lookups over it:
  `route_for_demo(Option<&str>) -> Option<&'static DemoRoute>` and the fallback that the two call
  sites actually use, `route_for_run(Option<&str>) -> &'static DemoRoute` (the run's route, or the
  Alftand one when the run has none — one extra public item beyond the three the brief fixed, so
  that `app.rs` needs neither a duplicated string nor a panic to express "no route, the old
  behaviour").
- `ALFTAND_ROUTE` is unchanged and still public; `RIVERWOOD_ROUTE` is the eight doors of the brief's
  table, as a private `[u32; 8]` with the house each pair belongs to in the comments.
- `run_demo_tour` resolves the route from `Res<EngineConfig>::demo` once a frame
  (`route_for_run(config.demo.as_deref())`, a name lookup) and logs it on frame 1:
  `tour route: riverwood (8 doors), demo Some("riverwood"), objective "..."`. The two places that
  were hard-wired — the end-of-route test in `Phase::Settle` and the door `Phase::FindDoor` looks
  for — read the route (`route.doors.len()`, `route.doors[tour.stage]`). Nothing else in the phase
  machine changed.
- Tests in the module: route resolution per demo name, the fallback, Riverwood's eight doors against
  the pairs table, distinct FormIDs per route, and "each objective states its route's door count in
  words" (the count the HUD starts from is `route.doors.len()`, so text and count cannot disagree).

### `crates/engine/src/app.rs`

Four hunks, all in the objective, listed here so the lead can see the whole footprint:

- `DemoObjective` gains `total: usize` (the countdown's start) and its doc comment now says "a
  demo" rather than "the Alftand -> Blackreach demo".
- `spawn_demo_objective` takes `Res<EngineConfig>`, resolves the route with `route_for_run`, and
  spawns `Text::new(route.objective)` and `total: route.doors.len()`.
- `update_demo_objective`'s countdown reads `state.total` instead of the deleted const.
- `const DEMO_ROUTE_DOORS` is **gone**: the count is the resolved route's door count, so the const
  had no job left (and an unused const fails `clippy -D warnings`). A run with no route still gets
  exactly today's Alftand line and a count of four, because the fallback route is the Alftand route.

### `crates/engine/src/config.rs` — no change

The brief asked for `pub demo: Option<String>` and the line that sets it. Both are already in the
base commit (`d2ef589`, `config.rs:56`, `:131`, `:277`, with a test at `:447`), so touching the file
would have been churn. `DemoStart::named` has no `"riverwood"` arm in this worktree — that is the
lead's edit (the brief says not to touch those match arms), and my route table does not depend on
it: `route_for_demo(Some("riverwood"))` resolves whether or not `--demo riverwood` is parsed yet.

## The returning route (brief item 4): what was actually broken

I read the phase machine end to end with the Riverwood route in hand. Two of the three things the
brief asks about were already sound; one was broken and is fixed.

1. **`FindDoor` finding a door in the current space — sound, and here is why.** `Phase::FindDoor`
   (`demo_tour.rs:440`) searches *every* resident `LoadDoor` by FormID (`doors.iter().find(|door|
   door.ref_id == wanted)`), with no space filter, and every route door is a distinct reference, so
   the lookup cannot pick a door of another space or a pre-streamed copy: it either finds the one
   reference it names, wherever it stands, or nothing. Each Riverwood pair is the house door and the
   door *inside* the house it leads to, so the odd-numbered crossings always stand in the cell the
   tour has just arrived in; all four exterior doors live in Tamriel's persistent cell `00000D74`
   and the whole village is inside one 5×5 stream box (`docs/design/riverwood-demo.md` §1, §5), so
   they are resident whenever the tour is outside. No change.
2. **Screenshot, log and walk-through naming — sound.** Every name is prefixed with `tour.stage`,
   which counts crossings and is never reset: `{:02}-arrived`, `{:02}-survey-N`, `{:02}-door`,
   `{:02}-portal-view`, `{:02}-view-N`, `walk-through/{stage:02}/f{frame:05}.png`. The tour is in
   Tamriel four separate times (stages 0, 2, 4, 6) and each of those visits writes its own
   stage-prefixed files; the one fixed name, `05-walked` in `Phase::Walk`, is written once, after
   the last crossing. No change. (I did not touch what the tour photographs.)
3. **Broken: the route's length and its door list were constants.** `Phase::Settle` sent the tour to
   `LookAround` after `ALFTAND_ROUTE.len()` crossings and `Phase::FindDoor` indexed `ALFTAND_ROUTE`,
   so a Riverwood run would have walked four doors, then stopped with four crossings untried, and
   its last four stages would have indexed a door the route does not have. That is the fix: both now
   read the resolved route.

Not verified: the run itself. The brief says not to run the engine and this checkout has no
guarantee of game data, so everything above is a reading of the phase machine plus the measured data
in `docs/design/riverwood-demo.md`, not an observed `--demo-tour riverwood` run.

## Two things outside my owned lines that the lead has to decide

- **The objective still only spawns for `--demo alftand`**: `app.rs:200` gates
  `spawn_demo_objective`/`update_demo_objective` on `walk && demo == Some("alftand")`, which is the
  lead's area ("change nothing else in `app.rs`"). Until that line names the run's route (e.g.
  `crate::demo_tour::route_for_run(config.demo.as_deref())`), a `--demo riverwood --walk` run shows
  no objective at all and the route-derived line never appears.
- **The per-crossing line is still Blackreach's**: `update_demo_objective` writes "Now in {place}.
  Find the next load door (E) - {left} more to Blackreach" for every place and "You made it:
  Blackreach" on arrival, which is wrong in register for Riverwood (the count is right now, the
  destination word is not). I left the text alone: the brief names only `DEMO_ROUTE_DOORS` and the
  `Text::new(...)` in `spawn_demo_objective`, and a fix needs a per-route word the lead may want to
  shape (a `DemoRoute` field, or a phrase the lead picks).

## Unverified here

- **The four return-door FormIDs** (`0001CBAF`, `000133E9`, `000133FB`, `00013419`) come from the
  brief's table and its `door_links` verification. I could not re-check them: reading
  `C:/Modding/SkyrimConverted/skyrim_world.db` (via `sqlite3` or a script) is blocked for this
  session — every such command needs an approval nobody can give — and the repository's documents
  name only the four exterior doors (`docs/design/riverwood-demo.md` §2). If a return door is
  wrong, the tour fails at that stage with the "door never spawned" line and nothing else.
- `cargo test -p engine` runs only the crate's unit tests; there is no integration test that walks a
  route, and no route test touches the database (CI has no game data).
