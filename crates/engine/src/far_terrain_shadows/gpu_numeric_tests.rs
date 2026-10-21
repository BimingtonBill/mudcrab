//! Ignored, headless GPU numeric proof for pinned production or explicit candidate WGSL.
//! Candidate sources certify numeric controls only, never current production/renderer acceptance.
//! Overrides reuse the compiled host layout/fixtures; host ABI changes require recompilation.
//! Root hook: child cfg(test) module in renderer_init.rs (see notes). No runtime registration.
//! Every run must use the build gate's TEST lane; no adapter/CPU fallback is accepted.

use crate::far_terrain_shadows::{
    AuthoredStatus, CanonicalRecipeIdentity, CellCoord, CellRect, CoreCacheIdentity,
    DatabaseIdentity, InventoryBuilder, RawGrid, RayResult, SnapshotIdentity, SurfaceInput,
    SurfaceKind, SurfaceQuery, SurfaceSource, SurfaceTriangulation, WorldHeightInventory,
    encode_heights_le,
    gpu::{GPU_VERSION, GpuUpload, UploadLimits},
    packet::{CellInput, RawVhgtStatus, ShadowPacket, SourceStatus, reason},
    receiving::{ARITHMETIC_WGSL_SOURCE, COPLANAR_REASON, WGSL_SOURCE, prepare_canonical_view},
    sha256,
    view::{PackedView, ViewContext},
};
use bevy::{render::settings::RenderResources, tasks::block_on};
use naga_oil::compose::{ComposableModuleDescriptor, Composer, NagaModuleDescriptor};
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};
use wgpu::{naga, util::DeviceExt};

const ARITHMETIC_SHA: &str = "29a20de5aff4979841537659fe0857dbc4fc146beba7cd135fa41094d9b9f341";
const RECEIVING_SHA: &str = "08e54d6513c0bc09c751b78c4bde82cf9390110675968e0622084e91c3738cc0";
const RECEIVER_PATH_ENV: &str = "MUDCRAB_F1_TEST_RECEIVER_PATH";
const RECEIVER_SHA_ENV: &str = "MUDCRAB_F1_TEST_RECEIVER_SHA256";
// Exact source's function name is f1_receive, not f1_query. This wrapper only
// supplies arguments and copies ALL returned words. No arithmetic/traversal stub.
const ENTRY: &str = r#"
#import far_terrain_shadows::receiving::{f1_receive, f1_receive_at_origin}
struct Probe { words: array<vec4<u32>, 4>, }
@group(0) @binding(0) var<uniform> probe: Probe;
@group(0) @binding(1) var<storage, read_write> observed: array<u32>;
@compute @workgroup_size(1)
fn main() {
    let r = f1_receive(bitcast<vec3<f32>>(probe.words[0].xyz), probe.words[0].w,
        probe.words[1].xy, probe.words[1].zw, probe.words[2].x, probe.words[2].y);
    observed[0] = r.state; observed[1] = r.reasons;
    observed[2] = r.cells; observed[3] = r.nodes;
    observed[4] = r.triangles; observed[5] = r.unknown_cells;
}
@compute @workgroup_size(1)
fn main_at_origin() {
    let r = f1_receive_at_origin(bitcast<vec3<f32>>(probe.words[0].xyz), probe.words[0].w, bitcast<vec2<i32>>(probe.words[2].zw),
        probe.words[1].xy, probe.words[1].zw, probe.words[2].x, probe.words[2].y);
    observed[0] = r.state; observed[1] = r.reasons;
    observed[2] = r.cells; observed[3] = r.nodes;
    observed[4] = r.triangles; observed[5] = r.unknown_cells;
}
"#;

fn normalized_source_sha(source: &str) -> String {
    shared::cluster::hex(&sha256(source.replace("\r\n", "\n").as_bytes()))
}

#[derive(Debug)]
struct ReceiverSource {
    source: Cow<'static, str>,
    candidate_path: Option<PathBuf>,
    sha: String,
}

