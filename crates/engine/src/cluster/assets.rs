//! Asset loaders for the two cluster payload kinds (ADR-0013 sections 2 and 3).
//!
//! A payload is validated once, on load, by the shared reader (`shared::cluster`): magic, version,
//! section bounds and alignment, record sanity and monotonic errors. A payload that fails never
//! becomes an asset, so the asset server reports it as failed and the streaming layer treats it as
//! a terminal failure. The loaders keep the validated bytes; the GPU upload reads the sections from
//! them without a second copy per section.

use bevy::{
    asset::{Asset, AssetLoader, LoadContext, io::Reader},
    prelude::*,
    reflect::TypePath,
};
use shared::cluster::{
    ClusterFormatError, FLAG_TANGENTS, InstanceRecord, MclSection, MclView, MeshRef, NO_GROUP,
    decode_oct_snorm16, decode_uv_unorm16, encode_oct_snorm16, read_mci, read_mcl,
    read_mcl_validated,
};
use std::sync::Arc;

struct PreparedTangents {
    words: Arc<[u32]>,
    usable: bool,
}

fn pack_tangent_word(tangent: [f32; 4]) -> u32 {
    let mut encoded = encode_oct_snorm16([tangent[0], tangent[1], tangent[2]]);
    encoded[1] = (encoded[1] & !1) | i16::from(tangent[3] < 0.0);
    u32::from(encoded[0] as u16) | (u32::from(encoded[1] as u16) << 16)
}

/// Preserve successful MikkTSpace words exactly, including zero slots only
/// parent clusters use. Failure words remain diagnostic, never usable proof.
fn prepare_generated_tangents(
    count: usize,
    succeeded: bool,
    attribute: Option<&bevy::mesh::VertexAttributeValues>,
) -> PreparedTangents {
    if succeeded
        && let Some(bevy::mesh::VertexAttributeValues::Float32x4(tangents)) = attribute
        && tangents.len() == count
        && tangents
            .iter()
            .flatten()
            .all(|component| component.is_finite())
    {
        return PreparedTangents {
            words: tangents
                .iter()
                .copied()
                .map(pack_tangent_word)
                .collect::<Vec<_>>()
                .into(),
            usable: true,
        };
    }
    PreparedTangents {
        words: vec![pack_tangent_word([1.0, 0.0, 0.0, 1.0]); count].into(),
        usable: false,
    }
}

/// The same unorm8 RGBA interpretation as the pooled vertex shader, including alpha.
fn source_colours(bytes: &[u8], count: usize) -> Vec<[f32; 4]> {
    if bytes.is_empty() {
        return vec![[1.0; 4]; count];
    }
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|rgba| std::array::from_fn(|channel| f32::from(rgba[channel]) / 255.0))
        .collect()
}

/// A validated per-mesh cluster payload (`.mcl`).
#[derive(Asset, TypePath, Clone)]
pub struct ClusterMeshAsset {
    bytes: Arc<[u8]>,
    pub builder_identity: [u8; 32],
    pub mesh_hash: [u8; 32],
    pub primitive_index: u32,
    pub flags: u32,
    pub vertex_count: u32,
    pub cluster_count: u32,
    pub group_count: u32,
    pub bvh_node_count: u32,
    pub root_error: f32,
    pub bounds: [f32; 4],
    pub uv_min: [f32; 2],
    pub uv_scale: [f32; 2],
    /// The packed tangent words, once computed (`prepare_tangents`): generating them runs
    /// mikktspace over the whole mesh, which is the dominant cost of building a mesh's GPU data, so
    /// it is done where the payload is loaded, in parallel, and not where it is pooled.
    /// Shared by every clone of the asset, so tangents prepared on a worker thread reach the copy
    /// the render world holds.
    tangents: Arc<std::sync::OnceLock<PreparedTangents>>,
}

