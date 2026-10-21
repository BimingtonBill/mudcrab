//! The cull counter (`--cull-counter`): how many instances each view's render phases submit to the
//! GPU preprocessing pass and how many survive it.
//!
//! Bevy 0.19 culls on the GPU. The mesh preprocessing shader tests each instance against the view
//! frustum and, on a camera with `OcclusionCulling`, against the previous frame's depth pyramid,
//! and writes the survivors into per-batch indirect draw commands. Nothing reads that result back:
//! the numbers live only in GPU buffers, so before this module the run report could say the
//! culling path was *active* (`RendererMetrics::gpu_culling_active`) but not what it *did*.
//!
//! # What is counted
//!
//! Per view (named by its size and whether it carries `OcclusionCulling`, e.g.
//! `1920x1080+occlusion`) and per render phase (`opaque`, `alpha_mask` and their prepass twins):
//!
//! * `submitted`: the instances in the multi-drawable bins the CPU handed to the GPU unpacking
//!   pass for that view (`BinUnpackingJob::mesh_instance_count`, `bevy_render-0.19.0/src/batching/
//!   gpu_preprocessing.rs`). These already passed the main world's visibility (frustum) test, so
//!   they are the instances the GPU tests;
//! * `drawn`: the instances the view's finished indirect draw commands draw. The last build pass of
//!   a view's frame (`main_build_indirect_parameters`, `MAIN_PHASE` in `build_indirect_params.wgsl`)
//!   writes `early + late` instances into each command, and the batch set's
//!   `indirect_parameters_count` says how many commands of the buffer that build wrote (entries
//!   past it are stale);
//! * `rejected = submitted - drawn`: what the preprocessing shader cut, the frustum test and the
//!   occlusion test together. Bevy gives no way to split the two on the GPU, so the split is a
//!   second run with `--no-main-occlusion`: the difference in `drawn` is what occlusion culling
//!   rejects.
//!
//! # Why the copy is made inside each view
//!
//! The indirect buffers are shared by every view and rewritten for each one: a view's schedule
//! starts by clearing every batch's instance counts (`clear_indirect_parameters_metadata`) and ends
//! its culling with a build that resets the batch sets and rebuilds them. After the frame only the
//! last view's result is left. So the copy is encoded in the view's own `Core3d` schedule, right
//! after its `main_build_indirect_parameters`, where the buffers hold that view and no other. The
//! shadow views are not counted: they get no main build, and the shadow phase's buffer holds their
//! early and late passes separately.
//!
//! Only multi-drawable bins have batch sets, so unbatchable meshes and the sorted transparent
//! phases are in neither number.
//!
//! # The cost, and why it is opt-in
//!
//! Reading a GPU buffer needs `COPY_SRC` on Bevy's indirect parameter buffers, which Bevy only sets
//! under `RenderDebugFlags::ALLOW_COPIES_FROM_INDIRECT_PARAMETERS` ("a debugging feature that may
//! reduce performance"). The engine sets that flag only when the run asks for the counter. A
//! sample copies the indirect command and batch-set buffers into staging buffers every
//! [`SAMPLE_INTERVAL`] frames and maps them without waiting; the numbers arrive a few frames later
//! and describe the frame that was copied.