/// Explicit arguments keep source selection testable without changing process environment.
fn receiver_source(path: Option<&Path>, expected: Option<&str>) -> Result<ReceiverSource, String> {
    let (source, candidate_path, expected) = match (path, expected) {
        (None, None) => (Cow::Borrowed(WGSL_SOURCE), None, RECEIVING_SHA),
        (Some(path), Some(expected)) => {
            if !path.is_absolute() {
                return Err(format!(
                    "{RECEIVER_PATH_ENV} must name an absolute file: {}",
                    path.display()
                ));
            }
            if expected.len() != 64
                || !expected
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            {
                return Err(format!(
                    "{RECEIVER_SHA_ENV} must contain exactly 64 lowercase hex characters"
                ));
            }
            if !path.is_file() {
                return Err(format!(
                    "{RECEIVER_PATH_ENV} is not an existing file: {}",
                    path.display()
                ));
            }
            let source = std::fs::read_to_string(path).map_err(|error| {
                format!(
                    "cannot read candidate receiver as UTF-8 {}: {error}",
                    path.display()
                )
            })?;
            (Cow::Owned(source), Some(path.to_owned()), expected)
        }
        _ => {
            return Err(format!(
                "{RECEIVER_PATH_ENV} and {RECEIVER_SHA_ENV} require both or neither"
            ));
        }
    };
    let sha = normalized_source_sha(&source);
    if sha != expected {
        return Err(format!(
            "receiver normalized-LF SHA256 mismatch: expected {expected}, actual {sha}"
        ));
    }
    Ok(ReceiverSource {
        source,
        candidate_path,
        sha,
    })
}

fn candidate_test_directory() -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("candidate source test directory");
    std::fs::write(
        directory.path().join("OWNER.txt"),
        "F1 candidate-source loader regression\n",
    )
    .expect("test output owner");
    directory
}

#[test]
fn receiver_source_defaults_to_literal_pinned_compiled_production() {
    let receiver = receiver_source(None, None).expect("reviewed production source pin");
    assert_eq!(receiver.source.as_ref(), WGSL_SOURCE);
    assert!(matches!(receiver.source, Cow::Borrowed(_)));
    assert!(receiver.candidate_path.is_none());
    assert_eq!(receiver.sha, RECEIVING_SHA);
    assert_eq!(
        normalized_source_sha(ARITHMETIC_WGSL_SOURCE),
        ARITHMETIC_SHA
    );
}

#[test]
fn receiver_source_refuses_incomplete_override_pairs() {
    let directory = candidate_test_directory();
    let path = directory.path().join("missing.wgsl");
    for result in [
        receiver_source(Some(&path), None),
        receiver_source(None, Some(RECEIVING_SHA)),
    ] {
        assert!(result.unwrap_err().contains("both or neither"));
    }
}

#[test]
fn receiver_source_refuses_relative_paths_and_malformed_hashes() {
    assert!(
        receiver_source(Some(Path::new("candidate.wgsl")), Some(RECEIVING_SHA))
            .unwrap_err()
            .contains("absolute file")
    );
    let directory = candidate_test_directory();
    let path = directory.path().join("candidate.wgsl");
    std::fs::write(&path, "// candidate\n").unwrap();
    for hash in [
        String::new(),
        "a".repeat(63),
        "a".repeat(65),
        "A".repeat(64),
        "g".repeat(64),
        "é".repeat(32),
    ] {
        assert!(
            receiver_source(Some(&path), Some(&hash))
                .unwrap_err()
                .contains("64 lowercase hex"),
            "malformed hash {hash:?}"
        );
    }
}

#[test]
fn receiver_source_refuses_missing_nonfile_invalid_utf8_and_mismatch() {
    let directory = candidate_test_directory();
    let path = directory.path().join("candidate.wgsl");
    assert!(
        receiver_source(Some(&path), Some(RECEIVING_SHA))
            .unwrap_err()
            .contains("not an existing file")
    );
    assert!(
        receiver_source(Some(directory.path()), Some(RECEIVING_SHA))
            .unwrap_err()
            .contains("not an existing file")
    );
    std::fs::write(&path, [0xff, 0xfe]).unwrap();
    assert!(
        receiver_source(Some(&path), Some(RECEIVING_SHA))
            .unwrap_err()
            .contains("cannot read candidate receiver as UTF-8")
    );
    std::fs::write(&path, "// different candidate\n").unwrap();
    assert!(
        receiver_source(Some(&path), Some(RECEIVING_SHA))
            .unwrap_err()
            .contains("normalized-LF SHA256 mismatch")
    );
}

