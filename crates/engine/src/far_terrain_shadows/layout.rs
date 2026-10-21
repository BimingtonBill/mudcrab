//! F1-only complete specialized-layout BUDGET validation against CREATED limits.
//! Does not reflect shaders or validate features/actual bindings/resource allocation.
use bevy::render::render_resource::{
    BindingType, BufferBindingType, RenderPipelineDescriptor, ShaderStages, WgpuLimits,
};
use std::collections::BTreeSet;

// Optional ABI occupies locations0..11, including the explicit validity field.
const F1_INTER_STAGE_LOCATIONS: u32 = 12;

#[derive(Default)]
struct Counts {
    uniform: u64,
    storage: u64,
    sampled: u64,
    samplers: u64,
    storage_textures: u64,
    arrays: u64,
    array_samplers: u64,
    array_acceleration: u64,
    acceleration: u64,
}
fn add(value: &mut u64, count: u64) -> Result<(), String> {
    *value = value
        .checked_add(count)
        .ok_or_else(|| "layout resource count overflow".to_owned())?;
    Ok(())
}
fn budget(name: &str, required: u64, limit: u64) -> Result<(), String> {
    if required > limit {
        Err(format!(
            "{name}: required {required}, created-device limit {limit}"
        ))
    } else {
        Ok(())
    }
}
impl Counts {
    fn entry(
        &mut self,
        ty: BindingType,
        count: Option<std::num::NonZeroU32>,
    ) -> Result<(), String> {
        // Exact pinned wgpu-core29 BindingTypeMaxCountValidator::add_binding:
        // binding arrays use separate element budgets, not scalar-type budgets.
        if let Some(count) = count {
            let count = u64::from(count.get());
            add(&mut self.arrays, count)?;
            match ty {
                BindingType::Sampler(_) => add(&mut self.array_samplers, count)?,
                BindingType::AccelerationStructure { .. } => {
                    add(&mut self.array_acceleration, count)?
                }
                _ => {}
            }
        } else {
            match ty {
                BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    ..
                } => add(&mut self.uniform, 1)?,
                BindingType::Buffer {
                    ty: BufferBindingType::Storage { .. },
                    ..
                } => add(&mut self.storage, 1)?,
                BindingType::Texture { .. } => add(&mut self.sampled, 1)?,
                BindingType::Sampler(_) => add(&mut self.samplers, 1)?,
                BindingType::StorageTexture { .. } => add(&mut self.storage_textures, 1)?,
                BindingType::AccelerationStructure { .. } => add(&mut self.acceleration, 1)?,
                BindingType::ExternalTexture => {
                    add(&mut self.sampled, 4)?;
                    add(&mut self.samplers, 1)?;
                    add(&mut self.uniform, 1)?;
                }
            }
        }
        Ok(())
    }
    fn check(&self, stage: &str, limits: &WgpuLimits) -> Result<(), String> {
        for (name, required, limit) in [
            (
                "uniform buffers",
                self.uniform,
                limits.max_uniform_buffers_per_shader_stage,
            ),
            (
                "storage buffers",
                self.storage,
                limits.max_storage_buffers_per_shader_stage,
            ),
            (
                "sampled textures",
                self.sampled,
                limits.max_sampled_textures_per_shader_stage,
            ),
            (
                "samplers",
                self.samplers,
                limits.max_samplers_per_shader_stage,
            ),
            (
                "storage textures",
                self.storage_textures,
                limits.max_storage_textures_per_shader_stage,
            ),
            (
                "binding-array elements",
                self.arrays,
                limits.max_binding_array_elements_per_shader_stage,
            ),
            (
                "binding-array sampler elements",
                self.array_samplers,
                limits.max_binding_array_sampler_elements_per_shader_stage,
            ),
            (
                "binding-array acceleration elements",
                self.array_acceleration,
                limits.max_binding_array_acceleration_structure_elements_per_shader_stage,
            ),
            (
                "acceleration structures",
                self.acceleration,
                limits.max_acceleration_structures_per_shader_stage,
            ),
        ] {
            budget(&format!("{stage} {name}"), required, u64::from(limit))?;
        }
        Ok(())
    }
}

