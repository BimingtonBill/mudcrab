//! CPU composition of the optional selected-sun hook; not runtime wiring or a GPU proof.
//! Test-only receiver coordinates are explicit uniforms, not invented production varyings.
use super::bindings_audit::{add, collect};
use naga_oil::compose::{Composer, NagaModuleDescriptor, ShaderDefValue, get_preprocessor_data};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use wgpu::naga::{
    self,
    valid::{Capabilities, ValidationFlags, Validator},
};

type Sources = HashMap<String, (String, String)>;
const CANDIDATE: &str = include_str!("pbr_functions.wgsl");
const STOCK_SHA: &str = "b0c544de95502051f9db70c67ee387d318c101800893915e463225020ffee180";
const CANDIDATE_SHA: &str = "60fdd4c69fd39b7020cf8736137a6dcb7e92ba677511535a60f0c734abb8de60";
const STOCK_IMPORT: &str = "#define_import_path bevy_pbr::pbr_functions";
const FAR_IMPORT: &str = "#define_import_path far_terrain_shadows::pbr_functions";
const STOCK_SIGNATURE: &str =
    "fn apply_pbr_lighting(\n    in: pbr_types::PbrInput,\n) -> vec4<f32> {";
const FAR_SIGNATURE: &str = "fn apply_far_terrain_lighting(\n    in: pbr_types::PbrInput,\n    selected_sun_index: u32,\n    canonical_visibility: f32,\n) -> vec4<f32> {";
const INSERTION: &str = "        if (i == selected_sun_index\n                && (in.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u\n                && ((*light).flags & mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u\n                && (*light).num_cascades > 0u\n                && (*light).num_cascades <= #{MAX_CASCADES_PER_LIGHT}u) {\n            let final_cascade = (*light).num_cascades - 1u;\n            if (-view_z >= (*light).cascades[final_cascade].far_bound) {\n                shadow *= canonical_visibility;\n            }\n        }\n";

const TEST_INPUT: &str = r#"
#import far_terrain_shadows::pbr_functions::apply_far_terrain_lighting
#import far_terrain_shadows::receiving::{f1_receive_at_origin, f1_visibility}
// Fixture-only input: deliberately independent of the production vertex ABI.
struct F1LightingTestInput {
    selected_sun_index: u32,
    receiver_world: u32,
    padding: vec2<u32>,
    actual_instance_origin: vec2<i32>,
    expected_view: vec2<u32>,
    expected_sun: vec2<u32>,
    work_budgets: vec2<u32>,
    instance_local_renderer: vec4<f32>,
}
@group(4) @binding(10) var<uniform> f1_lighting_test: F1LightingTestInput;
"#;
const FAR_CALL: &str = r#"    let canonical = f1_receive_at_origin(
        f1_lighting_test.instance_local_renderer.xyz,
        f1_lighting_test.receiver_world,
        f1_lighting_test.actual_instance_origin,
        f1_lighting_test.expected_view,
        f1_lighting_test.expected_sun,
        f1_lighting_test.work_budgets.x,
        f1_lighting_test.work_budgets.y);
    out.color = apply_far_terrain_lighting(pbr_input,
        f1_lighting_test.selected_sun_index, f1_visibility(canonical));"#;

fn registry() -> PathBuf {
    let home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                std::env::var_os("USERPROFILE")
                    .or_else(|| std::env::var_os("HOME"))
                    .expect("CARGO_HOME, USERPROFILE or HOME required"),
            )
            .join(".cargo")
        });
    fs::read_dir(home.join("registry/src"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.join("bevy_pbr-0.19.0").is_dir())
        .expect("installed locked Bevy 0.19.0 sources required")
}

fn normalize(source: &str) -> String {
    source.replace("\r\n", "\n")
}
fn digest(source: &str) -> String {
    format!("{:x}", Sha256::digest(source.as_bytes()))
}
fn replace_once(source: &str, from: &str, to: &str) -> String {
    assert_eq!(
        source.matches(from).count(),
        1,
        "exact transformation anchor: {from}"
    );
    source.replacen(from, to, 1)
}

#[test]
fn pinned_lighting_copy_reverses_to_exact_locked_stock() {
    let stock = normalize(
        &fs::read_to_string(registry().join("bevy_pbr-0.19.0/src/render/pbr_functions.wgsl"))
            .unwrap(),
    );
    let candidate = normalize(CANDIDATE);
    assert_eq!(digest(&stock), STOCK_SHA);
    assert_eq!(digest(&candidate), CANDIDATE_SHA);
    let restored = replace_once(&candidate, FAR_IMPORT, STOCK_IMPORT);
    let restored = replace_once(&restored, FAR_SIGNATURE, STOCK_SIGNATURE);
    let restored = replace_once(&restored, INSERTION, "");
    assert_eq!(
        restored, stock,
        "all other stock lighting/transmission/ambient/fog bytes preserved"
    );
    // bevy_pbr 0.19.0, upstream c6f634ca9f406d68ba5109d921247b654cb42c10,
    // MIT OR Apache-2.0. Root copied the installed license texts alongside this shader.
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/far_terrain_shadows");
    for name in ["LICENSE-MIT", "LICENSE-APACHE"] {
        assert_eq!(
            fs::read(directory.join(format!("BEVY-{name}"))).unwrap(),
            fs::read(registry().join("bevy_pbr-0.19.0").join(name)).unwrap()
        );
    }
}

