use super::*;

fn view() -> RetainedViewEntity {
    RetainedViewEntity {
        main_entity: Entity::PLACEHOLDER.into(),
        auxiliary_entity: Entity::PLACEHOLDER.into(),
        subview_index: 0,
    }
}

#[test]
fn disabled_audit_has_no_request_state_or_polling() {
    let audit = ShadowParityAudit::disabled();
    assert!(!audit.enabled());
    assert!(audit.0.is_none());
    assert_eq!(audit.request_frame(42), None);
}

#[test]
fn explicit_verification_paths_override_environment_without_reading_it() {
    let settings = ShadowReuseVerification {
        directory: "audit".into(),
        request: "shared request".into(),
    };
    assert_eq!(
        verification_paths(Some(&settings), || panic!(
            "explicit settings must not read legacy environment"
        )),
        Some((settings.directory.clone(), Some(settings.request.clone())))
    );
    assert_eq!(verification_paths(None, || None), None);
    assert_eq!(
        verification_paths(None, || Some(("legacy".into(), None))),
        Some(("legacy".into(), None))
    );
    assert_eq!(
        verification_paths(None, || Some((
            "legacy".into(),
            Some("legacy request".into())
        ))),
        Some(("legacy".into(), Some("legacy request".into())))
    );
    assert!(Audit::new(Path::new(""), None).is_err());
    assert!(Audit::new(Path::new("unused"), Some(PathBuf::new())).is_err());
}

#[test]
fn candidate_receipt_keeps_full_sorted_membership_and_bounded_log_samples() {
    let mut rows: Vec<_> = (0..1000)
        .map(|id| format!("stock_candidate main_entity={id} material=material-{id}"))
        .collect();
    rows.push("sampled_texture_candidate material=material-0 image_asset=image-4".into());
    rows.push("external_material_candidate material=material-8".into());
    rows.push("non_mesh_candidate command_id=7 carrier_main_entity=9 source_references=not_individually_identified".into());
    rows.push(rows[0].clone());
    let receipt = CandidateReceipt::from_rows(rows.clone());
    rows.reverse();
    assert_eq!(receipt.text, CandidateReceipt::from_rows(rows).text);
    assert!(receipt.text.contains("candidate_rows=1003\n"));
    assert!(receipt
        .text
        .contains("main_entity=999 material=material-999"));
    assert!(receipt.text.contains("candidate_set_not_isolated_culprit"));
    assert!(receipt.preview.contains("image_asset=image-4"));
    assert!(receipt.preview.contains("carrier_main_entity=9"));
    assert!(receipt.preview.chars().count() <= MAX_CANDIDATE_LOG_CHARS);
    assert!(!receipt.preview.contains("main_entity=999"));
}

