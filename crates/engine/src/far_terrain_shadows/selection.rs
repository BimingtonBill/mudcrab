//! Maps an explicitly designated owner into Bevy's actual extracted directional array.
//! No portal/world/source policy or GPU-publication authority belongs to this helper.
use bevy::{
    camera::visibility::RenderLayers,
    math::Vec3,
    pbr::{ExtractedDirectionalLight, MAX_DIRECTIONAL_LIGHTS},
    prelude::Entity,
    render::sync_world::MainEntity,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Missing {
    AbsentOwner,
    AmbiguousOwner,
    FilteredOwner,
    TruncatedOwner,
    InvalidDirection,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Selected {
    pub(crate) render: Entity,
    pub(crate) main: MainEntity,
    pub(crate) index: u32,
    pub(crate) toward_renderer: Vec3,
    pub(crate) shadow_maps_enabled: bool,
}

pub(crate) fn select<'a>(
    owner: MainEntity,
    view_layers: &RenderLayers,
    lights: impl IntoIterator<Item = (Entity, MainEntity, &'a ExtractedDirectionalLight)>,
) -> Result<Selected, Missing> {
    let mut facts: Vec<_> = lights.into_iter().collect();
    let mut designated = facts.iter().filter(|(_, main, _)| *main == owner);
    let Some((_, _, light)) = designated.next() else {
        return Err(Missing::AbsentOwner);
    };
    if designated.next().is_some() {
        return Err(Missing::AmbiguousOwner);
    }
    if !light.render_layers.intersects(view_layers) {
        return Err(Missing::FilteredOwner);
    }
    // Exact pinned Bevy0.19 prepare_lights ordering (light.rs1202), including
    // unrelated/replica lights. Filtering happens before view-local enumeration.
    facts.sort_by_cached_key(|(render, _, light)| {
        (light.volumetric, light.shadow_maps_enabled, *render)
    });
    let Some((index, (render, main, light))) = facts
        .into_iter()
        .filter(|(_, _, light)| light.render_layers.intersects(view_layers))
        .enumerate()
        .take(MAX_DIRECTIONAL_LIGHTS)
        .find(|(_, (_, main, _))| *main == owner)
    else {
        return Err(Missing::TruncatedOwner);
    };
    // GlobalTransform::back normalizes this exact axis and uses Dir3::new_unchecked.
    // Reject degenerate/nonfinite normalization BEFORE that API's debug assertion.
    let axis: Vec3 = light.transform.affine().matrix3.z_axis.into();
    let length_squared = axis.length_squared();
    if !axis.is_finite() || !length_squared.is_finite() || length_squared <= 0.0 {
        return Err(Missing::InvalidDirection);
    }
    let toward_renderer: Vec3 = light.transform.back().into();
    if !toward_renderer.is_finite() || toward_renderer.length_squared() == 0.0 {
        return Err(Missing::InvalidDirection);
    }
    Ok(Selected {
        render,
        main,
        index: u32::try_from(index).expect("public Bevy directional limit fits u32"),
        toward_renderer,
        shadow_maps_enabled: light.shadow_maps_enabled,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        color::LinearRgba,
        ecs::entity::EntityHashMap,
        light::CascadeShadowConfig,
        math::{Mat4, Quat},
        prelude::{GlobalTransform, Transform, World},
    };

    fn light(layers: RenderLayers, volumetric: bool, shadows: bool) -> ExtractedDirectionalLight {
        ExtractedDirectionalLight {
            color: LinearRgba::WHITE,
            illuminance: 0.0,
            transform: GlobalTransform::IDENTITY,
            shadow_maps_enabled: shadows,
            contact_shadows_enabled: false,
            volumetric,
            affects_lightmapped_mesh_diffuse: false,
            shadow_depth_bias: 0.0,
            shadow_normal_bias: 0.0,
            cascade_shadow_config: CascadeShadowConfig::default(),
            cascades: EntityHashMap::default(),
            frusta: EntityHashMap::default(),
            render_layers: layers,
            soft_shadow_size: None,
            occlusion_culling: false,
            sun_disk_angular_size: 0.0,
            sun_disk_intensity: 0.0,
        }
    }
    fn identities(world: &mut World, count: usize) -> Vec<(Entity, MainEntity)> {
        let mut ids: Vec<_> = (0..count)
            .map(|_| (world.spawn_empty().id(), world.spawn_empty().id().into()))
            .collect();
        ids.sort_by_key(|(render, _)| *render);
        ids
    }

    #[test]
    fn every_sort_key_and_entity_tie_position_matches_ascending_bevy_order() {
        let mut world = World::new();
        let ids = identities(&mut world, 6);
        let keys = [
            (false, false),
            (false, false),
            (false, true),
            (true, false),
            (true, true),
            (true, true),
        ];
        let lights: Vec<_> = keys
            .into_iter()
            .map(|(volume, shadow)| light(RenderLayers::layer(2), volume, shadow))
            .collect();
        let facts: Vec<_> = ids
            .iter()
            .zip(&lights)
            .map(|(&(render, main), light)| (render, main, light))
            .rev()
            .collect();
        for (expected, &(render, main)) in ids.iter().enumerate() {
            let result = select(main, &RenderLayers::layer(2), facts.iter().copied());
            if expected < MAX_DIRECTIONAL_LIGHTS {
                let selected = result.unwrap();
                assert_eq!(selected.index, expected as u32);
                assert_eq!((selected.render, selected.main), (render, main));
                assert_eq!(
                    selected.shadow_maps_enabled,
                    lights[expected].shadow_maps_enabled
                );
            } else {
                assert_eq!(result, Err(Missing::TruncatedOwner));
            }
        }
    }

    #[test]
    fn layer_filter_precedes_index_and_capacity_with_replicas_still_in_input() {
        let mut world = World::new();
        let ids = identities(&mut world, MAX_DIRECTIONAL_LIGHTS + 3);
        let replica = light(RenderLayers::layer(1), false, false);
        let designated = light(RenderLayers::layer(2), false, true);
        let &(owner_render, owner_main) = ids.last().unwrap();
        let mut facts: Vec<_> = ids[..ids.len() - 1]
            .iter()
            .map(|&(render, main)| (render, main, &replica))
            .collect();
        facts.push((owner_render, owner_main, &designated));
        assert_eq!(
            select(owner_main, &RenderLayers::layer(2), facts.iter().copied())
                .unwrap()
                .index,
            0
        );
        assert_eq!(
            select(owner_main, &RenderLayers::layer(1), facts.iter().copied()),
            Err(Missing::FilteredOwner)
        );
    }

    #[test]
    fn missing_ambiguous_and_truncated_owners_are_distinct() {
        let mut world = World::new();
        let ids = identities(&mut world, MAX_DIRECTIONAL_LIGHTS + 1);
        let ordinary = light(RenderLayers::default(), false, true);
        let facts: Vec<_> = ids
            .iter()
            .map(|&(render, main)| (render, main, &ordinary))
            .collect();
        let absent: MainEntity = world.spawn_empty().id().into();
        assert_eq!(
            select(absent, &RenderLayers::default(), facts.iter().copied()),
            Err(Missing::AbsentOwner)
        );
        let owner = ids.last().unwrap().1;
        assert_eq!(
            select(owner, &RenderLayers::default(), facts.iter().copied()),
            Err(Missing::TruncatedOwner)
        );
        let hidden = light(RenderLayers::layer(2), false, true);
        assert_eq!(
            select(
                owner,
                &RenderLayers::default(),
                [(ids[0].0, owner, &ordinary), (ids[1].0, owner, &hidden)]
            ),
            Err(Missing::AmbiguousOwner)
        );
    }

    #[test]
    fn disabled_shadows_and_zero_illuminance_are_properties_not_absence_and_translation_does_not_rotate()
     {
        let mut world = World::new();
        let ids = identities(&mut world, 1);
        let (render, main) = ids[0];
        let mut source = light(RenderLayers::default(), false, false);
        let rotation = Quat::from_rotation_y(0.5);
        source.transform = GlobalTransform::from(Transform::from_rotation(rotation));
        let initial = select(main, &RenderLayers::default(), [(render, main, &source)]).unwrap();
        assert!(!initial.shadow_maps_enabled);
        source.transform = GlobalTransform::from(
            Transform::from_rotation(rotation).with_translation(Vec3::new(1000.0, -400.0, 12.0)),
        );
        let translated = select(main, &RenderLayers::default(), [(render, main, &source)]).unwrap();
        assert_eq!(translated.toward_renderer, initial.toward_renderer);
    }

    #[test]
    fn degenerate_and_nonfinite_axes_refuse_before_back_normalization() {
        let mut world = World::new();
        let (render, main) = identities(&mut world, 1)[0];
        for z in [
            Vec3::ZERO,
            Vec3::splat(f32::NAN),
            Vec3::splat(f32::INFINITY),
            Vec3::splat(f32::MAX),
        ] {
            let mut source = light(RenderLayers::default(), false, true);
            let mut matrix = Mat4::IDENTITY;
            matrix.z_axis = z.extend(0.0);
            source.transform = GlobalTransform::from(matrix);
            assert_eq!(
                select(main, &RenderLayers::default(), [(render, main, &source)]),
                Err(Missing::InvalidDirection)
            );
        }
    }
}
