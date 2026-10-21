# Goal
Rebuild the old commit f8a5c03 ("feat(engine): opt-in Bevy Remote Protocol for local inspection behind --remote") on current upstream main: a `remote` cargo feature (off by default) that, with `--remote`, serves Bevy's Remote Protocol on 127.0.0.1 so developers can query a running engine.

# Context
- Read the old commit in full: `git show f8a5c03` (it touches crates/engine/Cargo.toml, Cargo.lock, crates/engine/src/{app.rs,config.rs,lib.rs,remote.rs,streaming.rs,world/components.rs} and docs/roadmap/02-core-engine.md). Port its behaviour; do not copy old surrounding code, because main has changed a lot since (the #65 staging merge: lights, sky, water, streaming pacing, render layers).
- Behaviour to keep: feature `remote = ["bevy/bevy_remote"]`, off by default so the default build's dependency set is unchanged (check `cargo tree -p engine -e features` or the lock diff only adds what the feature needs); `--remote` adds RemotePlugin + RemoteHttpPlugin bound to 127.0.0.1 on port 15702; `--remote-port <port>` picks another port and implies `--remote`; one startup log line names the address; an engine built WITHOUT the feature refuses `--remote`/`--remote-port` with an error saying to rebuild with `--features remote`; ports 0 and 15703 (Bevy's fixed render-world port) are refused. World components and the StreamingMetrics and RenderOrigin resources derive `Reflect` and are registered so the protocol can query them. Check which of the old list still exist on main and add any new world components in crates/engine/src/world/components.rs that belong in the list.
- Parser: main's config.rs parser ignores bad values silently. Keep `--remote`/`--remote-port` parsing in main's current style (small, two match arms); a separate open PR replaces the parser later.
- Docs: add the old "Inspecting a running engine (Bevy Remote Protocol)" section to docs/roadmap/02-core-engine.md (check the file still exists on main; if the roadmap was renamed, put it in the engine roadmap page that exists), including the no-authentication, local-only warning.

# Scope
Owned files: crates/engine/Cargo.toml, Cargo.lock, crates/engine/src/app.rs, crates/engine/src/config.rs, crates/engine/src/lib.rs, crates/engine/src/remote.rs, crates/engine/src/streaming.rs, crates/engine/src/world/components.rs, docs/roadmap/02-core-engine.md
- Change only the owned files. No other new or upgraded dependencies beyond enabling Bevy's own `bevy_remote` feature.
- No git commands: Claude reviews the diff and integrates it.

# Done when
Acceptance: cargo fmt --all -- --check, cargo clippy -p engine --all-targets -- -D warnings, cargo clippy -p engine --all-targets --features remote -- -D warnings, cargo nextest run -p engine, cargo nextest run -p engine --features remote
- Tests: option parsing (default off, `--remote`, `--remote-port N` implies remote, port 0 and 15703 refused, refusal without the feature under `#[cfg(not(feature = "remote"))]`), and that the reflected types are registered (a small App test under the feature).
- If practical, run `cargo build -p engine --features remote --bin engine` and start it with `--remote --streaming-fixture` for a few seconds, then query `rpc.discover` with a short Python script using urllib (POST JSON-RPC to http://127.0.0.1:15702). Report the output. Skip this step if the engine cannot be started from your environment, and say so.

# Report
- What you changed and the decisions that weren't obvious (especially which components got Reflect and any that could not).
- The commands you ran and what they printed.
- What you assumed, and the cheapest check that would show the change is wrong.
