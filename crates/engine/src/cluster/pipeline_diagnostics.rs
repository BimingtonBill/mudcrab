//! Separate semantic specialization keys, actual pipelines and per-entity retries.
use super::*;

pub(super) fn report(
    pipelines: Res<ClusterPipelines>,
    cache: Res<PipelineCache>,
    mut last: Local<Option<std::time::Instant>>,
) {
    if !bevy::log::tracing::enabled!(target: "engine::cluster::pipeline_diagnostics", bevy::log::tracing::Level::INFO)
    {
        return;
    }
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
