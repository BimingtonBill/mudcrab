//! Bind one structurally valid MCL to its certified database row before pooling.
//! Path ownership and the retained package lock belong to the caller.

use super::assets::ClusterMeshAsset;
use sha2::{Digest, Sha256};

/// Typed fields read from one `cluster_meshes` row. The package supplies the
/// builder identity separately, from its retained certified envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MeshExpectation {
    pub content_hash: [u8; 32],
    pub payload_bytes: u64,
    pub mesh_hash: [u8; 32],
    pub primitive_index: u32,
    pub vertex_count: u32,
    pub cluster_count: u32,
}

/// Only an `Ok` result may be inserted into scene mesh caches or GPU pools.
/// The generic asset loader remains useful without a dataset row; this gate
/// additionally verifies the row/header binding required for dataset drawing.
pub(crate) fn checked_mesh(
    bytes: Vec<u8>,
    builder: [u8; 32],
    expected: &MeshExpectation,
) -> Result<ClusterMeshAsset, String> {
    let length = u64::try_from(bytes.len()).map_err(|_| "mesh byte length exceeds u64")?;
    if length != expected.payload_bytes {
        return Err("mesh payload byte length differs from its row".into());
    }
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    if digest != expected.content_hash {
        return Err("mesh payload content hash differs from its row".into());
    }
    // One full structural walk. The asset then retains these exact bytes;
    // header comparisons below use its already-parsed public fields.
    let mesh = ClusterMeshAsset::from_bytes(bytes)
        .map_err(|error| format!("mesh payload format invalid: {error}"))?;
    if mesh.builder_identity != builder {
        return Err("mesh payload builder identity differs from its package".into());
    }
    if mesh.mesh_hash != expected.mesh_hash {
        return Err("mesh payload mesh hash differs from its row".into());
    }
    for (field, actual, wanted) in [
        (
            "primitive index",
            mesh.primitive_index,
            expected.primitive_index,
        ),
        ("vertex count", mesh.vertex_count, expected.vertex_count),
        ("cluster count", mesh.cluster_count, expected.cluster_count),
    ] {
        if actual != wanted {
            return Err(format!("mesh payload {field} differs from its row"));
        }
    }
    Ok(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::cluster::read_mcl;

    // Path is relative to the future engine/src/cluster/mesh_payload.rs,
    // matching the existing assets.rs fixture tests.
    const MCL: &[u8] = include_bytes!("../../../shared/tests/data/cluster/fixture.mcl");

    fn expectation() -> ([u8; 32], MeshExpectation) {
        let view = read_mcl(MCL).unwrap();
        (
            view.builder_identity,
            MeshExpectation {
                content_hash: Sha256::digest(MCL).into(),
                payload_bytes: MCL.len() as u64,
                mesh_hash: view.mesh_hash,
                primitive_index: view.primitive_index,
                vertex_count: view.vertex_count,
                cluster_count: view.cluster_count,
            },
        )
    }

    fn rejection(bytes: Vec<u8>, builder: [u8; 32], expected: &MeshExpectation) -> String {
        match checked_mesh(bytes, builder, expected) {
            Ok(_) => panic!("mismatched mesh was accepted"),
            Err(error) => error,
        }
    }

    #[test]
    fn committed_payload_matches_its_exact_row_and_package() {
        let (builder, expected) = expectation();
        let mesh = checked_mesh(MCL.to_vec(), builder, &expected).unwrap();
        assert_eq!(mesh.byte_len() as u64, expected.payload_bytes);
        assert_eq!(mesh.builder_identity, builder);
        assert_eq!(mesh.mesh_hash, expected.mesh_hash);
        assert_eq!(mesh.primitive_index, expected.primitive_index);
        assert_eq!(mesh.vertex_count, expected.vertex_count);
        assert_eq!(mesh.cluster_count, expected.cluster_count);
    }

    #[test]
    fn same_size_changed_bytes_fail_content_hash_before_header_checks() {
        let (builder, expected) = expectation();
        let mut bytes = MCL.to_vec();
        bytes[8] ^= 1; // A legal but different builder ID; structure remains valid.
        assert_eq!(bytes.len(), MCL.len());
        assert!(read_mcl(&bytes).is_ok());
        assert!(rejection(bytes, builder, &expected).contains("content hash"));
    }

    #[test]
    fn rehashed_legal_header_edits_still_have_to_match_row_and_package() {
        let (builder, expected) = expectation();
        for (offset, field) in [
            (8, "builder identity"),
            (40, "mesh hash"),
            (72, "primitive index"),
        ] {
            let mut bytes = MCL.to_vec();
            bytes[offset] ^= 1;
            assert!(read_mcl(&bytes).is_ok(), "legal edit: {field}");
            let mut rehashed = expected;
            rehashed.content_hash = Sha256::digest(&bytes).into();
            assert!(
                rejection(bytes, builder, &rehashed).contains(field),
                "{field}"
            );
        }
    }

    #[test]
    fn correct_content_hash_does_not_certify_wrong_row_counts() {
        let (builder, expected) = expectation();
        for field in ["vertex count", "cluster count"] {
            let mut wrong = expected;
            match field {
                "vertex count" => wrong.vertex_count += 1,
                "cluster count" => wrong.cluster_count += 1,
                _ => unreachable!(),
            }
            // The unchanged valid payload has exactly the expected content hash;
            // only the claimed row count is wrong.
            assert!(
                rejection(MCL.to_vec(), builder, &wrong).contains(field),
                "{field}"
            );
        }
    }

    #[test]
    fn length_and_structural_validation_remain_independent_gates() {
        let (builder, expected) = expectation();
        let mut wrong_size = expected;
        wrong_size.payload_bytes += 1;
        assert!(rejection(MCL.to_vec(), builder, &wrong_size).contains("byte length"));
        let truncated = MCL[..MCL.len() / 2].to_vec();
        assert!(rejection(truncated.clone(), builder, &expected).contains("byte length"));
        let mut self_consistent = expected;
        self_consistent.payload_bytes = truncated.len() as u64;
        self_consistent.content_hash = Sha256::digest(&truncated).into();
        assert!(rejection(truncated, builder, &self_consistent).contains("format invalid"));
    }
}
