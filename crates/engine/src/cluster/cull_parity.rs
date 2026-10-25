//! Opt-in same-frame readback of the native retained and forced cull outputs.
//! Diagnostic snapshots also supply the native raster audit's two input lanes.

use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU8, Ordering},
    },
};

use bevy::{
    diagnostic::FrameCount,
    prelude::*,
    render::{
        render_resource::{Buffer, BufferDescriptor, BufferUsages, MapMode, PollType},
        renderer::RenderDevice,
        view::RetainedViewEntity,
    },
};

use super::{ViewGpu, cull_reuse::CullInputs};

fn output_directory() -> Option<&'static PathBuf> {
    static DIRECTORY: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIRECTORY
        .get_or_init(|| std::env::var_os("MUDCRAB_CULL_PARITY_DIR").map(PathBuf::from))
        .as_ref()
}

pub(super) fn copy_usage() -> BufferUsages {
    if output_directory().is_some() {
        BufferUsages::COPY_SRC | BufferUsages::COPY_DST
    } else {
        BufferUsages::empty()
    }
}

struct Pair {
    name: &'static str,
    live: Buffer,
    retained: Buffer,
    forced: Buffer,
    readbacks: [Buffer; 2],
    mapped: [Arc<AtomicU8>; 2],
}

struct Job {
    ordinal: usize,
    frame: u32,
    inputs: String,
    uniform: Vec<u8>,
    pairs: Vec<Pair>,
    finished: bool,
    mapping: bool,
}

struct State {
    directory: PathBuf,
    request: Option<PathBuf>,
    paired_shadow: bool,
    frame: Option<u32>,
    next: usize,
    jobs: HashMap<RetainedViewEntity, Job>,
}

#[derive(Resource)]
pub(super) struct CullParity(Option<Mutex<State>>);

impl Default for CullParity {
    fn default() -> Self {
        let state = output_directory().map(|directory| {
            fs::create_dir(directory).expect("create a fresh native cull parity directory");
            fs::write(
                directory.join("OWNER.txt"),
                "Owner: sol-2153\nPurpose: native cull parity TEST; not timing evidence\n",
            )
            .expect("write native cull parity owner");
            Mutex::new(State {
                directory: directory.clone(),
                request: std::env::var_os("MUDCRAB_GPU_PARITY_REQUEST").map(PathBuf::from),
                paired_shadow: std::env::var_os("MUDCRAB_SHADOW_PARITY_DIR").is_some(),
                frame: None,
                next: 0,
                jobs: HashMap::new(),
            })
        });
        Self(state)
    }
}

impl CullParity {
    pub(super) fn request_frame(&self, frame: u32, shadow_frame: Option<u32>) {
        let Some(state) = &self.0 else { return };
        let mut state = state.lock().expect("native cull audit state");
        if state.frame.is_some() {
            return;
        }
        if state.paired_shadow {
            if shadow_frame == Some(frame) {
                state.frame = Some(frame);
            }
        } else if state.request.as_ref().is_none_or(|path| path.is_file()) {
            state.frame = Some(frame);
        }
    }

    /// Only a genuine cache hit can supply the retained side of a pair.
    pub(super) fn begin(
        &self,
        key: RetainedViewEntity,
        frame: u32,
        inputs: CullInputs,
        view: &ViewGpu,
        device: &RenderDevice,
        encoder: &mut wgpu::CommandEncoder,
    ) -> bool {
        let Some(state) = &self.0 else { return false };
        let mut state = state.lock().expect("native cull audit state");
        if state.frame != Some(frame) || state.jobs.contains_key(&key) {
            return false;
        }
        let mut pairs = Vec::new();
        for (name, live) in [
            ("arguments", &view.args),
            ("counts", &view.counts),
            ("visible", &view.visible),
            ("instances", &view.instance_list),
            ("dispatch", &view.dispatch),
        ] {
            let make = |usage| {
                device.create_buffer(&BufferDescriptor {
                    label: Some("native exact cull parity"),
                    size: live.size(),
                    usage,
                    mapped_at_creation: false,
                })
            };
            let retained = make(BufferUsages::COPY_SRC | BufferUsages::COPY_DST);
            let forced = make(BufferUsages::COPY_SRC | BufferUsages::COPY_DST);
            let readbacks = [
                make(BufferUsages::MAP_READ | BufferUsages::COPY_DST),
                make(BufferUsages::MAP_READ | BufferUsages::COPY_DST),
            ];
            encoder.copy_buffer_to_buffer(live, 0, &retained, 0, live.size());
            encoder.copy_buffer_to_buffer(&retained, 0, &readbacks[0], 0, live.size());
            pairs.push(Pair {
                name,
                live: live.clone(),
                retained,
                forced,
                readbacks,
                mapped: std::array::from_fn(|_| Arc::new(AtomicU8::new(0))),
            });
        }
        let ordinal = state.next;
        state.next += 1;
        state.jobs.insert(
            key,
            Job {
                ordinal,
                frame,
                inputs: format!(
                    "view={key:?}\nretained_main_entity={}\nretained_auxiliary_entity={}\nretained_subview={}\ninputs={inputs:?}\n",
                    key.main_entity.to_bits(), key.auxiliary_entity.to_bits(), key.subview_index,
                ),
                uniform: view
                    .params_uploaded
                    .as_ref()
                    .map_or_else(Vec::new, |params| bytemuck::bytes_of(params).to_vec()),
                pairs,
                finished: false,
                mapping: false,
            },
        );
        true
    }