#[test]
fn refusal_diagnostics_keep_unknown_stock_external_ids_and_missing_pairs() {
    let mut world = World::new();
    world.init_resource::<ErasedRenderAssets<crate::PreparedMaterial>>();
    world.init_resource::<super::super::ShadowCacheMaterialDependencies>();
    world.init_resource::<super::super::ShadowMaterialTypeNames>();
    world.init_resource::<ShadowStockInputs>();
    world.init_resource::<ShadowMapExternalInputs>();
    let stock_id = bevy_asset::AssetId::<crate::StandardMaterial>::default().untyped();
    let external_id = bevy_asset::AssetId::<crate::StandardMaterial>::invalid().untyped();
    let mut prepared = world.resource_mut::<ErasedRenderAssets<crate::PreparedMaterial>>();
    for id in [stock_id, external_id] {
        prepared.insert(
            id,
            crate::PreparedMaterial {
                binding: Default::default(),
                properties: Arc::new(Default::default()),
                shadow_cache_untracked_texture: true,
                shadow_cache_texture_views: None,
            },
        );
    }
    world
        .resource_mut::<super::super::ShadowMaterialTypeNames>()
        .register::<crate::StandardMaterial>();
    world.resource::<ShadowStockInputs>().set_caster(
        view(),
        Entity::PLACEHOLDER.into(),
        stock_id,
        Some(1),
        None,
        true,
    );
    let external = world.resource::<ShadowMapExternalInputs>();
    external.reset_materials(view(), 42);
    external.observe_material(
        view(),
        42,
        external_id,
        world
            .resource::<ErasedRenderAssets<crate::PreparedMaterial>>()
            .get(external_id)
            .unwrap(),
    );
    assert_eq!(
        world.resource::<ShadowStockInputs>().snapshot(view()),
        Err(super::super::Refusal::UnknownMaterial)
    );
    let (rows, ids) = refusal_material_rows(&world, view(), 42);
    assert_eq!(ids, HashSet::from_iter([stock_id, external_id]));
    let stock = rows
        .iter()
        .find(|row| row.contains("origin=stock"))
        .unwrap();
    assert!(stock.contains("unknown_id=true stored_pair=None prepared_revision=Some(1)"));
    assert!(stock.contains(&format!(
        "type_path={}",
        <crate::StandardMaterial as bevy_reflect::TypePath>::type_path()
    )));
    assert!(stock.contains("declaration_revision=Some(0)"));
    assert!(stock.contains("texture_flag=Some(true)"));
    assert!(stock.contains("caster_main_entities=["));
    let external = rows
        .iter()
        .find(|row| row.contains("origin=external"))
        .unwrap();
    assert!(external.contains("unknown_id=true stored_pair=None prepared_revision=Some(2)"));
    assert!(external
        .contains("caster_membership=view_material_closure_not_individual_source_reference"));
    // A diagnostic cannot repair the original refused admission.
    assert_eq!(
        world.resource::<ShadowStockInputs>().snapshot(view()),
        Err(super::super::Refusal::UnknownMaterial)
    );
}

#[test]
fn candidate_snapshot_joins_stock_and_external_materials_and_rejects_stale_closure() {
    let stock = ShadowStockInputs::default();
    let external = ShadowMapExternalInputs::default();
    let material = bevy_asset::AssetId::<crate::StandardMaterial>::default().untyped();
    let other = bevy_asset::AssetId::<crate::StandardMaterial>::invalid().untyped();
    stock.set_caster(
        view(),
        Entity::PLACEHOLDER.into(),
        material,
        Some(2),
        Some(0),
        false,
    );
    external.reset_materials(view(), 42);
    external.publish(view(), 42, 3, true);
    {
        let mut inputs = lock(&external.1);
        let state = inputs.views.get_mut(&view()).unwrap();
        state.materials.extend([material, other]);
        state.revisions.insert(material, (2, 0));
        state.revisions.insert(other, (4, 0));
    }
    let (rows, materials) = candidate_membership(&stock, &external, view(), 42).unwrap();
    assert_eq!(materials, HashSet::from_iter([material, other]));
    assert_eq!(
        rows.iter()
            .filter(|row| row.starts_with("stock_candidate "))
            .count(),
        1
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row.starts_with("external_material_candidate "))
            .count(),
        2
    );
    assert!(candidate_membership(&stock, &external, view(), 43).is_err());
    lock(&external.1)
        .views
        .get_mut(&view())
        .unwrap()
        .revisions
        .remove(&other);
    assert_eq!(
        candidate_membership(&stock, &external, view(), 42).unwrap_err(),
        "diagnostic_external_material_revision_missing"
    );
    stock.refuse_view(view());
    assert_eq!(
        candidate_membership(&stock, &external, view(), 42).unwrap_err(),
        "diagnostic_stock_inputs_incomplete"
    );
}

#[test]
fn diagnostic_bindings_refuse_opaque_or_unretained_materials() {
    let mut material = crate::PreparedMaterial {
        binding: Default::default(),
        properties: Arc::new(Default::default()),
        shadow_cache_untracked_texture: false,
        shadow_cache_texture_views: None,
    };
    assert_eq!(
        diagnostic_bindings(&material).unwrap_err(),
        "diagnostic_texture_bindings_not_retained"
    );
    material.shadow_cache_texture_views = Some(Vec::new());
    assert!(diagnostic_bindings(&material).unwrap().is_empty());
    material.shadow_cache_untracked_texture = true;
    assert_eq!(
        diagnostic_bindings(&material).unwrap_err(),
        "diagnostic_opaque_material_bindings"
    );
}

