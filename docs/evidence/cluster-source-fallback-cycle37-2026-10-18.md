# Runtime stock fallback for Complete geometry

Complete coverage certifies the source primitive mapping. It does not establish
that a view can draw the cluster representation. A real producer/loader fixture
demonstrates the distinction: adding authored `rotateAboutUp` node extras to an
otherwise accepted opaque grid leaves its geometry Complete, while the actual
loader template correctly fails static cluster qualification.

## Kept correction

Source extraction records definitive template/material refusals separately from
the positive template vector. Pending `false` becoming loaded-invalid `false`
must still produce new fallback demand. The render world projects these refusals
through actual Object RecordKeys and the matching certificate into exact
reference IDs. The main world requests their ordinary source scenes through the
existing sparse loader. A report grants no visibility, draw or suppression proof.

Reports use a weak, exact certificate allocation and one-use delivery freshness.
Unchanged extraction refreshes delivery without cloning the reference set or
advancing its revision. Missing or mismatched feedback leaves an explicit
`pending_far_stock_qualification` count. Whole-view readiness and requested-cell
residency wait for it; existing per-view geometry/material/pass proof is intact.
Loading source templates remain covered by the existing package dependency
pending/failure counters. An empty negative set does not certify a template.

The projection cache watches scene changes, negative batches and normal-map
requirements. Tangent preparation also needs an explicit dependency: workers
complete shared OnceLocks without changing the ECS scene tick. Unresolved mesh
IDs are retried without generating tangents on the frame thread. Qualification
feedback is withheld until their results arrive; a completed failure forces
reprojection. Missing required meshes or invalid record joins also withhold
feedback and log their reason.

Complete references already instantiated near the player retain their exact
source entity on retirement, including runtime pose edits, child scene and
pending handles. Newly arriving ordinary references enroll even without camera
movement. Once admitted, a source remains owned within the demanded horizon
through template recovery; the existing per-view handoff decides what draws.
Unrelated world feedback changes that leave effective local demand unchanged
preserve generation and in-flight sparse requests.

## Evidence and limits

- Independent full production and test diff reviews pass.
- Formatting and engine all-target/all-feature quick-profile clippy pass.
- Engine all-target nextest: **2,044 passed, eight existing skips**. Logs:
  `local/vg7/cycle37b-clippy.log` and `cycle37b-nextest.log`.
- Eleven added tests include actual producer, GLTF loader and ExtractSchedule
  inputs; exact certificate/freshness/withdrawal checks; a cached tangent result
  arriving without an ECS tick; and actual near/far planning and retirement.
- The tangent failure is an injected worker result testing cache delivery, not
  evidence of a particular real GLTF failing tangent generation.

These are source and CPU integration checks. No current-image GPU tour, visual
acceptance, far-shadow acceptance or 120 fps result is claimed. The accepted
launcher remains unset. No data regeneration, dev merge or publication occurred.
