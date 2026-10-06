# docs-030 - Make the Alftand → Blackreach demo runnable by other contributors (prepare only)

Read `tasks/deepseek/context.md` and `AGENTS.md` first.

## Goal

Other people working on Mudcrab should be able to run the walkable Alftand → Blackreach demo
on their own machine from their own copy of Skyrim SE. Prepare everything they need. **Nothing is
published**: this is local preparation, and the lead decides later what goes where.

## Context

- The demo today:
  - `play-blackreach-demo.cmd` runs `target\release\engine.exe --assets C:\Modding\SkyrimConverted --demo alftand --walk`;
  - `play-blackreach-shortcut.cmd` runs the Blackreach start.
  Both hard-code this machine's paths.
- The assets come from the converter (`crates/converter`), run against the user's own Skyrim SE
  install. Read its CLI (`crates/converter/src/main.rs` or `bin/`) for the real flags: the output
  folder, schema, `--resume-staging` and so on. Also read how conversion was run before (the
  `docs/` and `scripts/` folders, `docs/research/`, the handoff log in `AGENTS.md`). Bethesda
  data is never distributed: each person converts from their own install.
- Engine flags: `crates/engine/src/config.rs`. The demo also has `--demo-tour <dir>`, `--shots`,
  `--terrain-radius` and `--start-position`/`--start-yaw`. The controls are in
  `crates/engine/src/player.rs`: mouse look, WASD, E, F.
- Toolchain on Windows: Rust stable (MSVC), Visual Studio 2022 Build Tools (C++ workload), CMake
  and Ninja. Check `Cargo.toml`, `rust-toolchain*` and the README for the real requirements. Also
  cover Linux if the code supports it (check for platform-specific code).

## Deliverables

1. `docs/demo/README.md`, the contributor guide:
   - what the demo is, with the route and what works and what does not yet (take the known gaps
     from `AGENTS.md`'s handoff log);
   - prerequisites;
   - step-by-step build → convert → run, with the time and disk space conversion needs (find the
     numbers: the size of `C:/Modding/SkyrimConverted`, which you may read, and conversion times
     from the docs or logs);
   - controls, command-line options, troubleshooting (for example "Access is denied" on publish,
     GPU and driver issues, where the logs are), and how to take reference shots.
   No machine-specific paths: use placeholders like `<SkyrimSE>` and `<converted>`.
2. `tools/demo/setup-demo.ps1`. It:
   - checks prerequisites (cargo, and the MSVC linker via `where link` or rustup's msvc target),
     and prints install commands if missing (do not install);
   - builds the release converter and engine;
   - finds the Skyrim SE install (a parameter, else the Steam registry key and library folders)
     or asks for it;
   - runs the conversion into a chosen output folder, with resume support if the converter has
     it;
   - writes a launcher next to the output.

   Parameters: `-SkyrimPath`, `-OutDir`, `-SkipBuild`, `-SkipConvert`, `-DryRun`. `-DryRun` must
   print every command without running anything. Test it with
   `pwsh -File tools/demo/setup-demo.ps1 -DryRun -SkyrimPath "C:/Games/Steam/steamapps/common/Skyrim Special Edition" -OutDir local/demo-setup-test`,
   and also once with `-SkipConvert -SkipBuild` so the launcher is written. **Do not run a real
   conversion or a build** (they take long and touch the shared `target/`).
3. `tools/demo/play-demo.ps1`, a portable launcher: `-Assets <dir>` and `-Start alftand|blackreach`,
   and it finds `target/release/engine.exe` relative to the repo.
4. A short section in the root `README.md` pointing to `docs/demo/README.md`, only if the README
   has a natural place for it. Otherwise say so in the report.

## Rules

- Commit only `docs/demo/`, `tools/demo/`, and `README.md` if touched, with `[deepseek]` subjects,
  and a handoff note in `docs/handoff/`.
- No personal data: no user names or emails, no `C:/Users/...` paths in the committed files.
- Do not push or publish anything.

## Report

What you wrote, the dry-run output, and anything a contributor would still trip over.
