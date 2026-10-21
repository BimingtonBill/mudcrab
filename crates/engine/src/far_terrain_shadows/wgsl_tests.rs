//! CPU-only composition of the isolated receiver with actual checked terrain shaders.
//! The diagnostic insertion is deliberately not production lighting integration:
//! it keeps both original lighting and F1 resources live for entrypoint reflection.
use crate::far_terrain_shadows::bindings_audit::{add, checked_source, collect};
use naga_oil::compose::{
    ComposableModuleDescriptor, Composer, NagaModuleDescriptor, ShaderDefValue,
    get_preprocessor_data,
};
use std::{collections::HashMap, fs, path::Path};
use wgpu::naga::{
    self,
    valid::{Capabilities, ValidationFlags, Validator},
};

const RECEIVER: &str = include_str!("receiving.wgsl");
const IMPORT: &str = "#import far_terrain_shadows::receiving::{f1_receive, f1_visibility}\n";

fn composer() -> Composer {
    let mut composer = Composer::default().with_capabilities(Capabilities::all());
    if let Err(error) = composer.add_composable_module(ComposableModuleDescriptor {
        source: include_str!("arithmetic.wgsl"),
        file_path: "isolated/arithmetic.wgsl",
        ..Default::default()
    }) {
        panic!("{}", error.emit_to_string(&composer));
    }
    if let Err(error) = composer.add_composable_module(ComposableModuleDescriptor {
        source: RECEIVER,
        file_path: "isolated/receiving.wgsl",
        ..Default::default()
    }) {
        panic!("{}", error.emit_to_string(&composer));
    }
    composer
}

fn compose(
    composer: &mut Composer,
    source: &str,
    defs: HashMap<String, ShaderDefValue>,
) -> (naga::Module, naga::valid::ModuleInfo) {
    let module = composer
        .make_naga_module(NagaModuleDescriptor {
            source,
            file_path: "isolated/composition_fixture.wgsl",
            shader_defs: defs,
            ..Default::default()
        })
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(composer)));
    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .unwrap_or_else(|error| panic!("{error:?}"));
    (module, info)
}

fn storage_bindings(module: &naga::Module, info: &naga::valid::ModuleInfo) -> Vec<(u32, u32)> {
    let entry = module
        .entry_points
        .iter()
        .position(|entry| entry.name == "fragment" && entry.stage == naga::ShaderStage::Fragment)
        .unwrap();
    let usage = info.get_entry_point(entry);
    let mut bindings = Vec::new();
    for (handle, global) in module.global_variables.iter() {
        if let naga::AddressSpace::Storage { access } = global.space
            && !usage[handle].is_empty()
        {
            let binding = global.binding.as_ref().unwrap();
            if (binding.group, binding.binding) == (4, 8) {
                assert_eq!(access, naga::StorageAccess::LOAD, "F1 must be read-only");
            }
            bindings.push((binding.group, binding.binding));
        }
    }
    bindings.sort_unstable();
    bindings
}

fn uniform_span(module: &naga::Module, binding: u32) -> u32 {
    let (_, global) = module
        .global_variables
        .iter()
        .find(|(_, global)| {
            global
                .binding
                .as_ref()
                .is_some_and(|resource| resource.group == 4 && resource.binding == binding)
        })
        .unwrap();
    assert_eq!(global.space, naga::AddressSpace::Uniform);
    match module.types[global.ty].inner {
        naga::TypeInner::Struct { span, .. } => span,
        _ => panic!("uniform binding {binding} must have an explicit struct layout"),
    }
}

#[test]
fn isolated_receiver_api_and_uniform_layout_compose() {
    let source = format!(
        "{IMPORT}\n@fragment fn fragment() -> @location(0) vec4<f32> {{\n\
         let result = f1_receive(vec3<f32>(0.0), 0u, vec2<u32>(0u), vec2<u32>(0u), 32u, 128u);\n\
         return vec4<f32>(f1_visibility(result), f32(result.reasons), f32(result.cells), f32(result.nodes));\n\
         }}"
    );
    let (module, info) = compose(&mut composer(), &source, HashMap::new());
    assert_eq!(storage_bindings(&module, &info), [(4, 8)]);
    assert_eq!(uniform_span(&module, 7), 64);
    assert_eq!(uniform_span(&module, 9), 400);
    // Counters are returned to the caller, not atomically written from fragments.
    for (_, ty) in module.types.iter() {
        assert!(!matches!(ty.inner, naga::TypeInner::Atomic(_)));
    }
}