impl ClusterMeshAsset {
    /// Validates `bytes` and keeps them.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, ClusterFormatError> {
        let view = read_mcl(&bytes)?;
        let asset = Self {
            builder_identity: view.builder_identity,
            mesh_hash: view.mesh_hash,
            primitive_index: view.primitive_index,
            flags: view.flags,
            vertex_count: view.vertex_count,
            cluster_count: view.cluster_count,
            group_count: view.group_count,
            bvh_node_count: view.bvh_node_count,
            root_error: view.root_error,
            bounds: view.bounds,
            uv_min: view.uv_min,
            uv_scale: view.uv_scale,
            bytes: bytes.into(),
            tangents: Arc::default(),
        };
        Ok(asset)
    }

    /// A view of the validated payload (cheap: it re-reads the headers and the section table, not
    /// the records, which `from_bytes` checked once).
    pub fn view(&self) -> MclView<'_> {
        read_mcl_validated(&self.bytes).expect("bytes were validated when the asset was built")
    }

    /// One section's bytes, as validated.
    pub fn section(&self, section: MclSection) -> &[u8] {
        self.view().section(section)
    }

    /// The mesh as the stock path sees it: one ordinary indexed triangle list of the source
    /// geometry (the clusters no simplification produced), retaining colours and authored tangents.
    pub fn source_mesh(&self) -> Mesh {
        let view = self.view();
        let count = self.vertex_count as usize;
        let positions: Vec<[f32; 3]> = (0..count).map(|vertex| view.position(vertex)).collect();
        let normals: Vec<[f32; 3]> = (0..count)
            .map(|vertex| decode_oct_snorm16(view.normal(vertex)))
            .collect();
        let uvs: Vec<[f32; 2]> = (0..count)
            .map(|vertex| decode_uv_unorm16(view.uv(vertex), self.uv_min, self.uv_scale))
            .collect();
        let colours = source_colours(view.section(MclSection::Colours), count);
        let mut indices = Vec::new();
        for index in 0..self.cluster_count as usize {
            let cluster = view.cluster(index);
            if cluster.refined_group != NO_GROUP {
                continue;
            }
            for local in view.cluster_triangles(&cluster) {
                indices.push(view.cluster_vertex(cluster.vertex_offset as usize + *local as usize));
            }
        }
        let mut mesh = Mesh::new(
            bevy::mesh::PrimitiveTopology::TriangleList,
            bevy::asset::RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colours)
        .with_inserted_indices(bevy::mesh::Indices::U32(indices));
        if self.flags & FLAG_TANGENTS != 0 {
            let tangents: Vec<[f32; 4]> = self
                .section(MclSection::Tangents)
                .as_chunks::<4>()
                .0
                .iter()
                .map(|bytes| {
                    let word = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                    let direction =
                        decode_oct_snorm16([word as u16 as i16, ((word >> 16) as u16 & !1) as i16]);
                    [
                        direction[0],
                        direction[1],
                        direction[2],
                        if word & 0x10000 != 0 { -1.0 } else { 1.0 },
                    ]
                })
                .collect();
            mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents);
        }
        mesh
    }

    /// One packed tangent word per vertex, in the payload's encoding (octahedral snorm16x2, the
    /// handedness in the low bit of the second component, set when negative). The payload's own
    /// tangent section when it has one; otherwise the tangents the stock path would generate for
    /// the same geometry (Bevy's mikktspace, as the glTF loader runs it), so a normal-mapped
    /// material shades the same on both paths. A mesh mikktspace cannot handle gets the
    /// fallback `(1, 0, 0, +1)`.
    pub fn tangent_words(&self) -> std::borrow::Cow<'_, [u32]> {
        let prepared = self.tangents.get_or_init(|| self.generate_tangent_words());
        std::borrow::Cow::Borrowed(&prepared.words)
    }

    /// Computes and keeps the tangent words, so `tangent_words` is a read afterwards. Safe to run
    /// on many meshes at once (each is independent).
    pub fn prepare_tangents(&self) {
        self.tangents.get_or_init(|| self.generate_tangent_words());
    }

    /// Initialize the unused GPU tangent slots for a terrain-only mesh. These words are
    /// deliberately NOT usable tangent proof. The colour-only terrain material never reads
    /// them; object meshes (including geometry shared with terrain) must use MikkTSpace.
    pub(crate) fn prepare_unused_tangent_slots(&self) {
        self.tangents
            .get_or_init(|| prepare_generated_tangents(self.vertex_count as usize, false, None));
    }

    /// False means the cached words are only the existing diagnostic fallback.
    /// This fact alone does not authorize any view to draw or replace stock.
    pub fn tangents_usable(&self) -> bool {
        self.tangents
            .get_or_init(|| self.generate_tangent_words())
            .usable
    }

    /// Read a completed worker result without generating tangents on the frame thread.
    pub(crate) fn prepared_tangents_usable(&self) -> Option<bool> {
        self.tangents.get().map(|prepared| prepared.usable)
    }

    #[cfg(test)]
    pub(crate) fn fail_tangents_for_test(&self) {
        assert!(
            self.tangents
                .set(prepare_generated_tangents(
                    self.vertex_count as usize,
                    false,
                    None,
                ))
                .is_ok()
        );
    }

    fn generate_tangent_words(&self) -> PreparedTangents {
        let count = self.vertex_count as usize;
        if self.flags & FLAG_TANGENTS != 0 {
            let bytes = self.section(MclSection::Tangents);
            let words: Vec<u32> = (0..count)
                .map(|vertex| {
                    u32::from_le_bytes([
                        bytes[vertex * 4],
                        bytes[vertex * 4 + 1],
                        bytes[vertex * 4 + 2],
                        bytes[vertex * 4 + 3],
                    ])
                })
                .collect();
            return PreparedTangents {
                words: words.into(),
                usable: true,
            };
        }
        let mut mesh = self.source_mesh();
        let succeeded = mesh.generate_tangents().is_ok();
        prepare_generated_tangents(count, succeeded, mesh.attribute(Mesh::ATTRIBUTE_TANGENT))
    }

    /// The file size, for the upload budget and the byte metrics.
    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }
}

