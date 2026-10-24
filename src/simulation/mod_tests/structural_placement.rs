//! Pointed construction distinguishes slender post overlap from full-cell obstruction.

use super::*;
use crate::content::{FORM_CHIP, MATERIAL_STONE};
use crate::logistics::{PlayerActiveSelection, validate_set_active_selection};
use crate::material::MaterialComposition;
use crate::matter::calculate_matter_accounting;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, insert_surface_resource_for_test};

#[test]
fn direct_timber_post_can_share_a_cell_with_loose_matter() {
    for occupied in [false, true] {
        let (registries, mut state, foot_y) = generated_grounded_player_at(0, 0);
        let ground = VoxelCoord::new(1, foot_y - 1, 0);
        let destination = VoxelCoord::new(1, foot_y, 0);
        set_generated_block_for_test(registries.world(), &mut state, ground, BLOCK_STATE_STONE);
        for y in foot_y..=foot_y + 1 {
            set_generated_block_for_test(
                registries.world(),
                &mut state,
                VoxelCoord::new(1, y, 0),
                BlockStateId::AIR,
            );
        }
        let loose = occupied.then(|| {
            let spec = SurfaceResourceSpec::new(
                SurfaceResourceKind::Drop,
                destination,
                CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                Mass::from_milligrams(50_000),
                Temperature::from_millikelvin(293_150),
                MaterialComposition::pure(MATERIAL_STONE),
            )
            .unwrap();
            insert_surface_resource_for_test(&registries, &mut state, spec).unwrap()
        });
        let carried = state.logistics().player().unwrap().carried_stockpile();
        let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            timber,
            Mass::from_milligrams(6_500_000),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
        validate_set_active_selection(
            &registries,
            &state,
            Some(PlayerActiveSelection::material(timber)),
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
        let direction = ViewDirection::new(600_000, -800_000, 0).unwrap();
        assert_eq!(
            crate::world::raycast_player_block(&registries, &state, direction)
                .unwrap()
                .map(|target| (target.voxel(), target.face())),
            Some((ground, crate::spatial::CubeFace::Top))
        );
        let matter_before = calculate_matter_accounting(&state).unwrap().total();
        let surface_before = state.surface().clone();
        let mut outcome = advance_control_tick_with_direct_input(
            &registries,
            &mut state,
            PlayerDirectControlInput::NEUTRAL
                .with_secondary(SecondaryControlIntent::use_active(direction)),
        )
        .unwrap();
        assert_eq!(
            outcome.secondary_action(),
            Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
        );
        for _ in 1..CONTROL_TICKS_PER_SIMULATION_TICK {
            outcome = advance_control_tick(&registries, &mut state).unwrap();
        }
        assert!(
            matches!(
                outcome.secondary_action(),
                Some(SecondaryActionOutcome::StructuralConstructionStarted(_))
            ),
            "slender post did not start construction: {:?}",
            outcome.secondary_action()
        );
        assert_eq!(state.surface(), &surface_before);
        if let Some(resource) = loose {
            assert_eq!(
                state.surface().get(resource).unwrap().remaining_mass(),
                Mass::from_milligrams(50_000)
            );
        }
        assert_eq!(
            calculate_matter_accounting(&state).unwrap().total(),
            matter_before
        );
        assert_eq!(
            crate::core::state::validate_loaded_state(&registries, &state),
            Ok(())
        );
    }
}
