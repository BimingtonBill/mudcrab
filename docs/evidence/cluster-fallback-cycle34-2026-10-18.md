# Cluster fallback: cycle34 functional failure

Source: `fd89ec6c62ef183f7c5a9d5e31bfbc16c06a5448`.
Normal quick-profile image SHA256:
`99318622a5b52ca9f292b34791bc72cf92f42d8cb4291ea9e4a74431db0b126a`.
This TEST run used Showcase quality, seamless doors and cluster terrain, with
optional distant ray shadows off. It is neither release performance evidence nor
demo acceptance.

Retained local evidence: `local/vg7/cycle34-fallback-gpu.log`,
`cycle34-demand-inputs.json`, `cycle34-primitive-outcomes.json` and
`C:/Modding/Mudcrab/builds/vg7-cycle34-fallback-runtime` (image identity, process
arguments and shots log). The process exited naturally with code 1. All three
shots failed the existing 30-second readiness deadline; no screenshots were
produced. No deadline, reach or quality setting was relaxed.

## Observations

- Door request at 22:18:10.991047Z; open portal rendering at 22:18:12.177113Z.
  This proves that the door can respond while certification is pending; it does
  not prove smooth rendering or complete destination geometry.
- Package ready at 22:19:53.704775Z, about 108 seconds after initialization.
  It reports 1,230 meshes, 38,126 instances and 1,485 batches; 260 terrain batches
  subsequently become ready. Certification time is not isolated from startup.
- Initial far demand selects 213,978 references, with 3,152 existing near sources.
  The three shot failures still have 5,960 / 5,837 / 5,715 queued far queries,
  one pending far query each, and 256 / 332 / 404 failed far sources.
- The fallback incorrectly requests model-free references and marks the ordinary
  loader's intentional no-scene outcome as a failure. The correction uses the
  same model-path resolver as ordinary loading. Missing files with a resolved
  path must continue to be demanded and fail visibly. Coverage and unknown
  expected-primitive sets are unchanged.

## Read-only data inventory

The following counts cover all placed Tamriel references in the current SQLite
snapshot, not the exact camera horizon. They are data inventory, not runtime
certification or drawable proof.

| Source primitive outcome | References |
| --- | ---: |
| All clustered |14,766 |
| All rejected |153,971 |
| Mixed clustered and rejected |14,900 |

The current `cluster_build` threshold is 1,024 triangles. Rejected primitive rows
include 359,123 `below_triangle_threshold`, 7,416 unsupported material features,
176 non-opaque,19 missing attributes,9 non-rendering and3 effect shaders. Rows
are weighted by references and are not unique model counts. Removing the
intentional no-model demand does not resolve this larger stock workload.

The coverage inventory also retains16,061 not-eligible references with no ordinary
model path. References with an ordinary path include 183,637 expanded and 29,046
not-eligible entries. Alternate textures, reference overrides and unknown inputs
remain guarded; no permission to suppress them follows from these counts.

## Next bounded proof

The existing converter supports `--triangle-threshold 1`. Test identical invented
core/GLB inputs at thresholds 1,024 and 1, including a one-triangle boundary and
the reader's 18-triangle grid. Require real stage, authority, publication, reader
and payload checks. Material, override and unknown refusals must remain intact;
source/core bytes, warnings and unrelated extension entries must be preserved.

Only after this proof, prepare a separate new-output derivation preserving
`--terrain --include-trees`, the current declared scope and verified raw DDS
contract. An existing cluster store cannot be its source. Real regeneration is
still held; disk capacity and explicit authorization are prerequisites. A lower
threshold admits more source geometry and changes builder identity; it does not
reduce mesh quality or establish a performance gain.
