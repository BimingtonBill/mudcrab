# Goal
Review commit 1a56a701 on branch phase2/remote-dev-flag-v2 (worktree D:/Mudcrab/worktrees/p2-remote; `git -C D:/Mudcrab/worktrees/p2-remote show 1a56a701`): an opt-in Bevy Remote Protocol server for the engine behind a `remote` cargo feature and `--remote`. Is it correct and safe?

# Context
- Intent: off by default; the default build's dependencies are unchanged; the server binds to 127.0.0.1 only; `--remote-port` implies `--remote`; ports 0 and 15703 are refused; an engine built without the feature refuses `--remote`. World components and two resources derive Reflect and are registered. It is a port of an older commit f8a5c03 onto current main.
- Already checked by the lead: a live run of a feature build with `--remote` on real data answered `rpc.discover` (23 methods), `world.query` for FormId (1,100 entities) and `world.get_resources` StreamingMetrics.

# Scope
Read only. Run only `git log`, `git diff`, `git show` and reading/searching tools.

# Done when
Checked, with file:line: (1) no network exposure beyond loopback, and nothing listens without the feature and the flag; (2) the refusal paths and the port checks; (3) Reflect derives that could change behaviour or fail to compile under the feature (for example Mat4 or IVec2 reflection, and the serde interplay); (4) the Cargo.lock change is limited to what bevy_remote needs; (5) the tests test the behaviour. The verdict is one of accept, fix first, reject.

# Report
Findings as `path:line - problem - why it matters`, most serious first. Under 350 words.
