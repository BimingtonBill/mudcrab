//! Bounded GPU upload layout, distinct from the verbose CPU inspection packet.
//!
//! One read-only storage binding holds directory, original samples and compact
//! min/max pairs. Drop only the 1024 redundant quad-leaf nodes: their bounds are
//! exactly recoverable from four retained height samples. No caster is omitted.
//! Coverage/reasons remain in each directory entry; every uploaded surface page is
//! wholly validated. Unknown/pending pages must never be interpreted as compact nodes.

use super::world_height_tree::{self, CellInput, CellKind, GridRect, NodeKind};
use crate::far_terrain_shadows::SnapshotIdentity;
use crate::far_terrain_shadows::packet::{
    DIRECTORY_WORDS, HEADER_WORDS, NODES_PER_PAGE, NONE, ShadowPacket, VERSION, coverage, reason,
};

pub const GPU_VERSION: u32 = 3;
pub const WORLD_NODE_WORDS: u64 = 9;
pub const COMPACT_NODES_PER_PAGE: u64 = 341;
pub const HEIGHT_BYTES_PER_PAGE: u64 = 4356;
pub const COMPACT_NODE_BYTES_PER_PAGE: u64 = COMPACT_NODES_PER_PAGE * 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Footprint {
    pub directory: u64,
    pub heights: u64,
    pub nodes: u64,
    pub world_tree: u64,
    pub storage: u64,
    pub header: u64,
    pub view_instance: u64,
}

/// The limits of the CREATED RenderDevice, not the physical adapter's advertised
/// maxima. Existing counts must count FRAGMENT-visible layout entries, even if an
/// entrypoint does not read them. Entrypoint-pruned shader counts are a separate audit.
#[derive(Debug, Clone, Copy)]
pub struct UploadLimits {
    pub max_storage_buffer_binding_size: u64,
    pub max_buffer_size: u64,
    pub max_storage_buffers_per_shader_stage: u32,
    pub max_uniform_buffers_per_shader_stage: u32,
    pub max_uniform_buffer_binding_size: u64,
    pub max_bind_groups: u32,
    pub existing_fragment_storage: u32,
    pub existing_fragment_uniforms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadRefusal {
    AddressOverflow,
    MalformedPacket,
    AllocationFailed,
    TreeNodeLimit { limit: usize },
    StorageBytes { required: u64, limit: u64 },
    FragmentStorage { required: u32, limit: u32 },
    FragmentUniforms { required: u32, limit: u32 },
    UniformBytes { required: u64, limit: u64 },
    BindGroups { required: u32, limit: u32 },
}

impl Footprint {
    pub fn for_counts(entries: u64, pages: u64) -> Result<Self, UploadRefusal> {
        if pages > entries {
            return Err(UploadRefusal::MalformedPacket);
        }
        let mul = |a: u64, b: u64| a.checked_mul(b).ok_or(UploadRefusal::AddressOverflow);
        let directory = mul(entries, DIRECTORY_WORDS as u64 * 4)?;
        let heights = mul(pages, HEIGHT_BYTES_PER_PAGE)?;
        let nodes = mul(pages, COMPACT_NODE_BYTES_PER_PAGE)?;
        let storage = directory
            .checked_add(heights)
            .and_then(|n| n.checked_add(nodes))
            .ok_or(UploadRefusal::AddressOverflow)?;
        // All WGSL section offsets and array indexes are u32 words.
        if storage / 4 >= u64::from(u32::MAX) {
            return Err(UploadRefusal::AddressOverflow);
        }
        Ok(Self {
            directory,
            heights,
            nodes,
            world_tree: 0,
            storage,
            header: 64,
            view_instance: 400,
        })
    }