pub(crate) fn sources() -> Sources {
    let registry = registry();
    let mut sources = Sources::new();
    for package in ["bevy_pbr", "bevy_core_pipeline", "bevy_light"] {
        collect(
            &registry.join(format!("{package}-0.19.0/src")),
            &mut sources,
        );
    }
    let engine = Path::new(env!("CARGO_MANIFEST_DIR"));
    collect(
        &engine.join("../../vendor/bevy_render-0.19.0/src"),
        &mut sources,
    );
    sources.insert(
        "cluster_terrain::clip".into(),
        (
            "current/terrain_clip.wgsl".into(),
            include_str!("../cluster/shaders/terrain_clip.wgsl").into(),
        ),
    );
    sources
}

pub(crate) fn defs(
    bindless: bool,
    cascades: u32,
    variant: &[&str],
) -> HashMap<String, ShaderDefValue> {
    let mut defs = HashMap::new();
    for name in [
        "MESH_PIPELINE",
        "VERTEX_OUTPUT_INSTANCE_INDEX",
        "VERTEX_POSITIONS",
        "VERTEX_NORMALS",
        "VERTEX_UVS",
        "VERTEX_UVS_A",
        "VERTEX_TANGENTS",
        "VERTEX_COLORS",
        "VIEW_PROJECTION_PERSPECTIVE",
        "DISTANCE_FOG",
        "DFG_LUT",
        "PBR_SPECULAR_TEXTURES_SUPPORTED",
    ] {
        defs.insert(name.into(), ShaderDefValue::Bool(true));
    }
    for (name, value) in [
        ("MATERIAL_BIND_GROUP", 3),
        ("AVAILABLE_STORAGE_BUFFER_BINDINGS", 8),
        ("MAX_DIRECTIONAL_LIGHTS", 10),
        ("MAX_CASCADES_PER_LIGHT", cascades),
        ("MAX_RECT_LIGHTS", 8),
    ] {
        defs.insert(name.into(), ShaderDefValue::UInt(value));
    }
    if bindless {
        defs.insert("BINDLESS".into(), ShaderDefValue::Bool(true));
    }
    for name in variant {
        defs.insert((*name).into(), ShaderDefValue::Bool(true));
    }
    defs
}

fn compose(
    source: &str,
    sources: &Sources,
    defs: HashMap<String, ShaderDefValue>,
) -> (naga::Module, naga::valid::ModuleInfo) {
    let mut composer = Composer::default().with_capabilities(Capabilities::all());
    for import in get_preprocessor_data(source).1 {
        // Bevy resolves declared imports before applying shader definitions.
        add(&import.import, sources, &mut composer, &defs);
    }
    let module = composer
        .make_naga_module(NagaModuleDescriptor {
            source,
            file_path: "test-only/current_cluster_terrain_forward.wgsl",
            shader_defs: defs,
            ..Default::default()
        })
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&composer)));
    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .unwrap_or_else(|error| panic!("{error:?}"));
    (module, info)
}

fn live_bindings(module: &naga::Module, info: &naga::valid::ModuleInfo) -> Vec<(u32, u32)> {
    let fragment = module
        .entry_points
        .iter()
        .position(|entry| entry.name == "fragment" && entry.stage == naga::ShaderStage::Fragment)
        .unwrap();
    let usage = info.get_entry_point(fragment);
    let mut bindings = module
        .global_variables
        .iter()
        .filter_map(|(handle, global)| {
            let binding = global.binding.as_ref()?;
            (!usage[handle].is_empty()).then_some((binding.group, binding.binding))
        })
        .collect::<Vec<_>>();
    bindings.sort_unstable();
    bindings
}

fn live_storage(module: &naga::Module, info: &naga::valid::ModuleInfo) -> Vec<(u32, u32)> {
    let live = live_bindings(module, info);
    let mut result = module
        .global_variables
        .iter()
        .filter_map(|(_, global)| {
            let binding = global.binding.as_ref()?;
            if let naga::AddressSpace::Storage { access } = global.space {
                let pair = (binding.group, binding.binding);
                if live.contains(&pair) {
                    if pair == (4, 8) {
                        assert_eq!(access, naga::StorageAccess::LOAD);
                    }
                    return Some(pair);
                }
            }
            None
        })
        .collect::<Vec<_>>();
    result.sort_unstable();
    result
}

