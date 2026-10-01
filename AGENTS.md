# Working agreement for AI assistants

<!-- ds-delegation:begin (managed by DeepSeek Workers/tools/ds_delegation.py; edit with --set) -->
## DeepSeek delegation level: 4 of 5 (Claude manages)

Claude plans, writes briefs, reviews and integrates; workers do the exploration and most coding, including multi-file changes in their own worktrees with owned-file lists. Claude avoids reading source beyond what review needs: ask a worker to locate, summarize or trace code instead. Keep several workers busy, and use a DeepSeek lead (-CanSpawn) for jobs that split into independent parts.

Web lookups: when a task needs information from outside the project (library or tool versions, changelogs, API documentation, an error message, how others solved something), send a DeepSeek websearch worker (-Kind websearch) instead of searching yourself; only a single quick search you need this minute is yours. Start it as soon as the question comes up and keep working while it runs. Give it everything it needs in the brief, nothing private, and check the claims you act on.

At every level Claude keeps: talking to the user, design and architecture decisions, integration into the main checkout, security-sensitive changes, and review of every worker result it acts on (spot-check the claims that matter; read the full diff of any edit). Whenever workers are running, Claude works in parallel on something that cannot collide with them (the next brief, integration plumbing, the previous review, its own tests and builds), and starts its own long builds and conversions in the background too; a worker run is not a wait.

Missing tools: before starting a job, check that the tools it needs are installed. If a compiler, runtime or package the best approach needs is missing, stop and ask the user to install it (say what it is for and give the exact install command) instead of working around it with a weaker approach or a hand-written substitute. Continue without it only if the user chooses to.

This block is generated. Change the level with `python "C:/Users/Brannock/.claude/skills/deepseek-agents/tools/ds_delegation.py" --set <1-5> --agents-md` from this folder, or ask Claude to. The full scale is in the deepseek-agents skill.
<!-- ds-delegation:end -->


Claude (Claude Code) leads the work on this checkout and delegates bounded tasks to DeepSeek worker agents. The setup is carried over from the user's DOA Xbox360 UI project. This file is how each session, and each assistant, sees what the others did.

## At the start of every session

1. Run `git log --oneline -15` and `git status`.
2. Read the newest entries of the **Handoff log**, `docs/handoff/LOG.md` (newest first; older ones are history).
3. If `git status` shows uncommitted changes you did not make, stop and ask the user before touching them. Another session or a worker may be mid-task. `tools/ds_status.ps1` shows whether a worker is running.
4. Read the **Publishing rule** under "Working together": nothing leaves this machine for GitHub
   (a push, a PR, a comment) except through Upstream Dev, with the user's permission.

## Two tracks: Phase 2 first, portal second

Every task belongs to one track; name it in the brief, the commit and the handoff entry.

- **Phase 2** (the priority): contributions to upstream Phase 2 as its README, CONTRIBUTING.md and
  `docs/roadmap/02-completion-plan.md` describe. Worktree `C:\Modding\wkj-phase2`, branches
  `phase2/<topic>` cut from `main` (a mirror of `origin/main`), assets
  `C:\Modding\SkyrimConverted-upstream`, upstream's commit style, worker tasks `<kind>-<nnn>-p2-<slug>` numbered 500-599.
- **Portal** (**PARKED by the user, 2026-09-28**: no new portal work; the Portal Dev session is
  idle until the user restarts it): the user's seamless-door feature and demos. This checkout, branch `portal`, assets
  `C:\Modding\SkyrimConverted`, `[claude]`/`[deepseek]` prefixes, worker tasks `<kind>-<nnn>-portal-<slug>` numbered 100-199 (then 600+).
