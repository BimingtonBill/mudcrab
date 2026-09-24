# Goal
Write one plain-language explainer per staged Phase 2 fix, so the user (who is not a developer
and knows Skyrim modding well) understands each change before any of it is offered upstream.

# Context
- The staged fixes are git branches in `C:/Modding/wkj-phase2`. Each is cut from upstream
  `main`, except where noted. Read each branch's commits with
  `git -C C:/Modding/wkj-phase2 log --format=%B main..<branch>` and its diff with
  `git -C C:/Modding/wkj-phase2 diff main...<branch> --stat`. The branches:
  - `phase2/pruned-texture-complete`
  - `phase2/empty-model-bounds`
  - `phase2/shared-converter-schema`
  - `phase2/scripts-schema-from-source`
  - `phase2/material-publication` (includes the new effect-shader fix, 082a9e4)
  - `phase2/renderable-base-types`
  - `phase2/vertex-alpha-not-opacity` (still needs discussion with upstream)
  - `phase2/terrain-samplers-weights`
  - `phase2/terrain-seam-weld` (includes the new city-land fix, 4fe8f6e)
  - `phase2/sun-shadow-cascades`
  - `phase2/lod-mesh-containers`, `phase2/lod-database`, `phase2/lod-engine-terrain` (distant land, steps 1-3)
  - `phase2/resume-staging-validation`
  - `phase2/staging-hard-links`
  - `phase2/default-water-height`
  - `phase2/refr-rotation` (already open upstream as PR #25; one local follow-up commit)
- The one-line "plain" summaries in `local/roadmap/phase2.json` are a good starting point. Use
  them, but check each against the commits.
- The user's rules for anything public: they write and send public text themselves. AI-drafted
  prose that gets kept is labelled, e.g. "Drafted with Claude / DeepSeek agents, reviewed and
  edited by me". This document is private: it explains things to the user. It is not a PR body.

# What to write
`local/teach/phase2-pr-explainers.md`, with one section per branch:
- **What it fixes**, in one or two sentences, with a Skyrim-modding analogy where one fits (load
  order, a mod missing meshes, xmarkers, DynDOLOD, texture replacers, ENB lighting...).
- **What you'd see**: the visible or practical difference.
- **How big**: files and lines, from `--stat`.
- **What upstream will ask**: the one question a reviewer is most likely to raise, e.g. "does
  this need a reconversion?" or "why a schema bump?", and the answer.
- **Depends on**: other branches it needs first, if any.

End with a suggested sending order, one line of reasoning each: fixes first, small before big,
dependencies respected.

Keep it plain: no code, and no jargon without a one-phrase explanation.

# Scope
Write only `local/teach/phase2-pr-explainers.md`. Read anything. Use git only in read-only ways.

# Report
The file path, and one line per branch with its one-sentence "what it fixes".