/// A validated chunk instance block (`.mci`): the objects clusters own in one block of cells.
#[derive(Asset, TypePath, Clone)]
pub struct ClusterBlockAsset {
    pub builder_identity: [u8; 32],
    pub worldspace_id: u32,
    pub tier: u32,
    pub anchor_x: i32,
    pub anchor_y: i32,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    /// The mesh table: which `.mcl` payloads the instances name.
    pub meshes: Vec<MeshRef>,
    pub instances: Vec<InstanceRecord>,
}

impl ClusterBlockAsset {
    /// Validates `bytes` and copies the tables out (80 bytes per instance).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ClusterFormatError> {
        let view = read_mci(bytes)?;
        Ok(Self {
            builder_identity: view.builder_identity,
            worldspace_id: view.worldspace_id,
            tier: view.tier,
            anchor_x: view.anchor_x,
            anchor_y: view.anchor_y,
            bounds_min: view.bounds_min,
            bounds_max: view.bounds_max,
            meshes: (0..view.mesh_count as usize)
                .map(|index| view.mesh(index))
                .collect(),
            instances: (0..view.instance_count as usize)
                .map(|index| view.instance(index))
                .collect(),
        })
    }
}

/// Why a cluster payload failed to load: a read error is transient, a format error is terminal
/// (ADR section 7: the same classes as terrain LOD chunks).
#[derive(Debug)]
pub enum ClusterLoadError {
    Io(std::io::Error),
    Format(ClusterFormatError),
}

impl std::fmt::Display for ClusterLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "cluster payload could not be read: {error}"),
            Self::Format(error) => write!(f, "cluster payload is invalid: {error}"),
        }
    }
}

impl std::error::Error for ClusterLoadError {}

#[derive(Default, TypePath)]
pub struct ClusterMeshLoader;

impl AssetLoader for ClusterMeshLoader {
    type Asset = ClusterMeshAsset;
    type Settings = ();
    type Error = ClusterLoadError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .await
            .map_err(ClusterLoadError::Io)?;
        ClusterMeshAsset::from_bytes(bytes).map_err(ClusterLoadError::Format)
    }

    fn extensions(&self) -> &[&str] {
        &["mcl"]
    }
}

#[derive(Default, TypePath)]
pub struct ClusterBlockLoader;

impl AssetLoader for ClusterBlockLoader {
    type Asset = ClusterBlockAsset;
    type Settings = ();
    type Error = ClusterLoadError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .await
            .map_err(ClusterLoadError::Io)?;
        ClusterBlockAsset::from_bytes(&bytes).map_err(ClusterLoadError::Format)
    }

    fn extensions(&self) -> &[&str] {
        &["mci"]
    }
}

/// Registers the two asset types and their loaders.
pub struct ClusterAssetsPlugin;

