//! Test-only linkage of the actual optional cluster terrain forward ABI.
//! Requires the reviewed ABI files adopted at their intended paths; no runtime proof.
use super::{bindings_audit::add, lighting_tests};
use naga_oil::compose::{Composer, NagaModuleDescriptor, ShaderDefValue, get_preprocessor_data};
use std::collections::HashMap;
use wgpu::naga::{
    self,
    valid::{Capabilities, ValidationFlags, Validator},
};

type Sources = HashMap<String, (String, String)>;
const FLAG: &str = "CLUSTER_FAR_TERRAIN_SHADOWS";
const VERTEX: &str = include_str!("cluster_draw.wgsl");
const FRAGMENT: &str = include_str!("terrain_material.wgsl");
const ORDINARY_VERTEX: &str = include_str!("../cluster/shaders/cluster_draw.wgsl");
const ORDINARY_FRAGMENT: &str = include_str!("../cluster/shaders/terrain_material.wgsl");
// Pinned pre-ABI originals from integration 3c97548372202c974618452a3e4c43b595217520.
// Included fixtures keep this test usable from source archives/shallow checkouts.
fn stock_source(path: &str) -> &'static str {
    match path {
        "crates/engine/src/cluster/shaders/cluster_draw.wgsl" => {
            include_str!("test_fixtures/cluster_draw.pre_f1_3c975483.wgsl")
        }
        "crates/engine/src/cluster/shaders/terrain_material.wgsl" => {
            include_str!("test_fixtures/terrain_material.pre_f1_3c975483.wgsl")
        }
        _ => panic!("unrecognized pinned ABI fixture: {path}"),
    }
}
fn source_map(enabled: bool) -> Sources {
    let mut sources = lighting_tests::sources();
    if enabled {
        for (name, source) in [
            ("forward_io", include_str!("forward_io.wgsl")),
            ("arithmetic", include_str!("arithmetic.wgsl")),
            ("receiving", include_str!("receiving.wgsl")),
            ("pbr_functions", include_str!("pbr_functions.wgsl")),
            ("lighting", include_str!("lighting.wgsl")),
        ] {
            sources.insert(
                format!("far_terrain_shadows::{name}"),
                (format!("optional/{name}.wgsl"), source.into()),
            );
        }
    }
    sources
}
fn compose(
    source: &str,
    sources: &Sources,
    defs: HashMap<String, ShaderDefValue>,
) -> (naga::Module, naga::valid::ModuleInfo) {
    let mut composer = Composer::default().with_capabilities(Capabilities::all());
    for import in get_preprocessor_data(source).1 {
        // Match Bevy's ShaderCache: resolve every declared import, including
        // imports inside disabled preprocessor branches, before composition.
        add(&import.import, sources, &mut composer, &defs);
    }
    let module = composer
        .make_naga_module(NagaModuleDescriptor {
            source,
            file_path: "test-only/actual_cluster_forward_abi.wgsl",
            shader_defs: defs,
            ..Default::default()
        })
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&composer)));
    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .unwrap_or_else(|error| panic!("{error:?}"));
    (module, info)
}
#[derive(Debug, PartialEq)]
enum Shape {
    Scalar(naga::Scalar),
    Vector(naga::VectorSize, naga::Scalar),
}
#[derive(Debug, PartialEq)]
struct Field {
    name: String,
    binding: naga::Binding,
    shape: Shape,
}
fn structure(module: &naga::Module, ty: naga::Handle<naga::Type>) -> Vec<Field> {
    let naga::TypeInner::Struct { members, .. } = &module.types[ty].inner else {
        panic!("expected actual IO struct")
    };
    members
        .iter()
        .map(|member| Field {
            name: member.name.clone().expect("named IO field"),
            binding: member.binding.clone().expect("IO field binding"),
            shape: match module.types[member.ty].inner {
                naga::TypeInner::Scalar(scalar) => Shape::Scalar(scalar),
                naga::TypeInner::Vector { size, scalar } => Shape::Vector(size, scalar),
                ref other => panic!("unsupported IO shape: {other:?}"),
            },
        })
        .collect()
}
fn io(module: &naga::Module, stage: naga::ShaderStage) -> Vec<Field> {
    let entry = module
        .entry_points
        .iter()
        .find(|entry| entry.stage == stage)
        .unwrap();
    let ty = if stage == naga::ShaderStage::Vertex {
        entry.function.result.as_ref().unwrap().ty
    } else {
        entry.function.arguments[0].ty
    };
    structure(module, ty)
}
fn live_bindings(module: &naga::Module, info: &naga::valid::ModuleInfo) -> Vec<(u32, u32)> {
    let index = module
        .entry_points
        .iter()
        .position(|entry| entry.stage == naga::ShaderStage::Fragment)
        .unwrap();
    let usage = info.get_entry_point(index);
    module
        .global_variables
        .iter()
        .filter_map(|(handle, global)| {
            let binding = global.binding.as_ref()?;
            (!usage[handle].is_empty()).then_some((binding.group, binding.binding))
        })
        .collect()
}
fn definitions(bindless: bool, enabled: bool, fields: &[&str]) -> HashMap<String, ShaderDefValue> {
    let mut defs = lighting_tests::defs(bindless, 4, &[]);
    for name in [
        "VERTEX_UVS",
        "VERTEX_UVS_A",
        "VERTEX_UVS_B",
        "VERTEX_TANGENTS",
        "VERTEX_COLORS",
        "VISIBILITY_RANGE_DITHER",
    ] {
        defs.remove(name);
    }
    for name in fields {
        defs.insert((*name).into(), ShaderDefValue::Bool(true));
    }
    if fields
        .iter()
        .any(|field| matches!(*field, "VERTEX_UVS_A" | "VERTEX_UVS_B"))
    {
        defs.insert("VERTEX_UVS".into(), ShaderDefValue::Bool(true));
    }
    // Current actual terrain fragment requires instance_index; no unsupported
    // no-instance fragment variant is invented to make the test pass.
    assert!(defs.contains_key("VERTEX_OUTPUT_INSTANCE_INDEX"));
    if enabled {
        defs.insert(FLAG.into(), ShaderDefValue::Bool(true));
    }
    defs
}
fn check_extra_fields(fields: &[Field]) {
    let f32_scalar = naga::Scalar {
        kind: naga::ScalarKind::Float,
        width: 4,
    };
    let i32_scalar = naga::Scalar {
        kind: naga::ScalarKind::Sint,
        width: 4,
    };
    let u32_scalar = naga::Scalar {
        kind: naga::ScalarKind::Uint,
        width: 4,
    };
    let world_position_binding = &fields
        .iter()
        .find(|field| field.name == "world_position")
        .unwrap()
        .binding;
    for (location, name, shape) in [
        (
            8,
            "instance_local_renderer",
            Shape::Vector(naga::VectorSize::Tri, f32_scalar),
        ),
        (
            9,
            "canonical_cell_anchor",
            Shape::Vector(naga::VectorSize::Bi, i32_scalar),
        ),
        (10, "canonical_world", Shape::Scalar(u32_scalar)),
        (11, "canonical_metadata_valid", Shape::Scalar(u32_scalar)),
    ] {
        let field = fields.iter().find(|field| field.name == name).unwrap();
        assert_eq!(field.shape, shape);
        let naga::Binding::Location {
            location: actual,
            interpolation,
            sampling,
            ..
        } = &field.binding
        else {
            panic!("location field expected")
        };
        assert_eq!(*actual, location);
        if location == 8 {
            let naga::Binding::Location {
                interpolation: stock_interpolation,
                sampling: stock_sampling,
                ..
            } = world_position_binding
            else {
                panic!("stock world position location expected")
            };
            assert_eq!(interpolation, stock_interpolation);
            assert_eq!(sampling, stock_sampling);
        } else {
            assert_eq!(*interpolation, Some(naga::Interpolation::Flat));
        }
    }
}

