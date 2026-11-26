//! Actual nearby water, not a label, infinite refill, or remote hydration oracle.

use super::super::{HabitatKind, habitat_at, terrain_height};
use super::*;

use crate::content::{BLOCK_STATE_STONE, FLUID_WATER, build_registries};
use crate::core::quantity::Mass;
use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::equipment::EquipmentId;
use crate::fluid::{FluidValidationError, calculate_fluid_volume_accounting};
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_control_tick;
use crate::survival::{available_local_drink_sources, validate_drink};
use crate::world::{
    BlockExcavationError, BlockExcavationRequest, BlockStateId, ChunkCoord,
    available_visible_generated_water, validate_generate_chunk,
    validate_generate_player_neighborhood, validate_start_player_block_excavation,
};

#[test]
fn a_natural_spring_retains_its_last_collidable_stone_footing() {
    use crate::content::MINING_METHOD_HAND_PICK;
    use crate::spatial::{VIEW_DIRECTION_PARTS_PER_MILLION, ViewDirection};

    let (registries, mut state, position) = generated_water_neighborhood();
    let store = state.logistics().fluid_stores_at(position).next().unwrap();
    let original_player = state.logistics().player().unwrap().position();
    let floor = VoxelCoord::new(position.x(), position.y() - 1, position.z());
    let full = state
        .world()
        .represented_mass_at(registries.world(), floor)
        .unwrap();
    crate::logistics::relocate_player_for_test(&mut state, position);
    let down = ViewDirection::new(0, -VIEW_DIRECTION_PARTS_PER_MILLION, 0).unwrap();
    let dig = |mass| {
        BlockExcavationRequest::new(down, MINING_METHOD_HAND_PICK, EquipmentId::new(1), mass)
    };
    assert_eq!(
        crate::world::raycast_player_block(&registries, &state, down)
            .unwrap()
            .unwrap()
            .voxel(),
        floor
    );
    let before = state.clone();
    assert_eq!(
        validate_start_player_block_excavation(&registries, &state, dig(full)).err(),
        Some(BlockExcavationError::NaturalSpringRequiresFloor {
            target: floor,
            store
        })
    );
    assert_eq!(state, before);

    let revision = state.world().revision();
    state.world_state_mut().apply_excavated_cell_mass(
        revision,
        revision + 1,
        floor,
        BLOCK_STATE_STONE,
        full,
        Mass::from_milligrams(1),
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(
        validate_start_player_block_excavation(&registries, &state, dig(Mass::from_milligrams(1)),)
            .err(),
        Some(BlockExcavationError::NaturalSpringRequiresFloor {
            target: floor,
            store
        })
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
            .unwrap()
            .into_state(&registries)
            .unwrap(),
        state
    );

    crate::logistics::relocate_player_for_test(&mut state, original_player);
    let revision = state.world().revision();
    state.world_state_mut().apply_excavated_cell_mass(
        revision,
        revision + 1,
        floor,
        BLOCK_STATE_STONE,
        Mass::from_milligrams(1),
        Mass::ZERO,
    );
    assert_eq!(
        validate_loaded_state(&registries, &state),
        Err(StateValidationError::Fluid(
            FluidValidationError::InvalidNaturalSpring { store }
        ))
    );
}

fn generated_water_neighborhood() -> (crate::registry::Registries, AppState, VoxelCoord) {
    let registries = build_registries();
    for number in 0..80_u64 {
        let seed = super::super::WorldGenerationSeed::new(number);
        let mut state = AppState::new();
        validate_generate_chunk(
            &registries,
            &state,
            seed,
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE,
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
        let spawn = VoxelCoord::new(0, terrain_height(seed, 0, 0) + 1, 0);
        validate_admit_player(&registries, &state, spawn)
            .unwrap()
            .commit(&mut state)
            .unwrap();
        let expansion = validate_generate_player_neighborhood(&registries, &state)
            .unwrap()
            .expect("unseen adjacent columns must be generated");
        let duplicate = expansion.clone();
        expansion.commit(&mut state).unwrap();
        let source = state
            .logistics()
            .fluid_store_locations()
            .find_map(|(store, position)| {
                state.fluid().get_store(store).and_then(|record| {
                    record
                        .contents()
                        .is_some_and(|water| water.fluid() == FLUID_WATER)
                        .then_some(position)
                })
            });
        if let Some(position) = source {
            let before = state.clone();
            assert!(matches!(
                duplicate.commit(&mut state),
                Err(super::super::ChunkGenerationCommitError::StaleWorldRevision { .. })
            ));
            assert_eq!(
                state, before,
                "a repeated expansion cannot refill a wet district"
            );
            return (registries, state, position);
        }
    }
    panic!("a new generated wilderness must have distinct finite water sources");
}

#[test]
fn full_cell_construction_cannot_entomb_a_natural_basin_even_when_dry() {
    use crate::content::{
        BASIC_TIMBER_COVER_PANEL_AREA, BASIC_TIMBER_COVER_PANEL_THICKNESS, FORM_BOARD,
        MATERIAL_WOOD, STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
    };
    use crate::inventory::{deposit_lot_for_test, project_material_stacks};
    use crate::logistics::{project_player_standing_space, relocate_player_for_test};
    use crate::material::CommodityKey;
    use crate::spatial::VoxelBounds;
    use crate::structural::{
        GroundedStructuralElementPlan, PlayerGroundedStructuralConstructionError,
        PlayerStructuralConstructionError, StructuralConstructionError, StructuralElementGeometry,
        validate_plan_player_grounded_structural_element,
        validate_start_player_grounded_structural_construction_from_stack,
    };

    let (registries, mut state, position) = generated_water_neighborhood();
    let store = state.logistics().fluid_stores_at(position).next().unwrap();
    let neighbor = [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .map(|(dx, dz)| VoxelCoord::new(position.x() + dx, position.y(), position.z() + dz))
        .find(|&at| project_player_standing_space(&registries, &state, at))
        .expect("spring fixture must have a reachable standing cell");
    relocate_player_for_test(&mut state, neighbor);
    let carried = state.logistics().player().unwrap().carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        Mass::from_milligrams(6_500_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let plan = GroundedStructuralElementPlan::new(
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        StructuralElementGeometry::new(
            VoxelBounds::new(
                position,
                VoxelCoord::new(position.x() + 1, position.y() + 1, position.z() + 1),
            )
            .unwrap(),
            BASIC_TIMBER_COVER_PANEL_THICKNESS,
            BASIC_TIMBER_COVER_PANEL_AREA,
        )
        .unwrap(),
    );
    for dry in [false, true] {
        let mut candidate = state.clone();
        if dry {
            let volume = candidate.fluid().get_store(store).unwrap().stored_volume();
            let receiver = crate::fluid::add_fluid_store(&mut candidate, volume).unwrap();
            crate::logistics::validate_place_fluid_store(&candidate, receiver, position)
                .unwrap()
                .commit(&mut candidate)
                .unwrap();
            let accounting = calculate_fluid_volume_accounting(&candidate).unwrap();
            relocate_player_for_test(&mut candidate, position);
            let _ = crate::fluid::validate_transfer_fluid_between_local_stores(
                &registries,
                &candidate,
                store,
                receiver,
                volume,
            )
            .unwrap()
            .commit(&mut candidate)
            .unwrap();
            relocate_player_for_test(&mut candidate, neighbor);
            assert_eq!(
                calculate_fluid_volume_accounting(&candidate).unwrap(),
                accounting
            );
        }
        assert_eq!(
            candidate
                .fluid()
                .get_store(store)
                .unwrap()
                .stored_volume()
                .is_zero(),
            dry
        );
        assert_eq!(validate_loaded_state(&registries, &candidate), Ok(()));
        let stack = project_material_stacks(&registries, &candidate, carried)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let before = candidate.clone();
        let result = validate_start_player_grounded_structural_construction_from_stack(
            &registries,
            &candidate,
            STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
            plan,
            &stack,
        );
        let element =
            validate_plan_player_grounded_structural_element(&registries, &candidate, plan)
                .unwrap()
                .element();
        assert_eq!(
            result.err(),
            Some(PlayerGroundedStructuralConstructionError::Construction(
                PlayerStructuralConstructionError::Material(
                    StructuralConstructionError::NaturalSpringOccupied {
                        element,
                        store,
                        voxel: position
                    }
                )
            )),
            "a natural basin claims its cell even when dry: {dry}"
        );
        assert_eq!(candidate, before);

        // Narrow members do not block access to the basin and do not acquire its water.
        let narrow = GroundedStructuralElementPlan::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            StructuralElementGeometry::new(
                VoxelBounds::new(
                    position,
                    VoxelCoord::new(position.x() + 1, position.y() + 1, position.z() + 1),
                )
                .unwrap(),
                crate::core::quantity::Length::from_micrometers(2_000_000),
                crate::core::quantity::Area::from_square_millimeters(5_000),
            )
            .unwrap(),
        );
        let matter = crate::matter::calculate_matter_accounting(&candidate)
            .unwrap()
            .total();
        let accounting = calculate_fluid_volume_accounting(&candidate).unwrap();
        let work = validate_start_player_grounded_structural_construction_from_stack(
            &registries,
            &candidate,
            STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
            narrow,
            &stack,
        )
        .unwrap()
        .commit(&mut candidate)
        .unwrap();
        while candidate.tick() < work.completes_at() {
            let _ = crate::simulation::advance_tick(&registries, &mut candidate).unwrap();
        }
        assert!(!candidate.structures().blocks_full_cell(position));
        assert_eq!(
            crate::matter::calculate_matter_accounting(&candidate)
                .unwrap()
                .total(),
            matter
        );
        assert_eq!(
            calculate_fluid_volume_accounting(&candidate).unwrap(),
            accounting
        );
        assert_eq!(validate_loaded_state(&registries, &candidate), Ok(()));
        let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &candidate)).unwrap();
        assert_eq!(
            serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
                .unwrap()
                .into_state(&registries)
                .unwrap(),
            candidate
        );
    }
}

#[test]
fn trusted_load_rejects_a_full_cell_member_inside_a_natural_basin() {
    use crate::content::{
        BASIC_TIMBER_COVER_PANEL_AREA, BASIC_TIMBER_COVER_PANEL_THICKNESS, FORM_BOARD,
        MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
    };
    use crate::spatial::VoxelBounds;
    use crate::structural::{
        StructuralElementGeometry, add_structural_element, materialize_structural_element_for_test,
    };

    let (registries, mut state, position) = generated_water_neighborhood();
    let store = state.logistics().fluid_stores_at(position).next().unwrap();
    let at = VoxelCoord::new(position.x(), 100, position.z());
    crate::world::set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(at.x(), 99, at.z()),
        BLOCK_STATE_STONE,
    );
    let bounds = VoxelBounds::new(at, VoxelCoord::new(at.x() + 1, at.y() + 1, at.z() + 1)).unwrap();
    let member = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        StructuralElementGeometry::new(
            bounds,
            BASIC_TIMBER_COVER_PANEL_THICKNESS,
            BASIC_TIMBER_COVER_PANEL_AREA,
        )
        .unwrap(),
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, member, FORM_BOARD);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let geometry = &mut encoded["state"]["systems"]["structures"]["elements"]
        [member.value().to_string()]["configuration"]["geometry"];
    geometry["bounds"]["min"] = serde_json::to_value(position).unwrap();
    geometry["bounds"]["max_exclusive"] = serde_json::to_value(VoxelCoord::new(
        position.x() + 1,
        position.y() + 1,
        position.z() + 1,
    ))
    .unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded).unwrap();
    assert_eq!(
        decoded.into_state(&registries),
        Err(crate::persistence::LoadError::InvalidState(
            StateValidationError::Fluid(FluidValidationError::InvalidNaturalSpring { store })
        ))
    );
}

