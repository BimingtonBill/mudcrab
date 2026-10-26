// Included inside native_lod::tests after the behavior-preserving update extraction.
mod far_native_policy_regressions {
    use super::*;

    #[test]
    fn owned_window_changes_do_not_revise_effective_policy() {
        let mut policy = FarNativePolicy {
            revision: 27,
            window: Some((7, IVec2::new(3, -4), 2)),
            owned: true,
        };

        for window in [
            Some((7, IVec2::new(4, -4), 2)),
            Some((7, IVec2::new(-4, 5), 2)),
            Some((8, IVec2::new(-4, 5), 2)),
            None,
            Some((0, IVec2::ZERO, 0)),
            Some((0, IVec2::ZERO, 3)),
        ] {
            policy.update(window, true);
            assert_eq!(
                policy.revision, 27,
                "owned traversal changed revision for window {window:?}"
            );
        }
    }

    #[test]
    fn owned_updates_retain_every_latest_window_including_none() {
        let mut policy = FarNativePolicy {
            revision: 27,
            window: Some((7, IVec2::new(3, -4), 2)),
            owned: true,
        };

        for window in [
            Some((7, IVec2::new(4, -4), 2)),
            Some((8, IVec2::new(-4, 5), 3)),
            None,
            Some((0, IVec2::ZERO, 0)),
        ] {
            policy.update(window, true);
            assert_eq!(policy.window, window);
            assert!(policy.owned);
        }
    }

    #[test]
    fn disabling_ownership_revises_once_and_uses_latest_stored_window() {
        let old = Some((7, IVec2::new(3, -4), 2));
        let latest = Some((8, IVec2::new(-4, 5), 1));
        let mut policy = FarNativePolicy {
            revision: 27,
            window: old,
            owned: true,
        };
        policy.update(None, true);
        policy.update(latest, true);
        let before_disable = policy.revision;

        policy.update(latest, false);

        assert_eq!(policy.revision, before_disable + 1);
        assert_eq!(policy.window, latest);
        assert!(!policy.owned);
        assert!(policy.cluster_inside_grid(8, IVec2::new(-5, 6)));
        assert!(!policy.cluster_inside_grid(8, IVec2::new(-6, 6)));
        assert!(!policy.cluster_inside_grid(7, IVec2::new(3, -4)));
        assert!(suppress_unqualified_far_cluster(
            policy.cluster_inside_grid(7, IVec2::new(3, -4)),
            false,
            None,
            true,
        ));

        policy.update(latest, false);
        assert_eq!(policy.revision, before_disable + 1);
    }

    #[test]
    fn disabling_ownership_after_window_disappears_does_not_restore_stale_grid() {
        let mut policy = FarNativePolicy {
            revision: 27,
            window: Some((7, IVec2::new(3, -4), 2)),
            owned: true,
        };
        policy.update(None, true);
        let before_disable = policy.revision;

        policy.update(None, false);

        assert_eq!(policy.revision, before_disable + 1);
        assert_eq!(policy.window, None);
        assert!(!policy.cluster_inside_grid(7, IVec2::new(3, -4)));
        assert!(suppress_unqualified_far_cluster(
            policy.cluster_inside_grid(7, IVec2::new(3, -4)),
            true,
            Some(false),
            false,
        ));
    }

    #[test]
    fn unowned_revision_tracks_real_window_changes_only() {
        let initial = Some((7, IVec2::new(3, -4), 2));
        let crossed = Some((7, IVec2::new(4, -4), 2));
        let resized = Some((7, IVec2::new(4, -4), 3));
        let other_world = Some((8, IVec2::new(4, -4), 3));
        let mut policy = FarNativePolicy::default();

        for (window, expected_revision) in [
            (None, 0),
            (initial, 1),
            (initial, 1),
            (crossed, 2),
            (crossed, 2),
            (resized, 3),
            (other_world, 4),
            (other_world, 4),
            (None, 5),
            (None, 5),
            (initial, 6),
        ] {
            policy.update(window, false);
            assert_eq!(
                policy.revision, expected_revision,
                "unexpected unowned revision for {window:?}"
            );
            assert_eq!(policy.window, window);
            assert!(!policy.owned);
        }
    }