pub(crate) fn validate(
    descriptor: &RenderPipelineDescriptor,
    limits: &WgpuLimits,
) -> Result<(), String> {
    budget(
        "bind groups",
        u64::try_from(descriptor.layout.len()).map_err(|_| "bind-group count overflow")?,
        u64::from(limits.max_bind_groups),
    )?;
    budget(
        "F1 inter-stage locations",
        u64::from(F1_INTER_STAGE_LOCATIONS),
        u64::from(limits.max_inter_stage_shader_variables),
    )?;
    budget(
        "pipeline immediate bytes",
        u64::from(descriptor.immediate_size),
        u64::from(limits.max_immediate_size),
    )?;
    let mut stages = [Counts::default(), Counts::default(), Counts::default()];
    let mut dynamic_uniform = 0u64;
    let mut dynamic_storage = 0u64;
    for (group_index, group) in descriptor.layout.iter().enumerate() {
        let mut seen = BTreeSet::new();
        let mut has_array = false;
        let mut has_dynamic = false;
        let mut has_uniform = false;
        for entry in &group.entries {
            if entry.binding >= limits.max_bindings_per_bind_group {
                return Err(format!(
                    "group {group_index} binding {} reaches/exceeds created-device binding-index limit {}",
                    entry.binding, limits.max_bindings_per_bind_group
                ));
            }
            if !seen.insert(entry.binding) {
                return Err(format!(
                    "group {group_index} duplicates binding {}",
                    entry.binding
                ));
            }
            has_array |= entry.count.is_some();
            if let BindingType::Buffer {
                ty,
                has_dynamic_offset,
                min_binding_size,
            } = entry.ty
            {
                if let Some(size) = min_binding_size {
                    let limit = match ty {
                        BufferBindingType::Uniform => limits.max_uniform_buffer_binding_size,
                        BufferBindingType::Storage { .. } => limits.max_storage_buffer_binding_size,
                    };
                    budget(
                        &format!(
                            "group {group_index} binding {} minimum buffer bytes",
                            entry.binding
                        ),
                        size.get(),
                        limit.min(limits.max_buffer_size),
                    )?;
                }
                // Arrays are excluded from ordinary/dynamic counters by wgpu-core.
                if entry.count.is_none() {
                    has_uniform |=
                        matches!(ty, BufferBindingType::Uniform) && !entry.visibility.is_empty();
                    if has_dynamic_offset {
                        has_dynamic = true;
                        match ty {
                            BufferBindingType::Uniform => add(&mut dynamic_uniform, 1)?,
                            BufferBindingType::Storage { .. } => add(&mut dynamic_storage, 1)?,
                        }
                    }
                }
            }
            has_uniform |= entry.count.is_none()
                && !entry.visibility.is_empty()
                && matches!(entry.ty, BindingType::ExternalTexture);
            for (index, stage) in [
                ShaderStages::VERTEX,
                ShaderStages::FRAGMENT,
                ShaderStages::COMPUTE,
            ]
            .into_iter()
            .enumerate()
            {
                if entry.visibility.contains(stage) {
                    stages[index].entry(entry.ty, entry.count)?;
                }
            }
        }
        if has_array && has_dynamic {
            return Err(format!(
                "group {group_index} combines binding arrays with dynamic buffers"
            ));
        }
        if has_array && has_uniform {
            return Err(format!(
                "group {group_index} combines binding arrays with scalar uniform buffers"
            ));
        }
    }
    budget(
        "dynamic uniform buffers",
        dynamic_uniform,
        u64::from(limits.max_dynamic_uniform_buffers_per_pipeline_layout),
    )?;
    budget(
        "dynamic storage buffers",
        dynamic_storage,
        u64::from(limits.max_dynamic_storage_buffers_per_pipeline_layout),
    )?;
    for (stage, counts) in ["vertex", "fragment", "compute"].into_iter().zip(stages) {
        counts.check(stage, limits)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::render_resource::{BindGroupLayoutDescriptor, BindGroupLayoutEntry};
    use std::num::{NonZeroU32, NonZeroU64};
    fn limits() -> WgpuLimits {
        WgpuLimits {
            max_bind_groups: 5,
            max_bindings_per_bind_group: 64,
            max_binding_array_elements_per_shader_stage: 512,
            max_binding_array_sampler_elements_per_shader_stage: 256,
            ..Default::default()
        }
    }
    fn buffer(
        binding: u32,
        ty: BufferBindingType,
        visibility: ShaderStages,
        count: Option<u32>,
        minimum: Option<u64>,
        dynamic: bool,
    ) -> BindGroupLayoutEntry {
        BindGroupLayoutEntry {
            binding,
            visibility,
            ty: BindingType::Buffer {
                ty,
                has_dynamic_offset: dynamic,
                min_binding_size: minimum.and_then(NonZeroU64::new),
            },
            count: count.and_then(NonZeroU32::new),
        }
    }
    fn descriptor() -> RenderPipelineDescriptor {
        RenderPipelineDescriptor {
            layout: vec![BindGroupLayoutDescriptor::default(); 5],
            ..Default::default()
        }
    }
    fn storage(binding: u32, visibility: ShaderStages) -> BindGroupLayoutEntry {
        buffer(
            binding,
            BufferBindingType::Storage { read_only: true },
            visibility,
            None,
            None,
            false,
        )
    }

    #[test]
    fn all_declared_stage_buffers_count_even_without_shader_use_and_selection_uniform_counts() {
        let mut d = descriptor();
        let l = limits();
        d.layout[0].entries = (0..8)
            .map(|binding| storage(binding, ShaderStages::VERTEX_FRAGMENT))
            .collect();
        d.layout[4].entries = (0..12)
            .map(|binding| {
                buffer(
                    binding,
                    BufferBindingType::Uniform,
                    ShaderStages::FRAGMENT,
                    None,
                    None,
                    false,
                )
            })
            .collect();
        assert!(validate(&d, &l).is_ok());
        d.layout[4].entries.push(buffer(
            12,
            BufferBindingType::Uniform,
            ShaderStages::FRAGMENT,
            None,
            None,
            false,
        ));
        assert!(
            validate(&d, &l)
                .unwrap_err()
                .contains("fragment uniform buffers: required 13")
        );
        d.layout[4].entries.pop();
        d.layout[0].entries.push(storage(8, ShaderStages::VERTEX));
        assert!(
            validate(&d, &l)
                .unwrap_err()
                .contains("vertex storage buffers: required 9")
        );
    }

    #[test]
    fn material_buffer_arrays_use_actual_array_budget_instead_of_scalar_storage_budget() {
        let mut d = descriptor();
        let mut l = limits();
        d.layout[0].entries = (0..8)
            .map(|binding| storage(binding, ShaderStages::FRAGMENT))
            .collect();
        d.layout[3].entries.push(buffer(
            10,
            BufferBindingType::Storage { read_only: true },
            ShaderStages::FRAGMENT,
            Some(128),
            Some(16),
            false,
        ));
        l.max_binding_array_elements_per_shader_stage = 128;
        assert!(
            validate(&d, &l).is_ok(),
            "128-element material array does not become 128 scalar storage bindings"
        );
        l.max_binding_array_elements_per_shader_stage = 127;
        assert!(
            validate(&d, &l)
                .unwrap_err()
                .contains("binding-array elements: required 128")
        );
    }

    #[test]
    fn sparse_binding_indices_group_count_and_interstage_capacity_refuse_exact_boundaries() {
        let mut d = descriptor();
        let mut l = limits();
        l.max_bindings_per_bind_group = 11;
        d.layout[4].entries.push(buffer(
            10,
            BufferBindingType::Uniform,
            ShaderStages::FRAGMENT,
            None,
            None,
            false,
        ));
        assert!(validate(&d, &l).is_ok());
        d.layout[4].entries[0].binding = 11;
        assert!(
            validate(&d, &l)
                .unwrap_err()
                .contains("binding-index limit 11")
        );
        d.layout[4].entries[0].binding = 10;
        l.max_bind_groups = 4;
        assert!(validate(&d, &l).unwrap_err().contains("bind groups"));
        l.max_bind_groups = 5;
        l.max_inter_stage_shader_variables = 11;
        assert!(
            validate(&d, &l)
                .unwrap_err()
                .contains("F1 inter-stage locations")
        );
    }

    #[test]
    fn minimum_sizes_dynamic_budgets_and_illegal_array_uniform_group_are_explicit() {
        let mut d = descriptor();
        let mut l = limits();
        l.max_uniform_buffer_binding_size = 256;
        d.layout[4].entries.push(buffer(
            10,
            BufferBindingType::Uniform,
            ShaderStages::FRAGMENT,
            None,
            Some(256),
            false,
        ));
        assert!(validate(&d, &l).is_ok());
        d.layout[4].entries[0] = buffer(
            10,
            BufferBindingType::Uniform,
            ShaderStages::FRAGMENT,
            None,
            Some(257),
            false,
        );
        assert!(
            validate(&d, &l)
                .unwrap_err()
                .contains("minimum buffer bytes")
        );
        d.layout[4].entries[0] = buffer(
            10,
            BufferBindingType::Uniform,
            ShaderStages::VERTEX,
            None,
            Some(16),
            true,
        );
        l.max_dynamic_uniform_buffers_per_pipeline_layout = 0;
        assert!(
            validate(&d, &l)
                .unwrap_err()
                .contains("dynamic uniform buffers")
        );
        l.max_dynamic_uniform_buffers_per_pipeline_layout = 8;
        d.layout[4].entries.push(buffer(
            11,
            BufferBindingType::Storage { read_only: true },
            ShaderStages::FRAGMENT,
            Some(2),
            None,
            false,
        ));
        assert!(
            validate(&d, &l)
                .unwrap_err()
                .contains("combines binding arrays with dynamic")
        );
        d.layout[4].entries[0] = buffer(
            10,
            BufferBindingType::Uniform,
            ShaderStages::VERTEX,
            None,
            Some(16),
            false,
        );
        assert!(
            validate(&d, &l)
                .unwrap_err()
                .contains("combines binding arrays with scalar uniform")
        );
    }
    #[test]
    fn extra_selection_uniform_and_sampler_arrays_use_their_actual_stage_budgets() {
        use bevy::render::render_resource::SamplerBindingType;
        let mut d = descriptor();
        let mut l = limits();
        l.max_uniform_buffers_per_shader_stage = 11;
        d.layout[0].entries = (0..11)
            .map(|binding| {
                buffer(
                    binding,
                    BufferBindingType::Uniform,
                    ShaderStages::FRAGMENT,
                    None,
                    None,
                    false,
                )
            })
            .collect();
        assert!(validate(&d, &l).is_ok());
        d.layout[4].entries.push(buffer(
            10,
            BufferBindingType::Uniform,
            ShaderStages::FRAGMENT,
            None,
            Some(64),
            false,
        ));
        assert!(
            validate(&d, &l)
                .unwrap_err()
                .contains("fragment uniform buffers: required 12")
        );
        l.max_uniform_buffers_per_shader_stage = 12;
        l.max_samplers_per_shader_stage = 0;
        d.layout[3].entries.push(BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Sampler(SamplerBindingType::Filtering),
            count: NonZeroU32::new(9),
        });
        l.max_binding_array_elements_per_shader_stage = 9;
        l.max_binding_array_sampler_elements_per_shader_stage = 9;
        assert!(
            validate(&d, &l).is_ok(),
            "array samplers are not scalar sampler entries"
        );
        l.max_binding_array_sampler_elements_per_shader_stage = 8;
        assert!(
            validate(&d, &l)
                .unwrap_err()
                .contains("binding-array sampler elements: required 9")
        );
    }
}
