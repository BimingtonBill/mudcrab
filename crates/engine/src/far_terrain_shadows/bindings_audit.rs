//! CPU-only audit of checked terrain fragment entrypoints, imported only by the test harness.
//! Requires locked Bevy 0.19.0 registry sources, this checkout's patched bevy_render
//! vendor sources, and the local Git object d349ba57. The reference object is a
//! deliberate adoption-audit prerequisite; missing history must fail, not silently
//! substitute older branch shaders or handwritten stand-ins.
//! No stub modules, GPU, runtime registration or shader edits.
use naga_oil::compose::{
    ComposableModuleDescriptor, Composer, NagaModuleDescriptor, ShaderDefValue,
    get_preprocessor_data,
};
use std::{collections::HashMap, fs, path::Path, process::Command};
use wgpu::naga::{
    self,
    valid::{Capabilities, ValidationFlags, Validator},
};

pub(crate) fn collect(path: &Path, sources: &mut HashMap<String, (String, String)>) {
    for entry in fs::read_dir(path).expect("installed Bevy source directory") {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, sources);
        } else if path.extension().is_some_and(|ext| ext == "wgsl") {
            // PbrPlugin loads the dummy under this import name. Only the optional
            // MeshletPlugin replaces it with the real resolver; this engine uses
            // its own cluster renderer, not Bevy's experimental meshlet feature.
            if path.ends_with("meshlet/visibility_buffer_resolve.wgsl") {
                continue;
            }
            let source = fs::read_to_string(&path).unwrap();
            if let Some(name) = get_preprocessor_data(&source).0 {
                if let Some((previous, _)) = sources.get(&name) {
                    panic!(
                        "duplicate real shader module {name}: {previous} and {}",
                        path.display()
                    );
                }
                sources.insert(name, (path.display().to_string(), source));
            }
        }
    }
}

pub(crate) fn checked_source(path: &str) -> String {
    let output = Command::new("git")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["show", &format!("d349ba57:{path}")])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "local Git object d349ba57 with checked runtime source required: {path}"
    );
    String::from_utf8(output.stdout).unwrap()
}

pub(crate) fn add(
    name: &str,
    sources: &HashMap<String, (String, String)>,
    composer: &mut Composer,
    defs: &HashMap<String, ShaderDefValue>,
) {
    if composer.contains_module(name) {
        return;
    }
    let (path, source) = sources
        .get(name)
        .unwrap_or_else(|| panic!("missing real shader module {name}"));
    for import in get_preprocessor_data(source).1 {
        add(&import.import, sources, composer, defs);
    }
    if let Err(error) = composer.add_composable_module(ComposableModuleDescriptor {
        source,
        file_path: path,
        shader_defs: defs.clone(),
        ..Default::default()
    }) {
        panic!("{}", error.emit_to_string(composer));
    }
}

#[test]
fn checked_terrain_fragment_storage_resources() {
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(
                std::env::var_os("USERPROFILE")
                    .or_else(|| std::env::var_os("HOME"))
                    .expect(
                        "CARGO_HOME, USERPROFILE or HOME required to locate locked shader sources",
                    ),
            )
            .join(".cargo")
        });
    let registry = cargo_home.join("registry/src");
    let registry = fs::read_dir(registry)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.join("bevy_pbr-0.19.0").is_dir())
        .expect("locked Bevy 0.19.0 sources required");
    let mut sources = HashMap::new();
    for package in ["bevy_pbr", "bevy_core_pipeline", "bevy_light"] {
        collect(
            &registry.join(format!("{package}-0.19.0/src")),
            &mut sources,
        );
    }
    // Cargo.toml patches bevy_render to this vendor tree: compose the actual
    // renderer sources, even when registry copies happen to be identical.
    collect(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/bevy_render-0.19.0/src"),
        &mut sources,
    );
    let clip = checked_source("crates/engine/src/cluster/shaders/terrain_clip.wgsl");
    sources.insert(
        "cluster_terrain::clip".into(),
        ("checked/terrain_clip.wgsl".into(), clip),
    );
    for cluster in [false, true] {
        for bindless in [false, true] {
            if !cluster && bindless {
                continue; // Actual stock TerrainExtension does not support bindless.
            }
            // MeshPipeline's desktop storage path. This audit covers baseline forward
            // lighting, not optional atmosphere/OIT/decal variants. Full-layout device
            // limit checks are separately required before any pipeline adoption.
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
            if bindless {
                defs.insert("BINDLESS".into(), ShaderDefValue::Bool(true));
            }
            let source = checked_source(if cluster {
                "crates/engine/src/cluster/shaders/terrain_material.wgsl"
            } else {
                "crates/engine/src/shaders/terrain.wgsl"
            });
            let mut composer = Composer::default().with_capabilities(Capabilities::all());
            for import in get_preprocessor_data(&source).1 {
                add(&import.import, &sources, &mut composer, &defs);
            }
            let module = composer
                .make_naga_module(NagaModuleDescriptor {
                    source: &source,
                    file_path: "checked/terrain_fragment.wgsl",
                    shader_defs: defs.clone(),
                    ..Default::default()
                })
                .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&composer)));
            let info = Validator::new(ValidationFlags::all(), Capabilities::all())
                .validate(&module)
                .unwrap();
            let entry = module
                .entry_points
                .iter()
                .position(|entry| {
                    entry.name == "fragment" && entry.stage == naga::ShaderStage::Fragment
                })
                .unwrap();
            let usage = info.get_entry_point(entry);
            let mut bindings = Vec::new();
            for (handle, global) in module.global_variables.iter() {
                if matches!(global.space, naga::AddressSpace::Storage { .. })
                    && !usage[handle].is_empty()
                {
                    let binding = global.binding.as_ref().unwrap();
                    bindings.push((
                        binding.group,
                        binding.binding,
                        global.name.as_deref().unwrap_or("?"),
                    ));
                }
            }
            bindings.sort_unstable();
            println!(
                "cluster={cluster} bindless={bindless} defs={defs:?} storage={} {bindings:?}",
                bindings.len()
            );
            let pairs: Vec<_> = bindings
                .iter()
                .map(|&(group, binding, _)| (group, binding))
                .collect();
            let mut expected = vec![(0, 8), (0, 9), (0, 10), (2, 0)];
            if bindless {
                expected.extend([(3, 0), (3, 10)]);
            }
            if cluster {
                expected.push((4, 6));
            }
            assert_eq!(
                pairs, expected,
                "checked fragment resources: cluster={cluster}, bindless={bindless}"
            );
        }
    }
}