#[test]
fn explicit_instance_origin_entrypoint_composes_with_unchanged_bindings() {
    let source = "#import far_terrain_shadows::receiving::{f1_receive_at_origin, f1_visibility}\n
        @fragment fn fragment(@location(0) local: vec3<f32>,
            @location(1) @interpolate(flat) origin: vec2<i32>) -> @location(0) vec4<f32> {
            let result = f1_receive_at_origin(local, 31u, origin,
                vec2<u32>(11u, 0u), vec2<u32>(12u, 0u), 32u, 128u);
            return vec4<f32>(f1_visibility(result), f32(result.reasons),
                f32(result.cells), f32(result.nodes));
        }";
    let (module, info) = compose(&mut composer(), source, HashMap::new());
    assert_eq!(storage_bindings(&module, &info), [(4, 8)]);
    assert_eq!(uniform_span(&module, 7), 64);
    assert_eq!(uniform_span(&module, 9), 400);
}

#[test]
fn actual_nonbindless_terrain_fragment_composition_adds_one_storage_binding() {
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(
                std::env::var_os("USERPROFILE")
                    .or_else(|| std::env::var_os("HOME"))
                    .expect("CARGO_HOME, USERPROFILE or HOME required"),
            )
            .join(".cargo")
        });
    let registry = fs::read_dir(cargo_home.join("registry/src"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.join("bevy_pbr-0.19.0").is_dir())
        .expect("locked Bevy 0.19.0 shader sources required");
    let mut sources = HashMap::new();
    for package in ["bevy_pbr", "bevy_core_pipeline", "bevy_light"] {
        collect(
            &registry.join(format!("{package}-0.19.0/src")),
            &mut sources,
        );
    }
    collect(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/bevy_render-0.19.0/src"),
        &mut sources,
    );
    sources.insert(
        "cluster_terrain::clip".into(),
        (
            "checked/terrain_clip.wgsl".into(),
            checked_source("crates/engine/src/cluster/shaders/terrain_clip.wgsl"),
        ),
    );
    for cluster in [false, true] {
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
        if !cluster {
            defs.insert("VERTEX_UVS_B".into(), ShaderDefValue::Bool(true));
        }
        for (name, value) in [
            ("MATERIAL_BIND_GROUP", 3),
            ("AVAILABLE_STORAGE_BUFFER_BINDINGS", 8),
            ("MAX_DIRECTIONAL_LIGHTS", 10),
            ("MAX_CASCADES_PER_LIGHT", 4),
            ("MAX_RECT_LIGHTS", 8),
        ] {
            defs.insert(name.into(), ShaderDefValue::UInt(value));
        }
        let source = checked_source(if cluster {
            "crates/engine/src/cluster/shaders/terrain_material.wgsl"
        } else {
            "crates/engine/src/shaders/terrain.wgsl"
        });
        assert!(!defs.contains_key("BINDLESS"));
        let insertion = "    var out: FragmentOutput;";
        assert_eq!(source.matches(insertion).count(), 1);
        let source = format!("{IMPORT}{source}").replace(
            insertion,
            "    let f1_fixture = f1_receive(vec3<f32>(0.0), 0u, vec2<u32>(0u), vec2<u32>(0u), 32u, 128u);\n    var out: FragmentOutput;",
        ).replace(
            "    return out;",
            "    out.color = vec4<f32>(out.color.rgb + vec3<f32>(f1_visibility(f1_fixture) * 0.00001), out.color.a);\n    return out;",
        );
        // This diagnostic call is after cluster clipping and before lighting.
        // The output perturbation keeps F1 live without pretending to implement
        // direct-sun-only lighting in either shared shader.
        let mut composer = composer();
        for import in get_preprocessor_data(&source).1 {
            // This fixture is all-off for the optional terrain forward ABI.
            // Leave source intact so an unguarded optional import still fails.
            if import.import.starts_with("far_terrain_shadows::") {
                continue;
            }
            if import.import != "far_terrain_shadows::receiving" {
                add(&import.import, &sources, &mut composer, &defs);
            }
        }
        let (module, info) = compose(&mut composer, &source, defs);
        let mut expected = vec![(0, 8), (0, 9), (0, 10), (2, 0)];
        if cluster {
            expected.push((4, 6));
        }
        expected.push((4, 8));
        assert_eq!(storage_bindings(&module, &info), expected);
        assert_eq!(uniform_span(&module, 7), 64);
        assert_eq!(uniform_span(&module, 9), 400);
    }
}
