//! Stable data contracts shared by the offline converter and the runtime.

pub mod collision;
pub mod coordinates;

use rkyv::{Archive, Deserialize, Serialize};

/// The world database's schema version.
///
/// **Portal numbering.** On the `portal` branch this is `100_000 + main * 100 + portal_revision`:
/// main's number (4 when this was merged) in the hundreds, the portal's own extra tables (LOD,
/// lights, door links, per-space lighting) in the last two digits, and the 100_000 offset keeps a
/// portal number from ever being mistaken for a number main reaches by itself (main's next bump,
/// Chef's #105, takes world DB 5). When main's number moves, merge it into the middle digits and
/// reset the revision; never take main's raw value here. The matching converter constant is
/// `CONVERTER_SCHEMA_VERSION` in `converter::cache`; the converter has a test that fails if either
/// falls back below 100_000.
pub const WORLD_DATABASE_SCHEMA_VERSION: u32 = 100_401;
pub const CELL_CACHE_VERSION: u32 = 3;
pub const LAND_SIDE: u16 = 33;

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