impl Plugin for ClusterAssetsPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<ClusterMeshAsset>()
            .init_asset::<ClusterBlockAsset>()
            .init_asset_loader::<ClusterMeshLoader>()
            .init_asset_loader::<ClusterBlockLoader>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MCL: &[u8] = include_bytes!("../../../shared/tests/data/cluster/fixture.mcl");
    const MCI: &[u8] = include_bytes!("../../../shared/tests/data/cluster/fixture.mci");

    #[test]
    fn stock_source_preserves_alpha_and_colour_used_by_cutout_materials() {
        assert_eq!(
            source_colours(&[255, 128, 0, 0, 0, 64, 255, 127], 2),
            vec![
                [1.0, 128.0 / 255.0, 0.0, 0.0],
                [0.0, 64.0 / 255.0, 1.0, 127.0 / 255.0]
            ]
        );
        // Give the existing clean-room fixture an authored colour section so this assertion
        // fails if source_mesh drops RGBA, including alpha values on either side of a cutout.
        let mut bytes = MCL.to_vec();
        while !bytes
            .len()
            .is_multiple_of(shared::cluster::SECTION_ALIGNMENT)
        {
            bytes.push(0);
        }
        let offset = bytes.len();
        let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
        let rgba: Vec<u8> = (0..count)
            .flat_map(|vertex| [255, 128, 64, if vertex % 2 == 0 { 0 } else { 255 }])
            .collect();
        bytes.extend_from_slice(&rgba);
        let entry = shared::cluster::MCL_HEADER_BYTES + MclSection::Colours as usize * 16;
        bytes[entry..entry + 8].copy_from_slice(&(offset as u64).to_le_bytes());
        bytes[entry + 8..entry + 16].copy_from_slice(&(rgba.len() as u64).to_le_bytes());
        let flags = u32::from_le_bytes(bytes[76..80].try_into().unwrap())
            | shared::cluster::FLAG_VERTEX_COLOURS;
        bytes[76..80].copy_from_slice(&flags.to_le_bytes());
        let mesh = ClusterMeshAsset::from_bytes(bytes).unwrap();
        let source = mesh.source_mesh();
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colours)) =
            source.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("source must expose RGBA");
        };
        assert_eq!(colours.len(), mesh.vertex_count as usize);
        assert_eq!(
            *colours,
            source_colours(
                mesh.section(MclSection::Colours),
                mesh.vertex_count as usize
            )
        );
    }

    #[test]
    fn unused_terrain_slots_never_become_successful_tangent_proof_or_generate_later() {
        let mesh = ClusterMeshAsset::from_bytes(MCL.to_vec()).unwrap();
        assert_eq!(mesh.prepared_tangents_usable(), None);
        mesh.prepare_unused_tangent_slots();
        let clone = mesh.clone();
        let words = mesh.tangent_words();
        assert_eq!(words.len(), mesh.vertex_count as usize);
        assert_eq!(mesh.prepared_tangents_usable(), Some(false));
        let pointer = words.as_ptr();
        clone.prepare_tangents();
        assert_eq!(clone.prepared_tangents_usable(), Some(false));
        assert_eq!(clone.tangent_words().as_ptr(), pointer);
        assert!(
            words
                .iter()
                .all(|word| *word == pack_tangent_word([1.0, 0.0, 0.0, 1.0]))
        );
    }

    #[test]
    fn successful_generated_words_are_cached_and_shared_by_asset_clones() {
        let mesh = ClusterMeshAsset::from_bytes(MCL.to_vec()).unwrap();
        assert_eq!(
            mesh.flags & FLAG_TANGENTS,
            0,
            "fixture must exercise generation"
        );
        assert!(mesh.tangents.get().is_none());
        assert_eq!(mesh.prepared_tangents_usable(), None);
        assert!(mesh.tangents.get().is_none());
        let clone = mesh.clone();
        assert!(Arc::ptr_eq(&mesh.tangents, &clone.tangents));
        let words = mesh.tangent_words();
        assert!(matches!(&words, std::borrow::Cow::Borrowed(_)));
        assert_eq!(words.len(), mesh.vertex_count as usize);
        assert!(mesh.tangents_usable());
        assert_eq!(mesh.prepared_tangents_usable(), Some(true));
        let pointer = words.as_ptr();
        clone.prepare_tangents();
        assert!(clone.tangents_usable());
        let cloned_words = clone.tangent_words();
        assert_eq!(cloned_words.as_ptr(), pointer);
        assert_eq!(cloned_words.as_ref(), words.as_ref());

        let mut source = mesh.source_mesh();
        source.generate_tangents().unwrap();
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(generated)) =
            source.attribute(Mesh::ATTRIBUTE_TANGENT)
        else {
            panic!("fixture must produce Bevy tangent words");
        };
        let expected: Vec<_> = generated.iter().copied().map(pack_tangent_word).collect();
        assert_eq!(words.as_ref(), expected);
    }

    #[test]
    fn successful_packing_keeps_handedness_and_zero_parent_only_slots() {
        let values = bevy::mesh::VertexAttributeValues::Float32x4(vec![
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, -1.0],
            [0.0; 4],
        ]);
        let prepared = prepare_generated_tangents(3, true, Some(&values));
        assert!(prepared.usable);
        assert_eq!(prepared.words.as_ref(), [0x0000_7fff, 0x7fff_0000, 0]);
    }

    #[test]
    fn generation_and_attribute_failures_keep_diagnostic_words_but_are_unusable() {
        use bevy::mesh::VertexAttributeValues;
        let good = VertexAttributeValues::Float32x4(vec![[0.0, 1.0, 0.0, -1.0]; 2]);
        let wrong_type = VertexAttributeValues::Float32x3(vec![[1.0, 0.0, 0.0]; 2]);
        let short = VertexAttributeValues::Float32x4(vec![[1.0, 0.0, 0.0, 1.0]]);
        for (succeeded, attribute) in [
            (false, Some(&good)),
            (true, None),
            (true, Some(&wrong_type)),
            (true, Some(&short)),
        ] {
            let prepared = prepare_generated_tangents(2, succeeded, attribute);
            assert!(!prepared.usable);
            // Existing fallback is packed (+X,+1), unchanged for every vertex.
            assert_eq!(prepared.words.as_ref(), [0x0000_7fff; 2]);
        }
    }

    #[test]
    fn every_nonfinite_component_refuses_the_entire_generated_tangent_set() {
        for component in 0..4 {
            for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                let mut tangents = vec![[0.0, 1.0, 0.0, -1.0]; 2];
                tangents[1][component] = invalid;
                let values = bevy::mesh::VertexAttributeValues::Float32x4(tangents);
                let prepared = prepare_generated_tangents(2, true, Some(&values));
                assert!(!prepared.usable);
                assert_eq!(prepared.words.as_ref(), [0x0000_7fff; 2]);
            }
        }
    }

    #[test]
    fn the_fixture_mesh_payload_loads() {
        let asset = ClusterMeshAsset::from_bytes(MCL.to_vec()).expect("fixture is valid");
        assert!(asset.cluster_count > 1, "a hierarchy has many clusters");
        assert!(asset.group_count > 0);
        assert!(asset.root_error > 0.0);
        assert_eq!(asset.byte_len(), MCL.len());
        let clusters = asset.section(MclSection::Clusters);
        assert_eq!(clusters.len(), asset.cluster_count as usize * 48);
    }

    #[test]
    fn the_fixture_block_names_meshes_the_payload_matches() {
        let mesh = ClusterMeshAsset::from_bytes(MCL.to_vec()).unwrap();
        let block = ClusterBlockAsset::from_bytes(MCI).expect("fixture is valid");
        assert_eq!(block.instances.len(), 3, "three instances in two cells");
        assert!(
            block
                .meshes
                .iter()
                .any(|reference| reference.mesh_hash == mesh.mesh_hash
                    && reference.primitive_index == mesh.primitive_index)
        );
        assert_eq!(block.builder_identity, mesh.builder_identity);
    }

    #[test]
    fn a_truncated_or_corrupt_payload_fails_closed() {
        assert!(ClusterMeshAsset::from_bytes(MCL[..MCL.len() / 2].to_vec()).is_err());
        let mut bad_magic = MCL.to_vec();
        bad_magic[0] ^= 0xff;
        assert!(ClusterMeshAsset::from_bytes(bad_magic).is_err());
        assert!(ClusterBlockAsset::from_bytes(&MCI[..40]).is_err());
        let mut bad_version = MCI.to_vec();
        bad_version[4] = 9;
        assert!(ClusterBlockAsset::from_bytes(&bad_version).is_err());
    }
}

