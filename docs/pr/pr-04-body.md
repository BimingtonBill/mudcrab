# PR 4 - fix(engine): an empty converted model is not a bounds failure

Branch `phase2/empty-model-bounds` (`6d14613` on `2105bfd`). Written by DeepSeek impl-501 from fork
commit `2b69616`, reviewed by Claude. Not yet sent.

---

A model whose NIF has no renderable geometry converts to an empty glTF scene with no aggregate
bounds. The readiness scan counted every reference to one in `transform_bounds_validation_failures`,
a hard gate, although there is nothing to place, draw or bound. Two such models are enough to fail a
run.

A reference is now counted in a new `empty_model_references` metric instead, but only when both are
true: the model has no converted bounds, and its spawned scene holds no render primitive. A model
with geometry but no bounds, or with bounds but an empty scene, still fails.

**Tests** (no game data, a real Bevy `App` running `track_asset_readiness`): an empty scene is
counted and nothing fails; a scene with a mesh but no bounds still fails; a bounded model with an
empty scene still fails.

Serves Marco 0 item 2 (non-renderable content is not a real failure) and Marco 5 (zero streaming
failures) of `docs/roadmap/02-completion-plan.md`.
