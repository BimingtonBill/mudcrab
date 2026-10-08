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
5. Read the current engine goal and the foundation demo gate in `local/plans/foundations-to-portal.md`
   (the user, 2026-10-07): the portal demo is to hold a stable 160 fps while travelling and through
   load doors, with no shortcuts; and no engine-foundation draft or PR opens until it has run in the
   portal demo and the user has seen it there. That covers every public release of the work, also
   pushes of it to our public fork (`myfork`, `public/portal`).

## Foundations check (the user, 2026-10-07: every session, worker and review in the Mudcrab project)

Before proposing, approving, building or merging any fix or design, check it against these five points and
say how it meets them in the brief, handoff, review or PR text:

1. **No shortcuts.** It fixes the root cause. It does not hide a problem: no lower settings, draw distances or
   shadows, no frame or load caps, no delaying or hiding loading, no code that knows the test data or routes,
   no skipped cases, disabled checks or weakened tests, no silent fallbacks that turn errors into wrong output.
2. **Foundationally optimised.** The work sits in the right layer (the converter precomputes; the runtime stays
   light), its cost scales well with content (per entity, per cell, per mod), and it is measured before and
   after with the project's own tools (#184 baseline and routes).
3. **Flexible.** Data-driven, not hard-coded: no Skyrim FormIDs, paths or magic numbers baked into logic; works
   for any worldspace, plugin set or load order; versioned formats and clean interfaces between converter,
   shared contracts and engine.