/// Add an authored tangent section to the real hierarchy without changing its
/// source/parent clusters, vertices, groups or BVH records.
#[cfg(test)]
pub(crate) fn authored_tangent_fixture() -> ClusterMeshAsset {
    use shared::cluster::{MclContents, write_mcl};
    let source = ClusterMeshAsset::from_bytes(
        include_bytes!("../../../shared/tests/data/cluster/fixture.mcl").to_vec(),
    )
    .unwrap();
    let view = source.view();
    let count = source.vertex_count as usize;
    let positions: Vec<_> = (0..count).map(|i| view.position(i)).collect();
    let normals: Vec<_> = (0..count).map(|i| view.normal(i)).collect();
    let uvs: Vec<_> = (0..count).map(|i| view.uv(i)).collect();
    let tangents: Vec<_> = (0..count)
        .map(|i| {
            let word = pack_tangent_word(if i % 2 == 0 {
                [1.0, 0.0, 0.0, -1.0]
            } else {
                [0.0, 1.0, 0.0, 1.0]
            });
            [word as u16 as i16, (word >> 16) as u16 as i16]
        })
        .collect();
    let clusters: Vec<_> = (0..source.cluster_count as usize)
        .map(|i| view.cluster(i))
        .collect();
    let groups: Vec<_> = (0..source.group_count as usize)
        .map(|i| view.group(i))
        .collect();
    let bvh: Vec<_> = (0..source.bvh_node_count as usize)
        .map(|i| view.bvh_node(i))
        .collect();
    let vertices: Vec<_> = (0..view.section(MclSection::ClusterVertices).len() / 4)
        .map(|i| view.cluster_vertex(i))
        .collect();
    let triangles: Vec<[u8; 3]> = view
        .section(MclSection::ClusterTriangles)
        .as_chunks::<3>()
        .0
        .iter()
        .map(|v| [v[0], v[1], v[2]])
        .collect();
    let bytes = write_mcl(&MclContents {
        builder_identity: view.builder_identity,
        mesh_hash: view.mesh_hash,
        primitive_index: view.primitive_index,
        root_error: view.root_error,
        bounds: view.bounds,
        uv_min: view.uv_min,
        uv_scale: view.uv_scale,
        positions: &positions,
        normals: &normals,
        uvs: &uvs,
        tangents: Some(&tangents),
        colours: None,
        clusters: &clusters,
        cluster_vertices: &vertices,
        cluster_triangles: &triangles,
        groups: &groups,
        bvh: &bvh,
    })
    .unwrap();
    ClusterMeshAsset::from_bytes(bytes).unwrap()
}

