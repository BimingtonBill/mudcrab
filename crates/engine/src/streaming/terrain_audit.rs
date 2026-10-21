//! Optional full-rebuild checks for the incremental terrain indices.
//!
//! The resource and publisher exist only with `--terrain-audit`. Callers test resource presence
//! before reading the clock or rebuilding anything. The cadence uses wall time because game time
//! may be paused or clamped during streaming. Each index has its own deadline.

use crate::{config::EngineConfig, profiling::ProfilingState};
use bevy::prelude::*;
use std::{
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug)]
pub(super) enum AuditIndex {
    Identity,
    Lod,
}

impl AuditIndex {
    fn slot(self) -> usize {
        self as usize
    }
    fn name(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Lod => "lod",
        }
    }
}

#[derive(Default)]
struct Counts {
    checks: AtomicU64,
    desyncs: AtomicU64,
}

#[derive(Resource, Default)]
pub(super) struct TerrainAudit {
    last: Mutex<[Option<Instant>; 2]>,
    counts: [Counts; 2],
}

impl TerrainAudit {
    pub(super) fn due(&self, index: AuditIndex) -> bool {
        self.due_at(index, Instant::now())
    }

    fn due_at(&self, index: AuditIndex, now: Instant) -> bool {
        let mut last = self
            .last
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let slot = &mut last[index.slot()];
        if slot.is_some_and(|previous| {
            now.saturating_duration_since(previous) < Duration::from_secs(1)
        }) {
            return false;
        }
        *slot = Some(now);
        true
    }

    pub(super) fn record(&self, index: AuditIndex, result: Result<(), String>) {
        let counts = &self.counts[index.slot()];
        let checks = counts.checks.fetch_add(1, Ordering::Relaxed) + 1;
        if let Err(reason) = result {
            let desyncs = counts.desyncs.fetch_add(1, Ordering::Relaxed) + 1;
            error!(index = index.name(), checks, desyncs, %reason, "terrain audit desynchronization");
        }
    }
}

pub(super) fn install(app: &mut App) {
    if !app
        .world()
        .get_resource::<EngineConfig>()
        .is_some_and(|config| config.terrain_audit)
    {
        return;
    }
    app.init_resource::<TerrainAudit>().add_systems(
        Last,
        publish.after(super::terrain_identity::validate_relationships),
    );
    // Before the cluster renderer is introduced, ordinary streaming is the identity consumer.
    app.add_systems(PostUpdate, audit_stock_identities);
    info!("terrain full-rebuild audits enabled, once per wall-clock second per index");
}

fn audit_stock_identities(mut identities: super::TerrainIdentityAccess) {
    identities.identities_shared();
}