fn expected_storage(bindless: bool) -> Vec<(u32, u32)> {
    let mut expected = vec![(0, 8), (0, 9), (0, 10), (2, 0)];
    if bindless {
        expected.extend([(3, 0), (3, 10)]);
    }
    expected.push((4, 6));
    expected
}

#[test]
fn selected_sun_receiver_remains_live_in_actual_cluster_terrain_lighting() {
    let mut sources = sources();
    for (name, source) in [
        ("arithmetic", include_str!("arithmetic.wgsl")),
        ("receiving", include_str!("receiving.wgsl")),
        ("pbr_functions", CANDIDATE),
    ] {
        sources.insert(
            format!("far_terrain_shadows::{name}"),
            (format!("optional/{name}.wgsl"), source.into()),
        );
    }
    let stock = include_str!("../cluster/shaders/terrain_material.wgsl");
    let source = replace_once(
        stock,
        "pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing}",
        "pbr_functions::{alpha_discard, main_pass_post_lighting_processing}",
    );
    let source = format!(
        "{TEST_INPUT}\n{}",
        replace_once(
            &source,
            "    out.color = apply_pbr_lighting(pbr_input);",
            FAR_CALL
        )
    );
    assert!(!source.contains("apply_pbr_lighting(pbr_input)"));
    // No RGB perturbation: receiver visibility enters only the real lighting call.
    assert!(source.contains("f1_lighting_test.selected_sun_index, f1_visibility(canonical)"));
    for bindless in [false, true] {
        for cascades in [1, 4] {
            // Transmission cases check stock-preservation composition only: runtime cluster
            // eligibility remains opaque/alpha-masked and no transmissive feature is enabled here.
            for variant in [
                vec![],
                vec!["CONTACT_SHADOWS", "DEPTH_PREPASS"],
                vec!["STANDARD_MATERIAL_DIFFUSE_TRANSMISSION"],
                vec!["STANDARD_MATERIAL_SPECULAR_TRANSMISSION", "DEPTH_PREPASS"],
            ] {
                let (module, info) = compose(&source, &sources, defs(bindless, cascades, &variant));
                let live = live_bindings(&module, &info);
                for binding in 7..=10 {
                    assert!(
                        live.contains(&(4, binding)),
                        "F1 binding {binding} dead: {variant:?}"
                    );
                }
                let mut expected = expected_storage(bindless);
                expected.push((4, 8));
                assert_eq!(
                    live_storage(&module, &info),
                    expected,
                    "bindless={bindless}, cascades={cascades}, variant={variant:?}"
                );
                let function = module
                    .functions
                    .iter()
                    .find(|(_, function)| {
                        function
                            .name
                            .as_deref()
                            .is_some_and(|name| name.contains("apply_far_terrain_lighting"))
                    })
                    .map(|(_, function)| function)
                    .expect("selected-sun lighting function retained");
                assert_eq!(function.arguments.len(), 3);
                assert!(
                    function
                        .expressions
                        .iter()
                        .any(|(_, expression)| match expression {
                            naga::Expression::Binary {
                                op: naga::BinaryOperator::Equal,
                                left,
                                right,
                            } =>
                                matches!(
                                    function.expressions[*left],
                                    naga::Expression::FunctionArgument(1)
                                ) || matches!(
                                    function.expressions[*right],
                                    naga::Expression::FunctionArgument(1)
                                ),
                            _ => false,
                        }),
                    "nonconstant selected sun must participate in the light-index comparison"
                );
            }
        }
    }
}

#[test]
fn all_off_cluster_shader_needs_no_optional_module_or_binding() {
    let sources = sources();
    assert!(
        sources
            .keys()
            .all(|name| !name.starts_with("far_terrain_shadows::"))
    );
    let stock = include_str!("../cluster/shaders/terrain_material.wgsl");
    assert!(stock.contains("out.color = apply_pbr_lighting(pbr_input);"));
    assert!(!stock.contains("far_terrain_shadows::"));
    for bindless in [false, true] {
        for cascades in [1, 4] {
            let (module, info) = compose(stock, &sources, defs(bindless, cascades, &[]));
            assert!(module.types.iter().all(|(_, ty)| {
                ty.name
                    .as_deref()
                    .is_none_or(|name| !name.contains("FarVertexOutput"))
            }));
            assert_eq!(live_storage(&module, &info), expected_storage(bindless));
            assert!(
                live_bindings(&module, &info)
                    .iter()
                    .all(|&(group, binding)| group != 4 || !(7..=10).contains(&binding))
            );
            assert!(module.functions.iter().all(|(_, function)| {
                function
                    .name
                    .as_deref()
                    .is_none_or(|name| !name.contains("apply_far_terrain_lighting"))
            }));
        }
    }
}
