# review-1138-portal-1132-parallel-finish (track: Portal / Mudcrab Plus only). Read-only review.

## Goal
Review impl-1132 on branch portal-impl-1132-parallel-finish (`git log portal..portal-impl-1132-parallel-finish`,
`git diff portal...portal-impl-1132-parallel-finish -- . ':!vendor'` plus `git show 15153aee` for the vendored patch).
Brief: tasks/deepseek/impl-1132-portal-vendor-bevy-render-parallel-finish.md. Design:
local/research/engine-foundations/analysis-1131-parallel-encoder-finish.md. The user approved vendoring bevy_render
0.19.0 into portal only with this patch behind a flag (off = stock), and a Vulkan-only switch.

## Check, with file:line
1. Commit 9f6e9982 is a pristine copy: compare vendor/bevy_render-0.19.0 to the registry copy
   C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/bevy_render-0.19.0 (only the README added).
2. The patch (15153aee): with the switch OFF, is the code path identical to stock (same finish points, same order)?
   With it ON: is the submission order of all command buffers exactly the stock order, including interleaving with
   add_command_buffer, push_encoder (diagnostics) and anything else that pushes into PendingCommandBuffers? Is the
   scope safe (no deadlock if take() runs on a pool thread, no blocking on queued tasks), errors still routed, no
   lost encoders on early returns or panics? Is the AtomicBool read where it must be (once per take vs per push)?
3. Engine (43023c1c, 3295ffc5): --render-parallel-finish and --render-backends parsing and help; the setter is called
   once before rendering starts; the Vulkan path builds RenderPlugin with WgpuSettings.backends = VULKAN and clears
   VALIDATION_INDIRECT_CALL only in release. Is clearing it correct and safe on Vulkan (Bevy's own reasoning,
   bevy_render settings.rs:130-144)? What breaks if Vulkan is unavailable on a user's machine?
4. The real-device order test (tests/parallel_finish_order.rs): does it actually prove the order with the switch on,
   or could it pass when the order is wrong? Does it silently skip without an adapter?
5. The MUDCRAB-README says one patched file; renderer/mod.rs also changed (3-line re-export). Note it.
Foundations: what shortcut does this take, and what would break for a mod or another worldspace?

## Scope
Read only. Do not edit, build or run anything.

## Done when
Every check above has a finding or an explicit "no issue", with file:line.

## Report
Verdict (merge / fix first), findings ranked, each with file:line and a concrete failure scenario.
