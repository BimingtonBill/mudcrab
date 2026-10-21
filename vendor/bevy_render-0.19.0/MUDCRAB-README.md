# bevy_render 0.19.0 public dependency

Pristine published crates.io bevy_render 0.19.0 baseline for the B04 shared-clock seam.
The workspace selects it through [patch.crates-io]. The baseline was imported unchanged; later public commits add the changes below.

Registry: https://github.com/rust-lang/crates.io-index
Published crate SHA-256: a89b2cc850a4c3fd12a6a088a4fc3e80118b2e1b569da8936f6d4d99ca1b6ee7
Upstream revision: c6f634ca9f406d68ba5109d921247b654cb42c10 (crates/bevy_render).
LICENSE-APACHE and LICENSE-MIT are retained unchanged from the crate archive.
Cargo local bookkeeping is excluded. Later public changes are separate commits.

## Public changes

B04 adds the continuous shared-clock uniform seam and workspace acceptance changes,
including declared test dependencies, lint corrections and a feature-stable renderer
constructor. The engine and shader uniform layouts agree.

B06+B07 adds only `RenderMultidrawableBatchSet::entities()` and its membership
assertion in the existing batching test. The read-only accessor returns main entity
identities from the existing tightly packed CPU instance list for destination draw
qualification. It changes no batching or rendering behavior.

Private parallel command encoding is not part of these public changes. On an
upgrade, check upstream equivalents for each seam before retaining the path patch.