use std::{
    any::TypeId,
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use bevy::{
    core_pipeline::{
        core_3d::{AlphaMask3d, Opaque3d},
        prepass::{AlphaMask3dPrepass, Opaque3dPrepass},
        schedule::{Core3d, Core3dSystems},
    },
    pbr::{ShadowView, main_build_indirect_parameters},
    prelude::*,
    render::{
        Render, RenderApp, RenderDebugFlags, RenderSystems,
        batching::gpu_preprocessing::{
            BinUnpackingBuffers, IndirectParametersBuffers, UntypedPhaseIndirectParametersBuffers,
        },
        occlusion_culling::OcclusionCulling,
        render_resource::{Buffer, BufferDescriptor, BufferUsages, MapMode, PollType},
        renderer::{RenderContext, RenderDevice, ViewQuery},
        view::ExtractedView,
    },
};
use serde::Serialize;

use crate::config::EngineConfig;

/// Frames between two samples. A sample is a handful of small buffer copies; the cull rate of a
/// route changes over seconds, so every 15th frame (4 samples a second at 60 fps) is plenty.
pub const SAMPLE_INTERVAL: u64 = 15;

/// `instance_count` is word 1 of both `IndirectParametersIndexed` and `IndirectParametersNonIndexed`.
const INSTANCE_COUNT_WORD: usize = 1;
/// `IndirectParametersIndexed` is five words, `IndirectParametersNonIndexed` four.
const INDEXED_STRIDE_WORDS: usize = 5;
const NON_INDEXED_STRIDE_WORDS: usize = 4;
/// `IndirectBatchSet` is `indirect_parameters_count`, `indirect_parameters_base`.
const BATCH_SET_WORDS: usize = 2;

/// Registers the counter. A run without `--cull-counter` registers nothing.
pub struct CullCounterPlugin {
    enabled: bool,
}

impl CullCounterPlugin {
    pub fn from_config(config: &EngineConfig) -> Self {
        Self {
            enabled: config.cull_counter,
        }
    }

    /// The flags Bevy's `RenderPlugin` must be built with for the counter to be able to read the
    /// indirect buffers back.
    pub fn render_debug_flags(config: &EngineConfig) -> RenderDebugFlags {
        if config.cull_counter {
            RenderDebugFlags::ALLOW_COPIES_FROM_INDIRECT_PARAMETERS
        } else {
            RenderDebugFlags::empty()
        }
    }
}

impl Plugin for CullCounterPlugin {
    fn build(&self, app: &mut App) {
        if !self.enabled {
            return;
        }
        let bridge = CullCounterBridge::default();
        app.insert_resource(bridge.clone());
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .insert_resource(bridge)
            .init_resource::<Sampling>()
            .add_systems(Render, begin_sample.in_set(RenderSystems::PrepareViews))
            .add_systems(
                Render,
                finish_sample
                    .in_set(RenderSystems::Cleanup)
                    .before(RenderSystems::PostCleanup),
            )
            .add_systems(
                Core3d,
                copy_view_cull_counts
                    .after(main_build_indirect_parameters)
                    .before(Core3dSystems::MainPass),
            );
    }
}

/// What one phase's samples add up to, as `renderer.json` carries it.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct PhaseCull {
    /// Samples that reached the counter.
    pub samples: u64,
    /// Mean instances submitted to the GPU preprocessing pass per sample.
    pub submitted_mean: f64,
    /// Mean instances the finished draw commands draw per sample.
    pub drawn_mean: f64,
    /// `submitted_mean - drawn_mean`: frustum and occlusion rejections together.
    pub rejected_mean: f64,
    /// `rejected_mean / submitted_mean`, 0 when nothing was submitted.
    pub rejected_fraction: f64,
    /// The most instances one sample submitted.
    pub submitted_max: u64,
    /// The most instances one sample drew.
    pub drawn_max: u64,
    /// Batch sets (multi-draw groups) in the last sample.
    pub batch_sets: u64,
    #[serde(skip)]
    submitted_sum: u64,
    #[serde(skip)]
    drawn_sum: u64,
}

impl PhaseCull {
    fn add(&mut self, submitted: u64, drawn: u64, batch_sets: u64) {
        self.samples += 1;
        self.batch_sets = batch_sets;
        self.submitted_sum += submitted;
        self.drawn_sum += drawn;
        self.submitted_max = self.submitted_max.max(submitted);
        self.drawn_max = self.drawn_max.max(drawn);
        let samples = self.samples as f64;
        self.submitted_mean = self.submitted_sum as f64 / samples;
        self.drawn_mean = self.drawn_sum as f64 / samples;
        self.rejected_mean = self.submitted_mean - self.drawn_mean;
        self.rejected_fraction = if self.submitted_sum == 0 {
            0.0
        } else {
            self.submitted_sum.saturating_sub(self.drawn_sum) as f64 / self.submitted_sum as f64
        };
    }
}

/// Totals by view name, then phase name.
pub type CullTotals = BTreeMap<String, BTreeMap<String, PhaseCull>>;