    /// Refuse the entire optional upload before allocation; never truncate pages,
    /// raise limits, or mistake camera residency for world coverage.
    pub fn check(self, limits: UploadLimits) -> Result<(), UploadRefusal> {
        let allowed = limits
            .max_storage_buffer_binding_size
            .min(limits.max_buffer_size);
        if self.storage.max(4) > allowed {
            return Err(UploadRefusal::StorageBytes {
                required: self.storage.max(4),
                limit: allowed,
            });
        }
        let storage = limits
            .existing_fragment_storage
            .checked_add(1)
            .ok_or(UploadRefusal::AddressOverflow)?;
        if storage > limits.max_storage_buffers_per_shader_stage {
            return Err(UploadRefusal::FragmentStorage {
                required: storage,
                limit: limits.max_storage_buffers_per_shader_stage,
            });
        }
        let uniforms = limits
            .existing_fragment_uniforms
            .checked_add(2)
            .ok_or(UploadRefusal::AddressOverflow)?;
        if uniforms > limits.max_uniform_buffers_per_shader_stage {
            return Err(UploadRefusal::FragmentUniforms {
                required: uniforms,
                limit: limits.max_uniform_buffers_per_shader_stage,
            });
        }
        let required = self.header.max(self.view_instance);
        let allowed = limits
            .max_uniform_buffer_binding_size
            .min(limits.max_buffer_size);
        if required > allowed {
            return Err(UploadRefusal::UniformBytes {
                required,
                limit: allowed,
            });
        }
        if limits.max_bind_groups < 5 {
            return Err(UploadRefusal::BindGroups {
                required: 5,
                limit: limits.max_bind_groups,
            });
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct GpuUpload {
    /// Full host-side identity retained for comparison before binding to a view.
    pub identity: SnapshotIdentity,
    /// Same fields as CPU header, except version=3, word14=height section WORD
    /// offset, word15=node section WORD offset. Directory starts at word zero.
    /// World partition nodes follow all compact page nodes (nine words each).
    pub header: [u32; HEADER_WORDS],
    pub words: Vec<u32>,
    pub footprint: Footprint,
}

impl GpuUpload {
    pub fn build(packet: &ShadowPacket, limits: UploadLimits) -> Result<Self, UploadRefusal> {
        let entries = packet.directory.len() as u64;
        let pages = u64::from(packet.header[3]);
        let mut footprint = Footprint::for_counts(entries, pages)?;
        footprint.check(limits)?; // BEFORE constructing an upload-sized Vec.
        if packet.header[0] != VERSION
            || u64::from(packet.header[2]) != entries
            || packet.heights.len() as u64 * 4 != footprint.heights
            || packet.nodes.len() as u64 != pages * NODES_PER_PAGE as u64
        {
            return Err(UploadRefusal::MalformedPacket);
        }
        let mut header = packet.header;
        header[0] = GPU_VERSION;
        header[14] = (footprint.directory / 4) as u32;
        header[15] = ((footprint.directory + footprint.heights) / 4) as u32;
        // Validate the concrete CPU builder's dense page/root addresses and
        // whole-page status before relying on directory-level coverage.
        let mut expected_page = 0u32;
        for entry in &packet.directory {
            let words = entry.0;
            if words[2] == NONE {
                if words[3] != NONE || words[4] & coverage::HAS_SURFACE != 0 {
                    return Err(UploadRefusal::MalformedPacket);
                }
            } else {
                if words[2] != expected_page
                    || words[3] as u64 != (u64::from(expected_page) + 1) * NODES_PER_PAGE as u64 - 1
                    || words[4] != coverage::COMPLETE | coverage::HAS_SURFACE | coverage::HAS_BOUNDS
                    || words[5] != 0
                {
                    return Err(UploadRefusal::MalformedPacket);
                }
                expected_page += 1;
            }
        }
        if u64::from(expected_page) != pages {
            return Err(UploadRefusal::MalformedPacket);
        }
        // Structural checks are not a new content/recipe certificate: the caller
        // must supply an unchanged packet from ShadowPacket::build. In particular
        // the source builder's sample digests and extrema enclosure remain trusted.
        for page in packet.nodes.as_chunks::<NODES_PER_PAGE>().0 {
            for node in &page[1024..] {
                let min = f32::from_bits(node.0[0]);
                let max = f32::from_bits(node.0[1]);
                if node.0[2] != coverage::COMPLETE | coverage::HAS_SURFACE | coverage::HAS_BOUNDS
                    || node.0[3] != 0
                    || !min.is_finite()
                    || !max.is_finite()
                    || min > max
                {
                    return Err(UploadRefusal::MalformedPacket);
                }
            }
        }
        // Build once in the existing background, source-owned world job. Missing cells
        // become unbounded Unknown rectangles, never holes in a known-surface BVH.
        let mut inputs = Vec::new();
        inputs
            .try_reserve_exact(packet.directory.len())
            .map_err(|_| UploadRefusal::AllocationFailed)?;
        for entry in &packet.directory {
            let value = entry.0;
            let kind = if value[2] != NONE {
                // Derive extrema from the same retained canonical samples, not an
                // independently supplied acceleration bound.
                let start = value[2] as usize * 1089;
                let mut min = f32::INFINITY;
                let mut max = f32::NEG_INFINITY;
                for &bits in &packet.heights[start..start + 1089] {
                    let height = f32::from_bits(bits);
                    if !height.is_finite() {
                        return Err(UploadRefusal::MalformedPacket);
                    }
                    min = min.min(height);
                    max = max.max(height);
                }
                CellKind::Surface {
                    page: value[2],
                    min,
                    max,
                }
            } else if value[4] == coverage::COMPLETE | coverage::KNOWN_EMPTY && value[5] == 0 {
                CellKind::KnownEmpty
            } else {
                CellKind::Unknown {
                    reasons: if value[5] == 0 {
                        reason::UNLISTED
                    } else {
                        value[5]
                    },
                }
            };
            inputs.push(CellInput {
                cell: [value[0] as i32, value[1] as i32],
                kind,
            });
        }
        let available = limits
            .max_storage_buffer_binding_size
            .min(limits.max_buffer_size)
            .saturating_sub(footprint.storage);
        let max_nodes =
            usize::try_from((available / (WORLD_NODE_WORDS * 4)).min(u64::from(u32::MAX)))
                .map_err(|_| UploadRefusal::AddressOverflow)?;
        let tree = world_height_tree::build(
            GridRect {
                min: [packet.header[4] as i32, packet.header[5] as i32],
                max: [packet.header[6] as i32, packet.header[7] as i32],
            },
            &inputs,
            reason::UNLISTED,
            max_nodes,
        )
        .map_err(|error| match error {
            world_height_tree::BuildError::NodeLimit { limit } => {
                UploadRefusal::TreeNodeLimit { limit }
            }
            world_height_tree::BuildError::AllocationFailed => UploadRefusal::AllocationFailed,
            world_height_tree::BuildError::AllocationLimit
            | world_height_tree::BuildError::CountOverflow
            | world_height_tree::BuildError::AddressLimit
            | world_height_tree::BuildError::DepthLimit => UploadRefusal::AddressOverflow,
            _ => UploadRefusal::MalformedPacket,
        })?;
        debug_assert_eq!(
            tree.domain().min,
            [packet.header[4] as i32, packet.header[5] as i32]
        );
        footprint.world_tree = (tree.nodes().len() as u64)
            .checked_mul(WORLD_NODE_WORDS * 4)
            .ok_or(UploadRefusal::AddressOverflow)?;
        footprint.storage = footprint
            .storage
            .checked_add(footprint.world_tree)
            .ok_or(UploadRefusal::AddressOverflow)?;
        if footprint.storage / 4 >= u64::from(u32::MAX) {
            return Err(UploadRefusal::AddressOverflow);
        }
        footprint.check(limits)?;
        let mut words = Vec::new();
        words
            .try_reserve_exact((footprint.storage / 4) as usize)
            .map_err(|_| UploadRefusal::AllocationFailed)?;
        for entry in &packet.directory {
            let mut value = entry.0;
            if value[2] != NONE {
                value[3] = ((u64::from(value[2]) + 1) * COMPACT_NODES_PER_PAGE - 1) as u32;
            }
            words.extend(value);
        }
        words.extend_from_slice(&packet.heights);
        for page in packet.nodes.as_chunks::<NODES_PER_PAGE>().0 {
            for node in &page[1024..] {
                words.extend_from_slice(&node.0[..2]);
            }
        }
        let mut world_kinds = [0usize; 5];
        for node in tree.nodes() {
            let (kind, bounds, reasons) = match node.kind {
                NodeKind::Branch {
                    bounds: Some(bounds),
                } => (1, bounds, 0),
                NodeKind::Branch { bounds: None } => (0, [0.0; 2], 0),
                NodeKind::Surface { page: _, min, max } => (2, [min, max], 0),
                NodeKind::KnownEmpty => (3, [0.0; 2], 0),
                NodeKind::Unknown { reasons } => (4, [0.0; 2], reasons),
            };
            world_kinds[kind as usize] += 1;
            words.extend([
                node.rect.min[0] as u32,
                node.rect.min[1] as u32,
                node.rect.max[0] as u32,
                node.rect.max[1] as u32,
                bounds[0].to_bits(),
                bounds[1].to_bits(),
                node.escape,
                kind,
                reasons,
            ]);
        }
        bevy::log::info!(
            target: "engine::far_terrain_shadows::gpu",
            world = packet.identity.worldspace,
            cells = entries,
            pages,
            nodes = tree.nodes().len(),
            unbounded_branches = world_kinds[0],
            bounded_branches = world_kinds[1],
            surface_leaves = world_kinds[2],
            empty_regions = world_kinds[3],
            unknown_regions = world_kinds[4],
            "canonical shadow world index prepared"
        );
        Ok(Self {
            identity: packet.identity,
            header,
            words,
            footprint,
        })
    }

    pub fn bytes_le(&self) -> Vec<u8> {
        self.words
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::far_terrain_shadows::*;

    fn limits() -> UploadLimits {
        UploadLimits {
            max_storage_buffer_binding_size: 128 * 1024 * 1024,
            max_buffer_size: 256 * 1024 * 1024,
            max_storage_buffers_per_shader_stage: 8,
            max_uniform_buffers_per_shader_stage: 12,
            max_uniform_buffer_binding_size: 65536,
            max_bind_groups: 5,
            existing_fragment_storage: 7,
            existing_fragment_uniforms: 10,
        }
    }

    fn peak_packet() -> ShadowPacket {
        let identity = SnapshotIdentity {
            worldspace: 21,
            cell_size_bits: 32.0f64.to_bits(),
            database: DatabaseIdentity {
                schema_revision: 1,
                content_hash: [1; 32],
            },
            core_cache: CoreCacheIdentity {
                version: 1,
                hash: [2; 32],
            },
            canonical: Some(CanonicalRecipeIdentity {
                package: [3; 32],
                catalog: [4; 32],
                content_digest: [5; 32],
                recipe_revision: 1,
                triangulation: SurfaceTriangulation::CanonicalBase,
            }),
        };
        let cell = CellCoord { x: -1, y: 2 };
        let mut builder =
            InventoryBuilder::new(identity, 32.0, CellRect::new(cell, cell).unwrap()).unwrap();
        let mut samples = vec![0.0; SAMPLES_PER_CELL];
        samples[16 * SAMPLES_PER_SIDE + 16] = 8.0;
        let bytes = encode_heights_le(&samples);
        builder
            .add_cell(
                cell,
                AuthoredStatus::VhgtPresent,
                &[SurfaceInput {
                    kind: SurfaceKind::CanonicalFromAuthored,
                    raw: RawGrid {
                        bytes: &bytes,
                        expected_sha256: sha256(&bytes),
                    },
                }],
            )
            .unwrap();
        ShadowPacket::build(
            &builder.build(),
            [crate::far_terrain_shadows::packet::CellInput {
                cell,
                vhgt: crate::far_terrain_shadows::packet::RawVhgtStatus::PresentValid,
            }],
            crate::far_terrain_shadows::packet::SourceStatus {
                complete: false,
                record_data_complete: false,
                post_worldspace_weld: true,
            },
        )
        .unwrap()
    }

    #[test]
    fn compact_upload_preserves_all_samples_peak_bounds_and_closed_world_unknown() {
        let packet = peak_packet();
        let upload = GpuUpload::build(&packet, limits()).unwrap();
        assert_eq!(upload.words.len() * 4, 7152);
        assert_eq!(upload.header[0], 3);
        assert_eq!(upload.header[8], 0);
        let height_start = upload.header[14] as usize;
        let node_start = upload.header[15] as usize;
        assert_eq!(height_start, 8);
        assert_eq!(node_start, 8 + 1089);
        assert_eq!(&upload.words[height_start..node_start], &packet.heights);
        assert_eq!(upload.footprint.world_tree, 36);
        assert_eq!(
            &upload.words[upload.words.len() - 5..],
            &[0.0f32.to_bits(), 8.0f32.to_bits(), 1, 2, 0]
        );
        assert_eq!(upload.words[3], 340); // compact root, not old node1364
        let mut start = 0;
        for side in [16, 8, 4, 2, 1] {
            let max = (start..start + side * side)
                .map(|node| f32::from_bits(upload.words[node_start + node * 2 + 1]))
                .fold(0.0, f32::max);
            assert_eq!(max, 8.0);
            start += side * side;
        }
        assert_eq!(start, 341);
        assert_eq!(upload.bytes_le().len(), 7152);
        let mut corrupt = peak_packet();
        corrupt.directory[0].0[4] &= !coverage::COMPLETE;
        assert!(matches!(
            GpuUpload::build(&corrupt, limits()),
            Err(UploadRefusal::MalformedPacket)
        ));
        let mut corrupt = peak_packet();
        corrupt.directory[0].0[3] = NONE;
        assert!(matches!(
            GpuUpload::build(&corrupt, limits()),
            Err(UploadRefusal::MalformedPacket)
        ));
    }

    #[test]
    fn world_tree_cannot_bound_unknown_holes_by_the_known_surface_maximum() {
        let mut packet = peak_packet();
        // One known page and a large absent tail. Only the exact surface leaf
        // may carry height bounds; missing space remains explicit XY-only Unknown.
        packet.header[6] = 200;
        let upload = GpuUpload::build(&packet, limits()).unwrap();
        let start = upload.header[15] as usize + 682;
        let nodes = upload.words[start..].as_chunks::<9>().0;
        assert!(
            nodes.len() < 40,
            "sparse coverage is partitioned, not enumerated"
        );
        assert_eq!(
            nodes[0][7], 0,
            "unknown descendants forbid root height bounds"
        );
        assert!(
            nodes
                .iter()
                .any(|node| node[7] == 4 && node[8] == reason::UNLISTED)
        );
        assert_eq!(nodes.iter().filter(|node| node[7] == 2).count(), 1);
        for (index, node) in nodes.iter().enumerate() {
            assert!(node[6] as usize > index && node[6] as usize <= nodes.len());
        }
        assert_eq!(upload.footprint.storage as usize, upload.words.len() * 4);
        let mut tight = limits();
        tight.max_storage_buffer_binding_size = upload.footprint.storage - 1;
        assert!(
            GpuUpload::build(&packet, tight).is_err(),
            "no truncated acceleration upload"
        );
        tight.max_storage_buffer_binding_size = 7152;
        assert_eq!(
            GpuUpload::build(&packet, tight).unwrap_err(),
            UploadRefusal::TreeNodeLimit { limit: 1 }
        );
    }

    #[test]
    fn exact_tamriel_footprint_fits_default_binding_without_raising_caps() {
        let bytes = Footprint::for_counts(11186, 11186).unwrap();
        assert_eq!(bytes.directory, 357952);
        assert_eq!(bytes.heights, 48726216);
        assert_eq!(bytes.nodes, 30515408);
        assert_eq!(bytes.storage, 79599576);
        assert_eq!(bytes.header, 64);
        assert_eq!(bytes.view_instance, 400);
        assert_eq!(bytes.check(limits()), Ok(()));
    }

    #[test]
    fn refuses_size_stage_uniform_and_group_overflow_without_partial_upload() {
        let bytes = Footprint::for_counts(11186, 11186).unwrap();
        assert!(matches!(
            bytes.check(UploadLimits {
                max_storage_buffer_binding_size: 1,
                ..limits()
            }),
            Err(UploadRefusal::StorageBytes { .. })
        ));
        assert!(matches!(
            bytes.check(UploadLimits {
                existing_fragment_storage: 8,
                ..limits()
            }),
            Err(UploadRefusal::FragmentStorage {
                required: 9,
                limit: 8
            })
        ));
        assert!(matches!(
            bytes.check(UploadLimits {
                existing_fragment_uniforms: 11,
                ..limits()
            }),
            Err(UploadRefusal::FragmentUniforms { .. })
        ));
        assert!(matches!(
            bytes.check(UploadLimits {
                max_uniform_buffer_binding_size: 256,
                ..limits()
            }),
            Err(UploadRefusal::UniformBytes { .. })
        ));
        assert!(matches!(
            bytes.check(UploadLimits {
                max_bind_groups: 4,
                ..limits()
            }),
            Err(UploadRefusal::BindGroups { .. })
        ));
        assert!(Footprint::for_counts(u64::MAX, 1).is_err());
        assert!(Footprint::for_counts(1, 2).is_err());
        assert!(matches!(
            Footprint::for_counts(20000, 20000).unwrap().check(limits()),
            Err(UploadRefusal::StorageBytes { .. })
        ));
    }
}
