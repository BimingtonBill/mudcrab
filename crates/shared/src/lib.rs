//! Stable data contracts shared by the offline converter and the runtime.

pub mod asset_lock;
pub mod collision;
pub mod coordinates;
pub mod lod;
pub mod world_assets;

use rkyv::{Archive, Deserialize, Serialize};

/// The world database's schema version.
///
/// **Portal numbering.** On the `portal` branch this is `100_000 + main * 100 + portal_revision`:
/// main's number (7 since #105's LOD tables and #152's grass tables) in the hundreds, the portal's
/// own extra tables (lights, door links, per-space lighting) in the last two digits, and the
/// 100_000 offset keeps a portal number from ever being mistaken for a number main reaches by
/// itself. When main's number moves, merge it into the middle digits and reset the revision; never
/// take main's raw value here. The matching converter constant is `CONVERTER_SCHEMA_VERSION` in
/// `converter::cache` (= [`LOD_CONVERTER_SCHEMA_VERSION`]); the converter has a test that fails if
/// either falls back below 100_000.
pub const WORLD_DATABASE_SCHEMA_VERSION: u32 = 100_701;

/// The converter schema a LOD package's producer must have written. Main defines the converter's
/// own `CONVERTER_SCHEMA_VERSION` as this value and `world_assets::validate_lod_build_contract`
/// compares a conversion manifest's schema against it for equality, so on the portal branch it
/// carries the portal's converter number (`100_000 + 24 * 100 + 1`), not main's raw 24.
pub const LOD_CONVERTER_SCHEMA_VERSION: u32 = 102_401;

/// The first world database schema with LOD tables (main's 6). Only ever compared with `>=`/`<`,
/// so every portal number (100_000 and up) counts as LOD-capable, which it is.
pub const WORLD_DATABASE_LOD_SCHEMA_VERSION: u32 = 6;

/// The oldest world database schema the runtime (the engine and `world-inspect`) reads. On the
/// portal branch the data is portal-only, so this equals the current version.
pub const MIN_RUNTIME_WORLD_DATABASE_SCHEMA_VERSION: u32 = 100_701;

/// The oldest converter manifest schema the runtime loads (portal numbering: equals the converter's
/// own `CONVERTER_SCHEMA_VERSION`, which lives in the converter crate).
pub const MIN_RUNTIME_CONVERTER_SCHEMA_VERSION: u32 = 102_401;

/// Whether the runtime reads a world database of this schema: from
/// [`MIN_RUNTIME_WORLD_DATABASE_SCHEMA_VERSION`] through [`WORLD_DATABASE_SCHEMA_VERSION`].
pub fn supports_runtime_world_database_schema(version: u32) -> bool {
    (MIN_RUNTIME_WORLD_DATABASE_SCHEMA_VERSION..=WORLD_DATABASE_SCHEMA_VERSION).contains(&version)
}

pub const CELL_CACHE_VERSION: u32 = 3;
pub const LAND_SIDE: u16 = 33;

/// Default LAND texture repeats per full cell per axis. Skyrim advances texture UVs
/// by `fLandTextureTilingMult / 4` per sample interval; the vanilla default is 3,
/// so 32 intervals give `32 * (3 / 4) = 24` repeats per cell.
/// Apply this to diffuse/normal sampling, never to the LAND blend-weight grid.
/// See [landscape texture scale](../../../docs/specs/engine/landscape-texture-scale.md)
/// for sources and verification.
pub const LAND_TEXTURE_REPEATS_PER_CELL: f32 = 32.0 * (3.0 / 4.0);

#[derive(Debug, Clone, Archive, Serialize, Deserialize, PartialEq)]
#[rkyv(bytecheck())]
pub struct TerrainLayer {
    pub texture_form_id: u32,
    pub quadrant: u8,
    pub layer: u16,
    pub is_base: bool,
    pub weights: Vec<TerrainWeight>,
}

#[derive(Debug, Clone, Archive, Serialize, Deserialize, PartialEq)]
#[rkyv(bytecheck())]
pub struct TerrainWeight {
    pub vertex: u16,
    pub opacity: f32,
}

#[derive(Debug, Clone, Archive, Serialize, Deserialize, PartialEq)]
#[rkyv(bytecheck())]
pub struct CachedLand {
    pub cell_id: u32,
    pub width: u16,
    pub height: u16,
    /// Row-major, absolute Creation Engine height units.
    pub heights: Vec<f32>,
    /// Packed signed XYZ normals, three bytes per vertex.
    pub normals: Vec<i8>,
    /// Packed RGB colors, three bytes per vertex.
    pub vertex_colors: Vec<u8>,
    pub layers: Vec<TerrainLayer>,
    pub water_height: Option<f32>,
    pub water_type_form_id: Option<u32>,
}

#[derive(Debug, Clone, Archive, Serialize, Deserialize, PartialEq)]
#[rkyv(bytecheck())]
pub struct CellCache {
    pub version: u32,
    pub cells: Vec<CachedLand>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds3 {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Bounds3 {
    pub const UNIT: Self = Self {
        min: [-0.5; 3],
        max: [0.5; 3],
    };

    pub fn is_finite_and_ordered(self) -> bool {
        self.min
            .iter()
            .chain(self.max.iter())
            .all(|value| value.is_finite())
            && (0..3).all(|axis| self.min[axis] <= self.max[axis])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_world_database_schemas_run_from_the_oldest_to_the_current() {
        let oldest = MIN_RUNTIME_WORLD_DATABASE_SCHEMA_VERSION;
        assert!(!supports_runtime_world_database_schema(oldest - 1));
        assert!(supports_runtime_world_database_schema(oldest));
        assert!(supports_runtime_world_database_schema(
            WORLD_DATABASE_SCHEMA_VERSION
        ));
        assert!(!supports_runtime_world_database_schema(
            WORLD_DATABASE_SCHEMA_VERSION + 1
        ));
    }
}