#[test]
fn actual_optional_forward_stages_link_stock_fields_and_live_f1_fields() {
    let stock_vertex = stock_source("crates/engine/src/cluster/shaders/cluster_draw.wgsl");
    let stock_fragment = stock_source("crates/engine/src/cluster/shaders/terrain_material.wgsl");
    for bindless in [false, true] {
        for fields in [
            vec![],
            vec!["VERTEX_UVS_A"],
            vec![
                "VERTEX_UVS_A",
                "VERTEX_UVS_B",
                "VERTEX_TANGENTS",
                "VERTEX_COLORS",
                "VISIBILITY_RANGE_DITHER",
            ],
        ] {
            let sources = source_map(true);
            let defs = definitions(bindless, true, &fields);
            let (vertex, _) = compose(VERTEX, &sources, defs.clone());
            let (fragment, info) = compose(FRAGMENT, &sources, defs);
            let outputs = io(&vertex, naga::ShaderStage::Vertex);
            let inputs = io(&fragment, naga::ShaderStage::Fragment);
            assert_eq!(
                outputs, inputs,
                "actual linked IO bindless={bindless} fields={fields:?}"
            );
            check_extra_fields(&outputs);
            let live = live_bindings(&fragment, &info);
            for binding in 7..=10 {
                assert!(
                    live.contains(&(4, binding)),
                    "receiver binding {binding} must remain live"
                );
            }
            let stock_sources = source_map(false);
            let stock_defs = definitions(bindless, false, &fields);
            let (baseline_vertex, _) = compose(stock_vertex, &stock_sources, stock_defs.clone());
            let (baseline_fragment, _) = compose(stock_fragment, &stock_sources, stock_defs);
            let stock_outputs = outputs
                .into_iter()
                .filter(|field| {
                    !matches!(
                        field.binding,
                        naga::Binding::Location {
                            location: 8..=11,
                            ..
                        }
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(
                stock_outputs,
                io(&baseline_vertex, naga::ShaderStage::Vertex)
            );
            assert_eq!(
                stock_outputs,
                io(&baseline_fragment, naga::ShaderStage::Fragment)
            );
        }
    }
}

#[test]
fn actual_all_off_forward_abi_matches_stock_without_optional_registration() {
    let stock_vertex = stock_source("crates/engine/src/cluster/shaders/cluster_draw.wgsl");
    let stock_fragment = stock_source("crates/engine/src/cluster/shaders/terrain_material.wgsl");
    for bindless in [false, true] {
        let sources = source_map(false);
        assert!(
            sources
                .keys()
                .all(|name| !name.starts_with("far_terrain_shadows::"))
        );
        let defs = definitions(
            bindless,
            false,
            &[
                "VERTEX_UVS_A",
                "VERTEX_UVS_B",
                "VERTEX_TANGENTS",
                "VERTEX_COLORS",
                "VISIBILITY_RANGE_DITHER",
            ],
        );
        for (source, baseline, stage) in [
            (ORDINARY_VERTEX, stock_vertex, naga::ShaderStage::Vertex),
            (
                ORDINARY_FRAGMENT,
                stock_fragment,
                naga::ShaderStage::Fragment,
            ),
        ] {
            let (module, info) = compose(source, &sources, defs.clone());
            let (stock, _) = compose(baseline, &sources, defs.clone());
            assert_eq!(io(&module, stage), io(&stock, stage));
            assert!(module.global_variables.iter().all(|(_, global)| {
                global.binding.as_ref().is_none_or(|binding| {
                    binding.group != 4 || binding.binding < 7 || binding.binding > 10
                })
            }));
            if stage == naga::ShaderStage::Fragment {
                assert!(
                    live_bindings(&module, &info)
                        .iter()
                        .all(|&(group, binding)| group != 4 || !(7..=10).contains(&binding))
                );
            }
        }
    }
}

#[test]
fn ordinary_entry_assets_have_no_optional_bevy_shader_dependencies() {
    use bevy::shader::{Shader, ShaderImport};
    for (source, baseline) in [
        (
            ORDINARY_VERTEX,
            stock_source("crates/engine/src/cluster/shaders/cluster_draw.wgsl"),
        ),
        (
            ORDINARY_FRAGMENT,
            stock_source("crates/engine/src/cluster/shaders/terrain_material.wgsl"),
        ),
    ] {
        // The real Bevy import inventory is not filtered by shader definitions.
        let shader = Shader::from_wgsl(source, "ordinary-entry.wgsl");
        assert!(shader.imports.iter().all(|import| match import {
            ShaderImport::Custom(path) | ShaderImport::AssetPath(path) =>
                !path.starts_with("far_terrain_shadows::"),
        }));
        assert_eq!(source.replace("\r\n", "\n"), baseline.replace("\r\n", "\n"));
    }
    // Demonstrate that the optional entry really has dependencies that require
    // registration; disabling the definition does not make those imports vanish.
    assert!(Shader::from_wgsl(VERTEX, "optional-entry.wgsl").imports.iter().any(|import|
        matches!(import, ShaderImport::Custom(path) if path.starts_with("far_terrain_shadows::"))));
}
