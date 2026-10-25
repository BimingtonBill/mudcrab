//! Enabled runtime refusals keep the ordinary source path at every distance.

use super::*;
use crate::world::reference_state::{self, ReferenceEnableState, SetReferenceEnabled};

fn fixture() -> Fixture {
    Fixture::from_directory(
        crate::cluster::reader::producer_fixture_tests::published_for_runtime_draw_states(),
    )
}

fn app(fixture: &Fixture) -> App {
    let mut app = complete_plan_app(fixture, exterior(0x70, IVec2::new(200, 0)));
    app.init_resource::<ProfilingState>();
    reference_state::install(&mut app);
    app.world_mut()
        .resource_mut::<ReferenceEnableState>()
        .bind(fixture.package.enable_catalog().clone());
    {
        let mut config = app.world_mut().resource_mut::<EngineConfig>();
        config.cluster_own_statics = true;
        config.stream_radius = 0;
    }
    app.world_mut()
        .resource_mut::<SourceFallbackDemand>()
        .update(Some(&fixture.package), BTreeSet::new());
    app
}

fn desired(app: &App, reference: u32) -> bool {
    app.world()
        .resource::<FarStock>()
        .desired
        .contains_key(&(0x70, reference))
}

fn set(app: &mut App, reference: u32, enabled: bool) {
    app.world_mut()
        .trigger(SetReferenceEnabled { reference, enabled });
    app.update();
}

#[test]
fn distant_enable_parent_and_opposite_follow_runtime_state_without_settled_replanning() {
    let fixture = fixture();
    let mut app = app(&fixture);
    app.update();
    assert!(
        desired(&app, 0x111),
        "enabled parent must provide a distant source route"
    );
    assert!(!desired(&app, 0x112));
    set(&mut app, 0x110, false);
    assert!(!desired(&app, 0x111));
    assert!(desired(&app, 0x112));
    set(&mut app, 0x110, true);
    assert!(desired(&app, 0x111));
    assert!(!desired(&app, 0x112));
    let generation = app.world().resource::<FarStock>().generation;
    let revision = app.world().resource::<ReferenceEnableState>().revision();
    for _ in 0..8 {
        app.update();
    }
    assert_eq!(app.world().resource::<FarStock>().generation, generation);
    assert_eq!(
        app.world().resource::<ReferenceEnableState>().revision(),
        revision
    );
}

#[test]
fn initially_disabled_distant_static_waits_for_enable_and_scripted_static_uses_state() {
    let fixture = fixture();
    let mut app = app(&fixture);
    app.update();
    assert!(!desired(&app, 0x109));
    assert!(
        desired(&app, 0x113),
        "a script attachment does not mean disabled"
    );
    set(&mut app, 0x109, true);
    assert!(desired(&app, 0x109));
    set(&mut app, 0x113, false);
    assert!(!desired(&app, 0x113));
    set(&mut app, 0x113, true);
    assert!(desired(&app, 0x113));
}

#[test]
fn runtime_inventory_separates_disabled_state_from_missing_visible_draw_paths() {
    let fixture = fixture();
    let mut app = app(&fixture);
    app.update();
    assert_eq!(
        reported_undrawn_paths(&app, fixture.directory.path()),
        (Some(2), Some(0))
    );
    let report: serde_json::Value = serde_json::from_slice(
        &std::fs::read(fixture.directory.path().join("cpu-spans.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        report["gauges"]["cluster/runtime_refused_hidden_by_state"].as_f64(),
        Some(2.0)
    );
    set(&mut app, 0x109, true);
    reported_undrawn_paths(&app, fixture.directory.path());
    let report: serde_json::Value = serde_json::from_slice(
        &std::fs::read(fixture.directory.path().join("cpu-spans.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        report["gauges"]["cluster/runtime_refused_hidden_by_state"].as_f64(),
        Some(1.0)
    );
}

#[test]
fn distant_runtime_sources_publish_current_state_before_scene_admission() {
    let fixture = fixture();
    for (reference, controller) in [(0x109, 0x109), (0x111, 0x110), (0x113, 0x113)] {
        let (key, row) = fixture
            .rows
            .iter()
            .find(|(_, row)| row.form_id == reference)
            .unwrap();
        let mut app = layer_app(&fixture, *key, row, false);
        reference_state::install(&mut app);
        app.world_mut()
            .resource_mut::<ReferenceEnableState>()
            .bind(fixture.package.enable_catalog().clone());
        app.world_mut()
            .resource_mut::<EngineConfig>()
            .cluster_own_statics = true;
        app.world_mut()
            .resource_mut::<FarStock>()
            .demand
            .as_mut()
            .unwrap()
            .center += IVec2::new(200, 0);
        if !row.initially_enabled {
            app.world_mut().trigger(SetReferenceEnabled {
                reference,
                enabled: true,
            });
        }
        app.update();
        let identity = (cell_world(*key).unwrap(), reference);
        let root = app
            .world()
            .resource::<FarStock>()
            .owners
            .get(&identity)
            .expect("enabled runtime-refused static must publish an ordinary source")
            .entity;
        assert!(
            app.world()
                .get::<crate::streaming::requests::SceneRequest>(root)
                .is_some()
        );
        // The complete reference was refused: its whole ordinary body is needed.
        assert!(
            app.world()
                .get::<crate::streaming::partial_stock::PartialStock>(root)
                .is_none()
        );
        app.world_mut().trigger(SetReferenceEnabled {
            reference: controller,
            enabled: false,
        });
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Hidden)
        );
        app.world_mut().trigger(SetReferenceEnabled {
            reference: controller,
            enabled: true,
        });
        assert_ne!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Hidden)
        );
    }
}
