//! Separate semantic specialization keys, actual pipelines and per-entity retries.
//! Installed only with `--diagnostic-pipelines`, so a run without it spends nothing here.
use super::*;

pub(super) fn install(render: &mut bevy::app::SubApp, enabled: bool) {
    if enabled {
        render.add_systems(Render, report.in_set(RenderSystems::Cleanup));
    }
}

fn report(
    pipelines: Res<ClusterPipelines>,
    cache: Res<PipelineCache>,
    mut last: Local<Option<std::time::Instant>>,
) {
    if last.is_some_and(|last| last.elapsed() < std::time::Duration::from_secs(1)) {
        return;
    }
    *last = Some(std::time::Instant::now());
    for (pass, keys) in [
        (
            "forward",
            pipelines
                .render
                .iter()
                .map(|((key, _), id)| (key, *id))
                .collect::<Vec<_>>(),
        ),
        (
            "prepass",
            pipelines
                .prepass
                .iter()
                .filter_map(|(key, value)| value.map(|value| (key, value.pipeline)))
                .collect(),
        ),
        (
            "masked_shadow",
            pipelines
                .shadow_mask
                .iter()
                .map(|(key, id)| (key, *id))
                .collect(),
        ),
    ] {
        let ids: HashSet<_> = keys.iter().map(|(_, id)| *id).collect();
        let mesh_keys: HashSet<_> = keys.iter().map(|(key, _)| key.material.mesh_key).collect();
        let material_keys: HashSet<_> = keys
            .iter()
            .map(|(key, _)| (key.material.type_id, key.material.material_key.clone()))
            .collect();
        let mut states = [0_usize; 4];
        for id in &ids {
            let index = match cache.get_render_pipeline_state(*id) {
                CachedPipelineState::Queued => 0,
                CachedPipelineState::Creating(..) => 1,
                CachedPipelineState::Ok(_) => 2,
                CachedPipelineState::Err(_) => 3,
            };
            states[index] += 1;
        }
        info!(target: "engine::cluster::pipeline_diagnostics",
            pass, semantic_keys = keys.len(), distinct_pipelines = ids.len(),
            mesh_keys = mesh_keys.len(), material_features = material_keys.len(),
            tangent_keys = keys.iter().filter(|(key, _)| key.tangents).count(),
            queued = states[0], creating = states[1], ready = states[2], failed = states[3],
            "cluster pipeline cardinality");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_systems(enabled: bool) -> usize {
        let mut app = App::new();
        install(app.main_mut(), enabled);
        app.world()
            .get_resource::<Schedules>()
            .and_then(|schedules| schedules.get(Render))
            .map_or(0, |schedule| schedule.systems_len())
    }

    #[test]
    fn the_report_exists_only_with_the_diagnostics_flag() {
        assert_eq!(render_systems(false), 0, "off must install nothing");
        assert_eq!(render_systems(true), 1);
    }
}