#[test]
fn receiver_source_accepts_explicit_candidate_with_normalized_lf_hash() {
    let directory = candidate_test_directory();
    let path = directory.path().join("candidate.wgsl");
    let normalized = "#define_import_path far_terrain_shadows::receiving\nfn candidate() {}\n";
    let crlf = normalized.replace('\n', "\r\n");
    std::fs::write(&path, &crlf).unwrap();
    let hash = normalized_source_sha(normalized);
    let receiver = receiver_source(Some(&path), Some(&hash)).expect("explicit matching candidate");
    assert_eq!(receiver.source.as_ref(), crlf);
    assert_eq!(receiver.candidate_path.as_deref(), Some(path.as_path()));
    assert_eq!(receiver.sha, hash);
    assert_ne!(receiver.sha, RECEIVING_SHA);
}

fn composed_receiver() -> naga::Module {
    assert_eq!(
        normalized_source_sha(ARITHMETIC_WGSL_SOURCE),
        ARITHMETIC_SHA,
        "numeric fixture must execute reviewed immutable arithmetic; update pin only after source review"
    );
    let path = std::env::var_os(RECEIVER_PATH_ENV).map(PathBuf::from);
    let expected = match std::env::var(RECEIVER_SHA_ENV) {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => panic!("invalid {RECEIVER_SHA_ENV}: {error}"),
    };
    let receiver = receiver_source(path.as_deref(), expected.as_deref())
        .unwrap_or_else(|error| panic!("numeric receiver source selection failed: {error}"));
    let receiver_file = if let Some(path) = &receiver.candidate_path {
        eprintln!(
            "S5 numeric TEST-ONLY CANDIDATE source=external_receiver_file path={} normalized_lf_sha256={} immutable_arithmetic_sha256={ARITHMETIC_SHA} compiled_GPU_VERSION={GPU_VERSION}; candidate-source numeric evidence only, never current production/renderer acceptance",
            path.display(),
            receiver.sha
        );
        path.to_string_lossy().into_owned()
    } else {
        eprintln!(
            "S5 numeric source=pinned compiled production normalized_lf_sha256={} immutable_arithmetic_sha256={ARITHMETIC_SHA} compiled_GPU_VERSION={GPU_VERSION}",
            receiver.sha
        );
        "production/f1/receiving.wgsl".to_owned()
    };
    let mut composer = Composer::default().with_capabilities(naga::valid::Capabilities::all());
    for (source, file_path) in [
        (ARITHMETIC_WGSL_SOURCE, "production/f1/arithmetic.wgsl"),
        (receiver.source.as_ref(), receiver_file.as_str()),
    ] {
        if let Err(error) = composer.add_composable_module(ComposableModuleDescriptor {
            source,
            file_path,
            ..Default::default()
        }) {
            panic!("{}", error.emit_to_string(&composer));
        }
    }
    let module = composer
        .make_naga_module(NagaModuleDescriptor {
            source: ENTRY,
            file_path: "test/f1_gpu_numeric.wgsl",
            ..Default::default()
        })
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&composer)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .expect("actual receiver plus compute wrapper validates");
    module
}

fn words_le(words: impl IntoIterator<Item = u32>) -> Vec<u8> {
    words.into_iter().flat_map(u32::to_le_bytes).collect()
}
#[derive(Clone, Copy)]
struct Probe {
    renderer: [f32; 3],
    world: u32,
    view: u64,
    sun: u64,
    max_cells: u32,
    max_nodes: u32,
    actual_origin: Option<CellCoord>,
}
impl Probe {
    fn bytes(self) -> Vec<u8> {
        let mut words = [0u32; 16];
        for (at, value) in self.renderer.into_iter().enumerate() {
            words[at] = value.to_bits();
        }
        words[3] = self.world;
        words[4] = self.view as u32;
        words[5] = (self.view >> 32) as u32;
        words[6] = self.sun as u32;
        words[7] = (self.sun >> 32) as u32;
        words[8] = self.max_cells;
        words[9] = self.max_nodes;
        if let Some(origin) = self.actual_origin {
            words[10] = origin.x as u32;
            words[11] = origin.y as u32;
        }
        words_le(words)
    }
}
#[derive(Debug)]
struct Observation {
    state: u32,
    reasons: u32,
    cells: u32,
    nodes: u32,
    triangles: u32,
    unknown_cells: u32,
}

