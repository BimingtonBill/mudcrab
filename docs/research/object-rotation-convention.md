# Which Euler order and signs Skyrim `REFR` rotations use

Commissioned as `research-023` after the lead's fix `933b95f` (object rotations turn
clockwise). That fix chose `Rx(-x) * Ry(-y) * Rz(-z)` in Creation space
(`crates/shared/src/coordinates.rs:37`), but only the yaw half was measured, from load
doors. This note settles the tilt half — the order in which x and y combine, and their
signs — from Skyrim.esm's own data, and audits position/scale placement beside it.

**Verdict: the engine's current convention is correct, and no change is needed.** Up
vectors of 82,731 tilted references on sloped ground in Tamriel are a median of 8.18°
from the terrain normal under the engine's convention, against 19.45° for the nearest
alternative, and it beats each of the four alternatives on 78.5–88.7% of individual
references. The formula to keep is given in [The formula](#the-formula-the-engine-should-use-keep-the-current-one).

## Method

Tilted references are the evidence. Rocks, drifts, shrubs, stumps and trees are placed
standing on sloped ground, and their local up axis (+Z of the model) follows the terrain
normal where they stand. So for each reference:

1. take its `REFR/DATA` angles from the converter database
   (`C:/Modding/SkyrimConverted/skyrim_world.db`, read-only);
2. bilinearly sample the LAND heightmap of its own cell for the ground height, and take
   the terrain normal from central differences of the 33x33 `VHGT` grid (128 units
   between vertices, the decoding in `tools/research/uesp_space_map.py:80`);
3. keep it only if it stands on the ground (|`pos_z` − terrain| ≤ 128 units, so "it was
   tilted to match the slope" is a fair assumption) and the ground under it is sloped
   (terrain normal more than 5° from vertical, or there is nothing for a tilt to follow);
4. rotate the model's up axis `(0,0,1)` by each candidate convention and measure the
   angle to the terrain normal.

Everything is done in Creation space, where the terrain is defined, so the
Creation-to-runtime basis never enters the comparison.

The engine's function is **ported, not re-derived**: `axis_angle`, `multiply_quaternions`,
`conjugate_quaternion`, `normalize_quaternion` and the basis conjugation are copied line
for line from `crates/shared/src/coordinates.rs:37` into
`tools/research/rotation_convention.py:113`. The port is checked two ways every run:
the module's own Rust tests are re-run against it (all pass), and its rotation matrix is
matched against all 16 candidates — it equals `XYZ -/-/-` to 1.0e-15, the nearest other
candidate being off by 1.99.

## Results

Tamriel (worldspace 60), **all 11,186 LAND cells**: 228,187 exterior references with a
model, 110,498 tilted more than 3°, 91,386 of those standing on the ground, 82,731 on
ground sloped more than 5°. Median tilt 16.0°, median terrain slope 17.1°, median
|`pos_z` − terrain| 18 units.

Conventions are written `order signs`, where `XYZ` multiplies the matrices left to right
(the vector meets `Rz` first) and `ZYX` applies `Rx` first. `-x` means the x angle is
negated before it is used. `a`–`e` are the candidates the brief named.

| convention | median | mean | <10° | <20° |
|---|---:|---:|---:|---:|
| **XYZ -x-y-z  [a] — the engine today** | **8.18°** | **14.46°** | **58.0%** | **81.7%** |
| XYZ -x-y+z | 8.18° | 14.46° | 58.0% | 81.7% |
| XYZ -x+y-z | 21.02° | 27.21° | 19.3% | 47.7% |
| XYZ -x+y+z | 21.02° | 27.21° | 19.3% | 47.7% |
| XYZ +x-y-z | 21.81° | 28.28° | 18.4% | 45.7% |
| XYZ +x-y+z | 21.81° | 28.28° | 18.4% | 45.7% |
| XYZ +x+y-z  [c] | 31.77° | 37.75° | 4.4% | 25.2% |
| XYZ +x+y+z | 31.77° | 37.75° | 4.4% | 25.2% |
| ZYX -x-y-z  [b] | 19.45° | 25.84° | 23.1% | 51.4% |
| ZYX -x-y+z | 19.44° | 25.81° | 23.4% | 51.3% |
| ZYX -x+y-z | 21.97° | 28.14° | 17.8% | 45.3% |
| ZYX -x+y+z | 21.91° | 28.02° | 17.7% | 45.5% |
| ZYX +x-y-z | 21.02° | 27.39° | 19.5% | 47.6% |
| ZYX +x-y+z | 21.12° | 27.55° | 19.5% | 47.4% |
| ZYX +x+y-z  [d] | 23.43° | 29.54° | 15.8% | 42.3% |
| ZYX +x+y+z  [e] — the engine before 933b95f | 23.36° | 29.54° | 15.6% | 42.4% |

Rows that differ only in the z sign tie exactly: the up axis is invariant under `Rz`, so
a reference's tilt says nothing about the yaw sign. The yaw sign was settled separately
by the doors under `933b95f` and by `crates/shared/src/coordinates.rs:172`; nothing here
contradicts it.

Data: `docs/research/data/object-rotation-audit.json`.

### By kind of object

Median angle under the five named candidates. Flora, rock, snow and tree should follow
the slope; clutter (crates, barrels, carts) mostly stands upright and scores badly under
every convention, which is the expected control.

| group | n | **(a)** | (b) | (c) | (d) | (e) |
|---|---:|---:|---:|---:|---:|---:|
| flora | 41,025 | **7.37°** | 18.09° | 30.16° | 21.63° | 21.66° |
| rock | 15,566 | **9.30°** | 22.57° | 37.22° | 27.69° | 27.79° |
| snow | 13,008 | **9.96°** | 23.94° | 39.13° | 28.82° | 28.89° |
| other | 6,291 | **5.44°** | 15.25° | 24.33° | 17.92° | 17.95° |
| tree | 3,122 | **8.29°** | 15.64° | 24.25° | 19.14° | 18.97° |
| clutter | 3,062 | **27.97°** | 31.87° | 36.34° | 33.57° | 33.97° |
| building | 657 | **13.19°** | 17.48° | 21.28° | 18.70° | 18.65° |

The order of the groups is what it should be: geometry that follows the terrain scores
near zero, geometry that ignores it does not. Clutter is 28° even under the best
convention — a crate is tilted on purpose, not to match a slope — so it is evidence
about *nothing*, and it is worth noticing that the whole fit would look worse if the
sample were mostly clutter.

### The order question

`Rx` and `Ry` almost commute, so an object tilted in *one* axis tells the two orders
apart only weakly. They do **not** commute with the yaw: under `XYZ` the up axis is
`Rx(-x) Ry(-y) (0,0,1) = (-sin y, sin x cos y, cos x cos y)`, which depends on x and y
only, while under `ZYX` the up axis is `Rz(-z)` of that horizontal part, so the yaw
rotates the tilt direction. The two up vectors are therefore a median of **19.1°** apart
for references tilted in both axes, and 31.8° apart for those tilted more than 10° in
both. An object tilted to follow a slope has its tilt direction fixed by the ground, not
by where it happens to face, so the terrain decides between them.

| both-axis tilt | n | gap between the orders | median XYZ (`-x-y-z`) | median ZYX (`-x-y-z`) | winner |
|---|---:|---:|---:|---:|---|
| ≥ 3° | 56,324 | 19.10° | **7.95°** | 22.16° | XYZ |
| ≥ 10° | 20,432 | 31.82° | **9.98°** | 34.06° | XYZ |
| ≥ 20° | 5,671 | 51.56° | **21.07°** | 58.58° | XYZ |
| ≥ 30° | 2,322 | 67.26° | **75.88°** | 80.92° | XYZ (both bad) |

At the strictest tilt the medians are poor for every convention — a reference tilted more
than 30° in both axes is standing on a cliff face or is deliberately knocked over, and
its up axis has stopped meaning "up". The 10° row is the informative one: 9.98° against
34.06° with a 31.8° gap between the hypotheses.

### Paired sign test

Asking per reference which of two conventions puts its up axis closer to the terrain
normal needs no assumption about *how well* objects follow slopes, only that the answer
is the same for each of them. `z` is the normal approximation to the binomial.

| comparison | all references | both axes tilted > 10° |
|---|---|---|
| (a) beats (b) | 78.5% of 80,553 (z = 162) | 80.9% of 20,429 (z = 88) |
| (a) beats (c) | 88.7% of 82,631 (z = 223) | 90.0% of 20,432 (z = 114) |
| (a) beats (d) | 82.5% of 82,703 (z = 187) | 85.0% of 20,432 (z = 100) |
| (a) beats (e) | 82.5% of 82,703 (z = 187) | 84.9% of 20,432 (z = 100) |
| (b) beats (e) | 58.9% of 82,618 (z = 51) | 59.2% of 20,429 (z = 26) |

## The formula the engine should use (keep the current one)

In Creation space the reference is rotated by **`Rx(-x) * Ry(-y) * Rz(-z)`** applied to
column vectors — the transpose of `Rz(z) * Ry(y) * Rx(x)`, i.e. the inverse of the
Gamebryo matrix the stored angles describe. That is exactly what
`crates/shared/src/coordinates.rs:37` builds (it conjugates `Rz * Ry * Rx` and then
conjugates by `CREATION_TO_RUNTIME_ROTATION`), and it is what the audit measures as
`XYZ -x-y-z`.

Equivalently, in component form, the up axis of a reference is
`(-sin y, sin x cos y, cos x cos y)`: the tilt direction is fixed in world axes and the
yaw spins the object about the vertical without moving it.

No change is proposed. The alternatives the audit rules out, with their margins, are the
table above.

## Placement: position and scale

Read from the spawn path: `crates/engine/src/streaming.rs:763` builds one `Transform`
per reference as translation → rotation → scale (`crates/engine/src/streaming.rs:774`),
with the position mapped by `creation_to_runtime_vector`
(`crates/engine/src/streaming.rs:1608`) and the rotation by the function above
(`crates/engine/src/streaming.rs:1629`), and the scale applied uniformly
(`crates/engine/src/streaming.rs:776`). `Transform` composes `T * R * S`, which is the
order the data implies: scale about the model origin, then tilt/yaw, then place. The
scale column holds one float per reference, read from the single-float `XSCL` subrecord
and defaulting to 1.0 (`crates/converter/src/esm/exporter.rs:405`), so `Vec3::splat`
loses nothing.

Measured, over every exterior reference in Tamriel with a model and terrain under it
(228,187):

- **Position agrees with the terrain.** Median |`pos_z` − terrain| is 25 units (0.36 m
  at 1.43 cm per unit) and 69% are within 64 units. The control — the same local position in a *random other
  cell* — gives a median of 11,334 units and 0% within 64. An axes or sign error in
  `pos_x`/`pos_y`/`pos_z` would destroy this agreement completely.
- **Scale is sane.** 228,187 values, min 0.01, max 5.52, 53.1% exactly 1.0, none at or
  below zero (a negative scale would mirror geometry through the model origin, which the
  engine's `Vec3::splat` would happily do).
- **Model geometry sits where the reference implies.** For the 90,000 upright references
  standing on the ground, the bottom of the model's bounding box is a median of 48 units
  *below* the terrain and never systematically above it. Clutter is the sharpest case:
  median 0 units, p10 −37, p90 +72. Ground contact is right.

### Model-space offsets (checked, nothing found to act on)

`statics.bounds_*` are measured from the converted glb by the converter
(`crates/converter/src/integration.rs:102`) and are used by the engine for
`InstanceBounds` (`crates/engine/src/streaming.rs:777`). They are the union of the
POSITION accessor boxes under the full node hierarchy
(`crates/converter/src/mesh.rs:742`), and for `TreePineForest04` they reproduce a
hand-computed axis-swap of the glb's own accessor bounds exactly — the model geometry is
where the file says it is, and the conversion adds no offset of its own.

One observation, **not** a defect and **not** verified against the source NIF: the tree
models carry a large underground section. `Landscape\Trees\TreePineForest04.nif`'s full
mesh spans z −1188…2085 relative to the reference origin, while the same file's LOD meshes
span −88…1925; `TreeAspen01.nif` is −691…1439 against −16…1235. Since the reference origin
is on the ground (measured above) and the near-origin LOD mesh starts at ≈0, the visible
crown is the right size and the deep part is simply below the terrain, so the placement
is harmless. It is worth knowing about because it makes any "the model box touches the
ground" check for trees meaningless, and because 17 m of buried trunk is unusual enough
to be worth a glance at the NIF — which needs a BSA reader that `tools/research/` does
not have, so this note leaves it open.

## How to re-run

```
python tools/research/rotation_convention.py selftest          # port vs the Rust module's own tests
python tools/research/rotation_convention.py census            # what is out there, per group and model
python tools/research/rotation_convention.py audit  --cells 0 --json docs/research/data/object-rotation-audit.json
python tools/research/rotation_convention.py placement --cells 0
python tools/research/rotation_convention.py model TreePineForest04
```

`--cells 0` means every LAND cell of the worldspace (the full audit takes a few minutes);
`--cells 400` is a fast spot check that already gives the same ranking. `--world` picks
the worldspace, `--tilt`, `--ground` and `--slope` the filters. Conventions are
enumerated from `CONVENTIONS` at the top of the script, so a new candidate is one tuple.

## What is still unknown

- The z sign cannot be measured this way (the up axis does not depend on it). It rests on
  the door evidence in `933b95f` and the heading test at
  `crates/shared/src/coordinates.rs:172`.
- The audit measures exterior Tamriel only. Interior references are tilted far less often
  and their "ground" is furniture, not a heightmap; the same convention applies to them
  through the same function, but they are not independently verified here.
- Trees: see the open question above about the buried section.
