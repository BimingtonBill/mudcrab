//! Runs the builder on a converted game mesh. Reads the file only, skips when it is missing.
//! Override the path with `MESHOPT_CLUSTER_SAMPLE`. Run with `--nocapture` to see the numbers.

use std::path::Path;
use std::time::Instant;

use meshopt_cluster::{Config, Mesh, build};

const DEFAULT_SAMPLE: &str = "C:/Modding/Mudcrab/SkyrimConverted-portal-rooms/meshes/dungeons/dwemer/animated/astrolabe/armillary/dweastrolabearmillary01.glb";

type Primitive = (Vec<[f32; 3]>, Vec<u32>);

fn load_primitives(path: &Path) -> Vec<Primitive> {
    // Buffers only: the sample carries images in encodings this test does not decode.
    let gltf::Gltf { document, blob } = gltf::Gltf::open(path).expect("gltf open");
    let buffers = gltf::import_buffers(&document, path.parent(), blob).expect("gltf buffers");
    let mut out = Vec::new();
    for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|b| Some(buffers[b.index()].0.as_slice()));
            let positions: Vec<[f32; 3]> = reader.read_positions().expect("POSITION").collect();
            let indices: Vec<u32> = match reader.read_indices() {
                Some(i) => i.into_u32().collect(),
                None => (0..positions.len() as u32).collect(),
            };
            out.push((positions, indices));
        }
    }
    out
}

fn median_ms(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

#[test]
fn sample_glb_hierarchy_invariants_and_numbers() {
    let path =
        std::env::var("MESHOPT_CLUSTER_SAMPLE").unwrap_or_else(|_| DEFAULT_SAMPLE.to_string());
    if !Path::new(&path).exists() {
        eprintln!("skipped: sample {path} not found");
        return;
    }

    let primitives = load_primitives(Path::new(&path));
    let input_triangles: usize = primitives.iter().map(|(_, i)| i.len() / 3).sum();

    for (label, config) in [("classic", Config::classic(128)), ("rt", Config::rt(128))] {
        let mut clusters = 0usize;
        let mut groups = 0usize;
        let mut max_depth = 0u32;
        let mut payload_bytes = 0usize;
        let mut source_triangles = 0usize;
        let mut build_ms = 0.0f64;
        // largest primitive, to compare with the VG2 spike's single-mesh figure (20,112 triangles, 46 ms)
        let mut largest = (0usize, 0.0f64);

        for (positions, indices) in &primitives {
            let mesh = Mesh {
                positions,
                indices,
                attributes: None,
                vertex_lock: None,
            };

            // one untimed warm-up, then the median of three timed builds
            let _ = build(&mesh, config).expect("warm-up build");
            let mut samples = Vec::with_capacity(3);
            let mut hierarchy = None;
            for _ in 0..3 {
                let start = Instant::now();
                let h = build(&mesh, config).expect("timed build");
                samples.push(start.elapsed().as_secs_f64() * 1e3);
                hierarchy = Some(h);
            }
            let ms = median_ms(samples);
            build_ms += ms;
            if indices.len() / 3 > largest.0 {
                largest = (indices.len() / 3, ms);
            }

            let h = hierarchy.expect("hierarchy");
            clusters += h.clusters().len();
            groups += h.groups().len();
            max_depth = max_depth.max(h.groups().iter().map(|g| g.depth).max().unwrap_or(0));
            for c in h.clusters() {
                assert!(c.triangle_count() <= 128);
                // per emitted cluster: u8 local indices, u32 vertex refs, bounds + error (spike formula)
                payload_bytes += c.indices().len() + 4 * c.vertex_count + 24;
                if c.refined.is_none() {
                    source_triangles += c.triangle_count();
                }
            }
        }

        assert_eq!(
            source_triangles, input_triangles,
            "{label}: source triangles not all covered"
        );

        println!(
            "astrolabe sample [{label}, 128]: {} primitives, {} input triangles, {} groups, {} clusters, DAG depth max {}, payload {:.1} B/triangle, build {:.2} ms total ({:.0} ms per million input triangles)",
            primitives.len(),
            input_triangles,
            groups,
            clusters,
            max_depth,
            payload_bytes as f64 / input_triangles as f64,
            build_ms,
            build_ms * 1e6 / input_triangles as f64,
        );
        println!(
            "  largest primitive [{label}, 128]: {} triangles, {:.2} ms",
            largest.0, largest.1
        );
    }
}