struct Gpu {
    resources: RenderResources,
    pipeline: wgpu::ComputePipeline,
    origin_pipeline: wgpu::ComputePipeline,
    probe_layout: wgpu::BindGroupLayout,
    f1_layout: wgpu::BindGroupLayout,
}
impl Gpu {
    fn new() -> Self {
        let settings = crate::render_backend::wgpu_settings(crate::config::RenderBackends::All)
            .unwrap_or_default();
        let backends = settings
            .backends
            .expect("renderer settings must enable a backend for this TEST");
        // CHILD module hook is required: this is the actual engine's private
        // selected-backend negotiation, including its DX12 feature policy.
        #[cfg(target_os = "windows")]
        let resources = block_on(super::initialize_renderer(backends, None, &settings));
        // This is exactly RendererInitPlugin's non-Windows initialization path.
        #[cfg(not(target_os = "windows"))]
        let resources = block_on(bevy::render::renderer::initialize_renderer_without_callbacks(
            backends, None, &settings,
        ));
        eprintln!(
            "S5 numeric GPU adapter={} backend={:?} type={:?} created_limits={:?}",
            resources.2.name,
            resources.2.backend,
            resources.2.device_type,
            resources.0.limits()
        );
        assert_ne!(
            resources.2.device_type,
            wgpu::DeviceType::Cpu,
            "CPU/software fallback is NOT GPU execution proof"
        );
        assert_ne!(
            resources.2.backend,
            wgpu::Backend::Noop,
            "Noop is NOT GPU execution proof"
        );
        let device = resources.0.wgpu_device();
        assert!(
            device.limits().max_bind_groups >= 5,
            "actual created device cannot bind production group4; no cap raise permitted"
        );
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("actual S5 receiving arithmetic/traversal"),
            source: wgpu::ShaderSource::Naga(Cow::Owned(composed_receiver())),
        });
        let uniform = |binding, size| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(size),
            },
            count: None,
        };
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let probe_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("test args and observation"),
            entries: &[uniform(0, 64), storage(1, false)],
        });
        let f1_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("exact production F1 slots7/8/9"),
            entries: &[uniform(7, 64), storage(8, true), uniform(9, 400)],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("F1 actual group4 plus test-only group0"),
            bind_group_layouts: &[Some(&probe_layout), None, None, None, Some(&f1_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("execute actual f1_receive"),
            layout: Some(&layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let origin_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("execute actual f1_receive_at_origin"),
            layout: Some(&layout),
            module: &shader,
            entry_point: Some("main_at_origin"),
            compilation_options: Default::default(),
            cache: None,
        });
        let validation = block_on(scope.pop());
        assert!(
            validation.is_none(),
            "GPU shader/pipeline validation error: {validation:?}"
        );
        Self {
            resources,
            pipeline,
            origin_pipeline,
            probe_layout,
            f1_layout,
        }
    }
    fn limits(&self) -> UploadLimits {
        let limits = self.resources.0.limits();
        UploadLimits {
            max_storage_buffer_binding_size: limits.max_storage_buffer_binding_size,
            max_buffer_size: limits.max_buffer_size,
            max_storage_buffers_per_shader_stage: limits.max_storage_buffers_per_shader_stage,
            max_uniform_buffers_per_shader_stage: limits.max_uniform_buffers_per_shader_stage,
            max_uniform_buffer_binding_size: limits.max_uniform_buffer_binding_size,
            max_bind_groups: limits.max_bind_groups,
            // This compute harness has one test output storage + one test arg
            // uniform, in addition to F1. It is NOT a renderer fragment audit.
            existing_fragment_storage: 1,
            existing_fragment_uniforms: 1,
        }
    }
    fn run(&self, label: &str, upload: &GpuUpload, view: &PackedView, probe: Probe) -> Observation {
        let device = self.resources.0.wgpu_device();
        let queue = &self.resources.1;
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let make = |label, contents: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage,
            })
        };
        let header = make(
            "actual runtime GPU header",
            &words_le(upload.header),
            wgpu::BufferUsages::UNIFORM,
        );
        let words = make(
            "actual runtime GPU words",
            &upload.bytes_le(),
            wgpu::BufferUsages::STORAGE,
        );
        let context = make(
            "actual prepared400byte context",
            &words_le(view.words.iter().flatten().copied()),
            wgpu::BufferUsages::UNIFORM,
        );
        let args = make(
            "test arguments",
            &probe.bytes(),
            wgpu::BufferUsages::UNIFORM,
        );
        let output = make(
            "six shader result words",
            &words_le([0xdeadcafe; 6]),
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("S5 numeric readback"),
            size: 24,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let f1 = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("F1 exact group4"),
            layout: &self.f1_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: header.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: words.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: context.as_entire_binding(),
                },
            ],
        });
        let test = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("test group0"),
            layout: &self.probe_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: args.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(label) });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(label),
                timestamp_writes: None,
            });
            pass.set_pipeline(if probe.actual_origin.is_some() {
                &self.origin_pipeline
            } else {
                &self.pipeline
            });
            pass.set_bind_group(0, &test, &[]);
            pass.set_bind_group(4, &f1, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 24);
        let submission = queue.submit([encoder.finish()]);
        let (send, receive) = mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                send.send(result).expect("readback callback receiver alive");
            });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(30)),
            })
            .expect("tiny GPU numeric dispatch must complete within30seconds");
        receive
            .recv_timeout(Duration::from_secs(1))
            .expect("actual GPU map callback ran")
            .expect("actual GPU buffer maps");
        let mapped = readback.slice(..).get_mapped_range();
        let result: Vec<_> = mapped
            .as_chunks::<4>()
            .0
            .iter()
            .map(|word| u32::from_le_bytes(*word))
            .collect();
        assert_eq!(result.len(), 6);
        assert!(
            result.iter().all(|word| *word != 0xdeadcafe),
            "compute invocation did not write every result word"
        );
        let observed = Observation {
            state: result[0],
            reasons: result[1],
            cells: result[2],
            nodes: result[3],
            triangles: result[4],
            unknown_cells: result[5],
        };
        drop(mapped);
        readback.unmap();
        let validation = block_on(scope.pop());
        assert!(
            validation.is_none(),
            "{label}: GPU binding/dispatch validation error: {validation:?}"
        );
        eprintln!("S5 numeric {label}: {observed:?}");
        observed
    }
}