    pub(super) fn finish(
        &self,
        key: RetainedViewEntity,
        view_entity: Entity,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Option<Vec<(Buffer, Buffer, Buffer)>> {
        let state = self.0.as_ref()?;
        let mut state = state.lock().expect("native cull audit state");
        let job = state.jobs.get_mut(&key)?;
        if job.finished {
            return None;
        }
        for pair in &job.pairs {
            encoder.copy_buffer_to_buffer(&pair.live, 0, &pair.forced, 0, pair.live.size());
            encoder.copy_buffer_to_buffer(&pair.forced, 0, &pair.readbacks[1], 0, pair.live.size());
        }
        job.finished = true;
        job.inputs
            .push_str(&format!("render_view_entity={}\n", view_entity.to_bits()));
        Some(
            job.pairs
                .iter()
                .map(|pair| {
                    (
                        pair.live.clone(),
                        pair.retained.clone(),
                        pair.forced.clone(),
                    )
                })
                .collect(),
        )
    }
}

fn write_new(path: PathBuf, bytes: &[u8]) {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut file| file.write_all(bytes))
        .expect("write native parity evidence without replacement");
}

pub(super) fn poll(device: Res<RenderDevice>, parity: Res<CullParity>, frame: Res<FrameCount>) {
    let Some(state) = &parity.0 else { return };
    let _ = device.poll(PollType::Poll);
    let mut state = state.lock().expect("native cull audit state");
    let directory = state.directory.clone();
    state.jobs.retain(|key, job| {
        if !job.finished {
            if frame.0 != job.frame {
                error!(
                    ?key,
                    frame = job.frame,
                    "native cull parity INCOMPLETE: no full encode"
                );
                write_new(
                    directory.join(format!("view-{}-INCOMPLETE.txt", job.ordinal)),
                    job.inputs.as_bytes(),
                );
                return false;
            }
            return true;
        }
        if !job.mapping {
            for pair in &job.pairs {
                for index in 0..2 {
                    let mapped = pair.mapped[index].clone();
                    pair.readbacks[index]
                        .slice(..)
                        .map_async(MapMode::Read, move |result| {
                            mapped.store(if result.is_ok() { 1 } else { 2 }, Ordering::Release);
                        });
                }
            }
            job.mapping = true;
        }
        if job.pairs.iter().any(|pair| {
            pair.mapped
                .iter()
                .any(|mapped| mapped.load(Ordering::Acquire) == 0)
        }) {
            return true;
        }
        let mut report = format!("{}frame={}\n", job.inputs, job.frame);
        let mut failed = false;
        write_new(
            directory.join(format!("view-{}-uniform.bin", job.ordinal)),
            &job.uniform,
        );
        for pair in &job.pairs {
            if pair
                .mapped
                .iter()
                .any(|mapped| mapped.load(Ordering::Acquire) != 1)
            {
                failed = true;
                report.push_str(&format!("{}: MAP FAILED\n", pair.name));
                continue;
            }
            let retained = pair.readbacks[0].slice(..).get_mapped_range();
            let forced = pair.readbacks[1].slice(..).get_mapped_range();
            let different = retained
                .iter()
                .zip(forced.iter())
                .filter(|(a, b)| a != b)
                .count();
            failed |= different != 0 || retained.len() != forced.len();
            report.push_str(&format!(
                "{}: bytes={} differing_bytes={}\n",
                pair.name,
                retained.len(),
                different
            ));
            for (lane, bytes) in [("retained", retained.as_ref()), ("forced", forced.as_ref())] {
                write_new(
                    directory.join(format!("view-{}-{}-{lane}.bin", job.ordinal, pair.name)),
                    bytes,
                );
            }
            drop(retained);
            drop(forced);
            pair.readbacks[0].unmap();
            pair.readbacks[1].unmap();
        }
        report.push_str(if failed {
            "RESULT: FAIL\n"
        } else {
            "RESULT: PASS\n"
        });
        write_new(
            directory.join(format!("view-{}-receipt.txt", job.ordinal)),
            report.as_bytes(),
        );
        info!(
            ?key,
            frame = job.frame,
            passed = !failed,
            "native exact cull buffer parity"
        );
        false
    });
}
