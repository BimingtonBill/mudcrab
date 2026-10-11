# websearch-1139: Bevy upstream check before proposing a render change

Track: Portal (Mudcrab Plus), upstream offer to Bevy. Read-only web research. Cite a URL for every claim.

We plan to offer Bevy (github.com/bevyengine/bevy) a change: in bevy_render's `src/renderer/render_context.rs`, command
encoders queued by render systems are currently finished (`wgpu::CommandEncoder::finish`) one after another on the
render thread when the deferred `RenderContextState` is applied. We'd queue them unfinished in the same order and finish
them in parallel on the ComputeTaskPool just before the single queue submit (submission order unchanged).

Answer each:
1. Bevy `main` branch today: fetch `crates/bevy_render/src/renderer/render_context.rs` (raw.githubusercontent.com/bevyengine/bevy/main/...).
   Summarise how `RenderContextState`, `PendingCommandBuffers` (or whatever replaced them), `flush_encoder`/`queue`/`take`
   work now, and whether encoders are still finished serially. Quote short line ranges with line numbers. Same for the
   `v0.20.0` tag. Note anything that already finishes encoders in parallel (e.g. `add_command_buffer_generation_task`,
   `CommandBufferGenerationTask`, parallel finish).
2. Existing Bevy issues, PRs or discussions about parallel command encoder finish / parallel command buffer recording /
   render thread time in `finish` / "RenderContext" parallelism (search github issues and PRs, open and closed, 2023-2026).
   List number, title, state, date, one line each.
3. Bevy's contribution policy on AI-assisted or AI-generated contributions: CONTRIBUTING.md, the contributor guide
   (bevyengine.org/learn/contribute), any AI policy page. Quote the exact rule in under 30 words and give the URL and date.
4. Bevy's PR template and whether a perf change normally starts as an issue or directly as a PR; any "needs benchmark"
   or `tracy` evidence expectations.
5. Is `WgpuWrapper` gone on main (PR #25512)? What replaced it in render_context.rs?

Report: one section per question, facts with URLs, then "Uncertain:" listing anything not verified.