4. **Moddable.** Mods and load orders keep working: stable IDs, overrides respected, nothing that only holds for
   vanilla data (the XPOD pairs case); derived data knows when its inputs changed (#185); optional features can
   be switched off and never become something Mudcrab proper depends on.
5. **Aligned with the Mudcrab goals.** Upstream's roadmap and phases; Mudcrab as the best modding platform; the
   portal demo at a stable 160 fps at maximum visuals; long-term foundations over quick wins; the multiplayer
   design rules (stable IDs, per-cell change data, MP-ready scripting); licence-clean MIT/Apache code (clean-room
   rules); "plus" features kept separate.

Every review brief asks "what shortcut does this take, and what would break for a mod or another worldspace?"
A shortcut found is a "fix first". A deliberate temporary measure needs the user's yes, is labelled TEMPORARY
in code and text, and gets a follow-up item.

## Parallelism check (the user, 2026-10-07: standing, every session while working)

Whenever a session starts a work block, hands something off, receives a report, hits a hold (a timing window,
a conversion's build pause, a review wait) or is about to go idle, it checks:
- **What can run in parallel right now?** Independent briefs run at the same time in separate worktrees with
  disjoint owned-file lists, not one after another. During a build hold, coders keep writing code and research or
  analysis workers keep reading; only cargo waits.
- **Is anything idle that need not be?** A session with no worker running and nothing blocked on the user takes
  the next item from its queue, or tells Upstream Dev it has room.
- **Which provider has room?** `ds_spend.py status` (DeepSeek, Sol, Muse, MiMo) and the Claude plan pace; spread
  the work across them per the provider rules.
- Limits still apply: timing windows run alone, conversions pause new builds, unattended runs only when the user chooses them, one build folder
  per session, the DeepSeek pace.
- **Builds are the scarce part, not workers (2026-10-07: 26 cargo + 22 rustc at once left 3.3 GB RAM free and
  slowed a conversion to 4 MB/s). Every heavy cargo command goes through the machine-wide build gate:**
  `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p <crate>`
  (also for check, clippy, build, test; sessions, Claude subagents and DeepSeek workers alike). It waits for one of
  3 build slots (a cold build, whose target folder has no dependency artifacts yet, takes 2 at once, so at most one
  cold build runs; 2026-10-08: three cold Bevy builds crashed rustc out of memory) (at most 2 per session: `BUILD_GATE_OWNER`, else the label's track prefix p1/p2/p3/portal), at least 8 GB free RAM, at least 15 GB free on the drive the build writes to, and no hold, then runs and frees the slot on any exit. `... status` shows
  slots, holds, free RAM and converters. A conversion (extraction and convert phases) and a timing window set a
  hold (`... hold set conversion --owner <session> --reason <what> --until HH:MM`; timing windows add `--drain`, which lists gated runs still going and waits until they end) and clear it when done;
  this replaces per-session gate files and "no cargo" messages (still announce the job). Source:
  `tools/build_gate.py` (the copy in the gate folder is what everyone runs; Upstream Dev updates it). Workers
  launched from this checkout can no longer run heavy cargo directly (`.deepseek-agents.json`); the gate folder
  is in its `readOnlyDirs`, which `ds_impl` copies into every new coder worktree (workers started before
  2026-10-07 17:00 lack it: their lead runs their builds through the gate). Coder briefs write acceptance
  commands in the gated form, since `ds_impl` allows each brief command as written.
Upstream Dev runs this check across all sessions at least once an hour while the user is working, and at every
hold and unhold, and messages any session that is idle without a reason. (Since 2026-10-08 Team Lead holds
this duty; see "Team Lead".)
- **Every worker and helper shows in the app's background tasks (the user, 2026-10-08).** Launch each
  `ds-agent.ps1` / `ds_impl.ps1` worker (DeepSeek, GLM, Muse, MiMo) as its own Bash call with
  `run_in_background: true` and a description naming its label, provider and job; never start workers with `&`
  inside one shell command or from a detached script the panel cannot see. Claude helpers go through the Agent
  tool (background). A worker already running outside the panel gets a background watcher (a Bash loop waiting on
  its PID that prints the report head when it ends).
- **Batch reviews (the user, 2026-10-08).** Several merges or items needing review get about two reviewers in
  all (e.g. one GLM, one Muse), each covering every item with one verdict section per item, not one worker per item.

## Two tracks: Phase 2 first, portal second

Every task belongs to one track; name it in the brief, the commit and the handoff entry.

- **Phase 2** (the priority): contributions to upstream Phase 2 as its README, CONTRIBUTING.md and
  `docs/roadmap/02-completion-plan.md` describe. Worktrees under `C:\Modding\Mudcrab\worktrees\p2-*` and `D:\Mudcrab\worktrees\p2-*` (one per branch), branches
  `phase2/<topic>` cut from `main` (a mirror of `origin/main`), assets
  `C:\Modding\Mudcrab\SkyrimConverted-p2main-v24-doors` (since 2026-10-07 evening: p2main-v24's assets plus door_links from phase1/door-links-export 723c0c39; `C:\Modding\Mudcrab\SkyrimConverted-p2main-v24`, a full conversion with main f40ca1df, schema 24, stays for now; the older pre-#142 `D:\Mudcrab\SkyrimConverted-p2main` stays until the user decides), upstream's commit style, worker tasks `<kind>-<nnn>-p2-<slug>` numbered 500-599.
- **Portal** (parked 2026-09-28; **ACTIVE again since 2026-10-01** (the user): catch up with main and
  build a showcase of where the project and the portals are now): the user's seamless-door feature and demos. This checkout, branch `portal`, assets
  `C:\Modding\Mudcrab\SkyrimConverted-portal-rooms` (since 2026-10-07: full reconversion with room_graph, fresh UASTC/BC7 textures, #105 LOD; load check passed on portal-rooms 5dc5c9a2; the older `SkyrimConverted-portal-lod` is on the can-go list), `[claude]`/`[deepseek]` prefixes, worker tasks `<kind>-<nnn>-portal-<slug>` numbered 100-199 (then 600+).
  Portal schema numbers = 100_000 + main_version*100 + portal_revision (converter 102_401, world DB 100_701 at
  main 24/7); a main bump merged into portal moves the hundreds and resets the revision; never take main's raw numbers.
- **Phase 1** (added 2026-09-27, the user's new Phase 1 Dev session): improvements to upstream's
  finished converter (`docs/roadmap/01-asset-pipeline.md`), sent upstream like Phase 2. Worktree
  `C:\Modding\Mudcrab\worktrees\phase1`, branches `phase1/<topic>` cut from `main`, upstream's commit style, build
  targets and conversion output on D:, worker tasks `<kind>-<nnn>-p1-<slug>` numbered 800-899, then 1900-1999 (since 2026-10-08). It owns
  `crates/converter/**` and `crates/dummy-content/**`; `crates/shared/**` changes are agreed by message.
  Starting brief: `local/handover/phase1-dev.md`.
- **Phase 3** (added 2026-09-28, the user's Phase 3 Dev session; **PARKED for new work by the user,
  2026-10-01**: finish, package and send the launcher work already built, start nothing new): the launcher (upstream Phase 3
  item 3.4, `docs/roadmap/03-luau-and-ui.md`), sent upstream like Phase 2. Worktree
  `C:\Modding\Mudcrab\worktrees\phase3-safety`, branches `phase3/<topic>` cut from `main`, upstream's commit style
  (`feat(launcher): ...`), build targets on D:, worker tasks `<kind>-<nnn>-p3-<slug>` numbered
  900-999. It owns `crates/launcher/**` and `crates/converter-gui/**` (being folded into the
  launcher; the crate is then deleted); converter API changes go through Phase 1 Dev by message.
  Starting brief: `local/handover/phase3-dev.md`.
- Changes flow one way: `main`, `phase1/*` and `phase2/*` merge into `portal`, never the reverse. A
  Phase 1 or Phase 2 bug found on the portal is fixed on a `phase1/*` or `phase2/*` branch first.

Which work belongs to which track, the PR rules and the reasons: `docs/pr/two-tracks.md`. Read it
before starting any `phase2/*` branch or moving work between tracks.

## Team Lead (TRIAL from 2026-10-08, the user)

The user added a **Team Lead** session above the dev sessions and Upstream Dev, as an experiment
(kept if it works, reverted if not).

- **Priorities and hand-outs.** Team Lead sets the order of work across tracks and hands jobs to
  Phase 1 Dev, Phase 2 Dev, Phase 3 Dev, Portal Dev and Upstream Dev. A job the user approved in
  Team Lead's session and Team Lead hands out is a valid hand-out, like one from Upstream Dev.
- **Reporting.** Dev sessions report hand-offs, status, ETAs, findings and "I have room" to Team
  Lead (and the handoff log). Suggestions found in passing go to Team Lead in one line.
- **Upstream Dev keeps publishing.** The publishing rule below is unchanged: only Upstream Dev
  publishes, and only with the user's yes in its own session. Team Lead never runs `git push`,
  `gh pr create` or any GitHub write; it hands publish-ready items to Upstream Dev.
- **Oversight.** Team Lead runs the parallelism check across sessions (taking over the hourly
  duty from Upstream Dev), the morning brief and the evening decision table, and keeps the
  foundations check in every brief and review it hands out.
- Team Lead does not code on track branches itself; it plans, briefs, reviews and integrates
  decisions, and delegates per the DeepSeek level above.

## Working together: Phase 2 Dev, Portal Dev and Upstream Dev

> **Publishing rule (the user, 2026-09-27). Every session reads this, new ones first.**
> All publishing goes through **Upstream Dev**, and **nothing is published without the user's
> permission**. Publishing is anything other people can see: upstream PRs, issues, comments,
> reviews and replies (bots included), pushes to an open PR's branch, and pushes to the user's
> public fork (`BimingtonBill/mudcrab`, `myfork`), including `public/portal`.
>
> - Dev sessions (Phase 1 Dev, Phase 2 Dev, Phase 3 Dev, Portal Dev and any new one) never run `git push`,
>   `gh pr create`, `gh pr comment`, `gh issue create`, or any `gh api` call that writes to
>   GitHub. Hand the finished branch, commit and evidence to Upstream Dev by message instead.
> - Upstream Dev shows the user each item (a rendered preview for text, AI label at the top) and
>   publishes only after the user's clear yes, given in Upstream Dev's own session. A yes for
>   PUBLISHING relayed by another session does not count, and one yes covers only the item it was
>   given for. (Job hand-outs are different: a job the user approved in Upstream Dev's session and
>   Upstream Dev hands to a dev session is a valid hand-out.)
> - Exception (the user, 2026-09-28): Upstream Dev may reply to CodeRabbit (the bot) on our PRs
>   without asking, AI-labelled. Replies to people still need the user's yes.
> - **Standing yes for fix pushes (the user, 2026-10-02).** Upstream Dev may push, without asking
>   each time, a fix to one of OUR already-open PRs when all hold: it answers findings someone
>   raised (CodeRabbit, a reviewer, our own pre-open review), no new features; the dev session's
>   delta check passed and Upstream Dev read the change; it is a fast-forward of the PR branch;
>   the PR stays within the 3-round cap. Upstream Dev then answers the bot threads and lists every
>   such push in one summary table for the user the same day. Still the user's yes each time:
>   opening a new PR (drafts too), marking a draft ready, any reply to a person, anything on
>   someone else's PR, and anything that is not a fast-forward.
> - **Standing yes for comments on our own items (the user, 2026-10-07).** Upstream Dev may post
>   comments on issues and PRs opened by BimingtonBill without asking each time (AI-labelled, low
>   noise, listed in the evening table). Comments only: not reviews, body edits, new issues or PRs,
>   or pushes. Comments on anyone else's items, and anything that answers a person (even a new
>   top-level comment on our own item), still need the user's yes.
> - Reading GitHub (`gh pr view`, `gh api` GETs, `git fetch`) is fine for every session.

> **Handed-out jobs only (the user, 2026-09-30).** Dev sessions work only on jobs the user or
> Upstream Dev hands them. Anything else worth doing (a bug found in passing, a follow-up, an
> idea) is suggested to Upstream Dev in one line, with its evidence, and waits for the user's
> yes before any work starts. Finishing, testing and polishing a handed-out job is fine.

> **Before building and before sending (the user, 2026-09-30).** Announce first: before building
> anything that touches another contributor's area or open PR, Upstream Dev posts one line on the
> issue or Discord (the user's yes). Few open PRs: a soft cap of 5-6 of ours open as ready PRs plus 7-8 open drafts (the user, 2026-10-06; was about 5 in all); stack
> only when unavoidable, and say so at the top (Merge after / Stack line, stack label).

> **Daily rhythm (TRIAL from 2026-10-03, the user, 2026-10-02: kept if it works, reverted if not).**
> - **One reporting line.** Dev sessions report hand-offs, status, ETAs and findings to Upstream Dev
>   and the day log, not to the user. The user can still talk to any dev session directly at any
>   time, and the session answers normally; it just doesn't push unasked status to the user.
> - **Morning brief** from Upstream Dev, at the user's first message of the day, about 10 lines:
>   today's goal per track, who holds the PC for builds, timing runs or conversions and until when,
>   what the user will be asked in the evening, and anything blocked only on the user.
> - **Evening decision table** at 20:30 (the user, 2026-10-03; anything still running then rolls to
>   the next morning's brief): item, what it does in one sentence, state
>   (done / running / waiting on you / next), checks, and the ask. Small follow-ups (a flaky test,
>   a suggestion found in passing) are rows in it, not separate questions. Fix pushes under the
>   standing yes are listed there as done.
>   **Unattended runs: a preference, not a rule (the user, 2026-10-06, clarified 2026-10-08).** The user
>   prefers the PC and the sessions to work while they are present (late evenings included), so by default
>   don't plan builds, conversions, timing runs or workers for while they are away or asleep, and before they
>   sign off finish or stop running jobs, commit and log. The user may choose otherwise (e.g. an overnight
>   trial); when they do, the run follows every other rule here and leaves a full handoff entry.
> - **Quiet in between.** Upstream Dev handles hand-offs and logs them without messaging the user,
>   except for three urgencies: something public is wrong (our PR breaks main, a wrong push, a
>   maintainer waiting on us); a clash over the shared PC (a timing run against a build, a
>   conversion's publish); a tool outage blocking every track. Anything the user asks is answered
>   at once.
> - **Less work coming at the user (added 2026-10-03, same trial).** The evening table holds at most
>   about 8 decisions, most important first; the rest roll to the next day. Each morning brief
>   proposes at most 3 goals for the day that need the user's decisions; other goals wait
>   (finishing round-2 fixes on already-open PRs is allowed). Dev sessions may still run internal
>   work that reaches the user only as a finished result (an internal build, a demo, research).
>   "Track" keeps its meaning above (Phase 1, Phase 2, Phase 3, Portal); a goal is not a track
>   (changed 2026-10-03, the user). New work found during the day (an idea, a follow-up, a request)
>   goes into the next morning's plan instead of starting the same day, unless the user says
>   otherwise.

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
  **Research index: `local/research/INDEX.md`** (2026-10-04) catalogues all of it, about 300 notes
  by topic with date, status and keywords. Search it (`rg -i "<term>" local/research/INDEX.md`)
  before starting any research or brief, put the matching paths in briefs, and add one row whenever
  you file a new note. Skyrim reverse-engineering sources and the clean-room rules:
  `local/research/reference/skyrim-re-digest.md`.
- **Hand over finished work, not only news.** Phase 2 Dev sends Portal Dev merges it has already
  resolved (portal takes `phase2/*`, never the reverse); Portal Dev reports Phase 2 bugs with
  evidence. Ask the other dev to review a change that touches their area before merging it.
- **Shared findings go in shared places:** reference shots and grades (`local/reference/`), the
  look-gaps list (`docs/research/look-gaps-*.md`), research (`local/research/`), so any session can
  pick them up without asking.
- **Budgets are shared:** the DeepSeek credit is stretched to Tue 13 Oct (`ds_spend.py status` shows
  today's share, about $4.3-4.5 a day) and covers every session; say in the log when a session holds
  coders. **The split (the user, last changed 2026-10-01), as proportions of each day's share:**
  Phase 1 and Phase 2 about 38.5% each, Portal about 15.5%, Upstream Dev about 7.5%, Phase 3 none
  (packaging only). The launcher cannot enforce a per-session share,
  so each session keeps to its own; a session that finishes early says so in the log and the others
  may use the rest. It replaces the older "3 Phase 2 coders and 2 Portal coders a day". The Claude
  plan is shared too (one account): when its week runs ahead of pace, every session keeps Claude
  subagents to short lookups.
- **Other worker providers (the user, 2026-09-28):** every dev session may use them beside DeepSeek
  (details in the deepseek-agents skill, "Other providers"). **Meta Muse Spark:** the user opted
  Mudcrab in to its contributor tier (Meta may use what it is sent): `-Provider meta -Tier
  contributor`, panel `Muse <kind> #nnn: ...`. Since 2026-10-02 (the user, "not super sensitive
  data") it has full access like the other workers (`contributorFullAccess` in providers.json):
  it reads `local/` and `-AddDir` folders; still never captures, screenshots or game data. Use it for code research and analysis, standing audits, map and pitfalls refreshes, and a
  second review beside DeepSeek's, never as the only reviewer. Screenshots go to DeepSeek or MiMo
  (Muse runs only on its contributor tier here, the user's choice). **Xiaomi MiMo** (`-Provider xiaomi`,
  panel `MiMo ...`, plan ends 27 Oct): non-urgent background work only; rerun a failed MiMo run on
  DeepSeek. Coders stay on DeepSeek for now.

## While working

- Do not rewrite history (no amend, rebase or force-push on commits already made).
- **Division of labour (delegation level 4 of 5, set back to 4 by the user on 2026-09-24 after a morning at 3; spend follows the stretch in `ds_spend.py status`, and the generated delegation block governs; see the block above):** Claude plans, writes briefs, reviews and integrates; DeepSeek workers do the research, audits, analysis of completed profiling and acceptance runs, assigned documentation, and most implementation - including multi-file changes within a crate - each in its own worktree with an owned-file list (`tools/ds_impl.ps1`). Claude keeps integration, cross-crate architecture, the `crates/shared/` contracts, dependency changes, and the acceptance/sign-off runs. Every worker patch gets an independent review and harness-run acceptance before it is merged; a tuning or calibration task (thresholds, geometry, heuristics fitted to samples) must also pass on examples the worker never saw. Minimize Claude's investigation, review and repair load; DeepSeek cost is negligible to the user, so do not cut useful reasoning or review to save it. See `tasks/deepseek/delegation-policy.md`.
- Commit each meaningful step. On `portal` and this checkout's own files, prefix the commit subject with who wrote the change: `[claude]`, `[deepseek]`, or `[chatgpt]` if ChatGPT/Codex joins. Upstream-bound branches (`phase1/*`, `phase2/*`, `phase3/*`, `chore/*`, `docs/*`) use upstream's conventional commits instead (`fix(converter): ...`, no prefix). DeepSeek workers launched from `tasks/deepseek/` commit only their own paths and leave handoff notes in `docs/handoff/`.
- Task queue for DeepSeek workers and how to run it: `tasks/deepseek/README.md`. Review `[deepseek]` commits and their handoff notes before building on them.
- **The checks (one list; SEND-CHECKLIST.md and STYLE.md point here).** Before committing Rust changes:
  `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
  `cargo nextest run` for the crates you touched (`-p <crate> --all-targets`). Before handing a branch
  to Upstream Dev, and for every PR's Verification boxes: the whole workspace, `cargo nextest run
  --workspace --all-targets` plus `cargo test --workspace --doc` (nextest skips doctests). Each in the
  session's own build folder (see "Disk"); a combined or branch-vs-main check in a throwaway one.
- Keep `docs/` current. Findings go in the docs (`docs/research/`, `docs/roadmap/`), not only in chat.
- **xEdit is the record reference (the user, 2026-10-07; every session and worker).** xEdit
  4.1.5f is installed at `C:\Tools\SSEEdit` (`SSEEdit64.exe` GUI, `SSEDump64.exe` command-line
  dumper). Use it at these points:
  - **Before adding or changing how a Bethesda plugin record or field is interpreted** (converter
    or engine; our own Rust or database fields are not xEdit fields): read how xEdit
    defines it in `Core/wbDefinitionsTES5.pas` **at the `xedit-4.1.5f` tag** of TES5Edit/TES5Edit
    (the default branch is newer than what is installed) and cite "xEdit 4.1.5f defs, file:line" in
    the note, brief or PR. Where xEdit and UESP or our own reading disagree, say so.
  - **Bug triage when something looks wrong in game** (a mesh that shouldn't show, a wrong light,
    a missing door): check that record's fields and flags in xEdit first, to tell a data or
    converter fault from an engine fault. The user can open records in the GUI for a session;
    workers use the dumper. A single record or cell is triage; a systematic comparison is the next
    point. Rendering faults (shaders, passes) still go to a RenderDoc/PIX capture.
  - **Answer-key checks** (systematic comparison of converter output against xEdit's winning
    records) belong to Chef's #157/#158 programme upstream: help there (via Upstream Dev), never
    build a rival oracle.
  - **How workers use it:** the brief adds `-AddDir C:/Tools/SSEEdit` and an `-AllowTools` rule for
    the exact `SSEDump64.exe` command (smoke-tested 2026-10-07: `SSEDump64.exe "-d:<Data path>\" -top:N -dg:<GROUP> Skyrim.esm`
    prints decoded records with no window, 3 s for a small dump; a full `-check` of Skyrim.esm took
    398 s). Workers use the dumper only: the full `SSEEdit64.exe` (even with `-autoload -script
    -autoexit`) opens a window on the user's desktop and creates `SSEEdit Cache` / `SSEEdit Backups`
    folders inside the game's Data folder, so it runs only with the user present and told first; dump output goes to
    `D:\Mudcrab\scratch\ssedit-dumps` (OWNER.txt, class A), never into the game folder or
    `SkyrimConverted*`. Read only: never save a plugin (always "don't save" on exit). Running it on
    game data is CPU and disk work: not during a booked timing window.
  - **Clean room:** read and cite only. Never copy or translate xEdit code into Mudcrab, whatever
    its licence (the clean-room rule, `local/research/reference/skyrim-re-digest.md`). Using xEdit
    as a separate tool in the pipeline (scripts, or a fork kept in its own MPL-2.0 repository), as
    Chef proposed on 2026-10-07, is the user's decision taken with upstream, not a worker's.
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
  **Launched consoles (the user, 2026-10-07):** every console a session starts (conversion, bench,
  timing run, long script) opens visible and titled, and closes itself when the job ends, success or
  failure: no `-NoExit`, `cmd /k` or `pause`. Its output is teed to a log file and its exit code written
  to an exit file, so nothing is lost. Launch from PowerShell `Start-Process`, not Git Bash `start`.
  The window title says it is running and must not be closed ("RUNNING - do not close: <what>"), because
  the user may close a window that looks idle (2026-10-07: a conversion window closed during a silent stage).
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
    The GPU is shared with other projects too (see "Disk" below): before a timing run or a
    RenderDoc/PIX capture, run `python tools/disk_guard.py --gpu`. If a lock is held, message its
    owner session; Skyrim has priority and it frees the GPU within 5 minutes.
  - **A conversion's publish needs its output and staging folders closed:** publishing renames
    staging over the output folder, and on Windows that fails ("Access is denied") while any
    process holds a handle inside either: an engine reading `skyrim_world.db`, but also a size
    check (`du`, dust), an indexer, an Explorer window or antivirus. Nothing reads either folder while a
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

## Disk: folders, owners and clean-up (the user, 2026-10-02)

Deleting data is the risk here, so these rules lean towards keeping. Step 1 (now): owner files and a
read-only report; nothing is deleted automatically. A standing yes for class A may follow later,
only if the user gives one.

- **One build folder per session**, never shared between sessions; combined or branch-vs-main
  checks use a throwaway folder deleted afterwards. A shared folder mixed worktrees on 2026-10-02
  (an engine built against another branch's `shared` crate).
- **C:\Modding layout (the user, 2026-10-07):** top level holds only the main checkout, `.cargo`, the converted
  data (`SkyrimConverted*`) and the user's tools; every agent worktree or work repo goes in
  `C:\Modding\Mudcrab\worktrees\<name>` and every build folder in `C:\Modding\Mudcrab\builds\<name>` (no `wkj-` prefix for new ones).
- **Layout and naming (2026-10-07):** `local/LAYOUT.md` says where things go in `local/` (handover/,
  logs/, archive/, bin/) and how new files and folders are named (`<track>-<subject>-<YYYY-MM-DD>`, existing
  task ids unchanged); paths moved in the 2026-10-07 sweep are mapped in `local/disk/moves-2026-10-07.tsv`.
- **Every folder a session creates** under `C:\Modding` or `D:\Mudcrab` (build folder,
  worktree, evidence or data copy, scratch) gets an `OWNER.txt` the moment it is made
  (`python tools/disk_report.py --template`): owner, purpose, class, keep-until (`pr:N`,
  `date:YYYY-MM-DD` or `while-open`), links (yes/no).
  Converted data (`SkyrimConverted*`) keeps it BESIDE the folder as `<folder>.OWNER.txt`: a file inside a
  converted output makes a later `--reuse-assets` rebuild refuse it ("retained asset has no manifest provenance").
- **Classes.** A: rebuildable (build folders, scratch, throwaway checks). B: evidence or data
  copies; they go only after their PR has merged or closed plus a few days, and only once the
  evidence is safe in `local/review/` or the fork's `pr-assets`. C: converted game data, anything
  containing junctions, symlinks or hard links, and anything without an owner file: never on a
  routine list; each one is its own decision by the user, with the safe method written down.
- **Unknown means keep.** A folder without `OWNER.txt` is never treated as rubbish; it goes on the
  report's "Who owns this?" list.
- **Only the owner deletes**, only its own folders, only with the user's yes for the folders named
  (`tools/disk_report.py` and Upstream Dev only report). Never while the owner has a worker, build,
  engine or conversion running in or against it.
- **How to delete.** Worktrees with `git worktree remove` (it refuses uncommitted changes; never
  `--force`). Branches are not deleted as part of clean-up. Before any recursive delete, check the
  folder for junctions and symlinks; if there are any, stop: it is class C. A recursive delete can
  follow a junction and wipe the real folder behind it (`p2main-doors` links into Phase 2's
  conversion).
- **Never touch:** the repo checkouts themselves, the game install, `SkyrimConverted*` (except by
  the user's own decision), and the user's own tools in `C:\Modding` (Wabbajack, Modlists,
  Bethini Pie).
- **When clean-up happens:** when a PR merges or closes, Upstream Dev tells the owner, which offers
  that PR's worktree, folder and evidence for deletion; at the end of every session, each session
  removes its own throwaway (class A, expired) folders; before a build or conversion, a session
  below ~40 GB free on that drive offers its own expired folders first. Upstream Dev runs
  `python tools/disk_report.py --size-protected` at the start of each day and gives the user the
  "can go" list, plus the biggest never-touch folders as the user's own decisions.
- **Log every deletion** in that day's team log: path, size, owner, reason.
- **The PC is shared with other projects** (the user, 2026-10-05): Film VFX (`D:\VFXEnhance`) and
  DAO Xbox sessions run beside ours. Shared rules and ledger: `D:\VFXEnhance\coordination\RESOURCES.md`.
  Keep at least **10 GB free on C: and D:**; a job that would leave less than 15 GB messages the
  other sessions first; anything that will grow by more than 5 GB gets a ledger line before it
  starts; no session deletes another project's files; messages only on events.
- **Pre-flight before every big job:** `python tools/disk_guard.py --drive D: --need <GB>` (add
  `--ledger "<what and where>"` for jobs over 5 GB). Exit 1 means stop: free space first (owner's
  yes) or wait. Typical needs: a new build folder 15-20 GB, a coder worktree about 20 GB, a full
  conversion about 67 GB of staging beside the output. Worker briefs that build say "check
  `df -h /d` before each build; below 15 GB, stop and report".
- **Measuring folders:** never Git Bash `du` on big folders (slow on Windows, times out, keeps
  running after a timeout and once blocked a publish). Use dust (scans 210 GB in about 25 s; path
  in `tools/disk_report.py`, or set `DUST`; always add `2>/dev/null`), or `python
  tools/disk_report.py --size-protected`, which also sizes converted data and other never-touch
  folders with dust and skips that while a converter or launcher runs. WizTree is installed too,
  but its fast mode needs an admin prompt: only with the user present.

## At the end of every session

Commit your work, then add a dated entry at the top of `docs/handoff/LOG.md`: what changed, what was tested, whether a worker is still running, and the next step. Then delete your own expired class A folders (see "Disk" above; the user's yes covers only folders already named in a "can go" list) and log them.

## Handoff log

Moved to `docs/handoff/LOG.md` (newest first) so this file stays short; every session still adds its entry there.
