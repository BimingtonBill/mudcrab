# Goal
Track: Phase 2 (for upstream). The converter publishes the texture and shader animation Skyrim puts
on effect and lighting materials, so hearth flames, lava, steam and glow cards can move. Today the
controllers are dropped, and every such card is one still frame.

# Context
- Your checkout is cut from `phase2/material-publication` (unreleased converter schema 15: effect
  materials, blend factors, refraction exclusion). Ride that schema; do not bump it. Use upstream's
  commit style (`feat(converter): ...`), with no `[deepseek]` prefix.
- What Skyrim stores (decoded from `meshes/clutter/woodfires/fireplacewood01burning.nif`, extracted at
  `C:/Modding/Mudcrab/SkyrimConverted-upstream15/vfs/meshes/clutter/woodfires/`):
  - A shader property (`BSEffectShaderProperty` or `BSLightingShaderProperty`) has a controller
    reference (its `NiObjectNET` controller field). Controllers chain through `next_controller`.
  - `BSEffectShaderPropertyFloatController` / `BSLightingShaderPropertyFloatController`: an
    `NiTimeController` (flags, frequency, phase, start time, stop time, target), then an
    interpolator reference, then the controlled variable (u32).
  - Hearth flame card: flags 0x48 (loop mode = (flags >> 1) & 3: 0 cycle, 1 reverse, 2 clamp),
    frequency 1.0, phase 0, start 0, stop 5.6667, variable 8 (V Offset). Its
    `NiFloatInterpolator` → `NiFloatData` holds QUADRATIC keys (key type 2: time, value, forward
    tangent, backward tangent) (0, 0) → (5.6667, 1.0). A second card is the same with stop 4.2667.
  - Effect-shader variable enum (nif.xml `EffectShaderControlledVariable`): 0 emissiveMultiple,
    1 falloffStartAngle, 2 falloffStopAngle, 3 falloffStartOpacity, 4 falloffStopOpacity, 5 alpha,
    6 uOffset, 7 uScale, 8 vOffset, 9 vScale.
  - Lighting-shader enum (nif.xml `LightingShaderControlledVariable`): look it up in the vendored
    parser or nif.xml naming; it includes refraction strength, environment map scale, glossiness,
    specular strength, emissive multiple, alpha and U/V offset/scale. Map the overlapping ones to
    the same names; name the others in camelCase.
  - Key types: 1 LINEAR (time, value), 2 QUADRATIC (time, value, forward, backward), 5 CONST
    (step).
- The vendored parser (`vendor/project-wormhole-nif`) already parses these blocks
  (`NifBlock::BSEffectShaderPropertyFloatController`, `BSLightingShaderPropertyFloatController`,
  `NiFloatInterpolator`, `NiFloatData`). Read their structs before writing any byte-level code.
- The contract, agreed with the engine side (Portal Dev implements playback): a material-level glTF
  extension on the shape's published material:
  ```json
  "OPEN_SKYRIM_material_animation": {
    "channels": [{
      "variable": "vOffset",
      "interpolation": "LINEAR" | "QUADRATIC" | "STEP",
      "times": [0.0, 5.6667],
      "values": [0.0, 1.0],
      "tangents": [[forward, backward], ...],
      "loop": "cycle" | "reverse" | "clamp",
      "frequency": 1.0, "phase": 0.0, "start": 0.0, "stop": 5.6667
    }]
  }
  ```
  `tangents` is present only for QUADRATIC. List the extension in `extensionsUsed` (not
  `extensionsRequired`), as the other Mudcrab extensions are. One channel per float controller
  in the chain. A controller whose interpolator or data is missing, or has an unknown key type or
  variable, is skipped and counted: never fail the conversion for it.
- Find where materials are published (`crates/converter/src/material.rs`: the material contract
  `ValidatedNifMaterial`, `build_effect_material` / `build_lighting_material`, and
  `publish_material`). The contract is built per shape from the NIF, so collect the channels
  there and emit them at publication.

# Scope
Owned files: `crates/converter/src/material.rs`, `crates/converter/src/mesh.rs` (only if the
contract needs NIF access it lacks), `crates/dummy-content/src/nif.rs` (a fixture effect shape with a
float controller), `crates/converter/tests/fixture_round_trip.rs` or a new
`crates/converter/tests/fixture_material_animation.rs`, `docs/specs/converters/nif-to-gltf.md` (document
the extension), `docs/handoff/impl-566-p2-material-animation.md`. No engine changes, no schema bump,
no changes to the vendored parser unless a field you need is unparsed (then say so first).

# Done when
- Tests that fail on the old code and pass on yours:
  1. A fixture effect shape with a V-offset float controller (quadratic keys) converts to a material
     carrying the extension with the right variable, times, values, tangents, loop and timing.
  2. A linear-key controller and a U-scale variable map correctly; an unknown variable is skipped
     and the conversion still succeeds.
  3. A material without controllers carries no extension.
- `cargo fmt --all -- --check`, `cargo clippy -p converter --all-targets -- -D warnings` and
  `cargo test -p converter` pass.
- If you can, convert the real `fireplacewood01burning.nif` in a test marked `#[ignore]` (it reads
  the extracted file above) and show the two flame materials' extension values in the report.

# Report
- Files changed and the approach in 4 lines.
- The lighting-shader variable mapping you used, with its source.
- Each test's name and what it proves, and the real-file values if you ran it.
