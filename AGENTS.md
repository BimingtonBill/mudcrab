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
2. Read the **Handoff log** below, newest entry first.
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
- **Phase 3** (added 2026-09-28, the user's Phase 3 Dev session): the launcher (upstream Phase 3
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
> - Reading GitHub (`gh pr view`, `gh api` GETs, `git fetch`) is fine for every session.

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

Commit your work, then add a dated entry at the top of the handoff log: what changed, what was tested, whether a worker is still running, and the next step.

## Handoff log

### 2026-09-28 - Claude [phase2] Phase 2 Dev (overnight + day; wrap-up)

**The user's decisions:** plans cover one day or one night at a time; the portal is PARKED (Portal Dev
idle); tonight's acceptance run CANCELLED at the project-wide wrap-up (~14:30); PR texts and hand-overs
follow `local/pr-drafts/STYLE.md`; DeepSeek split Phase 1 $3.75 / Phase 2 $3.75 / Phase 3 $1.50 /
Upstream $0.73; a new **Phase 3 Dev** owns `crates/launcher` + `crates/converter-gui`.

**Branches (from main 6f583cd, green, reviewed, all with Upstream Dev, held by the PR pacing rule):**
`schema-mismatch-error` 7cce96c (which asset check failed + the reconvert command),
`fixture-interior-crossing` 3b41d4d (M5.1), `engine-help` a57dff0 (`--help`, unknown options and bad
values refused; send after #41/#45), and the crossing-stutter set `paced-cell-unloads` 8f980f6 ->
`upload-throttle` cb917c9 -> `paced-model-spawns` c8b2801: worst streaming frame ~21-25 -> ~17 ms,
p50-p99 unchanged (`C:/Modding/wkj-tools/crossing-ab/summary.txt`; the rebase costs nothing; the spikes
were model arrival). Also: `audit-engine` onto portal (merged d940251), the fast-texture FLIP pair for
Phase 1, research-615/618/625 (in `C:/Modding/wkj-phase2/local/agents/runs/`).

**Acceptance:** `accepted` needs three campaigns (evidence, `-UpdateBaseline` with the user's signed
review, compare): `local/acceptance-plan-2026-09-28.md`; clean pre-built worktree `C:/Modding/wkj-accept`.
Not run. **No workers running.** Details: `local/handover-2026-09-26-phase2.md` (2026-09-28 section).
**Next:** the acceptance campaigns; cell-commit cost (main_world ~11-12 ms, the remaining worst frames);
M4.1/M4.2 terrain inspection after #49/#42; M8.2 plan-doc fixes.

### 2026-09-28 - Claude [phase3] Phase 3 Dev (first session: converter window folded into the launcher)

**Set-up:** the user confirmed the Phase 3 rules (see "Two tracks"): worktree `C:/Modding/wkj-phase3`,
`phase3/*`, targets on D:, workers 900-999, owns `crates/launcher` + `crates/converter-gui`, DeepSeek share
$1.50 (from Phase 1's). The user's design call: converter-gui moves into the launcher and the crate is
deleted. Upstream Dev's split: L1 resume, L2 progress, L3 merged window, L4 engine exit message.

**Branches (all green, reviewed, handed to Upstream Dev; nothing pushed):**
| Branch | Head | What |
|---|---|---|
| `phase3/launcher-resume` (L1, from main) | eb5a0d6 | the launcher resumes a leftover `<output>.staging-*` instead of starting over. **Send after #31** (its journal re-checks schema/config/hash, so an automatic resume never publishes stale or torn outputs). Text `local/pr-drafts/phase3-L1-launcher-resume.md` |
| `phase3/engine-exit-message` (L4, from main) | bd26dfc | engine stdout+stderr to a temp log (never a pipe); on an early exit the launcher shows the tail + log path; no second engine. Not yet tried against a real refusing engine |
| `phase3/launcher-conversion` (L3) | e1ffb36 | converter-gui folded into `crates/launcher/src/conversion/`, crate deleted; no auto-start; Start/Stop/Resume/Delete staging/Check/Full check; leftover staging from an earlier session offered for Resume; Stop cancels a check (phase1/output-check b99bc4b merged); 270 tests. Stacks on #51, #45, the converter bundle: held |

**Found:** on main, `--resume-staging` could reuse a torn GLB (cache-hit copy) or Luau (plain write)
after a hard kill; Phase 1 fixed it on `phase1/atomic-writes` 50f421c (one copy path left, on their list).
Screenshots: `local/research/launcher-merged-*.png`, `launcher-resume-done.png`. Fixture data for launcher
runs: `D:/Modding-overflow/phase3-scratch/game_data` (run the exe with that folder as working directory).

**Test kit for the user:** `D:\OpenSkyrim-Launcher-Test\` (174 MB: quick launcher + engine from local-only
`phase3/try-launcher` 734982a = L3 + L4, fake Skyrim in `game_data\Data`, Start/Reset/Make-leftover scripts,
README). Play on the fake data opens a grey engine window: the launcher passes only `--assets`, and the
fixture needs a worldspace the engine doesn't start in by default.
**Parked by the user:** a folder picker (Browse) for the Data folder (`local/handover-phase3-dev.md` §7).
**Open:** L2 (progress, after #45), PR texts for L3/L4, the notice pane's mouse wheel untried by hand,
the launcher's monospace font (ask the user). **No workers running.** Project-wide wrap-up (the user).

### 2026-09-28 (day) - Claude [phase1] Phase 1 Dev (converter bundle evidence, check command, window handed to Phase 3)

**Branches (local, green; Upstream Dev holds the send queue, every send needs the user's yes):**
| Branch | Head | What |
|---|---|---|
| `phase1/quiet-basis` | 604150b | basisu's per-texture "KTX2 validator bug" printf off stdout (C stdout to null device, Rust stdout kept); Claude review #828 accept + fixes |
| `phase1/output-check` | b99bc4b | `converter check <output> [--full]` (quick check of a full install ~3 s) + `check_output_with_cancel(.., &AtomicBool)` |
| `phase1/atomic-writes` | 50f421c | cache-hit copy + Luau write via temp + rename (`atomic_file.rs`); `-zstd` aa567c6 = the ktx2-zstd resolution |
| `phase1/cheap-io-zstd` | e5a2047 | cheap-io merged with ktx2-zstd (outcome carries `(ConversionOutcome, Option<OutputStamp>)`) |
| `phase1/ktx2-zstd` / `fast-textures` | 5a490d0 / 762fe3b | #51 head f14c5a9 merged in (no longer reverts the spill fix); native usage text says RGBA8 |
| `phase1/converter-window` | a062046 | **now Phase 3 Dev's** (merged into the launcher); includes the Check buttons (Claude impl #830) |
Upstream Dev's `phase1/converter-bundle` 1746f48 = #51 + zstd + native + quiet-basis + output-check + cheap-io-zstd (320 green).

**Measured (quiet booked machine; `local/research/phase1-bundle-evidence.md`):** zstd frame time
unchanged (stress 6.20 vs 6.25 ms, textures only differ); native mode re-makes all 33,175 textures in
**4.7 min** (vs ~4.5-5.1 h UASTC; 247 fallbacks; textures 14.42 GB native vs 12.96 UASTC+zstd);
cheap-io A-B-A: texture stage 3.1 -> 1.4 min, other stages unchanged, no extraction regression.
**Extraction and validation are latency-bound** (11-15 % of 16 threads, disk ~95 % idle): issue draft
`local/pr-drafts/36-issue-latency-bound-conversion.md`, HELD for the user's decision on the reply to
Lucas (who called converter work top priority). `converter check` found main's prune pass leaves ~230
GLBs with stale manifest entries (already fixed by glue 0fdc08a, rides #30/#31).

**Next (not started; the user's project-wide wrap-up):** sweep stale `*.partial` files in staging on
resume + guard before publish (Upstream Dev's review of atomic-writes; main has the same gap for
texture/mesh temporaries); make the archive `copy_file`/link fallback atomic on top of #51; parallel
cache verification + validation hashing, `--io-jobs` wired (plan: see the issue draft); native mode
RGBA8 for 24-bit BGR8 and the remaining layouts; the #31 x ktx2-zstd resolution when the second lands.
**Running:** nothing. Scratch: `D:/Modding-overflow/p1-timing` (native tree + A/B results),
`wt/p1-merge`, `wt/p1-bundle`, `wt/p1-fasttex`, `shared-check-target/*` can be discarded.

### 2026-09-28 - Claude [phase2] Upstream Dev (converter priority, new dev lanes, PR style, bundle evidence)

**Upstream:** main still 6f583cd (no maintainer merge since #39 on 09-26). Ours open: #29-31, #38,
#40-41, #43-49, #51 (all CI green, all merge cleanly with main, checked 09-28). #51 (vfs hard links,
70.9 -> 45.65 GB) got its CodeRabbit follow-up (f14c5a9: NTFS link-limit spill) and a body fix
(mojibake from a cp1252 round trip; never pipe PR bodies through the console). Chef (TaylorTurnerIT)
was answered on #30: the user welcomes PRs against the fork's branches.

**The user's rules now:** all publishing goes through Upstream Dev with the user's yes per item
(AGENTS.md "Publishing rule"; fork pushes included). Plans cover one day or one night. The pacing
("new PRs wait for a maintainer merge") is LIFTED for Phase 1 converter and Phase 3 launcher work,
because Lucas (owner) said on Discord that converter work is top priority (resume/WAL, parallelism,
convert-while-playing); engine PRs keep the pacing. PR/issue text follows local/pr-drafts/STYLE.md
(short summary on top, details collapsed); old PR bodies are not edited.

**Queue** (docs/pr/upstream-queue.md has heads and order): converter send list 1 contributor-onboarding
22391dd, 2 ktx2-zstd 5a490d0, 2b atomic-writes 50f421c (+ a `.partial` sweep in progress), 3
output-check b99bc4b + quiet-basis, 4 cheap-io (clean A-B-A timing in progress), 5 fast-textures
762fe3b (issue 34 first), 6 cache-dir (issue first), 7 decoder-bounds, 8 converter-progress. Launcher:
L1 launcher-resume eb5a0d6 (after #31), L3 launcher-conversion e1ffb36 (held), L4 engine-exit-message
bd26dfc (needs text + a manual refusal check). Engine (paced): schema-mismatch-error,
fixture-interior-crossing, the crossing-stutter set c8b2801 (worst streaming frame ~21-25 -> ~17 ms;
272 green), engine-help (after #41/#45). Texts ready: local/pr-drafts/30-36.

**Evidence branch:** phase1/converter-bundle c7a79b2 (worktree C:/Modding/wkj-upstream; #51 + zstd +
native + quiet-basis + output-check b99bc4b + cheap-io-zstd; 323 green). Cheap-io A-B-A: texture
stage 3.1 -> 1.4 min, other stages unchanged. The #31 x zstd resolution is Phase 1 Dev's when the second lands.
Bundle evidence: local/research/phase1-bundle-evidence.md (native textures 4.7 min for 33,175;
extraction/validation latency-bound at ~14% CPU; the latency issue draft 36 is on hold until the user
decides on a reply to Lucas). **Running:** the 90-minute upstream check (session crons). No workers.

### 2026-09-27/28 (night) - Claude [portal] Portal Dev (audit clean-up, look-out bench, schema 28; PORTAL PARKED)

**Parked:** on 2026-09-28 09:45 the user parked portal work (relayed by Phase 2 Dev). The session is
idle: no workers, coders, engines or bookings. Portal HEAD 518022c (last code commit 73b2129).
Details, what was next and a list of disk to reclaim later: `local/handover-2026-09-26-portal.md`
("PARKED" section).

**Landed overnight (700 engine lib tests, short walked Riverwood tour PASSED):** audit leftovers
impl-265 (one doorway-plane owner, one heading helper, test-only `from_args`), impl-268 (retried
interiors not reported failed, one grid-offset owner, Full commits obey the time budget), impl-271 +
73b2129 (every grid-offset copy through streaming's helpers); Phase 2's audit-engine merge (read-only
DB opens, validate-once cell cache, safe asset paths); 1a97d0a water reflection only for water a
camera draws (it ran inside closed houses); impl-267 look-out bench states (`closed-out`,
`open-out-view`, `tools/bench/portal_bench_doors_out.json`). Phase 1's `prep/portal-zstd` merged
(2d5db56) and `C:\Modding\SkyrimConverted` reconverted to **schema 28** (hard-linked extraction,
KTX2 Zstandard; complete, integration passed); release and quick engines rebuilt for it.
Workers: DeepSeek (impl-265, 267), Claude subagents (impl-268, 271), MiMo (audits 262-264,
research 266/269/272, pitfalls digest), Muse (review 265.1).

### 2026-09-27/28 (night) - Claude [phase1] Phase 1 Dev (first session: output size, converter speed, contributor tooling)

**Set-up:** the user accepted the Phase 1 rules (worktree `C:/Modding/wkj-phase1`, `phase1/*` from main,
targets on D:, owns crates/converter + dummy-content). Plan: `local/research/phase1-plan.md` (items 1-12,
contributor experience C1-C11, upstream plan items U1-U6). Research: `local/research/phase1-*.md`.

**Branches (all green, handed to Upstream Dev; publishing only with the user's OK):**
| Branch | Head | What |
|---|---|---|
| `phase1/vfs-hard-links` = PR #51 | f14c5a9 | vfs + cache one file (-25 GB measured), NTFS 1,024-name spill |
| `phase1/ktx2-zstd` | e0496e7 | KTX2 zstd (textures 19.3 -> 13.0 GB), schema +1 with recompress-not-re-encode migration, bevy zstd_rust |
| `phase1/fast-textures` | 2f507dd | `--texture-mode native` (BCn + RGBA8; 200 tex 286.9 s -> 1.1 s; FLIP <= 0.042); issue draft `local/pr-drafts/30-...` |
| `phase1/cache-dir` | 56b6bdd | ingestion cache beside the output (`<output>.cache`, migration) - issue first |
| `phase1/cheap-io` | b583f3b | no per-file fsync in staging, extractor hashes reused, one plugin merge, GLB length check |
| `phase1/decoder-bounds` | 0914347 | on #43: DDS mips, EsmReader header, BA2/zlib ratio bounds |
| `phase1/converter-progress` | ba83c96 | live status line, ETA, Ctrl+C resume (needs #45) |
| `phase1/quick-profile` | 67e7af4 | `[profile.quick]` + CONTRIBUTING (the user said yes) |
| `prep/portal-zstd` | 28c2d18 | for Portal Dev: #51 + zstd on portal, schema 27 -> 28 (workspace 1070 green) |

**Open:** the clean zstd timing A/B at 08:30 (pair ready: D:/Modding-overflow/p1-timing/SkyrimConverted
vs -raw, same engine; Phase 2 chains a 3-min run after); a timed native reconversion for the issue draft;
Wednesday: U1-U6 and plan items 8-9. **Running:** DeepSeek impl-824 (ProgressEstimate, window step A),
MiMo research-825 (first-30-minutes guide). Lessons: never `du` a conversion folder (memory
`stray-du-blocks-publish`); coders need a private target dir (shared-check-target mixed branches).


### 2026-09-26 - Claude [phase2] Phase 2 Dev (project audit and plan; acceptance baseline; parser, cache and BRP work)

**The user's asks:** "audit and make project-wide plan for me to see" (the plan page:
https://claude.ai/artifact/2HSP4u8EVcvXrjCpu9scSm), then "yes to all and go ahead with the plan".
Done from the decisions: leftovers deleted (C: 66 -> 76 GB free), 8 stale branches kept as tags
`archive/phase2/*` (integration, -lod, -lod-shots, -shots, local-shots, render-check,
vertex-alpha-not-opacity, refr-rotation); `integration-night` is the one integration tree. Upstream
posts to realfakenerd are paused by the user (the fork is free, except the 7 open-PR head branches).

**Acceptance baseline on upstream main ddb6d70** (`C:/Modding/wkj-tools/acceptance-main-60s`,
`stress-ab`): functionally clean (preflight, asset closure, runtime audit, 15 robustness tests, 11
scenarios). On a quiet machine every scenario passes at 60 s (stress p95 ~12 ms); an earlier 18.4 ms
"fail" was cargo builds running beside it, so **a timing booking holds cargo too**. Still missing for
`accepted`: quality and release gates, 3 full-length reps (~3.4 h), a human visual sign-off. The
script needs pwsh 7 (fixed below).

**New local branches, reviewed, green, held** (Upstream Dev has the list):
`parser-never-panic` (591328a VMAD 96 GiB abort fixed, a666ebc proptest never-panic properties, fb7f928
bounded decompression), `scripts-utf8-bom` 232a286, `cell-cache-validate-once` ac91f54 (cell_commit
2.86 -> 0.37 ms), `remote-dev-flag` f8a5c03 (BRP behind feature `remote` + `--remote`, 127.0.0.1).
Tools: `C:/Modding/wkj-tools/flip_shots.py` (FLIP before/after of `--shots`; same-build noise <= 0.0054,
threshold 0.02; animated models such as the mill wheel need animation frozen in `--shots`),
`stress_ab.py`. All written by Claude subagents (impl-593..597, analysis-596) and reviewed by Claude.

**Running:** Claude impl-597, shadowless second sun for the reflection view
(`phase2/reflection-shadowless-sun`, worktree `C:/Modding/wkj-p2-refsun`), then its timing A/B.
Worktrees to discard when done: `wkj-p2-proptest`, `wkj-p2-brp` (+ `wkj-p2-brp-target`),
`wkj-p2-refsun` (+ target). **Next:** cheaper crossing frames (spread despawns, rebase without
unloads), the 8 standing audits on one day's DeepSeek budget, `--shots` animation freeze, bevy_brp_mcp
(needs the user's install and MCP config).

### 2026-09-26 - Claude [phase2] Upstream Dev (merges, lighting sent, upstream posts paused)

**Upstream:** #34 and #36 MERGED (main ddb6d70): 2 of 3 for the send hold. Open ours: #29 (4dcfb69,
realfakenerd merged main in; likely next), #30, #31, #38 (d21f357: fixed realfakenerd's broken main
merge), lighting #39 (fixture), #40 (converter, world DB 3->4, head 5a89364) and #41 (engine
`--lights`, head 358450a: XRDS cap + CodeRabbit fix). HazielMagallanes (contributor) reviewed
everything; his notes were fixed and answered. #27 (TaylorTurnerIT) is a stalled draft: the user
chose to WAIT on it for base types (Whiterun's water); ours (renderable-base-types) stays unsent.

**The user's rules now (memory `upstream-send-hold`, `feedback_public_posts`,
`feedback-self-contained-asks`, `feedback-path-only-commits`):** **posts to realfakenerd's
upstream are PAUSED** (comments, issues, PRs, bot replies; pushing to an open PR head counts).
The user's own fork may be updated freely. Every ask to the user is self-contained and labelled
🤖 AI / 👤 human / 🚫 no message. Commit named paths only in the shared checkout.

**Prepared, held:** texts 11 prepass, 12 sky-dome, 13 sky-fog, 18 LOD issue (the user said yes to
the issue before pausing); first sends when allowed: cell-name-nul, io-pool-stack, parser-never-panic,
sky-dome, sky-fog, reflection-land-only (its citations need a websearch worker, DeepSeek pacing
permitting). #30 (a) follow-up written (6bfee13). History paths: send as a fresh squashed commit.
public/portal re-exported by Portal Dev (ff96ac2). Full list: `docs/pr/upstream-queue.md`.

**Tools:** `local/pr-drafts/preview.py` (lists every post and a count at the top); shared warm
target `C:\Modding\target-upstream`; `cargo nextest` for CI-matching tests. **Running:** the
90-minute upstream check (session crons). No workers, no worktrees.

### 2026-09-26 - Claude [portal] Portal Dev (portal cost, door leaf, flames, benches)

**State:** portal HEAD c5cdad9. 637 engine lib tests pass; the full Riverwood tour passes 8/8. No workers running. The details are in `local/handover-2026-09-26-portal.md` and `local/team/2026-09-26.md`.

**Landed today (the user affirmed the whole queue):**
- impl-227: the doorway depth composite is the default.
- 4daa459: suns cast shadows only while they give light. -1.24 ms looking through an open door, measured.
- impl-231 through impl-239:
  - the bevy preset's exposure;
  - the tour's screenshot;
  - light spill measured at the threshold (still opt-in);
  - flames: palette clamp and soft fade;
  - the door leaf: one canonical leaf per pair in every state;
  - the doorway floor anchor is re-probed until stable;
  - the door model double load (partial);
  - billboard bounds validation;
  - the live AA-switch crash (a Bevy stale motion-vector pipeline; `--graphics-cycle`).
- Also: an FPS counter.

**Bench:** `tools/bench/portal-bench.ps1`:
- `-Variants 'a=','b=--flag' -Repeats 2`, interleaved, with spread, about 2.25 min a run;
- render-thread columns from Phase 2's render_timing (fe5e794);
- timing runs open centred, with `--run-label` titles;
- **every automated tool as quick as possible** (the user's project-wide rule, above).

**Next:** animated models rejected by bounds (the Riverwood mill wheel), Whiterun's double gate, frame rate toward 160 (the render thread is the limit), the user's project-plan §3 items.

### 2026-09-25 (midday) - Claude [phase2] Upstream Dev (sends, reviews, preview tool)

**Role:** the user renamed this session **Upstream Dev**. It watches upstream and prepares and
organises sends. The state lives in `docs/pr/upstream-queue.md` (open PRs, decisions, send order),
`local/pr-drafts/` (texts) and the team log.

**Upstream now:** our open PRs are #29 (dcf48c3), #30 (3cd8858), #31 (9cbeee7), #34 (issue #33,
phase2/model-bounds 157d0f8), #36 (issue #35, default-water-height 9d6508a) and #38 (issue #37,
terrain-seam-weld e91db89). Every CodeRabbit finding is answered, and all are fixed except #30 (a),
which becomes a later follow-up PR. The combined branches for whichever PR merges second are
`phase2/pr30-31-combined` and `phase2/pr34-38-combined` (fc9e834, which lacks the doc-only e91db89).
No maintainer activity since #25.

**The user's rules for public text** (memory `feedback_public_posts`): an AI label at the top of
every post; the user sees a rendered preview before anything is sent
(`python local/pr-drafts/preview.py <draft.md>...` writes the exact text to post to
`local/pr-drafts/out/` and renders `preview.html`; post from those files). Replies to bots may be
posted directly, labelled as AI. Python `markdown` is installed.

**Waiting on the user:** the preview of drafts 7-9 (reference-shots: issue + PR; lights: a comment on
#24 + a fixture PR + a converter PR, world DB 3 → 4; suggested: the comment and fixture now, the
converter held). Before sending, re-run the checks on those branches **after Portal Dev's s23
reconversion is done** (no builds until Portal Dev says "done"). Also waiting: the roadmap-page
Upstream track (yes/no), the Chef Discord reply, the fork history holds, and public/portal (after
Portal Dev's hearth check).

**Running:** the 90-minute upstream check (session cron; it can be skipped while the session is
busy, so run it by hand when that happens). No workers, no cargo.

### 2026-09-25 (morning) - Claude [phase2] Upstream session (PR watch, CodeRabbit fixes)

**New session, set up by the user: "Upstream collaboration and reviews".** It watches
realfakenerd/wah-krah-jol: PRs, reviews, CI and issues. Phase 2 Dev handed it #29/#30/#31. Memory:
`upstream-watch-session`. Its worktrees are `C:\Modding\wkj-upstream` (pr31 fixes),
`wkj-upstream-29` and `wkj-upstream-30`; each has its own `target/` and can be discarded once pushed.

**Upstream state:** #29-#31 CI green, behind main but mergeable, no maintainer comments. #26 (.lip
tools) merged, so origin/main is 417a262. #27 is still held by its author (head 6834e5e). Issue #24:
nothing new.

**CodeRabbit triage** (DeepSeek review-570, reviewed): `docs/pr/coderabbit-triage-29-31.md`. The user
said public-facing code takes priority, so Claude wrote the fixes. They are local and tested; see
the Status table in that file: pr29 9b6611d+dcf48c3, pr31 e87d894, #30 (c) 8d6a807 on the combined
tree. The glue commit does NOT fix #30 (c), so `integration-lod-shots` needs 8d6a807 as well.
`docs/pr/pr27-comparison.md`: `vertex-alpha-faithful` supersedes our older branch, and the old
SLSF1 note has been corrected.

**Waiting on the user:** permission to push the fix commits to the frozen #29/#31 heads, where the
#30 (c) fix goes, and the draft CodeRabbit replies (in the triage note; the user sends them). F30-a
needs a separate follow-up PR later.

**Running:** an upstream check every 90 min (CronCreate, this session only; it ends with the session).
No workers, no cargo.

### 2026-09-25 (morning) - Claude [portal] (team working made standard; the hearth's grey sheet found)

**The user's asks:** make the team way of working standard ("Working together" above, with the
user's later changes: resource claims by message and log, the Claude plan shared, QnA the user's own
session); distant LOD in the portal demo (yes).

**Grey sheet over the hearth flames:** a hide test (`local/gallery/glow-check-compare.jpg`) showed
it is the `Flames:0` card, not Glow. research-701 (QnA's NIF decode) plus the GLB: the converter
dropped every BSTriShape's vertex colours (the flames' alpha is the palette's alpha row) and the
`Flames` NiBillboardNode, and did not export falloff. Phase 2 Dev fixed all three plus the colour
controller (see their entry). Engine half on portal, all inert until the data arrives:
`302516e` (emissiveColor channel playback, vertex alpha in the alpha row, additive cards fade to
black in fog) and `19e5fc7` (billboard nodes, `billboard.rs`; Use_Falloff). softFalloffDepth is not
played (needs the depth prepass). Reviewed and approved Phase 2's `39c3497` (vertex-alpha-faithful).

**The bundle** (`bundle/portal-s23` in `C:\Modding\wkj-phase2`, Phase 2 Dev's) is **not ready**: it
still needs vertex-alpha-faithful, nif-billboard-nodes, 4546e6e, 227da41 and the schema bump 22->23.
When Phase 2 Dev hands it over: merge it, delete `render.rs`'s `force_opaque_vertex_alpha` **in the
same commit** (before the reconversion it would bring back see-through trees), announce the
reconversion by message and log, reconvert, then check the Sleeping Giant hearth
(`local/gallery/glow-check.json`) and the Whiterun houses' new vertex shading. The LOD engine half
merges after that (big conflicts: portal lacks upstream main since #23).

**Open (portal):** interior lighting (`local/reference/look-gaps/interior-lighting-design.md`),
imagespace colour grading (`local/research/imagespace-colour-grading.md`). The Claude week is at 84%:
subagents for short lookups only. **No workers running.**

### 2026-09-25 (morning) - Claude [phase2] (hearth-flame converter fixes; portal reconversion bundle started)

Worked with Portal Dev on the grey sheet over hearth flames (team log `local/team/2026-09-25.md`).
The upstream PRs #29-#31 moved to the "Upstream collaboration and reviews" session (heads frozen;
its CodeRabbit triage `docs/pr/coderabbit-triage-29-31.md` finds the glue 0fdc08a makes #30 (c) - a
lost prune record, output still correct - reachable; integration-lod-shots carries that too).

**Landed on local `phase2/*` branches (fmt, clippy, tests green; nothing pushed):**
- `vertex-alpha-faithful` 39c3497 (from main): the vendored exporter dropped vertex colours on every
  static BSTriShape (the flame cards' fade and palette row: the grey sheet; baked shading on SSE
  meshes). Now kept and published per the shaders' rule: no SLSF2_Vertex_Colors → dropped;
  Tree_Anim → alpha 1 (wind); else alpha is opacity. Supersedes `vertex-alpha-not-opacity`.
  Reviewed and approved by Portal Dev.
- `nif-billboard-nodes` 11b2e2b, e3d30a6 (from main): NiBillboardNode parsed and kept (the flame
  cards lay flat without its transform), marked with node extras `openSkyrim.billboard`.
- `material-publication` 4546e6e (effect falloff + soft depth in OPEN_SKYRIM_material, as stored:
  cosines) and 227da41 (colour controllers → `emissiveColor`/`specularColor` channels with
  `components: 3`; the chain walk no longer stops at a colour controller). Checked on the real
  fireplace NIF against Portal Dev's key dump.
- Portal Dev's engine half of all of this is on portal (19e5fc7, 302516e), inert until the data.

**In progress: `bundle/portal-s23`** (in `C:\Modding\wkj-phase2`, from portal 19e5fc7): the one
reconversion for portal. Done: LOD converter half merged (60e9fff, world DB schema 6; converter,
shared, dummy-content tests green). **Left:** merge `vertex-alpha-faithful`, `nif-billboard-nodes`,
cherry-pick 4546e6e + 227da41 (dummy-content nif.rs will conflict: add the colour writer to the
union), bump CONVERTER_SCHEMA_VERSION 22 → 23, full workspace tests, hand to Portal Dev (they delete
`force_opaque_vertex_alpha` in the same commit and announce the reconversion). The LOD engine half
merges later without a reconversion (portal lacks upstream main since #23: expect big conflicts).

**Also open:** distant tree billboards 2-4x too bright and grey; the vanilla DistantTree lighting
research is websearch-703 (QnA started it; report at
`C:\Users\Brannock\AppData\Local\Temp\claude\C--Modding-OpenSkyrimProject\d9ee62b9-a4a7-4ec8-af9c-bdc53350c97e\scratchpad\treelod\websearch-703.out`),
unread. QnA is no longer the research desk. **No workers running from this session.**

### 2026-09-24/25 (night) - Claude [phase2] (look fixes, distant LOD extended, audits, timing)

**Joint night with Portal Dev** (shared log `local/overnight-2026-09-24.md`; QnA researched water,
sky, LOD hiding, refraction and terrain-LOD normals in `local/research/`). Everything is on local
`phase2/*` branches; nothing pushed, and the send hold stands. `phase2/integration-lod-shots`
(`6aa71fa`) holds all of it: converter, engine, dummy-content and vendored-parser tests green.
Morning report for the user: `local/morning-report-2026-09-25.md`.

**Landed (branch, key commit):**
- Distant LOD, terrain (`lod-engine-terrain`): clipped under loaded full-detail terrain and shaded
  from its model-space normals (e75704b); the clip is a pipeline variant used only on blocks that
  touch a loaded cell (0d71cd7).
- Distant LOD, objects (`lod-mesh-containers` 9959944: per-cell segments kept; `lod-engine-objects`:
  exclusive quadtree, seamless handover, loaded-cell segment hiding, churn fix) and trees
  (`lod-engine-trees`: billboards batched per type, hidden per loaded cell, lit evenly).
- Water (`water-look`, `water-colours`), terrain normal maps fixed (`terrain-normal-maps` ba85290),
  heat-haze/refraction exclusion and material animation export with static UV transform
  (`material-publication`; Portal Dev plays it: flames scroll), reconversion hard links for vfs
  and the ingestion cache (`staging-hard-links`), strict-bounds precision (`bounds-precision`),
  audit fixes (`audit-converter`, `audit-engine`), archive/plugin precedence (`asset-precedence`).

**Timing:** the clean re-time (03:30, alone) passes all scenarios: rural 161 fps, with all LOD kinds
134 fps; the stress fly-through with LOD is p95 15.8 ms (19.3 and failing before). Results in
`C:/Modding/wkj-tools/retime2-2026-09-25`. An earlier re-time was spoiled by four runaway
`find /` scans left by subagents (memory `subagent-midtask-messages`).

**Upstream:** PR #27 (TaylorTurnerIT) overlaps renderable-base-types, vertex-alpha-not-opacity and
the schema-15 bump: comparison and recommendation in `docs/pr/pr27-comparison.md`, for the user.

**Open:** a one-frame LOD invariant warning after a camera teleport; tree billboard brightness;
the GUI launcher lacks Skyrim.ini/Plugins.txt discovery; the sky (QnA's `sky.md`). DeepSeek coders
are held by budget pacing until Fri 22:39; Claude subagents took the coding.

**04:40 upstream:** PR #23 merged (0c4592b). Local `main` fast-forwarded; `main` merged into the
LOD chain (one spec-doc conflict) and `renderable-base-types` (#23's interior test updated for
exported DOOR models); every `phase2/*` branch merges cleanly with origin/main, integration green
(440 tests). Whether the send hold lifts is the user's call.
**06:30:** PR #25 merged too (1f0a40e; main e003f67). main merged into `integration-lod-shots`
(upstream's accepted rotation code kept; 447 tests green). The #25 follow-up is re-cut as
`phase2/refr-rotation-followup` (3566737) for the user to send or not.
**Morning:** with the user's approval QnA opened upstream #29 (`refr-rotation-followup` 3566737), #30
(`pruned-texture-complete` 3cd8858) and #31 (`resume-staging-validation` 17d892f). These branches are
live PR heads: frozen unless the user OKs a change. #30 and #31 conflict with each other (`cache.rs`,
two tests at one spot; keep both), and the second to merge also needs the glue commit 0fdc08a. Local
`phase2/pr30-31-combined` (main + #30 + #31 + glue) holds the resolution and passes fmt, clippy and
the converter tests.

**No workers running.**

### 2026-09-24 (night) - Claude [portal] (the look gaps, worked with Phase 2 Dev; morning gallery)

**The user's asks:** a ranked list of how the demo's look differs from Skyrim
(`docs/research/look-gaps-2026-09-24.md`, 61 graded reference shots), then an overnight plan with
Phase 2 Dev working as one team, progress pictures, and pretty screenshots for the morning. Shared
log of the night: `local/overnight-2026-09-24.md`.

**Fixed on portal (each measured on the reference shots, tuning fitted on half and checked on the
other half):** dark city worldspaces take their parent's lighting (`7585b7d`); the sky fill is a
tenth of the sun, not half (`880a3f4`); shadows take 18 % of the sky's hue, no longer green
(`fe7d24e`); multi-pass snow materials are not drawn as a black layer - the Alftand ice blotches
(`aac5989`); effect shaders draw their greyscale texture through their palette (`b54dced`); glows
are capped under the tonemapper's white - Blackreach (`f4c40d7`); the city LAND edges past the weld
bound are drawn as authored (`80bf6de`, mirroring Phase 2's `4fe8f6e`). From Phase 2, cherry-picked
or merged: effect-shader emissive (`b8f152a`), default water height (Morthal's lake), refraction
planes excluded (the fire heat haze), the water look (white sheet gone), staging hard links for
reconversions, scripts reading the schema from source. Audit findings CC-01/02, RM-02, HT-01/02/04/05
closed.

**Data (at 23:00):** `C:\Modding\SkyrimConverted` was converter **schema 21** (complete, integration passed;
space_lighting also carries fog far colour, fog max and light fade). `engine-portal-fit.exe` is
schema 21. Tours on it: Riverwood 8/8, Alftand 4/4, 0 FAIL. Fallback copy `SkyrimConverted-s19`
(hard links; its vfs and ingestion cache removed after they filled C: at 20:40 - see memory
`reconversion-disk`). `target/debug` and `target/release` live on `D:\Modding-overflow` behind
junctions (the user allowed D:).

**Built, not switched on:** Skyrim's `1 - (d/r)^2` point-light falloff (`light_falloff.rs`, a
load-time patch of Bevy's attenuation line, QnA's research); `lights::SKYRIM_FALLOFF` is false
because alone it did not improve interiors, whose fog fills them. Next for interiors:
`local/reference/look-gaps/interior-lighting-design.md`.

**Open:** the fire cards still read as pale sheets (atlas animation, palette orientation); sky
(Phase 2); colour grading (QnA's `local/research/imagespace-colour-grading.md`: grade after the
tonemap, per weather and cell); distant land (Phase 2's LOD branches, not merged); a portal is not
drawn past ~600 units; a shots-run door once rendered closed (flaky).

**After midnight:** interiors draw their fog in the record's far colour (`7d89ae6`, Candlehearth's
cream haze gone); merged Phase 2's `water-colours` (`eea4448`, per-water colours; the DB was
re-exported at schema 21, integration passed, tours pass) and `audit-converter` (`ba3e5d9`, resolved
per Phase 2's notes: panicking assets become failures, the schema stamp comes from the shared
constant); the engine half of animated shader values (`1bc0be1`: flame atlases scroll from
`OPEN_SKYRIM_material_animation`, inert until Phase 2's converter export lands, which needs a
schema bump and a reconversion).

**Later still (01:00-02:30):** schema **22** published: Phase 2's material-animation export
(cherry-picked 43bda16, 00743f8, bd6e439, 9ef3104: `OPEN_SKYRIM_material_animation`, static
uvOffset/uvScale, tangents [outgoing, incoming] per NifSkope) plus the engine half (`1bc0be1`):
**the hearth flames scroll** (GIF `local/gallery/sleeping-giant-fire.gif`). Effect cards are unlit
and drawn at 8x (`d7f4e5c`); a grey translucent sheet over the flames is still open (suspects: fog
on the blended cards, or the additive Glow card). Audit ES-02 closed (`try_despawn`). Tours on
schema 22: Riverwood 8/8, Alftand 4/4; `engine-portal-fit.exe` is schema 22.

**Morning gallery:** https://claude.ai/artifact/1QMmmU5jKwDhdnhKUynJpZ (private; JPEGs in
`local/gallery/final/`). **No workers running from this session.** Phase 2 Dev still has coders
and an audit-converter merge in flight; take their pre-resolved merge, not the branch.

### 2026-09-24 (overnight) - Claude [phase2] (conversion, audit, fixes, reference sheet, distant LOD)

**Upstream:** realfakenerd replied on #24 (2026-09-23 21:02): be patient, the roadmap is being
reworked, management changes and a Discord server are coming. Sends stay on hold. PR #23 got the
review fixes and a reply (sent with the user's OK); #25 has a local follow-up commit (roadmap doc,
comment, test) that is NOT pushed.

**Conversion:** `C:/Modding/SkyrimConverted-upstream` (schema 14, fresh, 20:20-07:14):
complete, 242,969 converted, 0 skipped, 511 textures pruned, integration report passed. Converter
built from `phase2/pruned-texture-complete`, so this is PR 3's real-install evidence.

**Audit and fixes:** lead-515 (12 reviewers) audited every branch
(`docs/pr/phase2-branch-audit.md`); impl-520..530 fixed every branch (I read each diff, fixed
several test/clippy slips myself). Integration branch `phase2/integration` passes all tests.

**Distant LOD:** step 1 `phase2/lod-mesh-containers`, step 2 `phase2/lod-database` (world DB
schema 4; review-535 found ~2,000 offset-grid blocks dropped, fixed), step 3
`phase2/lod-engine-terrain` (engine, `--lod`, origin-relative lookup; never run on a GPU and needs
a schema-4 database). Tree LOD decoded (`docs/research/tree-lod-layout.md`).

**Reference sheet:** `docs/research/upstream-reference-sheet-2026-09-24.md`, rendered with
`phase2/render-check` (engine-side fixes + local `--shots`). Biggest gaps: no trees/plants/Guardian
Stones and see-through Alftand ice (both fixed by the schema-15 branches, which need a schema-15
conversion: ~70 GB staging, disk is short), no sky/fog, dim light, no Blackreach glow.

**Other findings:** Dragonborn "missing" refs are a tree-LOD FormID-slot mismatch, not a bug;
the 278 "lost" records are deletions by later plugins (not a bug); `plugins.priority` is not the
FormID slot for plugins after the first light plugin (small upstream candidate).

**No workers running.** **Next:** decide with the user on disk for a schema-15 conversion (to see
trees and materials), whether to push the #25 follow-up, and what to do about the Discord.

### 2026-09-23 (night) - Claude [portal] (doorway alignment landed; PortalPlugin steps 1-5)

**Doorway alignment (the user's Sven's House report) is merged and confirmed by the user** on "a
bunch of random transitions" (`2b2687b`). impl-103 anchors the map on the two doorways; the lead's
review corrections are in (gate to the placed doorway centre, height cap = step height, destination
door hidden only outside the active space). Found only on the real assets, both fixed: an exterior
doorway's grid came from its *cell* (Riverwood's doors are in the persistent cell, grid 0,0, so
the wrong part of Skyrim streamed), and a crossing landed inside the closed far door (impl-152
opens it at the swap). Non-obvious transitions (ladders, trapdoors, cave mouths) keep the `XTEL`
landing and are **parked by the user** until they get their own design.

**PortalPlugin refactor steps 1-5 merged** (impl-153, `cdefe05`): one `add_plugins(PortalPlugin)`
in `run()`, `PortalOptions` in `config.rs`, `main.rs` byte-identical to `main`. The worker could not
run cargo (harness denied it); the lead fixed three tests and fmt. Both tours pass on the final
tree: Riverwood 8/8, Alftand 4/4, 0 panics. **Step 6** (atmosphere and terrain ring out of `app.rs`)
is next; it needs builds, so it waits for the end of Phase 2 Dev's conversion.

**Also:** `docs/roadmap/backlog-engine-and-portal.md` (every idea and note, split main engine /
portal; main-engine half sent to Phase 2 Dev); research-151 incremental conversion (handed to
Phase 2); reference sheets `local/reference/cmp-103/` and doorway shots
`local/reference/doorway-152/`. `target/release/engine.exe` in this checkout predates the refactor
merge: rebuild before the next play session.

**No workers running.** Phase 2 Dev's upstream conversion runs overnight: no builds, links or new
worktrees from this checkout until they announce the end. **Next:** step 6; then the backlog
(water white sheet first, once Phase 2 Dev says whether it is fork-only).

### 2026-09-23 (evening) - Claude [phase2] (Phase 2 track running; upstream sends on hold)

**Upstream:** issue #24 (lighting scope) and PR #25 (REFR rotation) opened; #23 still open. The user
put a **hold** on sending anything more until maintainers respond (memory + `docs/pr/two-tracks.md`).

**Ready and held** (all from `main`, CI-equivalent checks run where noted), in send order:
`phase2/pruned-texture-complete` (checks pass, body `pr-03`), `phase2/empty-model-bounds` (checks
pass, body `pr-04`), `phase2/shared-converter-schema` (checks pass), `phase2/material-publication`
(schema 15; after empty-model), `phase2/vertex-alpha-not-opacity` (after material),
`phase2/terrain-samplers-weights` and `phase2/terrain-seam-weld` (need a GPU run),
`phase2/sun-shadow-cascades` (needs a GPU run and a benchmark). Workers impl-083, 501, 502,
504-508 (DeepSeek) wrote them; Claude reviewed every diff. Audit: `docs/pr/phase2-fix-audit.md`.

**Coordination with Portal Dev:** rules in "Two tracks" and "While working" (parallel engines;
timing runs alone; no reader in a folder being published; no new builds during a conversion).
Portal reconverted `SkyrimConverted` to schema 19 (complete, 0 skipped).

**No workers running.** **Next:** the upstream-schema conversion into
`C:\Modding\SkyrimConverted-upstream` (about 70 GB; 117 GB free now) so the GPU-dependent branches
can be checked, announced to Portal Dev first; then GPU runs of the terrain, shadow and material
branches. Check #23/#24/#25 for feedback before proposing any send.

### 2026-09-23 (afternoon) - Claude [phase2] [portal] (the work split into two tracks)

**The user's ask:** keep Phase 2 contributions and the portal feature clearly separate, with
Phase 2 first and done as upstream's README describes. The user agreed to all of the plan's
decisions, including lighting going upstream (issue first) and offering the portal work later as a
Phase 4 proposal. The plan is `docs/pr/two-tracks.md`; the rules are the "Two tracks" section above.

**Done:** `deepseek-setup` renamed to `portal`; `main` fast-forwarded to `origin/main` (`2105bfd`)
and tracking it; worktree `C:\Modding\wkj-phase2` on `main`.

**Blocked on the user:** disk. `SkyrimConverted` is 73.8 GB, C: has 43 GB free, so the
upstream-schema conversion (`SkyrimConverted-upstream`) does not fit yet.

**Seen in passing, not mine:** `local/conversion-s18.out.log` ends `complete: false` with 511 skipped
inputs; `s18b` is later and small. The untracked `tasks/deepseek/*` briefs and `scratch-069/` belong
to another session. The eight "modified" engine files are line-ending noise (`git diff` is empty).

**Next:** Phase 2 - rebase PR 2 (rotation) onto `main` as `phase2/refr-rotation`; open the lighting
issue after the user confirms the draft in `two-tracks.md`. Portal - the `PortalPlugin` refactor
(`portal-` worker task).

### 2026-09-23 (day) - Claude (the Riverwood demo, and two rendering bugs it exposed)

**The user's ask:** "start on the riverwood demo", off research-055's data
(`docs/design/riverwood-demo.md`). Section 8 of that document is the part written after the demo
was built and run, and is the part to trust about behaviour.

**The demo works.** `--demo riverwood --walk` starts on the Helgen road south-west of the village;
`play-riverwood-demo.cmd` is beside the Blackreach launchers. `--demo-tour` is no longer
Alftand-only (impl-056): a route is keyed off the `--demo` name, and Riverwood's is eight doors -
four houses, each entered and left again. The passing run is `local/demo/riverwood2/tour.txt`:
**8 crossings of 8, all by walking, 0 dark frames in 2,056 captured frames.** The seamless
crossing now holds in daylight and in both directions, which the Alftand route could not show.
One tour bug fixed on the way: the walk-through's standoffs were 60 and 320 units, and inside
Alvor's House the far wall is nearer than 320; a third, 160, is tried last.

**Two rendering bugs, both dataset-wide, both found by looking at Riverwood:**

1. **The forest was invisible** (research-059, impl-063, `abaa14f`). Pines rendered as a spray of
   dots. My hypothesis - alpha-tested foliage falling through `render.rs`'s `AlphaMode::Blend`
   fallback - was **wrong**, and the worker falsified it properly: the trees convert to `MASK`
   with the NIF's own threshold. Bevy tests `vertex_color.a * texture.a` against the cutoff, and
   Skyrim's tree NIFs carry a `COLOR_0` whose alpha is a shader parameter, not opacity. Fixed
   engine-side at mesh load, no reconversion. Proof: `local/reference/_tree-before-after.png` -
   the canopies fill in and the house, thatch and wall in the same frame do not move. This also
   un-breaks `snow.wgsl`, which multiplies coverage by `in.color.a` under a comment claiming no
   converted mesh has vertex colours; 1,337 of them do.
2. **Everything read wet** (research-058, websearch-060, impl-062 `59f1f20`, impl-064 running).
   `material.rs` mapped Skyrim's Blinn-Phong glossiness *exponent* as a linear percentage. All
   59,268 published materials were wrong; 14,825 were clamped to Bevy's roughness floor. Now
   `(2/(n+2))^0.25`. The other half - the specular mask in the normal map's alpha, which the
   converter drops - is impl-064; websearch-060 settled that the mask scales intensity, not the
   exponent, so it can be a `specularTexture` pointing at the normal map with no derived textures.

**Converter schema 17 -> 18.** impl-064 rides the same bump, so one reconversion covers both. The
first reconversion was running when this was written (`local/conversion-s18.out.log`); a second is
needed after impl-064 lands. **Until a reconversion completes, the engine refuses the assets on
disk** - that is its schema check working, not a fault.

**Also landed:** impl-057, `crates/engine/src/model_animation.rs` - a model's own looping `Idle`
clip on every non-door reference. The mill wheel the Riverwood finale looks at now turns (16.67 s
a revolution, confirmed in the log). The snow catalogue read `material_object IS NOT NULL`, but
the exporter writes a plain 0 for a static with no `MATO`: 9,924 markers, fences and road chunks
against 2,506 real snow statics. Now `> 0`, with a test that fails on the old query (`ad7f138`).

**Reference shots.** `tools/reference/riverwood_shots.json`, twelve poses, ten against a UESP
reference in `local/reference/riverwood/`. All twelve render. research-055 computed the eight
exteriors at ground + 120 (eye height), but UESP's "place" screenshots are free-camera shots from
above the rooftops, so four of them could not frame their reference at any yaw. A sweep of
elevated candidates is `local/reference/rw_sweep4/`; the mill finale has been re-fitted from it.
The inn and the whole-village aerial still need one more pass - `local/reference/rw_sweep2.json`
is written and unrendered.

**Running when this was written:** impl-064 (the specular mask), doc-065 (the demo guide covering
both demos). **Next:** finish the reconversion, re-render and check the wet look is gone, land
impl-064, reconvert again, re-fit the inn and village poses, then a regression run of the Alftand
tour.


### 2026-09-23 (overnight) - Claude (visual gaps, animated doors and seamless crossing, demo for contributors, upstream PR)

**The user's five goals, and where each stands.**

1. **Visual gaps.** lead-026 (4 research + 3 websearch workers) produced `docs/research/visual-gaps-spec.md`. Landed:
   - `23adbb4` glow: HDR + bloom, `EMISSIVE_EXPOSURE = 100` applied only to materials the converter marked with an emissive strength (mushrooms, the Blackreach sun, flames), so Own_Emit surfaces (ice, snowy trees) keep Tamriel's daylight within 0.8%; `intensity_for_radius` capped at a 256-unit reference, which is bit-identical below radius 512 and removes Blackreach's orange ceiling. Claude's holdout: AlftandWorld 1.14x, Blackreach 1.26x, clip 0.66%.
   - `2b69616` empty marker models (Blackreach's two) no longer fail the bounds gate.
   - `26324d5` converter `space_lighting` + `matos` + `statics` DNAM columns, layouts settled from the real bytes (LGTM payload is DATA; inherit bit set = from template; WTHR FNAM interleaves day/night; MATO's last field is an integer flag).
   - `66ee786` engine per-space lighting: backdrop, fog, ambient hue and sun tint per space, destination atmosphere on the portal camera. Record colours are applied as **hue** with the calibrated brightness kept.
   - `cfe4893` directional snow (DNAM/MATO projected material), inert until the data exists.
   - Ice (environment maps) is still open, and so is the calibration of the last two, which needs the reconversion.
2. **Animated doors and seamless crossing.** `f6749e2` converter exports Open/Close clips (all 768 door NIFs decode; 171 animated). `50bc0f1` the door state machine (clips found by name; a leaf whose swing is under 45 degrees hides when open). `49f2df5` the seamless crossing: the player's pose is carried through the door instead of snapping. `b535a30` the two wired together, plus the review corrections. Claude's check on the integrated build: **walk tour 4/4 by walking, 0 errors, 0 dark frames in 811 captured frames**; the frames either side of a swap are the same view (`local/demo/walk-swap-fixed.jpg`). The route's Dwemer doors only twitch 5-9 degrees in the real data; their non-load twins swing 86-88. **Open question for the user:** keep the faithful twitch or borrow the twin's swing?
3. **Demo for contributors.** `cc53baf`: `docs/demo/README.md`, `tools/demo/setup-demo.ps1` (prereqs, build, Skyrim detection, conversion with resume, launcher) and `tools/demo/play-demo.ps1`. Claude ran both dry runs. Nothing published.
4. **The project as a whole.** Upstream `origin/main` (16 commits: `dummy-content`, ADRs, CI) merged locally (`8c21447`, one test-only conflict). `40e0eb5` extends `dummy-content` with an interior cell and a linked load-door pair, so upstream can test doors and portals without Skyrim. research-028: the streaming fixture never exits without a benchmark window - not a ring bug.
5. **Audit and PR design.** `docs/pr/upstream-pr-plan.md` (research-029): 9 PRs, fixes first, with exclusion and cleanup lists. Cleanups done: `410ce31` converter (ADR-0002 `OPENSKYRIM_SKYRIM_DATA` / `OPENSKYRIM_CONVERTED_DIR`, no machine paths, no private names), `62c0f7b` + `944250e` engine parts 1-2, part 3 running. Not done: re-cutting the commits into the PR series (the plan's step), which needs the user.

**Blocked on the user:** the reconversion (see the entry below). Until it runs, door clips, per-space lighting and snow have no data, and each is inert by design.

**Process notes.** Every worker patch got an independent DeepSeek review (review-035/040/041/042/045/049) or a full read by Claude; four rounds of corrections were sent back. Discard each ds_impl worktree right after integrating: they seed ~11 GB of `target/` each.

### 2026-09-22 (night) - Claude (rotation fix, distant terrain, closed doors)

- **User asks:** the Ruined Tower was rotated wrongly in the side-by-side comparison, so check rotation and placement. Also increase the render distance so distant terrain shows.
- **Rotation (`933b95f`, Claude, shared contract):** Creation `REFR` angles turn clockwise. Objects are now rotated by `Rx(-x) Ry(-y) Rz(-z)`, the inverse of the old conversion.
  - Yaw was proven from the door links: 88% of doors were consistent against 42% under the old convention.
  - Tilt and order were confirmed by research-023 (`22ad24c`): on 82,731 tilted references the median error against the terrain normal is 8.2°, and every alternative is 19-32°. Scale and position are applied correctly.
  - Objects and the arrival camera now share one convention.
- **Reference poses:** re-fitted by research-024 (`6804978`, `9d6d844`): 1 matched, 15 close, 10 area only. The Alftand exteriors are shot from the north side of the ravine. The Ruined Tower door faces east.
- **Closed doors (`f9c439e`, impl-022):** a load door draws closed unless the portal is rendering through it.
- **Distant terrain (`5211bad`, impl-021 plus a review round):** a terrain-only ring out to `--terrain-radius` (default 8), with distance fog and `NotShadowCaster` on the ring.
  - Frame rate on the Pale view: 180 FPS without the ring, about 97 FPS at radius 8 (shared GPU, rough).
  - Claude's change: benchmark runs keep the ring off unless `--terrain-radius` is given. A default benchmark passed with 0 commit-budget violations.
  - DeepSeek review-025 verdict: accept after fixes.
  - Unexplained: Claude's own `--streaming-fixture --terrain-radius 8` run did not exit within 400 s. The worker's fixture run validated, but the ring terrain was not exercised there. Check this before the Phase 2 runs.
- **Harness:** `tools/ds_impl.ps1` synced to skill v1.3 (`54645f9`).
- **Queued, not started:** impl-023 (`tasks/deepseek/impl-023-door-front-from-xtel.md`). The portal takes a door's front from the model axis, which is wrong for many door models (the tower door opens only from its back). The fix takes the front from the return link's `XTEL`.
- **Open rendering gaps:** glow/emissive, per-cell lighting templates and interior fog, the snow shader (roofs render bronze), overcast sky and weather, ice reading as rock, `BlackPlane01` voids, two Blackreach models that fail validation.
- **No workers running.**

### 2026-09-22 (late) - Claude (reference screenshots and matched renders)

- **User ask:** collect Skyrim screenshots of the demo's locations, then render the same compositions in OpenSkyrim for comparison.
- **References:** 26 UESP screenshots (CC BY-SA; download approved by the user) in `local/reference/uesp/` (git-ignored, `SOURCES.txt`).
- **Built:** design `docs/design/reference-shots.md` (the shots-file contract). impl-017 `--shots <file> [--shots-out <dir>]` renders exact poses once streaming settles (`5e53712`, DeepSeek, verified at runtime by Claude). research-018 wrote the first poses, `tools/compare_shots.py` and the mapping/fitting scripts (`adbea02`, `4050696`).
- **First renders:** `local/reference/cmp1/`. The Ruined Tower matches; most other poses are rough (walls, and some pitch signs flipped).
- **Found:** the Alftand01 arrival is washed out to white at eye height. The suspect is the stopgap camera lantern (3e9) in `update_atmosphere`. It showed once crossings landed at eye height (`4b1dd7a`).
- **Done:** research-020 refined all 26 poses by rendering: 1 matched, 12 close, 13 area only (`036efbe`; compare-sheet fix `64129bb`). impl-019 calibrated the lighting (`efa5351`):
  - the lantern is removed;
  - Bevy's 4π on point lights was missing from `lights.rs`;
  - the ambient levels and colours were re-measured.
  Claude's holdout check on the refined poses: Alftand02 is 1.8x too dark (it was 8x), AlftandWorld 1.2x, Blackreach 1.5x too bright; Tamriel is unchanged. `.deepseek-agents.json` now has `implAllowTools` with `cargo build` and the release engine.
- **Renders:** refined poses with the new lighting are in `local/reference/cmp-lit/` (with `_sheet.jpg`). Measure with `python tools/research/exposure_stats.py --shots <file> --render <dir>`.
- **Rendering gaps the comparison found** (in `docs/research/reference-shot-poses.md`):
  - no emissive/glow (Blackreach);
  - no per-cell lighting templates (`XCLL`/`LTMP`) or fog, so the lighting is flat;
  - ice reads as rock;
  - no snow shader or weather;
  - black voids (`BlackPlane01`?);
  - two Blackreach models fail validation (`WispAmbush`, `FrostSpiderAmbush01`);
  - one 4334-radius `FalmerCityLight02NS` floods the Blackreach ceiling orange.
- **Next:** the user picks the order. Glow first is recommended.

### 2026-09-22 (day) - Claude (terrain textures, see-through meshes, open doors with portals)

- **User asks:** work on terrain and terrain textures; doors not rendered (or open) so you can see into the next interior; fix objects that render transparent or misplaced.
- **Merged (DeepSeek, reviewed and integrated by Claude):** impl-009 portals + open load doors + pre-streamed cells kept out of the active space (`f219a05`); impl-010 terrain - root cause of the stretched textures was the ClampToEdge sampler on tiled layers, now Repeat; bilinear layer weights in the material uniform (`0ece570`); impl-012 converter - vertex alpha no longer means blend (2,632 materials were see-through), blend factors kept, EditorMarker shapes dropped (`b4ce0d9`). research-011 (`bc54a17`) diagnosed the transparency/misplacement classes.
- **Claude fixes found only at runtime:** portal startup ordering panic; WGSL u32->f32 in terrain.wgsl; portal quad was a floor-oriented `Plane3d`; every crossing arrived facing the return door (`arrival_camera_rotation`, `a49b4df`). Verified with `--demo-tour`: 4/4 crossings, walk test, and a doorway screenshot showing AlftandWorld through the Alftand02 door (`local/demo/portal5/02-door.png`); the tour now saves the portal camera's own image per door.
- **Harness:** `claude.exe` lives under the MSIX package folder, so launches need the updated skill (fixed by the DeepSeek Workers session); `tools/ds_impl.ps1` skips acceptance when no worker ran and seeds each task's `target/` from the lead's debug build (ported from the harness). Pending: sync the other tools with the harness at a clean point.
- **Later the same day:** schema-15 assets published (see-through ice/rock and editor markers gone, verified on screen); impl-013 glows/terrain bases/labels (`8d4160b`); tools synced with the harness (`c17fc48`); impl-014 converter `lights` table + `references.radius_override`, DB schema v5 (`5cd1bf4`); impl-015 engine point lights with a nearest-64 budget (`2acd409`); impl-016 auto-load doors cross on contact (`83ec4cd`). All DeepSeek, all accepted without correction.
- **Claude runtime fixes:** `ds_impl -Integrate` now stamps copied files (a 1 s "build" had reused a stale binary); lights never rendered - model-less LIGH references lacked `Visibility` (B0004) - and were ~100x too dim for the camera exposure (`EXPOSURE_CALIBRATION = 50` in `lights.rs`, the brightness knob); `update_atmosphere` never applied when starting inside Blackreach (`188e184`).
- **Data state:** `C:\Modding\SkyrimConverted` is converter schema 15 / DB schema 5 (504 lights, 15,167 lit references, 315 auto-load doors). A publish can fail with "Access is denied" if anything holds `skyrim_world.db` open (a worker script did); `converter.exe ... --resume-staging <staging dir>` republishes.
- **Verified:** `--demo-tour` 4/4 crossings + walk test on the final build; screenshots in `local/demo/lights3/`. The user is running agentic tests on this PC, so no FPS/timing numbers from today are valid.
- **Evening fix (Claude):** the black Alftand01 -> Alftand02 (and Tamriel -> Alftand01) portals were a heading-sign bug, not rendering: a Creation `XTEL` heading turns *clockwise* (forward = `(sin z, cos z)`; every route link puts the arrival in front of its destination door along that), while `arrival_camera_rotation` reused the counter-clockwise object convention plus a half turn, which only happened to fit AlftandWorld. Now `Quat::from_rotation_y(-z)`, with a regression test built from the four real links. Crossings also land the camera at eye height (XTEL is the feet point; fly mode and the tour had sat on the floor). Tour 4/4, screenshots `local/demo/portalfix2/`. Open question: objects still use the counter-clockwise `Rz` convention and look right on screen; not re-verified against the clockwise heading finding.
- **Known gaps:** lights overexpose right next to a source (inverse-square; tune `EXPOSURE_CALIBRATION` or add a tonemapping/exposure pass); GPU clustering ignores render layers, so a portal destination's lights can faintly reach the active space (impl-015's note).
- **Next:** the user's look at the lit demo; then the Phase 2 shakedown and visual review (no GPU-heavy runs while the user's agentic tests run).

### 2026-09-22 - Claude (overnight: walkable Alftand -> Blackreach demo; Phase 2 sign-off paused)

- **The user's goal for the night:** "get this cool Blackreach demo up and running, extra points if it's interactable and I can walk through the level". Done: `play-blackreach-demo.cmd` (or `target\release\engine.exe --assets C:\Modding\SkyrimConverted --demo alftand --walk`) starts on foot at the Alftand exit; four load doors (E) lead down to Blackreach with no loading screen. `--demo blackreach` (or `play-blackreach-shortcut.cmd`) starts inside Blackreach.
- **Verified:** `--demo-tour <dir>` walks the route by itself, screenshots every place and door, then holds W in Blackreach. Final run on `3de1505`: 4/4 crossings, walk test 600 units grounded, 0 errors, 170-180 FPS. Screenshots: `local/demo/final/`.
- **Built by DeepSeek workers, reviewed and integrated by Claude:** impl-005 (converter: `XTEL` -> `door_links`, DOOR/ACTI/FLOR/CONT/TREE/LIGH models, schema v4), impl-006 (engine: `ActiveCell`, interiors, rebase guard, `TransitionPlugin` pre-streams and crosses), impl-007 (`PlayerPlugin`: mouse look, walking via Bevy mesh ray casting, E to open, F to fly), impl-008 (weld vanilla one-point terrain seams instead of dropping cells; no water plane on dry cells). All four accepted without correction. Claude: design + `doors.rs` contract, CLI (`--walk`, `--demo`, `--start-position/--start-yaw`, `--demo-tour`), app wiring, demo tour, atmosphere (sky / underground ambient / lantern), objective text, launchers.
- **Data:** `C:\Modding\SkyrimConverted` reconverted at schema v4 (complete, integration passed, 2204 door links all resolved; the four route doors match research t02 exactly).
- **Harness fix:** `tools/ds_impl.ps1` now gives every checkout its own `target/`; a shared one let parallel engine workers run each other's test binaries (found by impl-007).
- **Known gaps:** the terrain layer blending is still blocky (research-004's Defect A, fix recipe in `docs/research/terrain-blending-seams.md`, not yet implemented); `LIGH` references are not real lights (interiors use ambient + a camera lantern, Blackreach reads dark blue); load doors are opened with E only (Skyrim's auto-load doors fire on contact); no skinned meshes, NPCs or physics (Phase 4).
- **Phase 2 sign-off (paused for the demo):** shakedown on real assets passed every gate except the visual review; the terrain fixes above are what the visual review needs. Test locations chosen from data: rural (0,0), dense (4,-21), water (21,-3). Next: implement the blending fix, rerun the shakedown, user reviews screenshots, then two full campaigns.

### 2026-09-21 - Claude (clone, toolchain, DeepSeek harness)

- **Cloned** `realfakenerd/OpenSkyrim` at `8919096` into `C:\Modding\OpenSkyrimProject`.
- **Toolchain installed** on this machine: Visual Studio 2022 Build Tools (C++ workload) and Rust stable 1.98.1 (MSVC) via rustup. CMake and Ninja were already present.
- **DeepSeek harness** copied from the DOA Xbox360 UI project and adapted: `.deepseek-agents.json`, `tasks/deepseek/` (context, cookbook, delegation policy, reviewer, run analyst, advisors, self-test, implementation template, result schema), `tools/ds_impl.ps1` (isolated worktree per implementation task, sharing `target/`), `tools/ds_status.ps1`, `tools/run_ds_queue.ps1`, `tools/ds_report.py`, `tools/check_worker_result.py` (now recognises `cargo` commands), `tools/check_citations.py`, and their tests (`python -m unittest discover -s tests`).
- **Verified:** `cargo check --workspace` passes (5 min cold, only a future-incompat warning from the `proc-macro-error2` dependency). The harness self-test passed 6/6 (`local/agents/runs.csv`).
- **Skyrim SE** is installed at `C:\Games\Steam\steamapps\common\Skyrim Special Edition`. Workers can read it (in `readOnlyDirs` and described in `tasks/deepseek/context.md`) but never write to it.
- **Private project:** the user wants none of this public. Commit locally only, never push, never open PRs against the upstream repository.
- **Next:** run the harness self-test (`tasks/deepseek/selftest.md`), then choose the first tasks.