/// The samples' meeting point: the render world writes, the main world reads for the report.
#[derive(Resource, Clone, Default)]
pub struct CullCounterBridge(Arc<Mutex<CullTotals>>);

impl CullCounterBridge {
    /// The totals so far.
    pub fn snapshot(&self) -> CullTotals {
        self.0
            .lock()
            .map(|totals| totals.clone())
            .unwrap_or_default()
    }
}

/// The phase's name in the report, or `None` for one that is not counted: the shadow phase (its
/// views get no main build) and sorted phases (no batch sets).
fn phase_name(phase: TypeId) -> Option<&'static str> {
    if phase == TypeId::of::<Opaque3d>() {
        Some("opaque")
    } else if phase == TypeId::of::<AlphaMask3d>() {
        Some("alpha_mask")
    } else if phase == TypeId::of::<Opaque3dPrepass>() {
        Some("opaque_prepass")
    } else if phase == TypeId::of::<AlphaMask3dPrepass>() {
        Some("alpha_mask_prepass")
    } else {
        None
    }
}

/// One class (indexed or non-indexed) of one phase of one view: staging copies of the indirect
/// buffers, waiting to be mapped.
struct StagedClass {
    phase: &'static str,
    stride_words: usize,
    data: Buffer,
    sets: Buffer,
    data_mapped: Arc<AtomicBool>,
    sets_mapped: Arc<AtomicBool>,
}

struct StagedView {
    name: String,
    /// Submitted instances per phase name, for this view.
    submitted: BTreeMap<&'static str, u64>,
    classes: Vec<StagedClass>,
}

#[derive(Resource, Default)]
struct Sampling {
    frame: AtomicU64,
    /// Set for the frames that copy: the views' systems check it.
    active: AtomicBool,
    /// Copies made this frame, not yet mapped.
    staged: Mutex<Vec<StagedView>>,
    /// Copies mapped and waiting for the GPU.
    pending: Mutex<Vec<StagedView>>,
}

/// Decides whether this frame copies. At most one sample is in flight, so a slow map is never
/// piled on.
fn begin_sample(sampling: Res<Sampling>) {
    let frame = sampling.frame.fetch_add(1, Ordering::Relaxed) + 1;
    let busy = sampling
        .pending
        .lock()
        .map_or(true, |pending| !pending.is_empty());
    sampling.active.store(
        !busy && frame.is_multiple_of(SAMPLE_INTERVAL),
        Ordering::Relaxed,
    );
}