#[test]
fn diagnostic_command_candidates_require_complete_current_frame_certificates() {
    let stock = ShadowStockInputs::default();
    let external = ShadowMapExternalInputs::default();
    external.reset_materials(view(), 42);
    external.publish(view(), 42, 3, true);
    {
        let mut producers = lock(&external.2);
        producers.active[0] = true;
        producers.certified[0] = Some(super::super::CertifiedShadowDraw {
            command: core::any::TypeId::of::<()>(),
            views: HashMap::default(),
        });
    }
    assert_eq!(
        candidate_membership(&stock, &external, view(), 42).unwrap_err(),
        "diagnostic_external_command_incomplete"
    );
    for (frame, complete) in [(41, true), (42, false), (42, true)] {
        lock(&external.2).certified[0]
            .as_mut()
            .unwrap()
            .views
            .insert(
                view(),
                super::super::ExternalInput {
                    frame,
                    revision: 8,
                    complete,
                },
            );
        let result = candidate_membership(&stock, &external, view(), 42);
        if frame == 42 && complete {
            assert!(result
                .unwrap()
                .0
                .iter()
                .any(|row| row.starts_with("external_command slot=0 ")
                    && row.contains("revision=8 complete=true")));
        } else {
            assert_eq!(
                result.unwrap_err(),
                "diagnostic_external_command_incomplete"
            );
        }
    }
}

#[test]
fn shadow_parity_request_latches_one_actual_frame_and_independent_mode_stays_usable() {
    let mut request = RequestGate::default();
    assert_eq!(request.poll(41).unwrap(), Some(41));
    assert_eq!(request.poll(42).unwrap(), Some(42));
    request.sentinel = Some(PathBuf::from("owned-sentinel"));
    assert_eq!(request.select(43, false), None);
    // Other sites in this same render frame must not poll again.
    assert_eq!(request.poll(43).unwrap(), None);
    assert_eq!(request.select(44, true), Some(44));
    assert_eq!(request.poll(45).unwrap(), Some(44));
}

#[test]
fn shadow_parity_strips_padding_and_compares_background_exactly() {
    let a = logical_rows(&[0, 0, 0, 0, 99, 99, 99, 99, 1, 2, 3, 4], 4, 8, 2).unwrap();
    let b = logical_rows(&[0, 0, 0, 0, 17, 17, 17, 17, 1, 2, 3, 4], 4, 8, 2).unwrap();
    assert_eq!(a, [0, 0, 0, 0, 1, 2, 3, 4]);
    assert_eq!(
        compare_bytes(&a, &b).unwrap(),
        Difference {
            bytes: 0,
            first: None
        }
    );
    let mut mutated = a.clone();
    mutated[0] = 1; // A clear/background byte is part of the proof.
    assert_eq!(
        compare_bytes(&mutated, &b).unwrap(),
        Difference {
            bytes: 1,
            first: Some(0)
        }
    );
}

#[test]
fn shadow_parity_rejects_missing_rows_and_different_lengths() {
    assert!(logical_rows(&[0; 11], 4, 8, 2).is_err());
    assert!(logical_rows(&[0; 12], 8, 4, 2).is_err());
    assert!(logical_rows(&[], 0, 0, 0).is_err());
    assert!(compare_bytes(&[0], &[]).is_err());
}

#[test]
#[should_panic(expected = "assertion")]
fn shadow_parity_mutated_saved_a_must_fail_zero_difference_acceptance() {
    let mut saved_a = vec![0_u8; 16];
    let forced_b = saved_a.clone();
    saved_a[12] ^= 1;
    assert_eq!(compare_bytes(&saved_a, &forced_b).unwrap().bytes, 0);
}

#[test]
fn shadow_parity_missing_late_or_skipped_commands_never_complete() {
    assert!(!completion_ready(true, false, true, false, true));
    assert!(!completion_ready(true, true, true, true, true));
    assert!(!completion_ready(true, true, true, false, false));
    assert!(!completion_ready(false, true, true, false, true));
    assert!(completion_ready(true, true, true, false, true));
    assert!(completion_ready(true, false, false, false, true));
}
