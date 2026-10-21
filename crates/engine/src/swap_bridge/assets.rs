//! What a destination root is made of in the main world, for the readiness checks: every mesh,
//! material and texture under it, loaded or still waiting (fact 2 of
//! [`super::destination`]). Moved here from `door_crossing` so the crossing and the destination
//! reports read one rule.

use crate::render::{TerrainMaterial, WaterMaterial};
use bevy::{
    asset::{LoadState, UntypedAssetId},
    ecs::system::SystemParam,
    prelude::*,
};
use std::collections::HashSet;

/// What a landing cell is made of, for the readiness check.
#[derive(SystemParam)]
#[allow(clippy::type_complexity)]
pub(crate) struct LandingAssets<'w, 's> {
    server: Option<Res<'w, AssetServer>>,
    children: Query<'w, 's, &'static Children>,
    parts: Query<
        'w,
        's,
        (
            Option<&'static Mesh3d>,
            Option<&'static MeshMaterial3d<StandardMaterial>>,
            Option<&'static MeshMaterial3d<TerrainMaterial>>,
            Option<&'static MeshMaterial3d<WaterMaterial>>,
        ),
    >,
    meshes: Option<Res<'w, Assets<Mesh>>>,
    images: Option<Res<'w, Assets<Image>>>,
    standard: Option<Res<'w, Assets<StandardMaterial>>>,
    terrain: Option<Res<'w, Assets<TerrainMaterial>>>,
    water: Option<Res<'w, Assets<WaterMaterial>>>,
}

/// What the readiness check found under a landing root.
#[derive(Debug, Default)]
pub(crate) struct LandingReport {
    /// Assets still loading.
    pub(crate) waiting: usize,
    /// The first of them, by asset path when the server knows one.
    pub(crate) first_waiting: Option<String>,
    /// Assets whose load failed. They count as done, so a missing texture cannot hold the screen.
    pub(crate) failed: HashSet<UntypedAssetId>,
}

impl LandingAssets<'_, '_> {
    /// Notes an asset that is still loading, or failed, in `report`. A finished asset is loaded,
    /// or (no load tracked, as for an asset added directly) present in its store.
    fn check(
        &self,
        id: UntypedAssetId,
        present: impl FnOnce() -> bool,
        report: &mut LandingReport,
    ) {
        let Some(server) = &self.server else {
            return;
        };
        let waiting = match server.get_load_state(id) {
            Some(LoadState::Loaded) => false,
            Some(LoadState::Failed(_)) => {
                report.failed.insert(id);
                false
            }
            Some(_) => true,
            None => !present(),
        };
        if waiting {
            report.waiting += 1;
            if report.first_waiting.is_none() {
                report.first_waiting = Some(
                    server
                        .get_path(id)
                        .map_or_else(|| format!("{id:?}"), |path| path.to_string()),
                );
            }
        }
    }

    fn check_image(&self, id: UntypedAssetId, report: &mut LandingReport) {
        self.check(
            id,
            || match (&self.images, id.try_typed::<Image>()) {
                (Some(images), Ok(typed)) => images.contains(typed),
                _ => true,
            },
            report,
        );
    }

    /// Checks every dependency (the textures) of a material.
    fn check_dependencies<A: Asset>(&self, material: &A, report: &mut LandingReport) {
        material.visit_dependencies(&mut |id| self.check_image(id, report));
    }

    /// Checks a material handle and its textures.
    fn check_material<A: Asset>(
        &self,
        handle: &Handle<A>,
        store: &Option<Res<Assets<A>>>,
        textures_of: impl FnOnce(&Self, &A, &mut LandingReport),
        report: &mut LandingReport,
    ) {
        let present = || {
            store
                .as_ref()
                .is_none_or(|assets| assets.contains(handle.id()))
        };
        let before = report.waiting;
        self.check(handle.id().untyped(), present, report);
        if report.waiting != before {
            return;
        }
        if let Some(material) = store.as_ref().and_then(|assets| assets.get(handle.id())) {
            textures_of(self, material, report);
        }
    }

    /// Inspects every mesh, material and texture under `root`. Covered: `Mesh3d`,
    /// `MeshMaterial3d<StandardMaterial>` with all its texture dependencies, and the terrain and
    /// water materials with their base-material textures. The terrain and water extensions' own
    /// layer and reflection images are private to `render.rs` and are not checked.
    pub(crate) fn inspect_under(&self, root: Entity) -> LandingReport {
        let mut report = LandingReport::default();
        for entity in std::iter::once(root).chain(self.children.iter_descendants(root)) {
            let Ok((mesh, standard, terrain, water)) = self.parts.get(entity) else {
                continue;
            };
            if let Some(Mesh3d(handle)) = mesh {
                self.check(
                    handle.id().untyped(),
                    || {
                        self.meshes
                            .as_ref()
                            .is_none_or(|meshes| meshes.contains(handle.id()))
                    },
                    &mut report,
                );
            }
            if let Some(MeshMaterial3d(handle)) = standard {
                self.check_material(
                    handle,
                    &self.standard,
                    |this, m, report| this.check_dependencies(m, report),
                    &mut report,
                );
            }
            if let Some(MeshMaterial3d(handle)) = terrain {
                self.check_material(
                    handle,
                    &self.terrain,
                    |this, m, report| this.check_dependencies(&m.base, report),
                    &mut report,
                );
            }
            if let Some(MeshMaterial3d(handle)) = water {
                self.check_material(
                    handle,
                    &self.water,
                    |this, m, report| this.check_dependencies(&m.base, report),
                    &mut report,
                );
            }
        }
        report
    }
}