fn staging(device: &RenderDevice, label: &str, source: &Buffer) -> Buffer {
    device.create_buffer(&BufferDescriptor {
        label: Some(label),
        size: source.size(),
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// Records, in the view's own command encoder and right after the view's last indirect-parameter
/// build, copies of the indirect command and batch-set buffers.
fn copy_view_cull_counts(
    view: ViewQuery<(&ExtractedView, Has<OcclusionCulling>), Without<ShadowView>>,
    sampling: Res<Sampling>,
    indirect: Option<Res<IndirectParametersBuffers>>,
    unpacking: Option<Res<BinUnpackingBuffers>>,
    mut ctx: RenderContext,
) {
    if !sampling.active.load(Ordering::Relaxed) {
        return;
    }
    let (Some(indirect), Some(unpacking)) = (indirect, unpacking) else {
        return;
    };
    let (extracted, occlusion) = view.into_inner();
    let name = format!(
        "{}x{}{}",
        extracted.viewport.z,
        extracted.viewport.w,
        if occlusion { "+occlusion" } else { "" }
    );
    let mut staged = StagedView {
        name,
        submitted: BTreeMap::new(),
        classes: Vec::new(),
    };
    let device = ctx.render_device().clone();
    for (phase, buffers) in indirect.iter() {
        let Some(phase_label) = phase_name(*phase) else {
            continue;
        };
        let submitted = unpacking
            .view_phase_buffers
            .iter()
            .filter(|(key, _)| key.phase == *phase && key.view == extracted.retained_view_entity)
            .flat_map(|(_, jobs)| {
                jobs.indexed_unpacking_jobs
                    .iter()
                    .chain(&jobs.non_indexed_unpacking_jobs)
            })
            .map(|job| u64::from(job.mesh_instance_count))
            .sum::<u64>();
        staged.submitted.insert(phase_label, submitted);
        stage_phase(
            &device,
            ctx.command_encoder(),
            buffers,
            phase_label,
            &mut staged,
        );
    }
    if staged.classes.is_empty() {
        return;
    }
    if let Ok(mut views) = sampling.staged.lock() {
        views.push(staged);
    }
}

/// Copies one phase's two classes (indexed, non-indexed) into staging buffers, when they have any.
fn stage_phase(
    device: &RenderDevice,
    encoder: &mut wgpu::CommandEncoder,
    buffers: &UntypedPhaseIndirectParametersBuffers,
    phase: &'static str,
    view: &mut StagedView,
) {
    let classes = [
        (
            INDEXED_STRIDE_WORDS,
            buffers.indexed.data_buffer(),
            buffers.indexed.batch_sets_buffer(),
        ),
        (
            NON_INDEXED_STRIDE_WORDS,
            buffers.non_indexed.data_buffer(),
            buffers.non_indexed.batch_sets_buffer(),
        ),
    ];
    for (stride_words, data, sets) in classes {
        let (Some(data), Some(sets)) = (data, sets) else {
            continue;
        };
        if data.size() == 0 || sets.size() == 0 {
            continue;
        }
        let data_copy = staging(device, "cull counter data", data);
        let sets_copy = staging(device, "cull counter sets", sets);
        encoder.copy_buffer_to_buffer(data, 0, &data_copy, 0, data.size());
        encoder.copy_buffer_to_buffer(sets, 0, &sets_copy, 0, sets.size());
        view.classes.push(StagedClass {
            phase,
            stride_words,
            data: data_copy,
            sets: sets_copy,
            data_mapped: Arc::new(AtomicBool::new(false)),
            sets_mapped: Arc::new(AtomicBool::new(false)),
        });
    }
}

fn map_flag(buffer: &Buffer) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let done = flag.clone();
    buffer.slice(..).map_async(MapMode::Read, move |result| {
        if let Err(error) = result {
            warn!("cull counter: could not map a staging buffer: {error}");
        }
        // Set on failure too: the poll loop must not wait for a map that will never arrive.
        done.store(true, Ordering::Release);
    });
    flag
}

fn words(buffer: &Buffer) -> Vec<u32> {
    let view = buffer.slice(..).get_mapped_range();
    view.as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| u32::from_le_bytes(*chunk))
        .collect()
}

/// The instances the finished commands of one class draw: for each batch set, the commands from
/// its base for its count, each command's `instance_count`. Entries outside a set's range were not
/// written this frame and are skipped, and so is a set that points past the buffer.
fn drawn_instances(data: &[u32], sets: &[u32], stride_words: usize) -> (u64, u64) {
    let mut drawn = 0u64;
    let mut batch_sets = 0u64;
    for set in sets.as_chunks::<BATCH_SET_WORDS>().0 {
        let (count, base) = (set[0] as usize, set[1] as usize);
        if count == 0 {
            continue;
        }
        batch_sets += 1;
        for command in base..base.saturating_add(count) {
            let at = command * stride_words + INSTANCE_COUNT_WORD;
            if let Some(instances) = data.get(at) {
                drawn += u64::from(*instances);
            }
        }
    }
    (drawn, batch_sets)
}

/// After the frame's graph has been submitted: map this frame's copies, and fold the earlier
/// sample's copies into the totals once the GPU has finished with them.
fn finish_sample(
    device: Res<RenderDevice>,
    sampling: Res<Sampling>,
    bridge: Res<CullCounterBridge>,
    mut warned: Local<bool>,
) {
    let _ = device.poll(PollType::Poll);

    // Map what this frame copied.
    let staged = sampling
        .staged
        .lock()
        .map(|mut views| std::mem::take(&mut *views))
        .unwrap_or_default();
    if !staged.is_empty() {
        if !device
            .features()
            .contains(wgpu::Features::MULTI_DRAW_INDIRECT_COUNT)
        {
            // Without it Bevy builds one command per batch and never increments a batch set's
            // count, so there is no way to tell this frame's commands from stale ones.
            if !*warned {
                warn!(
                    "cull counter: the device lacks MULTI_DRAW_INDIRECT_COUNT, nothing is counted"
                );
                *warned = true;
            }
        } else if let Ok(mut pending) = sampling.pending.lock() {
            for mut view in staged {
                for class in &mut view.classes {
                    class.data_mapped = map_flag(&class.data);
                    class.sets_mapped = map_flag(&class.sets);
                }
                pending.push(view);
            }
        }
    }

    // Fold in the copies the GPU has delivered.
    let Ok(mut pending) = sampling.pending.lock() else {
        return;
    };
    let ready = !pending.is_empty()
        && pending.iter().all(|view| {
            view.classes.iter().all(|class| {
                class.data_mapped.load(Ordering::Acquire)
                    && class.sets_mapped.load(Ordering::Acquire)
            })
        });
    if !ready {
        return;
    }
    let Ok(mut totals) = bridge.0.lock() else {
        return;
    };
    for view in pending.drain(..) {
        let mut drawn: BTreeMap<&'static str, (u64, u64)> = BTreeMap::new();
        for class in &view.classes {
            let (class_drawn, class_sets) =
                drawn_instances(&words(&class.data), &words(&class.sets), class.stride_words);
            let entry = drawn.entry(class.phase).or_default();
            entry.0 += class_drawn;
            entry.1 += class_sets;
            class.data.unmap();
            class.sets.unmap();
        }
        for (phase, (drawn, batch_sets)) in drawn {
            let submitted = view.submitted.get(phase).copied().unwrap_or(0);
            totals
                .entry(view.name.clone())
                .or_default()
                .entry(phase.to_owned())
                .or_default()
                .add(submitted, drawn, batch_sets);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two sets: the first writes commands 2 and 3, the second one command (index 0). Stale
    /// entries (1 and 4) are not counted.
    #[test]
    fn drawn_instances_sum_only_the_commands_a_batch_set_wrote() {
        // indexed: five words per command, instance_count is word 1.
        let mut data = vec![0u32; 5 * 5];
        for (command, instances) in [(0, 7), (1, 999), (2, 11), (3, 13), (4, 888)] {
            data[command * 5 + 1] = instances;
        }
        let sets = [2, 2, 1, 0]; // (count 2, base 2), (count 1, base 0)
        assert_eq!(drawn_instances(&data, &sets, 5), (11 + 13 + 7, 2));
    }

    #[test]
    fn an_empty_batch_set_is_not_counted_and_a_range_past_the_buffer_is_clamped() {
        let data = vec![0, 4, 0, 0];
        assert_eq!(drawn_instances(&data, &[0, 0], 4), (0, 0));
        assert_eq!(drawn_instances(&data, &[5, 0], 4), (4, 1));
    }

    #[test]
    fn rejected_fraction_is_the_share_of_submitted_instances_not_drawn() {
        let mut stats = PhaseCull::default();
        stats.add(100, 60, 3);
        stats.add(300, 240, 3);
        assert_eq!(stats.samples, 2);
        assert_eq!(stats.submitted_mean, 200.0);
        assert_eq!(stats.drawn_mean, 150.0);
        assert_eq!(stats.rejected_mean, 50.0);
        assert!((stats.rejected_fraction - 0.25).abs() < 1e-12);
        assert_eq!((stats.submitted_max, stats.drawn_max), (300, 240));
    }

    #[test]
    fn a_phase_nothing_was_submitted_to_has_no_rejected_fraction() {
        let mut stats = PhaseCull::default();
        stats.add(0, 0, 0);
        assert_eq!(stats.rejected_fraction, 0.0);
    }

    #[test]
    fn the_counter_is_off_unless_asked_for() {
        let config = EngineConfig::default();
        assert!(CullCounterPlugin::render_debug_flags(&config).is_empty());
        let asked = EngineConfig {
            cull_counter: true,
            ..config
        };
        assert!(
            CullCounterPlugin::render_debug_flags(&asked)
                .contains(RenderDebugFlags::ALLOW_COPIES_FROM_INDIRECT_PARAMETERS)
        );
    }
}