- **Phase 1** (added 2026-09-27, the user's new Phase 1 Dev session): improvements to upstream's
  finished converter (`docs/roadmap/01-asset-pipeline.md`), sent upstream like Phase 2. Worktree
  `C:\Modding\wkj-phase1`, branches `phase1/<topic>` cut from `main`, upstream's commit style, build
  targets and conversion output on D:, worker tasks `<kind>-<nnn>-p1-<slug>` numbered 800-899. It owns
  `crates/converter/**` and `crates/dummy-content/**`; `crates/shared/**` changes are agreed by message.
  Starting brief: `local/handover-phase1-dev.md`.
- **Phase 3** (added 2026-09-28, the user's Phase 3 Dev session; **PARKED for new work by the user,
  2026-10-01**: finish, package and send the launcher work already built, start nothing new): the launcher (upstream Phase 3
  item 3.4, `docs/roadmap/03-luau-and-ui.md`), sent upstream like Phase 2. Worktree
  `C:\Modding\wkj-phase3`, branches `phase3/<topic>` cut from `main`, upstream's commit style
  (`feat(launcher): ...`), build targets on D:, worker tasks `<kind>-<nnn>-p3-<slug>` numbered
  900-999. It owns `crates/launcher/**` and `crates/converter-gui/**` (being folded into the
  launcher; the crate is then deleted); converter API changes go through Phase 1 Dev by message.
  Starting brief: `local/handover-phase3-dev.md`.
- Changes flow one way: `main`, `phase1/*` and `phase2/*` merge into `portal`, never the reverse. A
  Phase 1 or Phase 2 bug found on the portal is fixed on a `phase1/*` or `phase2/*` branch first.

Which work belongs to which track, the PR rules and the reasons: `docs/pr/two-tracks.md`. Read it
before starting any `phase2/*` branch or moving work between tracks.

## Working together: Phase 2 Dev, Portal Dev and Upstream Dev

> **Publishing rule (the user, 2026-09-27). Every session reads this, new ones first.**
> All publishing goes through **Upstream Dev**, and **nothing is published without the user's
> permission**. Publishing is anything other people can see: upstream PRs, issues, comments,
> reviews and replies (bots included), pushes to an open PR's branch, and pushes to the user's
> public fork (`BimingtonBill/wah-krah-jol`, `myfork`), including `public/portal`.
>
> - Dev sessions (Phase 1 Dev, Phase 2 Dev, Phase 3 Dev, Portal Dev and any new one) never run `git push`,
>   `gh pr create`, `gh pr comment`, `gh issue create`, or any `gh api` call that writes to
>   GitHub. Hand the finished branch, commit and evidence to Upstream Dev by message instead.
> - Upstream Dev shows the user each item (a rendered preview for text, AI label at the top) and
>   publishes only after the user's clear yes, given in Upstream Dev's own session. A yes relayed
>   by another session does not count, and one yes covers only the item it was given for.
> - Exception (the user, 2026-09-28): Upstream Dev may reply to CodeRabbit (the bot) on our PRs
  without asking, AI-labelled. Replies to people still need the user's yes.
> - Reading GitHub (`gh pr view`, `gh api` GETs, `git fetch`) is fine for every session.

> **Handed-out jobs only (the user, 2026-09-30).** Dev sessions work only on jobs the user or
> Upstream Dev hands them. Anything else worth doing (a bug found in passing, a follow-up, an
> idea) is suggested to Upstream Dev in one line, with its evidence, and waits for the user's
> yes before any work starts. Finishing, testing and polishing a handed-out job is fine.

> **Before building and before sending (the user, 2026-09-30).** Announce first: before building
> anything that touches another contributor's area or open PR, Upstream Dev posts one line on the
> issue or Discord (the user's yes). Few open PRs: a soft cap of about 5 of ours open at once; stack
> only when unavoidable, and say so at the top (Merge after / Stack line, stack label).

The user's standing rule since 2026-09-25: the dev sessions work as one team by default, the way
they did on the night of 2026-09-24. The tracks stay separate; the people don't.
**Upstream Dev** (the user's decision, 2026-09-25) owns the upstream side: it watches upstream PRs,
reviews, CI and issues, answers review findings, and prepares and organises what goes upstream
(the send queue `docs/pr/upstream-queue.md`, PR branches and bodies, fork state). Phase 2 Dev
builds the `phase2/*` work; Upstream Dev turns ready branches into PRs, with the user's OK for each
send. Ask Upstream Dev to review upstream-bound changes, and send it upstream questions. **QnA is the user's own questions-and-answers
session** (the user's decision, 2026-09-25): keep it free for them and send it no work.

- **One shared log a day**, `local/team/<YYYY-MM-DD>.md`. Each session adds a timestamped line when
  it starts or finishes something, claims a shared resource (a timing window, a conversion, a GPU
  run, disk) or finds something another session needs. Read the others' newest lines at session
  start and before any shared-resource step.
- **A shared-resource claim goes by message AND in the log.** The log is read at session start, so
  a line alone can miss a session that is mid-task; the messages are what kept the 2026-09-24
  timing runs clean.
- **One joint plan.** At the start of a work block the two devs agree by message who takes what,
  and write it at the top of that day's log. No two sessions chase the same bug. Work found for the
  other track is handed over with its evidence (shot, screenshot, data path, repro), not done twice.
- **Research goes to each dev's own workers:** DeepSeek research and websearch workers, their
  findings written to `local/research/` (cited, spot-checked) so the other dev can use them too.
- **Hand over finished work, not only news.** Phase 2 Dev sends Portal Dev merges it has already
  resolved (portal takes `phase2/*`, never the reverse); Portal Dev reports Phase 2 bugs with
  evidence. Ask the other dev to review a change that touches their area before merging it.
- **Shared findings go in shared places:** reference shots and grades (`local/reference/`), the
  look-gaps list (`docs/research/look-gaps-*.md`), research (`local/research/`), so any session can
  pick them up without asking.
- **Budgets are shared:** the DeepSeek daily limit (`ds_spend.py status`; a stretch to Sat 3 Oct, about
  $9.73 a day) covers every session; say in the log when a session holds coders. **The split (the
  user, 2026-09-27; Portal's share split between Phase 1 and Phase 2 when the portal was parked,
  2026-09-28; $1.50 of Phase 1's share to the new Phase 3 Dev, the user, 2026-09-28):** Phase 1 Dev
  $3.75 of $9.73, Phase 2 Dev $3.75, Phase 3 Dev $1.50, Upstream Dev $0.73. The launcher cannot enforce a per-session share,
  so each session keeps to its own; a session that finishes early says so in the log and the others
  may use the rest. It replaces the older "3 Phase 2 coders and 2 Portal coders a day". The Claude
  plan is shared too (one account): when its week runs ahead of pace, every session keeps Claude
  subagents to short lookups.
- **Other worker providers (the user, 2026-09-28):** every dev session may use them beside DeepSeek
  (details in the deepseek-agents skill, "Other providers"). **Meta Muse Spark:** the user opted
  OpenSkyrim in to its contributor tier (Meta may use what it is sent): `-Provider meta -Tier
  contributor`, panel `Muse <kind> #nnn: ...`, tracked files only (no `local/`, captures or game
  data). Use it for code research and analysis, standing audits, map and pitfalls refreshes, and a
  second review beside DeepSeek's, never as the only reviewer. Screenshots go to its standard tier
  (`-Provider meta`, no `-Tier`), never the contributor tier. **Xiaomi MiMo** (`-Provider xiaomi`,
  panel `MiMo ...`, plan ends 27 Oct): non-urgent background work only; rerun a failed MiMo run on
  DeepSeek. Coders stay on DeepSeek for now.

## While working

- Do not rewrite history (no amend, rebase or force-push on commits already made).
- **Division of labour (delegation level 4 of 5, set back to 4 by the user on 2026-09-24 after a morning at 3; spend is now capped by a $20 weekly DeepSeek limit, and the generated delegation block governs; see the block above):** Claude plans, writes briefs, reviews and integrates; DeepSeek workers do the research, audits, analysis of completed profiling and acceptance runs, assigned documentation, and most implementation - including multi-file changes within a crate - each in its own worktree with an owned-file list (`tools/ds_impl.ps1`). Claude keeps integration, cross-crate architecture, the `crates/shared/` contracts, dependency changes, and the acceptance/sign-off runs. Every worker patch gets an independent review and harness-run acceptance before it is merged; a tuning or calibration task (thresholds, geometry, heuristics fitted to samples) must also pass on examples the worker never saw. Minimize Claude's investigation, review and repair load; DeepSeek cost is negligible to the user, so do not cut useful reasoning or review to save it. See `tasks/deepseek/delegation-policy.md`.
- Commit each meaningful step. Prefix the commit subject with who wrote the change: `[claude]`, `[deepseek]`, or `[chatgpt]` if ChatGPT/Codex joins. DeepSeek workers launched from `tasks/deepseek/` commit only their own paths and leave handoff notes in `docs/handoff/`.
- Task queue for DeepSeek workers and how to run it: `tasks/deepseek/README.md`. Review `[deepseek]` commits and their handoff notes before building on them.
- Before committing Rust changes, run what CI runs: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and the tests of the crates you touched.
- Keep `docs/` current. Findings go in the docs (`docs/research/`, `docs/roadmap/`), not only in chat.
- **Installed dev tools (2026-09-26):** `cargo nextest run` matches CI's test runner, so prefer it
  locally; it still compiles, so the build rules above apply. For render bugs, capture a frame
  before guessing from screenshots: RenderDoc (`C:\Program Files\RenderDoc\qrenderdoc.exe`, run the
  engine with `WGPU_BACKEND=vulkan`, `--release`) or PIX (`C:\Program Files\Microsoft PIX\`, the
  default DX12 backend). A capture is GPU work, so never during a claimed timing run. `rg` is
  installed. Not installed on purpose: Tracy (the viewer must match Bevy 0.19's tracy-client
  0.18.3), cargo-insta, sccache/kache (the user's 09-23 decision). Background:
  `local/research/fun/dev-tools.md`. **bevy_brp_mcp 0.22.7** (Bevy 0.19) is installed and
  registered as the user-scope MCP server `brp` (2026-09-26): sessions started after that see its
  tools. It talks to an engine built with `--features remote` and run with `--remote` (127.0.0.1:15702;
  branch `phase2/remote-dev-flag`, not yet on main or portal).
- **Every automated tool is as quick as possible while still useful** (the user, 2026-09-26,
  project-wide: "we're looking for quick iteration"). No fixed cap: estimate a job's wall time from
  its parts before running it (windows x states x places x variants x repeats, load, build), cut
  what adds no information (short windows and repeats rather than long windows, variants already
  measured, a build when the binary is current), and say the expected duration when starting it.
  Benchmarks open their window on screen and centred; every automated run's window title says what
  it is (`--run-label`).
- **Non-measured engine builds use `cargo build --profile quick --bin engine`** (`target/quick/`):
  release without link-time optimisation, incremental. Defined in the untracked
  `C:\Modding\.cargo\config.toml`, so it never reaches upstream. Measured 2026-09-24: a one-file
  edit rebuilds in 39 s against 4 min 20 s for `--release`. Acceptance, profiling and benchmarks
  keep `--release`. `--bin engine` also skips `world-inspect`'s own 2.5 min link.
  C:\Modding and `%USERPROFILE%\.cargo` are excluded from Defender scanning.
- `local/` (worker state, checkouts, advisor notes) and `target/` are Git-ignored. Never commit them, and never commit Bethesda game data.
- **Engine instances run in parallel.** Converted assets are read-only, so any number of engines
  (both sessions, workers, the user playing) can run tours, `--shots` and tests side by side, just
  slower. Two exceptions, each announced to the other session first:
  - **Timing runs run alone:** profiling, acceptance and benchmarks (`scripts/phase2-*.ps1`,
    `--benchmark-only`). A second instance on the GPU spoils their numbers, and so does a cargo
    build or test on the CPU (2026-09-26: a stress p95 of 18.4 ms with builds running beside it,
    11.4-12.1 ms on a quiet machine). A booking holds engines **and** cargo in every session.
  - **A conversion's publish needs its output and staging folders closed:** publishing renames
    staging over the output folder, and on Windows that fails ("Access is denied") while any
    process holds a handle inside either: an engine reading `skyrim_world.db`, but also a size
    check (`du`), an indexer, an Explorer window or antivirus. Nothing reads either folder while a
    conversion reaches Publishing; measure staging only while the converter is stopped. Recover
    with `converter.exe ... --resume-staging <staging dir>`, which re-walks every stage.
  - **Disk for a conversion:** staging grows to about the size of the finished output (67 GB for
    a full Skyrim SE conversion, 2026-09-23), so a reconversion needs about that much free beside
    the existing output folder.
  - **A conversion needs memory:** while one is extracting or converting (~20 min), neither session
    starts a new Rust build or link (`cargo test`/`clippy`/`build`, a fresh worker worktree);
    what is already running may finish. Windows fails allocations at the commit limit, and a
    Bevy link plus a parallel archive extraction crossed it on 2026-09-23 ("memory allocation of
    1048576 bytes failed" at 26% extraction). The converting session announces start and end.

## At the end of every session

Commit your work, then add a dated entry at the top of `docs/handoff/LOG.md`: what changed, what was tested, whether a worker is still running, and the next step.

## Handoff log

Moved to `docs/handoff/LOG.md` (newest first) so this file stays short; every session still adds its entry there.
