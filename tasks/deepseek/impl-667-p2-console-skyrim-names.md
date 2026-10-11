# Goal
Rename the developer console's commands to Skyrim's real console commands and remove every invented command or alias. Track: Phase 2 (upstream issue #88).

# Context
- Worktree `D:/Mudcrab/worktrees/p2-console`, branch `phase2/dev-console` (checked out; do not switch). Use `CARGO_TARGET_DIR=D:/Mudcrab/builds/p2-console`... if the environment prefix is refused, use the worktree's own `target/`.
- Console code: `crates/engine/src/console.rs` (registry, dispatch, suggestions, Tab autofill, `help`), command registrations in the plugins that own them (search `register` / the console registry in `crates/engine/src/*.rs`, e.g. physics.rs and the free-fly/noclip code).
- Decided list (sources in `C:/Modding/Mudcrab/mudcrab/local/research/skyrim-console-commands.md`):
  | Today | Becomes |
  | `help`, `help <command>` | `Help` with Skyrim's syntax `help <text>`: no argument lists all commands with their one-line descriptions; with text, lists the commands whose long or short name or description contains the text (case-insensitive) |
  | `clear` | `ClearConsole` (long name only; Skyrim has no short form) |
  | `noclip` and alias `v` | `ToggleCollision`, short form `tcl` |
  | `collision` and alias `f3` | `ToggleCollisionGeometry`, short form `tcg` |
  | `tankard` and alias `t` | no console command (T key stays) |
  | `grab` and alias `e` | no console command (E key stays) |
- Command names are case-insensitive, as in Skyrim (`TCL`, `tcl`, `ToggleCollision` all work). A command has at most a long name and Skyrim's short form; no other aliases.
- Suggestions ("did you mean") and Tab autofill work on long names and short forms; keep the existing behaviour otherwise (one match fills, several extend to the common start or are listed). Autofill should complete to the casing of the registered name.
- The keys V, F3, T and E keep working exactly as now; V and F3 share their code path with `tcl` and `tcg`.

# Scope
`crates/engine/src/console.rs` and the files that register commands (only their registration and the tests about them); `docs/` if a doc lists console commands. No behaviour change beyond the renames, case-insensitivity and the help filter. No new dependencies.

# Done when
- Tests updated and added: every command by long and short name in mixed case; removed commands answer "unknown command" with a suggestion only when a real command is close; help with and without text; autofill on `tc` (lists or extends `tcl`/`tcg`/`ToggleCollision...`) and on `Clear`.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run --workspace` pass.
- One commit: `feat(engine): console commands use Skyrim's names and syntax`, trailer `Co-Authored-By: DeepSeek V4.1 <noreply@deepseek.com>`. Stage files by name. No push, no amend. Never run engine.exe.

# Report
Commit id, the final command table (long, short, what it does), files changed, test count. Under 250 words.
