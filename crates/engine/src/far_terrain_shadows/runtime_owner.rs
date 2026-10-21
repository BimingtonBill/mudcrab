//! Optional runtime ownership only: no plugin, options, render registration or readiness claim.
//! The caller creates this owner only for the enabled feature and replaces it when
//! matches_source fails. Retain the publication with every GPU binding using its packets.

use super::{
    gpu::UploadLimits,
    reader::{CertifiedGpuUpload, CertifiedTerrainSnapshot},
};
use crate::cluster::{reader::CertifiedPackage, terrain::CanonicalTerrainHeights};
use bevy::tasks::{AsyncComputeTaskPool, IoTaskPool, Task, futures_lite::future};
use std::{collections::BTreeMap, sync::Arc};

/// Exact source ownership, without exposing a constructor or mutable source inputs.
/// Equality is allocation identity, never an abbreviated digest or path equality.
#[derive(Clone)]
pub(crate) struct SourceOwnerArc(Arc<SourceIdentity>);

struct SourceIdentity {
    package: Arc<CertifiedPackage>,
    heights: CanonicalTerrainHeights,
}

impl SourceOwnerArc {
    pub(crate) fn matches_package(&self, package: &Arc<CertifiedPackage>) -> bool {
        Arc::ptr_eq(&self.0.package, package)
    }

    pub(crate) fn same_allocation(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Clone)]
pub(crate) enum WorldState {
    Pending,
    Ready(Arc<CertifiedGpuUpload>),
    /// Includes an explicit world with no inventory: unknown, never known empty.
    Unavailable(String),
}

/// Immutable main/render publication. The Arc is replaced only when world state changes.
pub(crate) struct Publication {
    pub(crate) source: SourceOwnerArc,
    pub(crate) revision: u64,
    pub(crate) worlds: BTreeMap<u32, WorldState>,
}

type SnapshotTask = Task<Result<Arc<CertifiedTerrainSnapshot>, String>>;
type WorldTask = Task<Result<Arc<CertifiedGpuUpload>, String>>;

pub(crate) struct SourceOwner {
    source: SourceOwnerArc,
    upload_limits: UploadLimits,
    snapshot_task: Option<SnapshotTask>,
    snapshot: Option<Arc<CertifiedTerrainSnapshot>>,
    snapshot_error: Option<String>,
    tasks: BTreeMap<u32, WorldTask>,
    publication: Arc<Publication>,
}

impl SourceOwner {
    /// Starts exactly one snapshot job. IoTaskPool must already be initialized by Bevy.
    /// Explicit upload limits are immutable for this owner; complete specialized
    /// pipeline-layout approval remains the integrating caller's responsibility.
    pub(crate) fn new(
        package: Arc<CertifiedPackage>,
        heights: CanonicalTerrainHeights,
        upload_limits: UploadLimits,
    ) -> Self {
        let source = SourceOwnerArc(Arc::new(SourceIdentity { package, heights }));
        let inputs = source.clone();
        let snapshot_task = IoTaskPool::get().spawn(async move {
            CertifiedTerrainSnapshot::open(inputs.0.package.clone(), inputs.0.heights.clone())
                .map_err(|error| format!("{error:#}"))
        });
        let publication = Arc::new(Publication {
            source: source.clone(),
            revision: 0,
            worlds: BTreeMap::new(),
        });
        Self {
            source,
            upload_limits,
            snapshot_task: Some(snapshot_task),
            snapshot: None,
            snapshot_error: None,
            tasks: BTreeMap::new(),
            publication,
        }
    }

    pub(crate) fn matches_source(
        &self,
        package: &Arc<CertifiedPackage>,
        heights: &CanonicalTerrainHeights,
    ) -> bool {
        Arc::ptr_eq(&self.source.0.package, package)
            && self.source.0.heights.same_allocation(heights)
    }

    pub(crate) fn publication(&self) -> Arc<Publication> {
        self.publication.clone()
    }

    /// Idempotent. Pending requests survive snapshot preparation; failed requests
    /// are cached and never silently retried. Jobs start only from poll.
    pub(crate) fn request_world(&mut self, world: u32) -> bool {
        if self.publication.worlds.contains_key(&world) {
            return false;
        }
        let state = self
            .snapshot_error
            .as_ref()
            .map_or(WorldState::Pending, |error| {
                WorldState::Unavailable(error.clone())
            });
        let mut worlds = self.publication.worlds.clone();
        worlds.insert(world, state);
        self.publish(worlds);
        true
    }

    /// Checks each task once; poll_once never waits for its work. No database
    /// hashing, world enumeration or packet building executes on this thread.
    /// Returns whether the immutable publication changed, not whether jobs started.
    pub(crate) fn poll(&mut self) -> bool {
        let mut updated = None;
        if let Some(result) = self
            .snapshot_task
            .as_mut()
            .and_then(|task| future::block_on(future::poll_once(task)))
        {
            self.snapshot_task = None;
            match result {
                Ok(snapshot) => self.snapshot = Some(snapshot),
                Err(error) => {
                    if !self.publication.worlds.is_empty() {
                        let mut worlds = self.publication.worlds.clone();
                        for state in worlds.values_mut() {
                            *state = WorldState::Unavailable(error.clone());
                        }
                        updated = Some(worlds);
                    }
                    self.snapshot_error = Some(error);
                }
            }
        }
        // Only completed tasks allocate a result entry. Pending-task checks do
        // not clone the world map or manufacture a new publication allocation.
        let mut finished = Vec::new();
        self.tasks.retain(|world, task| {
            if let Some(result) = future::block_on(future::poll_once(task)) {
                finished.push((*world, result));
                false
            } else {
                true
            }
        });
        for (world, result) in finished {
            let worlds = updated.get_or_insert_with(|| self.publication.worlds.clone());
            worlds.insert(
                world,
                match result {
                    Ok(packet) => WorldState::Ready(packet),
                    Err(error) => WorldState::Unavailable(error),
                },
            );
        }
        let changed = if let Some(worlds) = updated {
            self.publish(worlds);
            true
        } else {
            false
        };
        if let Some(snapshot) = &self.snapshot {
            for (&world, state) in &self.publication.worlds {
                if !matches!(state, WorldState::Pending) || self.tasks.contains_key(&world) {
                    continue;
                }
                let snapshot = snapshot.clone();
                let upload_limits = self.upload_limits;
                let task = IoTaskPool::get().spawn(async move {
                    let inventory = snapshot
                        .world(world)
                        .map_err(|error| format!("{error:#}"))?
                        .ok_or_else(|| {
                            "no explicit terrain inventory; world coverage remains unknown"
                                .to_owned()
                        })?;
                    // Inventory reads stay on IO; CPU packet construction must
                    // not occupy the executor used to load streaming assets.
                    AsyncComputeTaskPool::get()
                        .spawn(async move {
                            let packet = inventory
                                .build_packet()
                                .map_err(|error| format!("{error:#}"))?;
                            packet
                                .into_gpu(upload_limits)
                                .map(Arc::new)
                                .map_err(|error| format!("canonical upload refused: {error:?}"))
                        })
                        .await
                });
                self.tasks.insert(world, task);
            }
        }
        changed
    }

    fn publish(&mut self, worlds: BTreeMap<u32, WorldState>) {
        self.publication = Arc::new(Publication {
            source: self.source.clone(),
            revision: self
                .publication
                .revision
                .checked_add(1)
                .expect("F1 owner publication revision exhausted"),
            worlds,
        });
    }
}