#[cfg(test)]
mod source_tangent_regressions {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    #[test]
    fn source_mesh_keeps_authored_tangents_without_a_normal_map() {
        let asset = authored_tangent_fixture();
        let source = asset.source_mesh();
        let Some(VertexAttributeValues::Float32x4(tangents)) =
            source.attribute(Mesh::ATTRIBUTE_TANGENT)
        else {
            panic!("authored TANGENT semantic was dropped");
        };
        assert_eq!(tangents.len(), asset.vertex_count as usize);
        assert!(
            tangents
                .iter()
                .enumerate()
                .all(|(i, t)| t[3] == if i % 2 == 0 { -1.0 } else { 1.0 })
        );
        let mut mapped = asset.source_mesh();
        if !mapped.contains_attribute(Mesh::ATTRIBUTE_TANGENT) {
            mapped.generate_tangents().unwrap();
        }
        assert_eq!(
            mapped.attribute(Mesh::ATTRIBUTE_TANGENT),
            source.attribute(Mesh::ATTRIBUTE_TANGENT)
        );
        asset.prepare_tangents();
        let expected: Vec<_> = asset
            .section(MclSection::Tangents)
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
            .collect();
        assert_eq!(asset.tangent_words().as_ref(), expected);
    }

    #[test]
    fn source_tangent_direction_masks_the_handedness_bit_like_the_cluster_shader() {
        let asset = authored_tangent_fixture();
        let source = asset.source_mesh();
        let Some(VertexAttributeValues::Float32x4(tangents)) =
            source.attribute(Mesh::ATTRIBUTE_TANGENT)
        else {
            panic!("missing authored tangents")
        };
        for (tangent, bytes) in tangents.iter().zip(
            asset
                .section(MclSection::Tangents)
                .as_chunks::<4>()
                .0
                .iter(),
        ) {
            let word = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            let direction =
                decode_oct_snorm16([word as u16 as i16, ((word >> 16) as u16 & !1) as i16]);
            assert_eq!(&tangent[..3], &direction);
        }
    }
}