    #[test]
    fn ownership_toggles_revise_once_even_without_a_window() {
        for window in [None, Some((7, IVec2::new(3, -4), 2))] {
            let mut policy = FarNativePolicy {
                revision: 0,
                window,
                owned: false,
            };

            for (owned, expected_revision) in
                [(true, 1), (true, 1), (false, 2), (false, 2), (true, 3)]
            {
                policy.update(window, owned);
                assert_eq!(policy.revision, expected_revision);
                assert_eq!(policy.owned, owned);
                assert_eq!(policy.window, window);
                assert_eq!(policy.cluster_inside_grid(99, IVec2::ZERO), owned);
            }
        }
    }

    #[test]
    fn simultaneous_window_and_ownership_changes_revise_once() {
        let mut policy = FarNativePolicy::default();
        let initial = Some((7, IVec2::new(3, -4), 2));
        let latest = Some((8, IVec2::new(-4, 5), 1));

        policy.update(initial, true);
        assert_eq!(policy.revision, 1);
        assert_eq!(policy.window, initial);
        assert!(policy.owned);

        policy.update(latest, false);
        assert_eq!(policy.revision, 2);
        assert_eq!(policy.window, latest);
        assert!(!policy.owned);
        assert!(policy.cluster_inside_grid(8, IVec2::new(-4, 5)));
        assert!(!policy.cluster_inside_grid(7, IVec2::new(3, -4)));
    }

    fn direct_window_oracle(
        window: Option<(u32, IVec2, i32)>,
        owned: bool,
        world: u32,
        cell: IVec2,
    ) -> bool {
        if owned {
            return true;
        }
        let Some((window_world, center, radius)) = window else {
            return false;
        };
        // Absolute coordinate bounds are independent of the native delta/abs predicate.
        let radius = i64::from(radius);
        window_world == world
            && (i64::from(center.x) - radius..=i64::from(center.x) + radius)
                .contains(&i64::from(cell.x))
            && (i64::from(center.y) - radius..=i64::from(center.y) + radius)
                .contains(&i64::from(cell.y))
    }

    #[test]
    fn native_grid_and_suppression_match_independent_window_oracle() {
        let coordinates = [
            i32::MIN,
            i32::MIN + 1,
            i32::MIN + 2,
            i32::MIN + 3,
            -7,
            -6,
            -5,
            -4,
            -3,
            -2,
            -1,
            0,
            1,
            2,
            3,
            4,
            5,
            6,
            7,
            i32::MAX - 3,
            i32::MAX - 2,
            i32::MAX - 1,
            i32::MAX,
        ];
        for window in [
            None,
            Some((7, IVec2::ZERO, 0)),
            Some((7, IVec2::new(3, -4), 2)),
            Some((8, IVec2::new(-3, 4), 2)),
            Some((0, IVec2::new(-4, -4), 2)),
            Some((u32::MAX, IVec2::new(i32::MIN, i32::MAX), 2)),
            Some((7, IVec2::ZERO, i32::MAX)),
        ] {
            for owned in [false, true] {
                let policy = FarNativePolicy {
                    revision: 0,
                    window,
                    owned,
                };
                for world in [0, 7, 8, u32::MAX] {
                    for x in coordinates {
                        for y in coordinates {
                            let cell = IVec2::new(x, y);
                            let expected_inside = direct_window_oracle(window, owned, world, cell);
                            let actual_inside = policy.cluster_inside_grid(world, cell);
                            assert_eq!(
                                actual_inside, expected_inside,
                                "window={window:?}, owned={owned}, world={world}, cell={cell:?}"
                            );
                            for tree in [false, true] {
                                for base_flag in [None, Some(false), Some(true)] {
                                    for visible_when_distant in [false, true] {
                                        // Spell out the contributor truth table independently.
                                        let expected_suppressed = if expected_inside {
                                            false
                                        } else {
                                            match (tree, base_flag, visible_when_distant) {
                                                (true, _, _) => true,
                                                (false, Some(false), _) => false,
                                                (false, None | Some(true), vwd) => vwd,
                                            }
                                        };
                                        assert_eq!(
                                            suppress_unqualified_far_cluster(
                                                actual_inside,
                                                tree,
                                                base_flag,
                                                visible_when_distant,
                                            ),
                                            expected_suppressed,
                                            "window={window:?}, owned={owned}, world={world}, \
                                             cell={cell:?}, tree={tree}, base={base_flag:?}, \
                                             visible_when_distant={visible_when_distant}"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
