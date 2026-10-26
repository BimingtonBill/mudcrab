// Included inside native_lod::tests.
mod draw_role_regressions {
    use super::*;

    fn coherent_role_inputs() -> (Inputs, [MainEntity; 9]) {
        let mut inputs = Inputs::default();
        let visible = std::array::from_fn(|index| {
            MainEntity::from(Entity::from_raw_u32(index as u32 + 1).unwrap())
        });

        // The first eight entities cover every overlap; the ninth is unrelated.
        // Distinct payloads expose an accidental lookup of another entity or role.
        for (mask, main) in visible[..8].iter().copied().enumerate() {
            let offset = mask as i32;
            if mask & 1 != 0 {
                inputs.near.insert(
                    main,
                    NativeLodCell {
                        world: 11,
                        cell: IVec2::new(offset, -offset),
                    },
                );
            }
            if mask & 2 != 0 {
                inputs.native.insert(
                    main,
                    NativeLodCell {
                        world: 23,
                        cell: IVec2::new(-offset, offset + 16),
                    },
                );
            }
            if mask & 4 != 0 {
                let block: BlockKey = (4, IVec2::new(offset * 4, -offset * 4));
                inputs.object_meshes.insert(main, block);
            }
        }

        // Publish exactly the union of the source maps, as extraction does.
        for (main, cell) in &inputs.near {
            inputs.roles.entry(*main).or_default().near = Some(*cell);
        }
        for (main, block) in &inputs.object_meshes {
            inputs.roles.entry(*main).or_default().block = Some(*block);
        }
        for (main, cell) in &inputs.native {
            inputs.roles.entry(*main).or_default().native = Some(*cell);
        }
        (inputs, visible)
    }

    #[test]
    fn draw_role_matches_source_map_priority_for_every_overlap_and_unrelated_entity() {
        let (inputs, visible) = coherent_role_inputs();

        for main in visible {
            // Independent oracle: never consult the merged inventory or helper.
            let expected = match (
                inputs.object_meshes.get(&main),
                inputs.native.get(&main),
                inputs.near.get(&main),
            ) {
                (Some(block), _, _) => Some(DrawRole::Block(*block)),
                (None, Some(cell), _) => Some(DrawRole::Native(*cell)),
                (None, None, Some(_)) => Some(DrawRole::Near),
                (None, None, None) => None,
            };
            let mut reads = 0;

            assert_eq!(
                draw_role(&inputs, main, &mut reads),
                expected,
                "draw role priority or entity identity changed for {main:?}"
            );
        }
    }

    #[test]
    fn draw_role_reads_one_map_per_visible_entity_including_unrelated_entities() {
        let (inputs, visible) = coherent_role_inputs();
        let mut reads = 37;
        let mut per_entity_reads = Vec::new();

        for main in visible {
            let before = reads;
            let _ = draw_role(&inputs, main, &mut reads);
            per_entity_reads.push(reads - before);
        }

        assert_eq!(
            per_entity_reads,
            vec![1; visible.len()],
            "every visible entity must use one merged-role lookup, even when absent"
        );
    }
}