struct Scene {
    inventory: WorldHeightInventory,
    upload: GpuUpload,
    context: ViewContext,
    source_heights: Vec<f32>,
}
fn scene(
    limits: UploadLimits,
    origin: CellCoord,
    cells: &[(i32, i32)],
    domain: [(i32, i32); 2],
    spacing: f64,
    sun: [f64; 3],
    peak: bool,
) -> Scene {
    let peak = peak.then_some((0, 0));
    scene_with_peak(limits, origin, cells, domain, spacing, sun, peak.as_slice())
}

fn scene_with_peak(
    limits: UploadLimits,
    origin: CellCoord,
    cells: &[(i32, i32)],
    domain: [(i32, i32); 2],
    spacing: f64,
    sun: [f64; 3],
    peaks: &[(i32, i32)],
) -> Scene {
    let coordinate = |(x, y): (i32, i32)| CellCoord {
        x: origin.x.checked_add(x).unwrap(),
        y: origin.y.checked_add(y).unwrap(),
    };
    let mut inputs = Vec::new();
    let mut identity_bytes = Vec::new();
    let mut source_heights = None;
    for &(x, y) in cells {
        let cell = coordinate((x, y));
        let samples: Vec<_> = (0..33)
            .flat_map(|north| {
                (0..33).map(move |east| {
                    if peaks.contains(&(x, y)) && east == 20 && north == 16 {
                        8.0_f32
                    } else {
                        0.0
                    }
                })
            })
            .collect();
        if (x, y) == (0, 0) {
            source_heights = Some(samples.clone());
        }
        let bytes = encode_heights_le(&samples);
        identity_bytes.extend(cell.x.to_le_bytes());
        identity_bytes.extend(cell.y.to_le_bytes());
        identity_bytes.extend(&bytes);
        inputs.push((cell, bytes));
    }
    let digest = sha256(&identity_bytes);
    let identity = SnapshotIdentity {
        worldspace: 0x70,
        cell_size_bits: spacing.to_bits(),
        database: DatabaseIdentity {
            schema_revision: 9,
            content_hash: [1; 32],
        },
        core_cache: CoreCacheIdentity {
            version: 3,
            hash: [2; 32],
        },
        canonical: Some(CanonicalRecipeIdentity {
            package: [3; 32],
            catalog: digest,
            content_digest: digest,
            recipe_revision: 1,
            triangulation: SurfaceTriangulation::CanonicalBase,
        }),
    };
    let mut builder = InventoryBuilder::new(
        identity,
        spacing,
        CellRect::new(coordinate(domain[0]), coordinate(domain[1])).unwrap(),
    )
    .unwrap();
    for (cell, bytes) in &inputs {
        builder
            .add_cell(
                *cell,
                AuthoredStatus::VhgtPresent,
                &[SurfaceInput {
                    kind: SurfaceKind::CanonicalFromAuthored,
                    raw: RawGrid {
                        bytes,
                        expected_sha256: sha256(bytes),
                    },
                }],
            )
            .unwrap();
    }
    let inventory = builder.build();
    let packet = ShadowPacket::build(
        &inventory,
        inputs.iter().map(|(cell, _)| CellInput {
            cell: *cell,
            vhgt: RawVhgtStatus::PresentValid,
        }),
        SourceStatus {
            complete: false,
            record_data_complete: false,
            post_worldspace_weld: true,
        },
    )
    .unwrap();
    let upload = GpuUpload::build(&packet, limits)
        .expect("tiny real packet fits actual created-device limits");
    let context = ViewContext {
        snapshot: identity,
        view_id: 0x1_0000_0011,
        view_generation: 4,
        sun_id: 0x2_0000_0012,
        sun_revision: 6,
        toward_sun: sun,
        view_origin: coordinate((-2, 3)),
        instance_origin: coordinate((1, 1)),
        origin_revision: 7,
        cell_size: spacing,
    };
    Scene {
        inventory,
        upload,
        context,
        source_heights: source_heights.expect("fixture includes receiver cell"),
    }
}
impl Scene {
    fn probe(&self, sample: [f32; 2]) -> Probe {
        let step = (self.context.cell_size / 32.0) as f32;
        // instance origin is exactly one cell northeast of the receiver cell;
        // negative local positions exercise actual exponent/remainder mapping.
        Probe {
            renderer: [
                sample[0] * step - self.context.cell_size as f32,
                0.0,
                self.context.cell_size as f32 - sample[1] * step,
            ],
            world: self.context.snapshot.worldspace,
            view: self.context.view_id,
            sun: self.context.sun_id,
            max_cells: 64,
            max_nodes: 4096,
            actual_origin: None,
        }
    }
    fn oracle(&self, probe: Probe, sample: [f32; 2]) -> u32 {
        // These fixtures deliberately select a flat ZERO contact quad, making
        // the exact canonical origin dyadic; no approximate interpolation is
        // allowed to manufacture an oracle contact height.
        let east = sample[0].floor() as usize;
        let north = sample[1].floor() as usize;
        for sample in [
            north * 33 + east,
            north * 33 + east + 1,
            (north + 1) * 33 + east,
            (north + 1) * 33 + east + 1,
        ] {
            assert_eq!(self.source_heights[sample], 0.0);
        }
        let origin = [
            f64::from(self.context.instance_origin.x) * self.context.cell_size
                + f64::from(probe.renderer[0]),
            f64::from(self.context.instance_origin.y) * self.context.cell_size
                - f64::from(probe.renderer[2]),
            0.0,
        ];
        match self.inventory.trace(
            &SurfaceQuery::canonical_base(self.context.snapshot, SurfaceSource::Canonical),
            origin,
            self.context.toward_sun,
        ) {
            RayResult::Unknown(_) => 0,
            RayResult::Clear(_) => 1,
            RayResult::Occluded(_) => 2,
        }
    }
    fn execute(&self, gpu: &Gpu, label: &str, sample: [f32; 2], expected: u32) -> Observation {
        let probe = self.probe(sample);
        assert_eq!(
            self.oracle(probe, sample),
            expected,
            "{label}: exact dyadic CPU fixture expectation"
        );
        let view = prepare_canonical_view(&self.upload, &self.context).unwrap();
        let result = gpu.run(label, &self.upload, &view, probe);
        assert_eq!(
            result.state, expected,
            "{label}: actual WGSL disagrees with exact CPU oracle; Unknown cannot replace known controls"
        );
        result
    }
}