#[test]
fn regional_spring_is_real_finite_water_and_drinking_preserves_total_volume() {
    let (registries, mut state, position) = generated_water_neighborhood();
    let store = state
        .logistics()
        .fluid_stores_at(position)
        .next()
        .expect("natural water is located in logistics");
    let original = state.fluid().get_store(store).unwrap().stored_volume();
    assert!(!original.is_zero());
    assert!(state.world().has_generated_terrain_at(position));
    assert_eq!(state.world().block_at(position), Some(BlockStateId::AIR));
    assert_eq!(
        state.world().block_at(VoxelCoord::new(
            position.x(),
            position.y() - 1,
            position.z()
        )),
        Some(BLOCK_STATE_STONE)
    );
    assert!(
        matches!(
            habitat_at(
                state.world().generation_profile().unwrap().seed(),
                position.x(),
                position.z()
            ),
            HabitatKind::DampGrove | HabitatKind::MixedWoodland
        ),
        "dry scrub must not spontaneously provide surface water"
    );
    crate::logistics::relocate_player_for_test(&mut state, position);
    assert_eq!(
        available_visible_generated_water(&registries, &state)
            .into_iter()
            .map(|source| (source.store(), source.position()))
            .collect::<Vec<_>>(),
        vec![(store, position)]
    );
    assert!(
        available_local_drink_sources(&registries, &state).any(|source| source.store() == store)
    );
    let initial = calculate_fluid_volume_accounting(&state).unwrap();
    let amount = registries
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume();
    let finished = validate_drink(&registries, &state, store, amount)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    for _ in 0..(72 * 20) {
        if state.player_work().active().is_none() {
            break;
        }
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert!(
        state.player_work().active().is_none(),
        "a real drink must finish"
    );
    assert_eq!(
        state.fluid().get_store(store).unwrap().stored_volume(),
        original.checked_sub(finished.volume()).unwrap()
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&state).unwrap(),
        initial,
        "every consumed microliter transfers to survival custody"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
    // The spring can lie on the edge of the generated neighborhood. Ask
    // for the original footprint, not previously unexplored outer chunks.
    let seed = state.world().generation_profile().unwrap().seed();
    crate::logistics::relocate_player_for_test(
        &mut state,
        VoxelCoord::new(0, terrain_height(seed, 0, 0) + 1, 0),
    );
    assert!(
        validate_generate_player_neighborhood(&registries, &state)
            .unwrap()
            .is_none(),
        "revisiting generated land cannot refill its water"
    );
}

#[test]
fn water_is_not_a_remote_resource_or_visible_through_rock() {
    let (registries, mut state, position) = generated_water_neighborhood();
    let store = state.logistics().fluid_stores_at(position).next().unwrap();
    crate::logistics::relocate_player_for_test(&mut state, position);
    assert!(
        available_visible_generated_water(&registries, &state)
            .iter()
            .any(|source| source.store() == store)
    );
    let delta = if position.x().rem_euclid(16) <= 11 {
        1
    } else {
        -1
    };
    let observer = VoxelCoord::new(position.x() + 2 * delta, position.y(), position.z());
    let wall = VoxelCoord::new(position.x() + delta, position.y(), position.z());
    for voxel in [
        observer,
        wall,
        VoxelCoord::new(observer.x(), observer.y() + 1, observer.z()),
    ] {
        crate::world::set_generated_block_for_test(
            registries.world(),
            &mut state,
            voxel,
            BlockStateId::AIR,
        );
    }
    crate::logistics::relocate_player_for_test(&mut state, observer);
    assert!(
        available_visible_generated_water(&registries, &state)
            .iter()
            .any(|source| source.store() == store)
    );
    crate::world::set_generated_block_for_test(
        registries.world(),
        &mut state,
        wall,
        BLOCK_STATE_STONE,
    );
    assert!(
        available_visible_generated_water(&registries, &state)
            .iter()
            .all(|source| source.store() != store),
        "a solid intervening rock blocks discovery without deleting water"
    );
}

#[test]
fn competing_fluid_allocation_cannot_partially_commit_a_spring_neighborhood() {
    let (registries, grown, _) = generated_water_neighborhood();
    let seed = grown.world().generation_profile().unwrap().seed();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    validate_admit_player(
        &registries,
        &state,
        VoxelCoord::new(0, terrain_height(seed, 0, 0) + 1, 0),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let pending = validate_generate_player_neighborhood(&registries, &state)
        .unwrap()
        .unwrap();
    crate::fluid::validate_generate_fluid_store(
        &registries,
        &state,
        GeneratedFluidStoreSpec::new(
            Volume::from_microliters(200_000),
            FLUID_WATER,
            Volume::from_microliters(200_000),
            Temperature::from_millikelvin(293_150),
        ),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let before = state.clone();
    assert!(matches!(
        pending.commit(&mut state),
        Err(super::super::ChunkGenerationCommitError::StaleFluidRevision { .. })
    ));
    assert_eq!(
        state, before,
        "stale fluid custody must block every planned chunk and source"
    );
    assert_eq!(state.world().chunks().len(), 1);
}

#[test]
fn candidate_springs_do_not_flood_depleted_living_roots() {
    use crate::content::{FORM_FOOD, MATERIAL_BERRIES};
    use crate::core::quantity::Mass;
    use crate::core::time::SimulationTick;
    use crate::material::{CommodityKey, MaterialComposition};
    use crate::surface::{
        SurfaceResourceKind, SurfaceResourceLifecycle, SurfaceResourceSpec,
        validate_create_surface_resources,
    };

    let registries = build_registries();
    let pristine = AppState::new();
    let (profile, chunk, position) = (0..64_u64)
        .find_map(|value| {
            let profile = WorldGenerationProfile::new(
                super::super::WorldGenerationSeed::new(value),
                BLOCK_STATE_STONE,
            );
            let chunk = super::super::build_generated_chunk(
                registries.world(),
                profile,
                ChunkCoord::new(1, 0),
            )
            .ok()?;
            seeded_spring_spec(&pristine, profile, &chunk, &[])
                .map(|(position, _)| (profile, chunk, position))
        })
        .expect("bounded seeds expose a natural spring candidate");

    let mut occupied = AppState::new();
    let berry_mass = Mass::from_milligrams(200_000);
    let source = SurfaceResourceSpec::new(
        SurfaceResourceKind::Opportunity,
        position,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        berry_mass,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_BERRIES),
    )
    .unwrap()
    .with_perennial_reserve(Mass::from_milligrams(berry_mass.milligrams() / 2));
    validate_create_surface_resources(&registries, &occupied, [source])
        .unwrap()
        .commit(&mut occupied)
        .unwrap();
    let (id, mut plant, visible) = occupied
        .surface()
        .resources()
        .map(|record| {
            (
                record.id(),
                record.perennial.unwrap(),
                record.remaining_mass(),
            )
        })
        .next()
        .unwrap();
    plant.root_reserve = plant.root_reserve.checked_add(visible).unwrap();
    let next = occupied.surface().revision() + 1;
    occupied.surface_state_mut().apply_perennial_cycle(
        vec![(
            id,
            plant,
            Mass::ZERO,
            SurfaceResourceLifecycle::Depleted,
            Some(SimulationTick::ZERO),
        )],
        next,
    );
    assert_eq!(
        occupied.surface().get(id).unwrap().remaining_mass(),
        Mass::ZERO
    );
    assert!(
        occupied
            .surface()
            .get(id)
            .unwrap()
            .rooted_biomass_occupies_site()
    );
    let before = occupied.clone();
    let displaced = seeded_spring_spec(&occupied, profile, &chunk, &[]);
    assert_ne!(displaced.map(|(position, _)| position), Some(position));
    assert_eq!(
        occupied, before,
        "natural water candidate selection must be read-only"
    );
}
