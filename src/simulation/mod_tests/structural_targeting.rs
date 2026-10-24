//! Direct interaction chooses real structural matter over overlapping unbuilt plans.

use super::*;
use crate::content::{MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION};
use crate::core::quantity::{Area, Length};
use crate::spatial::VoxelBounds;
use crate::structural::{
    StructuralElementGeometry, add_structural_element, materialize_structural_element_for_test,
};

#[test]
fn direct_targeting_prefers_materialized_member_over_overlapping_inert_plan() {
    // Both a full-footprint panel and a thin supporting member must be
    // selectable without an unrealized plan intercepting the physical target.
    for cross_section in [1_000_000, 5_000] {
        let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
        let target = VoxelCoord::new(2, floor_y + 1, 0);
        for x in 1..=2 {
            set_generated_block_for_test(
                registries.world(),
                &mut state,
                VoxelCoord::new(x, floor_y + 1, 0),
                BlockStateId::AIR,
            );
        }
        let geometry = StructuralElementGeometry::new(
            VoxelBounds::new(target, VoxelCoord::new(3, floor_y + 2, 1)).unwrap(),
            Length::from_micrometers(1_000_000),
            Area::from_square_millimeters(cross_section),
        )
        .unwrap();
        let inert = add_structural_element(
            &registries,
            &mut state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            geometry,
            true,
        )
        .unwrap();
        let materialized = add_structural_element(
            &registries,
            &mut state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            geometry,
            true,
        )
        .unwrap();
        materialize_structural_element_for_test(&registries, &mut state, materialized, FORM_LOG);

        let direction = ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0).unwrap();
        let outcome = advance_control_tick_with_direct_input(
            &registries,
            &mut state,
            PlayerDirectControlInput::NEUTRAL
                .with_primary(PrimaryControlIntent::break_target(direction)),
        )
        .unwrap();
        assert_eq!(
            outcome.primary_action(),
            Some(&PrimaryActionOutcome::QueuedUntilSimulationBoundary)
        );
        assert_eq!(
            state
                .logistics()
                .pending_primary_break()
                .map(|pending| pending.element()),
            Some(materialized),
            "cross-section {cross_section} must target real matter, not an inert plan"
        );
        assert_ne!(materialized, inert);
    }
}
