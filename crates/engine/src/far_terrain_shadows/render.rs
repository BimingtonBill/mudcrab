//! Enabled-only renderer ownership. Publications are never drawable proof by themselves.
use super::{
    CellCoord,
    reader::CertifiedGpuUpload,
    runtime::Published,
    runtime_owner::{SourceOwnerArc, WorldState},
    selection::{self, Selected},
    view::{ReceiverRelationship, ViewContext},
};
use crate::cluster::render::ViewDomains;
use bevy::{
    camera::visibility::RenderLayers,
    pbr::{ExtractedDirectionalLight, ViewLightsUniformOffset},
    prelude::*,
    render::{
        Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
        render_resource::{Buffer, BufferInitDescriptor, BufferUsages},
        renderer::RenderDevice,
        sync_world::MainEntity,
        view::{ExtractedView, RetainedViewEntity},
    },
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

pub(crate) struct WorldGpu {
    pub(crate) cpu: Arc<CertifiedGpuUpload>,
    pub(crate) header: Buffer,
    pub(crate) words: Buffer,
}
pub(crate) struct Prepared {
    pub(crate) source: SourceOwnerArc,
    pub(crate) world: u32,
    pub(crate) origin: IVec2,
    pub(crate) selected: Selected,
    pub(crate) nonce: u64,
    pub(crate) gpu: Arc<WorldGpu>,
    pub(crate) context: Buffer,
    pub(crate) selection: Buffer,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Readiness {
    NotRequired,
    Waiting(&'static str),
    Unavailable(String),
    Ready,
}
#[derive(Resource, Default)]
pub(crate) struct State {
    pub(crate) active: HashMap<RetainedViewEntity, Arc<Prepared>>,
    cached: HashMap<RetainedViewEntity, Arc<Prepared>>,
    worlds: HashMap<u32, Arc<WorldGpu>>,
    source: Option<SourceOwnerArc>,
    main_sun: Option<MainEntity>,
    portal_sun: Option<MainEntity>,
    portal_views: HashSet<MainEntity>,
    next: u64,
    readiness: HashMap<RetainedViewEntity, Readiness>,
    reported: HashMap<RetainedViewEntity, Readiness>,
}
#[derive(Resource)]
struct ShaderLibraries(#[allow(dead_code)] Vec<Handle<bevy::shader::Shader>>);

pub(super) fn install(app: &mut App) {
    bevy::asset::embedded_asset!(app, "arithmetic.wgsl");
    bevy::asset::embedded_asset!(app, "receiving.wgsl");
    bevy::asset::embedded_asset!(app, "forward_io.wgsl");
    bevy::asset::embedded_asset!(app, "pbr_functions.wgsl");
    bevy::asset::embedded_asset!(app, "lighting.wgsl");
    // Separate entry assets keep optional imports out of the ordinary shaders.
    // Bevy resolves imports before it applies conditional shader definitions.
    bevy::asset::embedded_asset!(app, "cluster_draw.wgsl");
    bevy::asset::embedded_asset!(app, "terrain_material.wgsl");
    let server = app.world().resource::<AssetServer>();
    let handles = [
        "arithmetic",
        "receiving",
        "forward_io",
        "pbr_functions",
        "lighting",
    ]
    .map(|name| server.load(format!("embedded://engine/far_terrain_shadows/{name}.wgsl")))
    .to_vec();
    app.insert_resource(ShaderLibraries(handles));
    app.add_systems(Update, log_shader_inventory);
    app.sub_app_mut(RenderApp)
        .init_resource::<State>()
        .add_systems(ExtractSchedule, extract_roles)
        .add_systems(Render, prepare.in_set(RenderSystems::PrepareViews));
}

/// Read-only startup/hot-reload diagnostics; no graph scan unless DEBUG is
/// requested and the shader assets actually changed.
#[allow(clippy::type_complexity)]
fn log_shader_inventory(
    shaders: Res<Assets<bevy::shader::Shader>>,
    mut previous: Local<Option<(Vec<String>, Vec<String>)>>,
) {
    if !bevy::log::tracing::enabled!(bevy::log::tracing::Level::DEBUG) || !shaders.is_changed() {
        return;
    }
    let known: HashSet<_> = shaders
        .iter()
        .map(|(_, shader)| shader.import_path.clone())
        .collect();
    let mut loaded = Vec::new();
    let mut missing = Vec::new();
    for (id, shader) in shaders
        .iter()
        .filter(|(_, shader)| shader.path.contains("far_terrain_shadows"))
    {
        loaded.push(format!("{id:?}: {}", shader.path));
        for import in &shader.imports {
            if !known.contains(import) {
                missing.push(format!("{} -> {import:?}", shader.path));
            }
        }
    }
    loaded.sort();
    missing.sort();
    missing.dedup();
    let current = (loaded, missing);
    if previous.as_ref() != Some(&current) {
        debug!(loaded = ?current.0, missing_imports = ?current.1,
            "optional terrain shader asset inventory");
        *previous = Some(current);
    }
}
#[allow(clippy::type_complexity)]
fn extract_roles(
    mut state: ResMut<State>,
    main: Extract<
        Query<
            Entity,
            (
                With<crate::graphics_settings::MainSun>,
                Without<crate::shadow_route::UnshadowedSun>,
            ),
        >,
    >,
    portal: Extract<Query<Entity, With<crate::portal::PortalDestinationSun>>>,
    views: Extract<Query<Entity, With<crate::portal::PortalCamera>>>,
) {
    // Reset current authorization every frame. A skipped prepare cannot inherit it.
    state.active.clear();
    state.readiness.clear();
    state.main_sun = main.single().ok().map(MainEntity::from);
    state.portal_sun = portal.single().ok().map(MainEntity::from);
    state.portal_views.clear();
    state
        .portal_views
        .extend(views.iter().map(MainEntity::from));
}
type CameraViews<'w, 's> = Query<
    'w,
    's,
    (
        &'static ExtractedView,
        Option<&'static RenderLayers>,
        Option<&'static ViewLightsUniformOffset>,
    ),
    With<bevy::render::camera::ExtractedCamera>,
>;

fn prepare(
    mut state: ResMut<State>,
    published: Res<Published>,
    domains: ViewDomains,
    device: Res<RenderDevice>,
    views: CameraViews,
    lights: Query<(Entity, &MainEntity, &ExtractedDirectionalLight)>,
) {
    let publication = published.0.as_ref();
    if publication.is_none_or(|publication| {
        state
            .source
            .as_ref()
            .is_none_or(|source| !source.same_allocation(&publication.source))
    }) {
        state.cached.clear();
        state.worlds.clear();
        state.source = publication.map(|publication| publication.source.clone());
    }
    let mut retained = HashSet::new();
    for (view, layers, light_offset) in &views {
        let key = view.retained_view_entity;
        macro_rules! waiting {
            ($reason:expr) => {{
                state.readiness.insert(key, Readiness::Waiting($reason));
                continue;
            }};
        }
        let world = match domains.world(key) {
            Some(None) => {
                state.readiness.insert(key, Readiness::NotRequired);
                continue;
            }
            Some(Some(world)) => world,
            None => waiting!("view world ownership"),
        };
        if light_offset.is_none() {
            waiting!("prepared view lights");
        }
        let portal = state.portal_views.contains(&key.main_entity)
            || state.portal_views.contains(&key.auxiliary_entity);
        let Some(owner) = (if portal {
            state.portal_sun
        } else {
            state.main_sun
        }) else {
            waiting!("unique designated sun");
        };
        let Ok(selected) = selection::select(
            owner,
            layers.unwrap_or(&RenderLayers::default()),
            lights.iter().map(|(e, m, l)| (e, *m, l)),
        ) else {
            waiting!("selected sun in prepared light array");
        };
        if !selected.shadow_maps_enabled {
            state.readiness.insert(key, Readiness::NotRequired);
            continue;
        }
        let Some(publication) = publication else {
            waiting!("certified source preparation");
        };
        let cpu = match publication.worlds.get(&world) {
            Some(WorldState::Ready(cpu)) => cpu,
            Some(WorldState::Unavailable(reason)) => {
                state
                    .readiness
                    .insert(key, Readiness::Unavailable(reason.clone()));
                continue;
            }
            _ => waiting!("canonical terrain world preparation"),
        };
        let gpu = if let Some(gpu) = state
            .worlds
            .get(&world)
            .filter(|gpu| Arc::ptr_eq(&gpu.cpu, cpu))
        {
            gpu.clone()
        } else {
            let header = device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("canonical shadow header"),
                contents: bytemuck::cast_slice(&cpu.upload().header),
                usage: BufferUsages::UNIFORM,
            });
            let empty = [0u8; 4];
            let words = device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("canonical shadow terrain"),
                contents: if cpu.storage_bytes().is_empty() {
                    &empty
                } else {
                    cpu.storage_bytes()
                },
                usage: BufferUsages::STORAGE,
            });
            let gpu = Arc::new(WorldGpu {
                cpu: cpu.clone(),
                header,
                words,
            });
            info!(
                world,
                bytes = cpu.storage_bytes().len(),
                "uploaded certified canonical terrain shadows"
            );
            state.worlds.insert(world, gpu.clone());
            gpu
        };
        state.readiness.insert(key, Readiness::Ready);
        let origin = domains.origin();
        retained.insert(key);
        if let Some(old) = state
            .cached
            .get(&key)
            .filter(|old| {
                old.selected == selected
                    && old.origin == origin
                    && old.world == world
                    && Arc::ptr_eq(&old.gpu, &gpu)
            })
            .cloned()
        {
            state.active.insert(key, old);
            continue;
        }
        state.next = state
            .next
            .checked_add(1)
            .expect("canonical shadow view revision exhausted");
        let nonce = state.next;
        let sun = selected.render.to_bits();
        let direction = selected.toward_renderer;
        let context = ViewContext {
            snapshot: gpu.cpu.upload().identity,
            view_id: nonce,
            view_generation: nonce,
            sun_id: sun,
            sun_revision: nonce,
            toward_sun: [
                f64::from(direction.x),
                -f64::from(direction.z),
                f64::from(direction.y),
            ],
            view_origin: CellCoord {
                x: origin.x,
                y: origin.y,
            },
            instance_origin: CellCoord { x: 0, y: 0 },
            origin_revision: nonce,
            cell_size: f64::from(crate::world::components::CELL_SIZE),
        };
        let Ok(mut packed) = context.pack_view(ReceiverRelationship::Unresolved) else {
            state.readiness.insert(
                key,
                Readiness::Unavailable("view coordinate representation".into()),
            );
            continue;
        };
        // Model2: canonical XY field, not a claim of exact simplified-mesh contact.
        packed.words[3][3] = 2;
        let context = device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("canonical shadow view"),
            contents: bytemuck::cast_slice(&packed.words),
            usage: BufferUsages::UNIFORM,
        });
        let h = gpu.cpu.upload().header;
        // Full finite inventory traversal upper bounds, not a distance/quality cutoff.
        let tree_nodes =
            u32::try_from(gpu.cpu.upload().footprint.world_tree / 36).unwrap_or(u32::MAX);
        let cells = h[2];
        let words = [
            selected.index,
            1,
            nonce as u32,
            (nonce >> 32) as u32,
            sun as u32,
            (sun >> 32) as u32,
            cells,
            h[3].saturating_mul(1365).saturating_add(tree_nodes),
            direction.x.to_bits(),
            direction.y.to_bits(),
            direction.z.to_bits(),
            0,
        ];
        let selection = device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("canonical shadow selected sun"),
            contents: bytemuck::cast_slice(&words),
            usage: BufferUsages::UNIFORM,
        });
        let prepared = Arc::new(Prepared {
            source: publication.source.clone(),
            world,
            origin,
            selected,
            nonce,
            gpu,
            context,
            selection,
        });
        state.cached.insert(key, prepared.clone());
        state.active.insert(key, prepared);
    }
    state.cached.retain(|key, _| retained.contains(key));
    // Prepared views and queued bindings retain their own Arcs. Dropping only
    // this lookup's unused entries cannot invalidate an in-flight bind group.
    let retained_worlds: HashSet<_> = state.cached.values().map(|view| view.world).collect();
    state
        .worlds
        .retain(|world, _| retained_worlds.contains(world));
    let statuses: Vec<_> = state
        .readiness
        .iter()
        .filter(|(key, status)| state.reported.get(key) != Some(*status))
        .map(|(key, status)| (*key, status.clone()))
        .collect();
    for (key, status) in statuses {
        info!(
            ?key,
            ?status,
            "canonical terrain shadow view state; ordinary terrain remains drawn"
        );
        state.reported.insert(key, status);
    }
    let alive: HashSet<_> = state.readiness.keys().copied().collect();
    state.reported.retain(|key, _| alive.contains(key));
}