fn publish(
    audit: Res<TerrainAudit>,
    relationships: Option<Res<super::terrain_identity::TerrainRelationshipChecks>>,
    mut profiler: ResMut<ProfilingState>,
    mut published: Local<[u64; 11]>,
) {
    let mut values = [0; 11];
    for index in [AuditIndex::Identity, AuditIndex::Lod] {
        let slot = index.slot() * 2;
        values[slot] = audit.counts[index.slot()].checks.load(Ordering::Relaxed);
        values[slot + 1] = audit.counts[index.slot()].desyncs.load(Ordering::Relaxed);
    }
    if let Some(relationships) = relationships {
        let relationships = relationships.snapshot();
        values[4] = relationships.fast_checks;
        values[5] = relationships.linear_checks;
        values[6] = relationships.recovery_requests;
        values[7] = relationships.linear_siblings_examined;
        values[8] = relationships.max_linear_siblings;
        values[9] = relationships.pending_suspects;
        values[10] = relationships.confirmed_missing;
    }
    if *published == values {
        return;
    }
    let audit_changed = published[..4] != values[..4];
    *published = values;
    for (name, value) in [
        "terrain/audit/identity/checks",
        "terrain/audit/identity/desyncs",
        "terrain/audit/lod/checks",
        "terrain/audit/lod/desyncs",
        "terrain/identity/backlink_fast",
        "terrain/identity/backlink_linear",
        "terrain/identity/backlink_recoveries",
        "terrain/identity/backlink_linear_siblings",
        "terrain/identity/backlink_max_siblings",
        "terrain/identity/backlink_pending",
        "terrain/identity/backlink_missing",
    ]
    .into_iter()
    .zip(values)
    {
        profiler.set_gauge(name, value as f64);
    }
    if audit_changed {
        info!(
            identity_checks = values[0],
            identity_desyncs = values[1],
            lod_checks = values[2],
            lod_desyncs = values[3],
            backlink_fast = values[4],
            backlink_linear = values[5],
            backlink_recoveries = values[6],
            backlink_linear_siblings = values[7],
            backlink_max_siblings = values[8],
            backlink_pending = values[9],
            backlink_missing = values[10],
            "terrain audit counters"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cadence_uses_independent_wall_clock_deadlines_without_catching_up() {
        let audit = TerrainAudit::default();
        let now = Instant::now();
        assert!(audit.due_at(AuditIndex::Identity, now));
        assert!(!audit.due_at(AuditIndex::Identity, now + Duration::from_millis(999)));
        assert!(audit.due_at(AuditIndex::Lod, now + Duration::from_millis(999)));
        assert!(audit.due_at(AuditIndex::Identity, now + Duration::from_secs(1)));
        assert!(!audit.due_at(AuditIndex::Lod, now + Duration::from_secs(1)));
        assert!(audit.due_at(AuditIndex::Identity, now + Duration::from_secs(100)));
        assert!(!audit.due_at(AuditIndex::Identity, now + Duration::from_secs(100)));
    }

    #[test]
    fn disabled_installs_no_audit_resource_or_publisher() {
        let mut app = App::new();
        app.insert_resource(EngineConfig::default());
        install(&mut app);
        assert!(!app.world().contains_resource::<TerrainAudit>());
        // No ProfilingState exists: an accidentally installed publisher would fail here.
        app.update();
    }

    #[test]
    fn stock_played_build_checks_identities_without_cluster_extraction() {
        let mut app = App::new();
        app.insert_resource(EngineConfig {
            terrain_audit: true,
            ..default()
        });
        app.init_resource::<ProfilingState>();
        super::super::identity_changes::install_terrain(app.world_mut());
        install(&mut app);
        app.update();
        let profiler = app.world().resource::<ProfilingState>();
        assert_eq!(profiler.gauge("terrain/audit/identity/checks"), Some(1.0));
        assert_eq!(profiler.gauge("terrain/audit/identity/desyncs"), Some(0.0));
    }

    #[test]
    fn failures_are_counted_and_exported_without_panicking() {
        let mut app = App::new();
        app.insert_resource(EngineConfig {
            terrain_audit: true,
            ..default()
        });
        app.init_resource::<ProfilingState>();
        // This test drives the actual audit publisher in isolation; the stock consumer has
        // its own installation regression above and must not add an extra identity sample.
        app.init_resource::<TerrainAudit>()
            .add_systems(Last, publish);
        let audit = app.world().resource::<TerrainAudit>();
        audit.record(
            AuditIndex::Identity,
            Err("injected identity mismatch".to_owned()),
        );
        audit.record(AuditIndex::Identity, Ok(()));
        audit.record(AuditIndex::Lod, Err("injected LOD mismatch".to_owned()));
        app.update();
        let profiler = app.world().resource::<ProfilingState>();
        assert_eq!(profiler.gauge("terrain/audit/identity/checks"), Some(2.0));
        assert_eq!(profiler.gauge("terrain/audit/identity/desyncs"), Some(1.0));
        assert_eq!(profiler.gauge("terrain/audit/lod/checks"), Some(1.0));
        assert_eq!(profiler.gauge("terrain/audit/lod/desyncs"), Some(1.0));
    }
}