#[test]
#[ignore = "Requires build-gate TEST GPU ownership; executes actual S5 production WGSL, no no-GPU skip"]
fn production_receiver_executes_known_and_unknown_controls_on_gpu() {
    let gpu = Gpu::new();
    let limits = gpu.limits();
    let zero = CellCoord { x: 0, y: 0 };
    let shadow = scene(
        limits,
        zero,
        &[(0, 0)],
        [(0, 0), (0, 0)],
        32.0,
        [1.0, 0.0, 0.0625],
        true,
    );
    let hit = shadow.execute(
        &gpu,
        "single-vertex narrow occluder, nonzero low sun",
        [4.25, 16.25],
        2,
    );
    assert!(
        hit.cells > 0 && hit.nodes > 0 && hit.triangles > 0,
        "known-shadow must traverse real triangles"
    );
    let flat = scene(
        limits,
        zero,
        &[(0, 0)],
        [(0, 0), (0, 0)],
        32.0,
        [0.0, 0.0, 1.0],
        false,
    );
    let clear = flat.execute(
        &gpu,
        "simple canonical contact: only t=0 excluded",
        [16.25, 16.25],
        1,
    );
    assert_eq!(clear.reasons, 0);
    // The complete flat root itself proves that an upward ray at z=0 has no
    // positive intersection: the new broadphase need not enter a local page.
    assert_eq!(clear.cells, 0);
    assert_eq!(clear.nodes, 1);

    let seam = scene_with_peak(
        limits,
        zero,
        &[(-1, 0), (0, 0)],
        [(-1, 0), (0, 0)],
        32.0,
        [0.0, 0.0, 1.0],
        &[(-1, 0), (0, 0)],
    );
    let seam_clear = seam.execute(
        &gpu,
        "exact shared-edge contact, parallel horizontal axes",
        [0.0, 16.25],
        1,
    );
    assert!(seam_clear.cells >= 2);
    let corner = scene_with_peak(
        limits,
        zero,
        &[(-1, -1), (-1, 0), (0, -1), (0, 0)],
        [(-1, -1), (0, 0)],
        32.0,
        [0.0, 0.0, 1.0],
        &[(-1, -1), (-1, 0), (0, -1), (0, 0)],
    );
    let corner_clear = corner.execute(
        &gpu,
        "exact shared-corner contact with all four cells",
        [0.0, 0.0],
        1,
    );
    assert!(corner_clear.cells >= 4);

    let gaps = scene(
        limits,
        zero,
        &[(0, 0), (1, 1)],
        [(0, 0), (1, 1)],
        32.0,
        [1.0, 1.0, 0.125],
        false,
    );
    let gap = gaps.execute(
        &gpu,
        "corner supercover includes unknown side neighbours",
        [31.5, 31.5],
        0,
    );
    assert_ne!(gap.reasons & reason::UNLISTED, 0);
    assert!(gap.unknown_cells > 0);
    let tail = scene(
        limits,
        zero,
        &[(0, 0), (128, 0)],
        [(0, 0), (128, 0)],
        32.0,
        [1.0, 0.0, 1.0],
        false,
    );
    let unknown_tail = tail.execute(
        &gpu,
        "unknown tail remains unknown after rising above every known height",
        [4.25, 16.25],
        0,
    );
    assert_ne!(unknown_tail.reasons & reason::UNLISTED, 0);
    let cells: Vec<_> = (0..=8).map(|x| (x, 0)).collect();
    let remote = scene_with_peak(
        limits,
        zero,
        &cells,
        [(0, 0), (8, 0)],
        32.0,
        [1.0, 0.0, 1.0 / 128.0],
        &[(8, 0)],
    );
    remote.execute(
        &gpu,
        "remote narrow caster survives known-flat subtree skips",
        [4.25, 16.25],
        2,
    );
    let mut malformed = scene(
        limits,
        zero,
        &[(0, 0)],
        [(0, 0), (0, 0)],
        32.0,
        [0.0, 0.0, 1.0],
        false,
    );
    let tree_start = malformed.upload.header[15] as usize + 682;
    malformed.upload.words[tree_start + 6] = 0;
    let malformed_view = prepare_canonical_view(&malformed.upload, &malformed.context).unwrap();
    let refused = gpu.run(
        "non-forward world escape refuses instead of skipping casters",
        &malformed.upload,
        &malformed_view,
        malformed.probe([16.25, 16.25]),
    );
    assert_eq!(refused.state, 0);
    assert_ne!(refused.reasons & 16384, 0);
    let parallel = scene(
        limits,
        zero,
        &[(0, 0), (1, 0)],
        [(0, -1), (1, 0)],
        32.0,
        [1.0, 0.0, 0.125],
        false,
    );
    let gap = parallel.execute(
        &gpu,
        "ray on seam cannot skip unknown parallel neighbour",
        [4.25, 0.0],
        0,
    );
    assert_ne!(gap.reasons & reason::UNLISTED, 0);
    assert!(gap.unknown_cells > 0);
    let coplanar = scene(
        limits,
        zero,
        &[(0, 0)],
        [(0, 0), (0, 0)],
        32.0,
        [1.0, 0.0, 0.0],
        false,
    );
    let unknown = coplanar.execute(
        &gpu,
        "coplanar canonical ray is explicit Unknown",
        [4.25, 16.25],
        0,
    );
    assert_ne!(unknown.reasons & COPLANAR_REASON, 0);
    assert!(unknown.triangles > 0);

    let view = prepare_canonical_view(&shadow.upload, &shadow.context).unwrap();
    let mut budget = shadow.probe([4.25, 16.25]);
    budget.max_cells = 0;
    let result = gpu.run("cell budget exhaustion", &shadow.upload, &view, budget);
    assert_eq!(result.state, 0);
    assert_ne!(result.reasons & reason::WORK_BUDGET, 0);
    assert_eq!(result.cells, 0);
    budget.max_cells = 64;
    budget.max_nodes = 0;
    let result = gpu.run("node budget exhaustion", &shadow.upload, &view, budget);
    assert_eq!(result.state, 0);
    assert_ne!(result.reasons & reason::WORK_BUDGET, 0);
    assert_eq!(result.nodes, 0);

    // Same production shader/packet builder, actual game spacing and huge signed
    // origins. No large absolute f32 point is constructed for GPU mapping.
    let far = scene(
        limits,
        CellCoord {
            x: -1_000_000_000,
            y: 1_000_000_000,
        },
        &[(0, 0)],
        [(0, 0), (0, 0)],
        4096.0,
        [128.0, 0.0, 0.0625],
        true,
    );
    far.execute(
        &gpu,
        "far negative origin and4096 spacing low-sun occluder",
        [4.25, 16.25],
        2,
    );
    // The per-instance-origin entry accepts an actual instance origin even
    // when a shared view uniform carries another origin. The unchanged legacy
    // entry must fail at that unrelated location; it cannot accidentally be
    // the function under test for this explicit-origin control.
    let mut shared_context = far.context;
    shared_context.instance_origin = CellCoord { x: 0, y: 0 };
    let shared_view = prepare_canonical_view(&far.upload, &shared_context).unwrap();
    let mut explicit = far.probe([4.25, 16.25]);
    let legacy = gpu.run(
        "legacy origin cannot map the unrelated shared uniform",
        &far.upload,
        &shared_view,
        explicit,
    );
    assert_eq!(legacy.state, 0);
    assert_ne!(legacy.reasons & reason::RECEIVER_UNRESOLVED, 0);
    assert_eq!(far.oracle(explicit, [4.25, 16.25]), 2);
    explicit.actual_origin = Some(far.context.instance_origin);
    let actual = gpu.run(
        "actual per-instance origin entry preserves far known shadow",
        &far.upload,
        &shared_view,
        explicit,
    );
    assert_eq!(
        actual.state, 2,
        "explicit-origin known control cannot become allUnknown"
    );
    assert!(actual.cells > 0 && actual.nodes > 0 && actual.triangles > 0);
    explicit.actual_origin = Some(CellCoord { x: 0, y: 0 });
    let wrong = gpu.run(
        "wrong explicit instance origin stays unresolved",
        &far.upload,
        &shared_view,
        explicit,
    );
    assert_eq!(wrong.state, 0);
    assert_ne!(wrong.reasons & reason::RECEIVER_UNRESOLVED, 0);

    let mut other_context = far.context;
    other_context.snapshot.worldspace += 1;
    assert!(
        prepare_canonical_view(&far.upload, &other_context).is_err(),
        "host rejects FULL snapshot world mismatch"
    );
    let far_view = prepare_canonical_view(&far.upload, &far.context).unwrap();
    for field in 0..3 {
        let mut wrong = far.probe([4.25, 16.25]);
        match field {
            0 => wrong.world += 1,
            1 => wrong.view += 1,
            _ => wrong.sun += 1,
        }
        let result = gpu.run(
            "per-view/world/sun identity refusal",
            &far.upload,
            &far_view,
            wrong,
        );
        assert_eq!(result.state, 0);
        assert_ne!(result.reasons & reason::IDENTITY_MISMATCH, 0);
        assert_eq!(result.cells, 0);
    }
}
