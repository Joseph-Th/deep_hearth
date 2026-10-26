//! Contracts for local observation, finite gathering, persistence, and exposed-matter age.

use super::*;
use crate::content::{
    BASIC_TIMBER_COVER_PANEL_AREA, BASIC_TIMBER_COVER_PANEL_THICKNESS,
    BASIC_TIMBER_POST_CROSS_SECTION, BASIC_TIMBER_POST_LENGTH, BLOCK_STATE_STONE,
    CAMPFIRE_FUEL_MASS, ENERGY_STONE_RING_CAMPFIRE, FLUID_WATER, FORM_BOARD, FORM_CHIP, FORM_FOOD,
    FORM_LOG, FORM_LUMP, FORM_ROUGH_BOX_BODY, MATERIAL_BERRIES, MATERIAL_STONE, MATERIAL_WOOD,
    STORAGE_CLAY_LINED_SPOIL_BAY, STORAGE_ROUGH_TIMBER_FIELD_BOX,
    STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
    SURFACE_GATHERING_HAND_COLLECT_STONE, SURFACE_GATHERING_HAND_COLLECT_STONE_CHIPS,
    SURFACE_GATHERING_HAND_FORAGE_BERRIES, build_registries,
};
use crate::core::quantity::{AggregateMass, Energy, Mass, Temperature, Volume};
use crate::core::state::{
    AppState, StateValidationError, apply_clock_advance, validate_loaded_state,
};
use crate::core::time::{CONTROL_TICKS_PER_SIMULATION_TICK, SimulationTick, TickSpan};
use crate::energy::validate_assemble_energy_store;
use crate::fluid::add_fluid_store_with_contents_for_fixture;
use crate::inventory::{
    PlayerStoragePlacementError, StockpileStorageError, deposit_lot_for_test,
    project_material_stacks,
};
use crate::labor::PlayerWorkValidationError;
use crate::logistics::{
    HorizontalControlInput, PlayerActiveSelection, PlayerControlInput,
    PlayerOverheadCoverAssessment, PlayerQuickAccessSlot, add_traversable_edge_for_test,
    assess_player_overhead_cover, relocate_player_for_test, validate_place_fluid_store,
    validate_set_active_selection,
};
use crate::material::{CommodityKey, MaterialComposition, MaterialPhase};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope, validate_persistable_state};
use crate::player::validate_admit_player;
use crate::simulation::{
    ActiveSelectionDropOutcome, PlayerDirectControlInput, PrimaryActionOutcome,
    PrimaryControlIntent, SecondaryActionOutcome, SecondaryControlIntent, advance_control_tick,
    advance_control_tick_with_direct_input, advance_control_tick_with_input, advance_tick,
};
use crate::spatial::{VIEW_DIRECTION_PARTS_PER_MILLION, ViewDirection, VoxelBounds, VoxelCoord};
use crate::structural::{
    GroundedStructuralElementPlan, StructuralElementGeometry, StructuralLifecycle,
    SupportedStructuralElementPlan, add_structural_element,
    calculate_prismatic_material_mass_ceiling, materialize_structural_element_for_test,
    validate_start_player_grounded_structural_construction_from_stack,
    validate_start_player_supported_structural_construction_from_stack,
};
use crate::survival::{
    FoodFreshness, Vitality, assess_food_freshness, assess_survival, player_record,
};
use crate::world::{
    BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed,
    set_generated_block_for_test, validate_generate_chunk,
};

const TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);
const WORLD_SEED: WorldGenerationSeed = WorldGenerationSeed::new(0x5355_5246_4143_455F);

fn positive_x_view() -> ViewDirection {
    ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0)
        .unwrap_or_else(|error| panic!("surface positive-X view failed: {error}"))
}

fn materialize_test_wall(
    registries: &crate::registry::Registries,
    state: &mut AppState,
    position: VoxelCoord,
    full_cell: bool,
) {
    let bounds = VoxelBounds::new(
        position,
        VoxelCoord::new(position.x() + 1, position.y() + 1, position.z() + 1),
    )
    .unwrap();
    let geometry = StructuralElementGeometry::new(
        bounds,
        if full_cell {
            BASIC_TIMBER_COVER_PANEL_THICKNESS
        } else {
            BASIC_TIMBER_POST_LENGTH
        },
        if full_cell {
            BASIC_TIMBER_COVER_PANEL_AREA
        } else {
            BASIC_TIMBER_POST_CROSS_SECTION
        },
    )
    .unwrap();
    let element = add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(registries, state, element, FORM_LOG);
}

#[test]
fn surface_creation_respects_built_wall_but_permits_narrow_support() {
    for full_cell in [false, true] {
        let registries = build_registries();
        let mut state = AppState::new();
        let position = VoxelCoord::new(2, 0, 0);
        materialize_test_wall(&registries, &mut state, position, full_cell);
        assert_eq!(state.structures().blocks_full_cell(position), full_cell);
        let specification = SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            position,
            CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
            Mass::from_milligrams(100_000),
            TEMPERATURE,
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap();
        let before = state.clone();
        let result = validate_create_surface_resources(&registries, &state, [specification]);
        if full_cell {
            let resource = SurfaceResourceId::new(state.surface().next_resource_id());
            assert_eq!(
                result.err(),
                Some(SurfaceResourceCreationError::InvalidResource(
                    SurfaceResourceValidationError::EmbeddedInSolidStructure {
                        resource,
                        voxel: position,
                    }
                ))
            );
            assert_eq!(state, before);
        } else {
            result.unwrap().commit(&mut state).unwrap();
            assert_eq!(
                state
                    .surface()
                    .available_resources_at(Some(position))
                    .count(),
                1
            );
            assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
        }
    }
}

#[test]
fn player_material_drop_reports_full_cell_obstruction_without_spending_inventory() {
    for full_cell in [true, false] {
        let registries = build_registries();
        let mut state = AppState::new();
        let player_position = VoxelCoord::new(0, 0, 0);
        materialize_test_wall(&registries, &mut state, player_position, full_cell);
        validate_admit_player(&registries, &state, player_position)
            .unwrap()
            .commit(&mut state)
            .unwrap();
        let carried = state.logistics().player().unwrap().carried_stockpile();
        let commodity = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            commodity,
            Mass::from_milligrams(50_000),
            TEMPERATURE,
        )
        .unwrap();
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
        let before = state.clone();
        let total = calculate_matter_accounting(&state).unwrap().total();
        if full_cell {
            assert_eq!(
                validate_drop_player_material(&registries, &state, commodity).err(),
                Some(PlayerMaterialDropError::SurfacePlacement(
                    SurfaceResourceValidationError::EmbeddedInSolidStructure {
                        resource: SurfaceResourceId::new(state.surface().next_resource_id()),
                        voxel: player_position,
                    }
                ))
            );
            assert_eq!(state, before);
        } else {
            let drop = validate_drop_player_material(&registries, &state, commodity)
                .unwrap()
                .commit(&mut state)
                .unwrap();
            assert_eq!(drop.position(), player_position);
            assert_eq!(
                state
                    .inventory()
                    .get_stockpile(carried)
                    .unwrap()
                    .get_mass(commodity),
                Mass::ZERO
            );
            assert_eq!(
                state
                    .surface()
                    .available_resources_at(Some(player_position))
                    .count(),
                1
            );
        }
        assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
        let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
        let decoded: LoadedSaveEnvelope = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded.into_state(&registries).unwrap(), state);
    }
}

#[test]
fn surface_creation_token_and_scavenged_drop_cannot_cross_a_new_wall() {
    let registries = build_registries();
    let mut state = AppState::new();
    let blocked = VoxelCoord::new(2, 0, 0);
    let adjacent = VoxelCoord::new(3, 0, 0);
    let specification = |position| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            position,
            CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
            Mass::from_milligrams(100_000),
            TEMPERATURE,
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap()
    };
    let drop =
        insert_surface_resource_for_test(&registries, &mut state, specification(adjacent)).unwrap();
    let pending =
        validate_create_surface_resources(&registries, &state, [specification(blocked)]).unwrap();
    let expected = state.structures().revision();
    materialize_test_wall(&registries, &mut state, blocked, true);
    let actual = state.structures().revision();
    let before = state.clone();
    assert_eq!(
        pending.commit(&mut state),
        Err(SurfaceResourceCreationCommitError::StructureRevision { expected, actual })
    );
    assert_eq!(state, before);
    assert!(!relocate_scavenged_drop_step(
        &registries,
        &mut state,
        drop,
        adjacent,
        blocked
    ));
    assert_eq!(state, before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
            .unwrap()
            .into_state(&registries)
            .unwrap(),
        state
    );

    let revision = state.surface().revision() + 1;
    state
        .surface_state_mut()
        .apply_scavenged_drop_step(drop, adjacent, blocked, revision);
    assert_eq!(
        validate_loaded_state(&registries, &state),
        Err(StateValidationError::Surface(
            SurfaceResourceValidationError::EmbeddedInSolidStructure {
                resource: drop,
                voxel: blocked,
            }
        ))
    );
    let tampered = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&tampered)
            .unwrap()
            .into_state(&registries)
            .is_err()
    );
}

#[test]
fn indexed_surface_bounds_exclude_remote_and_depleted_custody() {
    let registries = build_registries();
    let mut state = AppState::new();
    let positions = [
        VoxelCoord::new(0, 4, 0),
        VoxelCoord::new(i64::MAX - 1, 0, 0),
        VoxelCoord::new(-2, 0, 0),
        VoxelCoord::new(1, 0, 1),
        VoxelCoord::new(0, 0, 0),
        VoxelCoord::new(0, 0, 4),
        VoxelCoord::new(-3, 0, 0),
        VoxelCoord::new(2, 0, 0),
        VoxelCoord::new(i64::MAX, 0, 0),
    ];
    let specs = positions.map(|position| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            position,
            CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
            Mass::from_milligrams(100_000),
            TEMPERATURE,
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap()
    });
    let allocation = validate_create_surface_resources(&registries, &state, specs).unwrap();
    let ids = allocation.resource_ids().collect::<Vec<_>>();
    allocation.commit(&mut state).unwrap();
    let bounds = VoxelBounds::new(VoxelCoord::new(-2, -1, -1), VoxelCoord::new(2, 2, 2)).unwrap();
    let observed = |state: &AppState| {
        state
            .surface()
            .available_resources_in_bounds(bounds)
            .map(|record| record.id())
            .collect::<Vec<_>>()
    };
    assert_eq!(observed(&state), vec![ids[2], ids[4], ids[3]]);

    let next_revision = state.surface().revision() + 1;
    let at = state.tick();
    state.surface_state_mut().apply_gathering(
        ids[4],
        Mass::from_milligrams(100_000),
        at,
        next_revision,
    );
    assert_eq!(observed(&state), vec![ids[2], ids[3]]);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let extreme = VoxelBounds::new(
        VoxelCoord::new(i64::MAX - 1, -1, -1),
        VoxelCoord::new(i64::MAX, 1, 1),
    )
    .unwrap();
    assert_eq!(
        state
            .surface()
            .available_resources_in_bounds(extreme)
            .map(|record| record.id())
            .collect::<Vec<_>>(),
        vec![ids[1]]
    );
    let last_column = crate::world::ChunkCoord::containing(positions[8]);
    assert_eq!(
        state
            .surface()
            .available_resources_in_chunk(last_column)
            .map(|record| record.id())
            .collect::<Vec<_>>(),
        vec![ids[1], ids[8]],
        "the final representable voxel belongs to its chunk despite half-open bounds limits"
    );
    assert_eq!(
        state
            .surface()
            .available_resources_in_chunk(crate::world::ChunkCoord::new(0, 0))
            .map(|record| record.id())
            .collect::<Vec<_>>(),
        vec![ids[5], ids[0], ids[3], ids[7]],
        "negative and remote columns and depleted records are excluded"
    );
}

#[test]
fn local_surface_index_tracks_creation_partial_use_relocation_and_depletion_across_load() {
    let registries = build_registries();
    let mut state = AppState::new();
    let local = VoxelCoord::new(1, 0, 0);
    let remote = VoxelCoord::new(2, 0, 0);
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    let specs = [
        (remote, 100_000_u64),
        (local, 200_000),
        (local, 300_000),
        (remote, 400_000),
    ]
    .map(|(position, mass)| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            position,
            commodity,
            Mass::from_milligrams(mass),
            TEMPERATURE,
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap()
    });
    let creation = validate_create_surface_resources(&registries, &state, specs).unwrap();
    let ids = creation.resource_ids().collect::<Vec<_>>();
    creation.commit(&mut state).unwrap();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let nearby = |state: &AppState, position| {
        state
            .surface()
            .available_resources_at(Some(position))
            .map(|record| record.id())
            .collect::<Vec<_>>()
    };
    assert_eq!(nearby(&state, local), vec![ids[1], ids[2]]);
    assert_eq!(nearby(&state, remote), vec![ids[0], ids[3]]);
    assert!(
        state
            .surface()
            .available_resources_at(None)
            .next()
            .is_none()
    );

    let gathered_at = state.tick();
    let revision = state.surface().revision() + 1;
    state.surface_state_mut().apply_gathering(
        ids[1],
        Mass::from_milligrams(50_000),
        gathered_at,
        revision,
    );
    assert_eq!(nearby(&state, local), vec![ids[1], ids[2]]);
    assert!(super::relocate_scavenged_drop_step(
        &registries,
        &mut state,
        ids[2],
        local,
        remote
    ));
    assert_eq!(nearby(&state, local), vec![ids[1]]);
    assert_eq!(nearby(&state, remote), vec![ids[0], ids[2], ids[3]]);

    let revision = state.surface().revision() + 1;
    state.surface_state_mut().apply_gathering(
        ids[1],
        Mass::from_milligrams(150_000),
        gathered_at,
        revision,
    );
    assert!(nearby(&state, local).is_empty());
    assert_eq!(nearby(&state, remote), vec![ids[0], ids[2], ids[3]]);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded).unwrap();
    let loaded = decoded.into_state(&registries).unwrap();
    assert_eq!(loaded, state);
    assert!(nearby(&loaded, local).is_empty());
    assert_eq!(nearby(&loaded, remote), vec![ids[0], ids[2], ids[3]]);
    assert_eq!(
        calculate_matter_accounting(&loaded).unwrap().total(),
        conserved
            .checked_sub(AggregateMass::from_milligrams(200_000))
            .unwrap()
    );
}

#[test]
fn scavenged_drop_cannot_relocate_into_generated_full_cube() {
    let registries = build_registries();
    let mut state = AppState::new();
    let air = VoxelCoord::new(5, 100, 5);
    let solid = VoxelCoord::new(6, 100, 5);
    validate_generate_chunk(
        &registries,
        &state,
        WORLD_SEED,
        ChunkCoord::containing(air),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    set_generated_block_for_test(registries.world(), &mut state, solid, BLOCK_STATE_STONE);
    assert_eq!(state.world().block_at(air), Some(BlockStateId::AIR));
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        air,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(100_000),
        TEMPERATURE,
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let plan = validate_create_surface_resources(&registries, &state, [spec]).unwrap();
    let id = plan.first_resource_id();
    plan.commit(&mut state).unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let before = state.clone();
    assert!(
        !super::relocate_scavenged_drop_step(&registries, &mut state, id, air, solid),
        "scavenger custody must not move available matter into a full-cube block"
    );
    assert_eq!(state, before);
    set_generated_block_for_test(registries.world(), &mut state, solid, BlockStateId::AIR);
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    assert!(super::relocate_scavenged_drop_step(
        &registries,
        &mut state,
        id,
        air,
        solid
    ));
    assert_eq!(state.surface().get(id).unwrap().position(), solid);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
}

#[test]
fn wedge_abrasion_and_bulk_salvage_are_one_conserved_surface_transaction() {
    let registries = build_registries();
    let mut state = AppState::new();
    let junction = VoxelCoord::new(0, 0, 0);
    let den = VoxelCoord::new(-1, 0, 0);
    let specs = [(FORM_LUMP, 1_000_000_u64), (FORM_CHIP, 25_000_u64)].map(|(form, amount)| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            junction,
            CommodityKey::new(MATERIAL_STONE, form),
            Mass::from_milligrams(amount),
            TEMPERATURE,
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap()
    });
    let creation = validate_create_surface_resources(&registries, &state, specs).unwrap();
    let ids = creation.resource_ids().collect::<Vec<_>>();
    creation.commit(&mut state).unwrap();
    let (bulk, wedge) = (ids[0], ids[1]);
    let mass_before = calculate_matter_accounting(&state).unwrap().total();
    let initial = state.clone();
    let at = state.tick();
    assert!(
        split_bulk_scavenged_drop_with_head_wear(
            &registries,
            &mut state,
            ScavengedPortionRequest::new(bulk, junction, den, at, Mass::from_milligrams(500_000)),
            wedge,
            Mass::from_milligrams(25_000),
            FORM_CHIP,
        )
        .is_none(),
        "wear cannot exhaust a wedge in the same split"
    );
    assert_eq!(state, initial);

    let (daughter, divided) = split_bulk_scavenged_drop_with_head_wear(
        &registries,
        &mut state,
        ScavengedPortionRequest::new(bulk, junction, den, at, Mass::from_milligrams(500_000)),
        wedge,
        Mass::from_milligrams(10_000),
        FORM_CHIP,
    )
    .expect("two conserved portions must be created before either source shrinks");
    assert_eq!(divided, Mass::from_milligrams(500_000));
    assert_eq!(state.surface().get(daughter).unwrap().position(), den);
    assert_eq!(
        state.surface().get(bulk).unwrap().remaining_mass(),
        Mass::from_milligrams(500_000)
    );
    assert_eq!(
        state.surface().get(wedge).unwrap().remaining_mass(),
        Mass::from_milligrams(15_000)
    );
    let abrasion = state
        .surface()
        .available_resources_at(Some(junction))
        .filter(|resource| resource.id() != bulk && resource.id() != wedge)
        .collect::<Vec<_>>();
    assert_eq!(abrasion.len(), 1);
    assert_eq!(abrasion[0].remaining_mass(), Mass::from_milligrams(10_000));
    assert_eq!(
        abrasion[0].material_profile(),
        state.surface().get(wedge).unwrap().material_profile()
    );
    assert_eq!(
        abrasion[0].material_provenance(),
        state.surface().get(wedge).unwrap().material_provenance()
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        mass_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let after = state.clone();
    assert!(
        split_bulk_scavenged_drop_with_head_wear(
            &registries,
            &mut state,
            ScavengedPortionRequest::new(bulk, junction, den, at, Mass::from_milligrams(100_000)),
            wedge,
            Mass::from_milligrams(20_000),
            FORM_CHIP,
        )
        .is_none()
    );
    assert_eq!(
        state, after,
        "rejected subsequent wear may not move bulk matter"
    );
    let save = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&save)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
}

#[test]
fn selected_stone_and_timber_assemble_and_fuel_a_local_campfire() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let carried = admit_player(&registries, &mut state, position);
    let stone = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    for (commodity, mass) in [
        (stone, Mass::from_milligrams(1_000_000)),
        (timber, CAMPFIRE_FUEL_MASS),
    ] {
        let _ = deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            commodity,
            mass,
            TEMPERATURE,
        )
        .unwrap_or_else(|error| panic!("direct campfire material fixture failed: {error}"));
    }
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("direct campfire matter audit failed: {error}"))
        .total();

    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(stone)),
    )
    .unwrap_or_else(|error| panic!("direct campfire stone selection failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("direct campfire stone selection commit failed: {error}"));
    let assembled = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(positive_x_view())),
    )
    .unwrap_or_else(|error| panic!("direct campfire assembly control failed: {error}"));
    let fire = match assembled.secondary_action() {
        Some(SecondaryActionOutcome::EnergyStoreAssembled { definition, store }) => {
            assert_eq!(*definition, ENERGY_STONE_RING_CAMPFIRE);
            *store
        }
        other => panic!("selected stone did not assemble the local campfire: {other:?}"),
    };
    assert_eq!(
        state.logistics().energy_store_position(fire),
        Some(position)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.get_mass(stone)),
        Some(Mass::ZERO)
    );

    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(timber)),
    )
    .unwrap_or_else(|error| panic!("direct campfire timber selection failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("direct campfire timber selection commit failed: {error}"));
    let fueled = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(positive_x_view())),
    )
    .unwrap_or_else(|error| panic!("direct campfire fuel control failed: {error}"));
    match fueled.secondary_action() {
        Some(SecondaryActionOutcome::CombustionFuelAdded { store, released }) => {
            assert_eq!(*store, fire);
            assert!(!released.is_zero());
            assert_eq!(
                state.energy().get_store(fire).map(|record| record.stored()),
                Some(*released)
            );
        }
        other => panic!("selected timber did not fuel the local campfire: {other:?}"),
    }
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.get_mass(timber)),
        Some(Mass::ZERO)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("direct campfire final matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn selected_fuel_rejects_ambiguous_local_combustion_stores_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let carried = admit_player(&registries, &mut state, position);
    let stone = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        stone,
        Mass::from_milligrams(2_000_000),
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("ambiguous fire stone fixture failed: {error}"));
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        timber,
        CAMPFIRE_FUEL_MASS,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("ambiguous fire timber fixture failed: {error}"));
    let mut fires = Vec::new();
    for _ in 0..2 {
        let fire = validate_assemble_energy_store(
            &registries,
            &state,
            ENERGY_STONE_RING_CAMPFIRE,
            carried,
        )
        .unwrap_or_else(|error| panic!("ambiguous local fire setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("ambiguous local fire setup commit failed: {error}"));
        fires.push(fire);
    }
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(timber)),
    )
    .unwrap_or_else(|error| panic!("ambiguous fire timber selection failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("ambiguous fire timber selection commit failed: {error}"));
    let before = state.clone();

    let outcome = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(positive_x_view())),
    )
    .unwrap_or_else(|error| panic!("ambiguous fire control failed: {error}"));
    assert_eq!(
        outcome.secondary_action(),
        Some(&SecondaryActionOutcome::AmbiguousLocalCombustionStores { count: 2 })
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.get_mass(timber)),
        before
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.get_mass(timber))
    );
    for fire in fires {
        assert_eq!(
            state.energy().get_store(fire).map(|store| store.stored()),
            Some(Energy::ZERO)
        );
    }
}

fn backward_shallow_down_view() -> ViewDirection {
    ViewDirection::new(-866_025, -500_000, 0)
        .unwrap_or_else(|error| panic!("surface backward-down view failed: {error}"))
}

#[test]
fn player_world_drop_projection_excludes_natural_opportunities() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let opportunity = SurfaceResourceSpec::new(
        SurfaceResourceKind::Opportunity,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(100_000),
        TEMPERATURE,
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap_or_else(|error| panic!("drop-projection opportunity specification failed: {error}"));
    insert_surface_resource_for_test(&registries, &mut state, opportunity)
        .unwrap_or_else(|error| panic!("drop-projection opportunity creation failed: {error}"));
    let drop = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(200_000),
        TEMPERATURE,
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap_or_else(|error| panic!("drop-projection drop specification failed: {error}"));
    let drop = insert_surface_resource_for_test(&registries, &mut state, drop)
        .unwrap_or_else(|error| panic!("drop-projection drop creation failed: {error}"));
    let _carried = admit_player(&registries, &mut state, position);

    let drops = available_player_world_drops(&state).collect::<Vec<_>>();
    assert_eq!(drops.len(), 1);
    assert_eq!(drops[0].resource(), drop);
    assert_eq!(drops[0].position(), position);
    assert_eq!(
        drops[0].commodity(),
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP)
    );
    assert_eq!(drops[0].mass(), Mass::from_milligrams(200_000));
    assert_eq!(drops[0].temperature(), TEMPERATURE);
    assert_eq!(
        drops[0].composition(),
        &MaterialComposition::pure(MATERIAL_STONE)
    );
    assert_eq!(drops[0].ambient_storage_age_parts(), 0);
    assert_eq!(drops[0].automatic_pickup_available_at(), None);
    assert!(drops[0].automatic_pickup_ready());
}

#[test]
fn player_world_drop_projection_exposes_only_exact_local_custody_after_admission_and_load() {
    let registries = build_registries();
    let mut state = AppState::new();
    let home = VoxelCoord::new(0, 0, 0);
    let adjacent = VoxelCoord::new(1, 0, 0);
    let remote = VoxelCoord::new(100, 0, -50);
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    let mut drops = Vec::new();
    for (kind, position) in [
        (SurfaceResourceKind::Drop, home),
        (SurfaceResourceKind::Drop, adjacent),
        (SurfaceResourceKind::Drop, remote),
        (SurfaceResourceKind::Opportunity, home),
    ] {
        let spec = SurfaceResourceSpec::new(
            kind,
            position,
            commodity,
            Mass::from_milligrams(25_000),
            TEMPERATURE,
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap();
        drops.push(insert_surface_resource_for_test(&registries, &mut state, spec).unwrap());
    }
    assert!(
        available_player_world_drops(&state).next().is_none(),
        "unadmitted callers must not enumerate remote or local surface custody"
    );
    let _carried = admit_player(&registries, &mut state, home);
    assert_eq!(
        available_player_world_drops(&state)
            .map(|drop| drop.resource())
            .collect::<Vec<_>>(),
        vec![drops[0]],
        "adjacent and remote parcels must not appear in a local drop presentation"
    );
    relocate_player_for_test(&mut state, adjacent);
    assert_eq!(
        available_player_world_drops(&state)
            .map(|drop| drop.resource())
            .collect::<Vec<_>>(),
        vec![drops[1]]
    );
    relocate_player_for_test(&mut state, remote);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let save = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&save)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, restored);
    assert_eq!(
        available_player_world_drops(&restored)
            .map(|drop| drop.resource())
            .collect::<Vec<_>>(),
        vec![drops[2]],
        "the local position index must be rebuilt from saved custody"
    );
}

#[test]
fn automatic_pickup_skips_unallocatable_drop_and_merges_a_later_parcel() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let wood = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let chips = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    let mut resources = Vec::new();
    for commodity in [wood, chips] {
        let specification = SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            position,
            commodity,
            Mass::from_milligrams(50_000),
            TEMPERATURE,
            MaterialComposition::pure(commodity.material()),
        )
        .unwrap_or_else(|error| panic!("exhausted-pickup resource spec failed: {error}"));
        resources.push(
            insert_surface_resource_for_test(&registries, &mut state, specification)
                .unwrap_or_else(|error| panic!("exhausted-pickup resource failed: {error}")),
        );
    }
    let carried = admit_player(&registries, &mut state, position);
    let existing = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        chips,
        Mass::from_milligrams(20_000),
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("exhausted-pickup carried lot failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("exhausted-pickup save failed: {error}"));
    encoded["state"]["systems"]["inventory"]["next_lot_id"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("exhausted-pickup decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("exhausted-pickup trusted load failed: {error}"));
    let before = calculate_matter_accounting(&loaded)
        .unwrap_or_else(|error| panic!("exhausted-pickup baseline accounting failed: {error}"))
        .total();
    let outcome = advance_control_tick(&registries, &mut loaded)
        .unwrap_or_else(|error| panic!("exhausted-pickup control tick failed: {error}"));
    let pickup = outcome
        .surface_drop_pickup()
        .unwrap_or_else(|| panic!("mergeable later drop must not be starved"));
    assert_eq!(pickup.resource(), resources[1]);
    assert_eq!(pickup.lot(), existing);
    assert_eq!(pickup.picked_mass(), Mass::from_milligrams(50_000));
    assert_eq!(
        loaded
            .surface()
            .available_resources_at(Some(position))
            .find(|resource| resource.id() == resources[0])
            .map(|resource| resource.remaining_mass()),
        Some(Mass::from_milligrams(50_000))
    );
    assert_eq!(
        loaded
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.get_mass(chips)),
        Some(Mass::from_milligrams(70_000))
    );
    assert_eq!(
        calculate_matter_accounting(&loaded)
            .unwrap_or_else(|error| panic!("exhausted-pickup conservation failed: {error}"))
            .total(),
        before
    );
    assert_eq!(validate_loaded_state(&registries, &loaded), Ok(()));
}

#[test]
fn exposed_residue_footing_counts_distinct_densities_without_treating_nature_as_disposal() {
    let registries = build_registries();
    let mut state = AppState::new();
    let at = VoxelCoord::new(5, 0, 5);
    for (kind, commodity, mass) in [
        (
            SurfaceResourceKind::Opportunity,
            CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
            Mass::from_milligrams(200_000_000),
        ),
        (
            SurfaceResourceKind::Drop,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(200_000_000),
        ),
        (
            SurfaceResourceKind::Drop,
            CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
            Mass::from_milligrams(40_000_000),
        ),
        (
            SurfaceResourceKind::Drop,
            CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
            Mass::from_milligrams(8_000_000),
        ),
    ] {
        let spec = SurfaceResourceSpec::new(
            kind,
            at,
            commodity,
            mass,
            TEMPERATURE,
            MaterialComposition::pure(commodity.material()),
        )
        .unwrap_or_else(|error| panic!("mixed-footing input invalid: {error}"));
        insert_surface_resource_for_test(&registries, &mut state, spec)
            .unwrap_or_else(|error| panic!("mixed-footing input failed: {error}"));
    }
    assert_eq!(
        exposed_residue_compact_depth_um(&registries, &state, at),
        27_402,
        "only dropped residual solids contribute their host-density compact volumes",
    );
    assert_eq!(
        exposed_residue_compact_depth_um(&registries, &state, VoxelCoord::new(6, 0, 5)),
        0,
        "a footprint cannot arbitrarily obstruct neighboring terrain",
    );
}

#[test]
fn secondary_use_sets_supported_timber_member_on_pointed_active_post_face() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WORLD_SEED,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("supported timber terrain failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("supported timber terrain commit failed: {error}"));
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(0, *y, 0)) == Some(BLOCK_STATE_STONE))
        .unwrap_or_else(|| panic!("supported timber terrain has no solid surface"));
    let foot_y = surface_y + 1;
    for x in 1..=3 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(x, surface_y, 0),
            BLOCK_STATE_STONE,
        );
        for y in foot_y..=foot_y + 1 {
            set_generated_block_for_test(
                registries.world(),
                &mut state,
                VoxelCoord::new(x, y, 0),
                BlockStateId::AIR,
            );
        }
    }
    let initial_player = VoxelCoord::new(2, foot_y, 0);
    let framing_player = VoxelCoord::new(3, foot_y, 0);
    let support_position = VoxelCoord::new(1, foot_y, 0);
    let framing_position = VoxelCoord::new(2, foot_y, 0);
    let carried = admit_player(&registries, &mut state, initial_player);
    let commodity = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let member_mass = Mass::from_milligrams(3_250_000);
    let total_timber = member_mass
        .checked_add(member_mass)
        .and_then(|mass| mass.checked_add(member_mass))
        .unwrap_or_else(|| panic!("supported timber fixture mass overflowed"));
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        total_timber,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("supported timber carried material failed: {error}"));
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(commodity)),
    )
    .unwrap_or_else(|error| panic!("supported timber selection failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("supported timber selection commit failed: {error}"));
    let support_bounds = VoxelBounds::new(
        support_position,
        VoxelCoord::new(
            support_position.x() + 1,
            support_position.y() + 1,
            support_position.z() + 1,
        ),
    )
    .unwrap_or_else(|error| panic!("supported timber post bounds failed: {error}"));
    let support_geometry = StructuralElementGeometry::new(
        support_bounds,
        BASIC_TIMBER_POST_LENGTH,
        BASIC_TIMBER_POST_CROSS_SECTION,
    )
    .unwrap_or_else(|error| panic!("supported timber post geometry failed: {error}"));
    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("supported timber initial stack failed: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("supported timber initial stack disappeared"));
    let support_start = validate_start_player_grounded_structural_construction_from_stack(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        GroundedStructuralElementPlan::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            support_geometry,
        ),
        &stack,
    )
    .unwrap_or_else(|error| panic!("supported timber post validation failed: {error}"));
    let support = support_start.element();
    let support_work = support_start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("supported timber post commit failed: {error}"));
    while state.tick() < support_work.completes_at() {
        let _ = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("supported timber post completion failed: {error}"));
    }
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Active)
    );

    let second_support_position = VoxelCoord::new(2, foot_y, 1);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(2, surface_y, 1),
        BLOCK_STATE_STONE,
    );
    for y in foot_y..=foot_y + 1 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(2, y, 1),
            BlockStateId::AIR,
        );
    }
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("supported timber matter baseline failed: {error}"))
        .total();
    let second_support_bounds =
        VoxelBounds::new(second_support_position, VoxelCoord::new(3, foot_y + 1, 2))
            .unwrap_or_else(|error| panic!("supported timber second-post bounds failed: {error}"));
    let second_stack = project_material_stacks(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("supported timber second stack failed: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("supported timber second stack disappeared"));
    let second_support_start = validate_start_player_grounded_structural_construction_from_stack(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        GroundedStructuralElementPlan::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            StructuralElementGeometry::new(
                second_support_bounds,
                BASIC_TIMBER_POST_LENGTH,
                BASIC_TIMBER_POST_CROSS_SECTION,
            )
            .unwrap_or_else(|error| {
                panic!("supported timber second-post geometry failed: {error}")
            }),
        ),
        &second_stack,
    )
    .unwrap_or_else(|error| panic!("supported timber second-post validation failed: {error}"));
    let second_support = second_support_start.element();
    let second_support_work = second_support_start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("supported timber second-post commit failed: {error}"));
    while state.tick() < second_support_work.completes_at() {
        let _ = advance_control_tick(&registries, &mut state).unwrap_or_else(|error| {
            panic!("supported timber second-post completion failed: {error}")
        });
    }
    assert_eq!(
        state
            .structures()
            .get_element(second_support)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Active)
    );

    let east = PlayerControlInput::new(
        HorizontalControlInput::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0)
            .unwrap_or_else(|error| panic!("supported timber movement input failed: {error}")),
    );
    while state
        .logistics()
        .player()
        .is_some_and(|player| player.position() != framing_player)
    {
        let _ = advance_control_tick_with_input(&registries, &mut state, east)
            .unwrap_or_else(|error| panic!("supported timber movement failed: {error}"));
    }
    while state.logistics().player().is_some_and(|player| {
        let velocity = player.body().horizontal_velocity();
        velocity.x_nanometers_per_second() != 0 || velocity.z_nanometers_per_second() != 0
    }) {
        let _ = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("supported timber braking failed: {error}"));
        assert_eq!(
            state.logistics().player().map(|player| player.position()),
            Some(framing_player)
        );
    }

    let input = PlayerDirectControlInput::NEUTRAL.with_secondary(
        SecondaryControlIntent::use_active(backward_shallow_down_view()),
    );
    let queued = advance_control_tick_with_direct_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("supported timber direct queue failed: {error}"));
    assert_eq!(
        queued.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    let work = loop {
        let control = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("supported timber continuation failed: {error}"));
        match control.secondary_action() {
            Some(SecondaryActionOutcome::StructuralConstructionStarted(work)) => break *work,
            None => {}
            other => panic!("supported timber returned unexpected outcome: {other:?}"),
        }
    };
    let element = work.element();
    let record = state
        .structures()
        .get_element(element)
        .unwrap_or_else(|| panic!("supported timber framing member disappeared"));
    assert!(record.bounds().has_voxel(framing_position));
    assert!(!record.is_grounded());
    assert_eq!(record.embodied_mass(), member_mass);
    assert_eq!(record.lifecycle(), StructuralLifecycle::Planned);
    assert_eq!(
        state
            .structures()
            .supports(element)
            .map(|supports| supports.collect::<Vec<_>>()),
        Some(vec![support, second_support])
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.get_mass(commodity)),
        Some(Mass::ZERO)
    );

    let mut completions = 0_u32;
    while state.tick() < work.completes_at() {
        let control = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("supported timber hand-setting failed: {error}"));
        if control
            .simulation()
            .is_some_and(|outcome| outcome.structural_construction() == Some(element))
        {
            completions += 1;
        }
    }
    assert_eq!(completions, 1);
    assert_eq!(
        state
            .structures()
            .get_element(element)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Active)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("supported timber matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn automatic_drop_pickup_takes_only_the_mass_that_fits() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let drop_mass = Mass::from_milligrams(500_000);
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        drop_mass,
        TEMPERATURE,
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap_or_else(|error| panic!("partial pickup drop specification failed: {error}"));
    let drop = insert_surface_resource_for_test(&registries, &mut state, spec)
        .unwrap_or_else(|error| panic!("partial pickup drop creation failed: {error}"));
    let carried = admit_player(&registries, &mut state, position);
    let available = state
        .inventory()
        .get_stockpile(carried)
        .unwrap_or_else(|| panic!("partial pickup carried stockpile disappeared"))
        .available_capacity();
    let pickup_capacity = Mass::from_milligrams(200_000);
    let filler = available
        .checked_sub(pickup_capacity)
        .unwrap_or_else(|| panic!("partial pickup fixture has insufficient body capacity"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_ROUGH_BOX_BODY),
        filler,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("partial pickup filler deposit failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("partial pickup matter baseline failed: {error}"));

    let control = advance_control_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("partial pickup control tick failed: {error}"));
    let pickup = control
        .surface_drop_pickup()
        .unwrap_or_else(|| panic!("partial fitting drop mass was not picked up"));
    assert_eq!(pickup.resource(), drop);
    assert_eq!(pickup.destination(), carried);
    assert_eq!(pickup.picked_mass(), pickup_capacity);
    assert_eq!(
        state
            .surface()
            .get(drop)
            .map(|resource| (resource.lifecycle(), resource.remaining_mass())),
        Some((
            SurfaceResourceLifecycle::Available,
            Mass::from_milligrams(300_000),
        ))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.available_capacity()),
        Some(Mass::ZERO)
    );
    assert_eq!(
        available_player_world_drops(&state)
            .map(|drop| drop.mass())
            .collect::<Vec<_>>(),
        vec![Mass::from_milligrams(300_000)]
    );
    let matter_after = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("partial pickup matter audit failed: {error}"));
    assert_eq!(matter_after.total(), matter_before.total());
}

fn downward_view() -> ViewDirection {
    ViewDirection::new(0, -VIEW_DIRECTION_PARTS_PER_MILLION, 0)
        .unwrap_or_else(|error| panic!("surface downward view failed: {error}"))
}

fn forward_down_view() -> ViewDirection {
    ViewDirection::new(500_000, -866_025, 0)
        .unwrap_or_else(|error| panic!("surface forward-down view failed: {error}"))
}

fn forward_up_view() -> ViewDirection {
    ViewDirection::new(707_107, 707_107, 0)
        .unwrap_or_else(|error| panic!("surface forward-up view failed: {error}"))
}

#[test]
fn selected_boards_build_material_backed_overhead_cover_that_reduces_exposure() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WORLD_SEED,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("cover terrain failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("cover terrain commit failed: {error}"));
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(0, *y, 0)) == Some(BLOCK_STATE_STONE))
        .unwrap_or_else(|| panic!("cover terrain has no solid surface"));
    let foot_y = surface_y + 1;
    let player_position = VoxelCoord::new(0, foot_y, 0);
    let panel_position = VoxelCoord::new(0, foot_y + 2, 0);
    let support_position = VoxelCoord::new(1, foot_y + 2, 0);
    for voxel in [
        player_position,
        VoxelCoord::new(0, foot_y + 1, 0),
        panel_position,
        support_position,
    ] {
        set_generated_block_for_test(registries.world(), &mut state, voxel, BlockStateId::AIR);
    }

    let carried = admit_player(&registries, &mut state, player_position);
    let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let support_mass = calculate_prismatic_material_mass_ceiling(
        registries.materials(),
        MATERIAL_WOOD,
        BASIC_TIMBER_POST_CROSS_SECTION,
        BASIC_TIMBER_POST_LENGTH,
    )
    .unwrap_or_else(|error| panic!("cover support mass failed: {error}"));
    let support_timber = Mass::from_milligrams(
        support_mass
            .milligrams()
            .checked_mul(3)
            .unwrap_or_else(|| panic!("cover support timber mass overflowed")),
    );
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        timber,
        support_timber,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("cover support timber failed: {error}"));
    let support_geometry = |position: VoxelCoord| {
        StructuralElementGeometry::new(
            VoxelBounds::new(
                position,
                VoxelCoord::new(position.x() + 1, position.y() + 1, position.z() + 1),
            )
            .unwrap_or_else(|error| panic!("cover support bounds failed: {error}")),
            BASIC_TIMBER_POST_LENGTH,
            BASIC_TIMBER_POST_CROSS_SECTION,
        )
        .unwrap_or_else(|error| panic!("cover support geometry failed: {error}"))
    };
    let lower_position = VoxelCoord::new(1, foot_y, 0);
    let middle_position = VoxelCoord::new(1, foot_y + 1, 0);
    for voxel in [lower_position, middle_position] {
        set_generated_block_for_test(registries.world(), &mut state, voxel, BlockStateId::AIR);
    }

    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("cover lower-support stack failed: {error}"))
        .into_iter()
        .find(|stack| stack.commodity() == timber)
        .unwrap_or_else(|| panic!("cover lower-support timber disappeared"));
    let lower_start = validate_start_player_grounded_structural_construction_from_stack(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        GroundedStructuralElementPlan::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            support_geometry(lower_position),
        ),
        &stack,
    )
    .unwrap_or_else(|error| panic!("cover lower-support validation failed: {error}"));
    let lower = lower_start.element();
    let lower_work = lower_start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("cover lower-support commit failed: {error}"));
    while state.tick() < lower_work.completes_at() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("cover lower-support work failed: {error}"));
    }

    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("cover middle-support stack failed: {error}"))
        .into_iter()
        .find(|stack| stack.commodity() == timber)
        .unwrap_or_else(|| panic!("cover middle-support timber disappeared"));
    let middle_start = validate_start_player_supported_structural_construction_from_stack(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        SupportedStructuralElementPlan::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            support_geometry(middle_position),
            lower,
        ),
        &stack,
    )
    .unwrap_or_else(|error| panic!("cover middle-support validation failed: {error}"));
    let middle = middle_start.element();
    let middle_work = middle_start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("cover middle-support commit failed: {error}"));
    while state.tick() < middle_work.completes_at() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("cover middle-support work failed: {error}"));
    }

    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("cover upper-support stack failed: {error}"))
        .into_iter()
        .find(|stack| stack.commodity() == timber)
        .unwrap_or_else(|| panic!("cover upper-support timber disappeared"));
    let upper_start = validate_start_player_supported_structural_construction_from_stack(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        SupportedStructuralElementPlan::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            support_geometry(support_position),
            middle,
        ),
        &stack,
    )
    .unwrap_or_else(|error| panic!("cover upper-support validation failed: {error}"));
    let support = upper_start.element();
    let upper_work = upper_start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("cover upper-support commit failed: {error}"));
    while state.tick() < upper_work.completes_at() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("cover upper-support work failed: {error}"));
    }
    assert_eq!(
        state
            .structures()
            .supports(support)
            .map(|supports| supports.collect::<Vec<_>>()),
        Some(vec![middle])
    );

    let boards = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    let panel_mass = Mass::from_milligrams(6_500_000);
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        boards,
        panel_mass,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("cover board material failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("cover matter baseline failed: {error}"))
        .total();
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(boards)),
    )
    .unwrap_or_else(|error| panic!("cover board selection failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("cover board selection commit failed: {error}"));
    assert_eq!(
        assess_player_overhead_cover(&registries, &state),
        Some(PlayerOverheadCoverAssessment::Open)
    );

    let input = PlayerDirectControlInput::NEUTRAL
        .with_secondary(SecondaryControlIntent::use_active(forward_up_view()));
    let queued = advance_control_tick_with_direct_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("cover direct-use queue failed: {error}"));
    assert_eq!(
        queued.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    let work = loop {
        let control = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("cover direct-use continuation failed: {error}"));
        match control.secondary_action() {
            Some(SecondaryActionOutcome::StructuralConstructionStarted(work)) => break *work,
            None => {}
            other => panic!("cover direct-use returned unexpected outcome: {other:?}"),
        }
    };
    let panel = work.element();
    let record = state
        .structures()
        .get_element(panel)
        .unwrap_or_else(|| panic!("cover panel disappeared after construction start"));
    assert!(record.bounds().has_voxel(panel_position));
    assert_eq!(record.embodied_mass(), panel_mass);
    assert_eq!(record.lifecycle(), StructuralLifecycle::Planned);
    assert!(record.geometry().covers_horizontal_footprint());
    assert_eq!(
        record.geometry().length(),
        BASIC_TIMBER_COVER_PANEL_THICKNESS
    );
    assert_eq!(
        record.geometry().cross_section(),
        BASIC_TIMBER_COVER_PANEL_AREA
    );
    assert_eq!(
        state
            .structures()
            .supports(panel)
            .map(|supports| supports.collect::<Vec<_>>()),
        Some(vec![support])
    );
    assert_eq!(
        assess_player_overhead_cover(&registries, &state),
        Some(PlayerOverheadCoverAssessment::Open),
        "planned panel must not protect the player before hand-setting completes"
    );

    while state.tick() < work.completes_at() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("cover hand-setting failed: {error}"));
    }
    assert_eq!(
        state
            .structures()
            .get_element(panel)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Active)
    );
    assert_eq!(
        assess_player_overhead_cover(&registries, &state),
        Some(PlayerOverheadCoverAssessment::Covered)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("cover matter audit failed: {error}"))
            .total(),
        matter_before
    );

    let comfort_before = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("cover survival assessment disappeared"))
        .thermal_comfort_ppm();
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("covered survival tick failed: {error}"));
    let comfort_after = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("cover survival result disappeared"))
        .thermal_comfort_ppm();
    assert_eq!(
        comfort_before.saturating_sub(comfort_after),
        registries
            .survival()
            .physiology()
            .thermal_comfort()
            .overhead_cover_loss_ppm_per_tick()
    );
}

fn seed_surface(
    registries: &crate::registry::Registries,
    state: &mut AppState,
    position: VoxelCoord,
    commodity: CommodityKey,
    mass: Mass,
) -> SurfaceResourceId {
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Opportunity,
        position,
        commodity,
        mass,
        TEMPERATURE,
        MaterialComposition::pure(commodity.material()),
    )
    .unwrap_or_else(|error| panic!("surface test resource specification failed: {error}"));
    insert_surface_resource_for_test(registries, state, spec)
        .unwrap_or_else(|error| panic!("surface test resource creation failed: {error}"))
}

#[test]
fn player_surface_gathering_options_are_local_natural_and_revision_bound() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let remote_position = VoxelCoord::new(1, 0, 0);
    let stone_commodity = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let stone = seed_surface(
        &registries,
        &mut state,
        position,
        stone_commodity,
        Mass::from_milligrams(2_000_000),
    );
    let _remote = seed_surface(
        &registries,
        &mut state,
        remote_position,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(500_000),
    );
    let drop_spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(100_000),
        TEMPERATURE,
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap_or_else(|error| panic!("gathering-option drop specification failed: {error}"));
    let _drop = insert_surface_resource_for_test(&registries, &mut state, drop_spec)
        .unwrap_or_else(|error| panic!("gathering-option drop creation failed: {error}"));
    let _carried = admit_player(&registries, &mut state, position);

    let options = player_surface_gathering_options(&registries, &state)
        .unwrap_or_else(|error| panic!("player gathering option projection failed: {error}"));
    assert_eq!(options.len(), 1);
    let option = options[0];
    assert_eq!(option.resource(), stone);
    assert_eq!(option.commodity(), stone_commodity);
    assert_eq!(option.source_mass(), Mass::from_milligrams(2_000_000));
    assert_eq!(option.direct_batch_mass(), Mass::from_milligrams(1_000_000));
    assert_eq!(
        option.maximum_batch_mass(),
        Mass::from_milligrams(5_000_000)
    );
    assert_eq!(
        option.duration_for_mass(Mass::from_milligrams(1_000_000)),
        Some(TickSpan::new(6))
    );
    assert!(option.exertion().energy_cost_per_tick() > Energy::ZERO);

    validate_start_player_surface_gathering_option(
        &registries,
        &state,
        option,
        Mass::from_milligrams(1_000_000),
    )
    .unwrap_or_else(|error| panic!("player gathering option start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("player gathering option commit failed: {error}"));
    let completes_at = match state.player_work().active() {
        Some(crate::labor::PlayerWork::SurfaceGathering { work }) => work.completes_at(),
        other => panic!("player gathering option did not start gathering work: {other:?}"),
    };
    while state.tick() < completes_at {
        let _ = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("player gathering option completion failed: {error}"));
    }

    let before_stale = state.clone();
    assert!(matches!(
        validate_start_player_surface_gathering_option(
            &registries,
            &state,
            option,
            Mass::from_milligrams(1),
        ),
        Err(SurfaceGatheringError::StalePlayerOption {
            expected_surface_revision,
            actual_surface_revision,
        }) if expected_surface_revision != actual_surface_revision
    ));
    assert_eq!(state, before_stale);
}

#[test]
fn one_shot_secondary_use_composes_selected_food_without_exposing_lot_identity() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let carried = admit_player(&registries, &mut state, position);
    let commodity = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        Mass::from_milligrams(500_000),
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("secondary-use food fixture failed: {error}"));
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(commodity)),
    )
    .unwrap_or_else(|error| panic!("secondary-use selection failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("secondary-use selection commit failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("secondary-use depletion tick failed: {error}"));
    let input = PlayerDirectControlInput::NEUTRAL
        .with_secondary(SecondaryControlIntent::use_active(positive_x_view()));

    let first = advance_control_tick_with_direct_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("secondary-use first control failed: {error}"));
    assert_eq!(
        first.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );

    let started = loop {
        let control = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("secondary-use neutral continuation failed: {error}"));
        match control.secondary_action() {
            Some(SecondaryActionOutcome::EatingStarted(outcome)) => break outcome.clone(),
            None => {}
            other => panic!("secondary-use returned unexpected outcome: {other:?}"),
        }
    };
    assert!(!started.total_mass().is_zero());
    assert!(matches!(
        state.player_work().active(),
        Some(crate::labor::PlayerWork::Eating { .. })
    ));

    let busy = advance_control_tick_with_direct_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("secondary-use busy control failed: {error}"));
    assert_eq!(busy.secondary_action(), None);
    assert!(matches!(
        state.player_work().active(),
        Some(crate::labor::PlayerWork::Eating { .. })
    ));
}

#[test]
fn dropping_queued_food_does_not_eat_it_after_automatic_repickup() {
    for keep_use_pressed in [false, true] {
        let registries = build_registries();
        let mut state = AppState::new();
        let position = VoxelCoord::new(0, 0, 0);
        let carried = admit_player(&registries, &mut state, position);
        let commodity = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
        let mass = Mass::from_milligrams(500_000);
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            commodity,
            mass,
            TEMPERATURE,
        )
        .unwrap();
        validate_set_active_selection(
            &registries,
            &state,
            Some(PlayerActiveSelection::material(commodity)),
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
        let _ = advance_tick(&registries, &mut state).unwrap();
        let queued = advance_control_tick_with_direct_input(
            &registries,
            &mut state,
            PlayerDirectControlInput::NEUTRAL
                .with_secondary(SecondaryControlIntent::use_active(positive_x_view())),
        )
        .unwrap();
        assert_eq!(
            queued.secondary_action(),
            Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
        );
        assert!(state.logistics().pending_secondary_use().is_some());
        let mut drop_input = PlayerDirectControlInput::NEUTRAL.with_drop_active_selection();
        if keep_use_pressed {
            drop_input =
                drop_input.with_secondary(SecondaryControlIntent::use_active(positive_x_view()));
        }
        let dropped =
            advance_control_tick_with_direct_input(&registries, &mut state, drop_input).unwrap();
        assert_eq!(
            dropped.secondary_action(),
            None,
            "a simultaneous held-use press must not requeue food while dropping it"
        );
        assert!(matches!(
            dropped.active_selection_drop(),
            Some(ActiveSelectionDropOutcome::MaterialDropped(outcome))
                if outcome.dropped_mass() == mass
        ));
        assert!(
            state.logistics().pending_secondary_use().is_none(),
            "explicit drop must cancel the older queued food use"
        );
        let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
        let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
            .unwrap()
            .into_state(&registries)
            .unwrap();
        let matter_before = calculate_matter_accounting(&state).unwrap().total();
        let mut repicked = Mass::ZERO;
        for _ in 2..CONTROL_TICKS_PER_SIMULATION_TICK {
            let current = advance_control_tick(&registries, &mut state).unwrap();
            let restored = advance_control_tick(&registries, &mut replay).unwrap();
            assert_eq!(current, restored);
            assert_eq!(
                current.secondary_action(),
                None,
                "dropping selected food must not leave an after-the-fact meal"
            );
            if let Some(pickup) = current.surface_drop_pickup() {
                repicked = repicked.checked_add(pickup.picked_mass()).unwrap();
            }
        }
        assert_eq!(
            repicked, mass,
            "automatic pickup crosses the original buffered-use boundary"
        );
        assert_eq!(state.player_work().active(), None);
        assert_eq!(
            state
                .inventory()
                .get_stockpile(carried)
                .unwrap()
                .get_mass(commodity),
            mass
        );
        assert_eq!(
            calculate_matter_accounting(&state).unwrap().total(),
            matter_before
        );
        assert_eq!(state, replay);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }
}

#[test]
fn dropping_queued_food_on_due_boundary_supersedes_the_meal() {
    for keep_use_pressed in [false, true] {
        let registries = build_registries();
        let mut state = AppState::new();
        let position = VoxelCoord::new(0, 0, 0);
        let carried = admit_player(&registries, &mut state, position);
        let commodity = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
        let mass = Mass::from_milligrams(500_000);
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            commodity,
            mass,
            TEMPERATURE,
        )
        .unwrap();
        validate_set_active_selection(
            &registries,
            &state,
            Some(PlayerActiveSelection::material(commodity)),
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
        let _ = advance_tick(&registries, &mut state).unwrap();
        let queued = advance_control_tick_with_direct_input(
            &registries,
            &mut state,
            PlayerDirectControlInput::NEUTRAL
                .with_secondary(SecondaryControlIntent::use_active(positive_x_view())),
        )
        .unwrap();
        assert_eq!(
            queued.secondary_action(),
            Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
        );
        for _ in 1..CONTROL_TICKS_PER_SIMULATION_TICK - 1 {
            let step = advance_control_tick(&registries, &mut state).unwrap();
            assert_eq!(step.secondary_action(), None);
        }
        assert!(state.logistics().pending_secondary_use().is_some());
        let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
        let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
            .unwrap()
            .into_state(&registries)
            .unwrap();
        let matter_before = calculate_matter_accounting(&state).unwrap().total();
        let mut drop = PlayerDirectControlInput::NEUTRAL.with_drop_active_selection();
        if keep_use_pressed {
            drop = drop.with_secondary(SecondaryControlIntent::use_active(positive_x_view()));
        }
        let current =
            advance_control_tick_with_direct_input(&registries, &mut state, drop).unwrap();
        let restored =
            advance_control_tick_with_direct_input(&registries, &mut replay, drop).unwrap();
        assert_eq!(current, restored);
        assert_eq!(current.secondary_action(), None);
        assert!(
            matches!(
                current.active_selection_drop(),
                Some(ActiveSelectionDropOutcome::MaterialDropped(outcome))
                    if outcome.dropped_mass() == mass
            ),
            "due-boundary drop must take precedence over prior food use: {:?}",
            current.active_selection_drop()
        );
        assert!(state.player_work().active().is_none());
        assert!(state.logistics().pending_secondary_use().is_none());
        assert_eq!(
            state
                .inventory()
                .get_stockpile(carried)
                .unwrap()
                .get_mass(commodity),
            Mass::ZERO
        );
        assert_eq!(
            calculate_matter_accounting(&state).unwrap().total(),
            matter_before
        );
        assert_eq!(state, replay);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }
}

#[test]
fn explicit_storage_placement_supersedes_pending_contextual_use_before_boundary() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let carried = admit_player(&registries, &mut state, position);
    let commodity = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let mass = Mass::from_milligrams(500_000);
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        mass,
        TEMPERATURE,
    )
    .unwrap();
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(commodity)),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let _ = advance_tick(&registries, &mut state).unwrap();
    let queued = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(positive_x_view())),
    )
    .unwrap();
    assert_eq!(
        queued.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    assert!(state.logistics().pending_secondary_use().is_some());
    let matter_before = calculate_matter_accounting(&state).unwrap().total();
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut restored = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let placement = PlayerDirectControlInput::NEUTRAL.with_secondary(
        SecondaryControlIntent::place_storage(STORAGE_ROUGH_TIMBER_FIELD_BOX, positive_x_view()),
    );
    let current =
        advance_control_tick_with_direct_input(&registries, &mut state, placement).unwrap();
    let replay =
        advance_control_tick_with_direct_input(&registries, &mut restored, placement).unwrap();
    assert_eq!(current, replay);
    assert!(
        matches!(
            current.secondary_action(),
            Some(SecondaryActionOutcome::StorageTargetRejected(_))
        ),
        "the explicit storage command should resolve immediately, not masquerade as queued use: {:?}",
        current.secondary_action()
    );
    assert!(
        state.logistics().pending_secondary_use().is_none(),
        "replaced one-shot use must not eat automatically at the next boundary"
    );
    for _ in 2..CONTROL_TICKS_PER_SIMULATION_TICK {
        let current = advance_control_tick(&registries, &mut state).unwrap();
        let replay = advance_control_tick(&registries, &mut restored).unwrap();
        assert_eq!(current, replay);
        assert_eq!(current.secondary_action(), None);
    }
    assert_eq!(state, restored);
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(commodity),
        mass
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn newly_selected_contextual_use_replaces_older_buffer_before_slow_boundary() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let carried = admit_player(&registries, &mut state, position);
    let commodity = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let mass = Mass::from_milligrams(500_000);
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        mass,
        TEMPERATURE,
    )
    .unwrap();
    let water_volume = Volume::from_microliters(1_000_000);
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        water_volume,
        FLUID_WATER,
        water_volume,
        TEMPERATURE,
    )
    .unwrap();
    validate_place_fluid_store(&state, water, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(commodity)),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let _ = advance_tick(&registries, &mut state).unwrap();
    let queued = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(positive_x_view())),
    )
    .unwrap();
    assert_eq!(
        queued.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary),
    );
    let empty_slot = PlayerQuickAccessSlot::new(1).unwrap();
    let redirected = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_quick_access_slot(empty_slot)
            .with_secondary(SecondaryControlIntent::use_active(positive_x_view())),
    )
    .unwrap();
    assert!(redirected.quick_access_selection().is_some());
    assert_eq!(
        redirected.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary),
    );
    assert_eq!(state.logistics().active_selection(), None);
    assert_eq!(
        state
            .logistics()
            .pending_secondary_use()
            .map(|pending| pending.selection()),
        Some(None),
        "the newest one-shot use must bind the newly selected empty hand",
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut restored = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let mut last = None;
    for _ in 2..CONTROL_TICKS_PER_SIMULATION_TICK {
        let current = advance_control_tick(&registries, &mut state).unwrap();
        let replay = advance_control_tick(&registries, &mut restored).unwrap();
        assert_eq!(current, replay);
        last = Some(current);
    }
    let boundary = last.unwrap();
    assert!(
        matches!(
            boundary.secondary_action(),
            Some(SecondaryActionOutcome::DrinkingStarted(outcome)) if outcome.store() == water
        ),
        "the new water action must execute instead of eating or canceling as stale: {:?}",
        boundary.secondary_action(),
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(commodity),
        mass,
    );
    assert_eq!(state, restored);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn queued_secondary_use_yields_to_same_boundary_quick_access_and_current_use() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let carried = admit_player(&registries, &mut state, position);
    let commodity = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let mass = Mass::from_milligrams(500_000);
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        mass,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("queued-context food fixture failed: {error}"));
    let source_volume = Volume::from_microliters(1_000_000);
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        source_volume,
        FLUID_WATER,
        source_volume,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("queued-context water fixture failed: {error}"));
    validate_place_fluid_store(&state, water, position)
        .unwrap_or_else(|error| panic!("queued-context water placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("queued-context water placement commit failed: {error}"));
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(commodity)),
    )
    .unwrap_or_else(|error| panic!("queued-context selection failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("queued-context selection commit failed: {error}"));

    let queued = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(positive_x_view())),
    )
    .unwrap_or_else(|error| panic!("queued-context use failed: {error}"));
    assert_eq!(
        queued.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    while state.control_tick().value() + 1 < CONTROL_TICKS_PER_SIMULATION_TICK {
        let control = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("queued-context continuation failed: {error}"));
        assert_eq!(control.secondary_action(), None);
    }
    let empty_slot = PlayerQuickAccessSlot::new(1)
        .unwrap_or_else(|| panic!("queued-context empty quick-access slot disappeared"));
    let boundary = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_quick_access_slot(empty_slot)
            .with_secondary(SecondaryControlIntent::use_active(positive_x_view())),
    )
    .unwrap_or_else(|error| panic!("queued-context boundary slot/use failed: {error}"));
    assert!(boundary.quick_access_selection().is_some());
    assert_eq!(state.logistics().active_selection(), None);
    let drinking = match boundary.secondary_action() {
        Some(SecondaryActionOutcome::DrinkingStarted(outcome)) => *outcome,
        other => panic!("same-boundary replacement use did not start drinking: {other:?}"),
    };
    assert_eq!(drinking.store(), water);
    assert!(state.logistics().pending_secondary_use().is_none());
    assert!(matches!(
        state.player_work().active(),
        Some(crate::labor::PlayerWork::Drinking { .. })
    ));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.get_mass(commodity)),
        Some(mass)
    );
}

#[test]
fn persisted_state_rejects_secondary_use_left_buffered_on_its_due_boundary() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let carried = admit_player(&registries, &mut state, position);
    let commodity = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        Mass::from_milligrams(500_000),
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("boundary-buffer food fixture failed: {error}"));
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(commodity)),
    )
    .unwrap_or_else(|error| panic!("boundary-buffer selection failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("boundary-buffer selection commit failed: {error}"));
    let queued = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(positive_x_view())),
    )
    .unwrap_or_else(|error| panic!("boundary-buffer use failed: {error}"));
    assert_eq!(
        queued.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );

    apply_clock_advance(&mut state, SimulationTick::new(1));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(
        validate_persistable_state(&registries, &state),
        Err(LoadError::PendingDirectActionAtSimulationBoundary)
    );
}

#[path = "mod_tests/empty_hand.rs"]
mod empty_hand;

#[test]
fn secondary_use_refuses_ambiguous_physical_food_cohorts() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let carried = admit_player(&registries, &mut state, position);
    let commodity = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    for temperature in [
        Temperature::from_millikelvin(290_000),
        Temperature::from_millikelvin(300_000),
    ] {
        let _ = deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            commodity,
            Mass::from_milligrams(250_000),
            temperature,
        )
        .unwrap_or_else(|error| panic!("ambiguous secondary-use food fixture failed: {error}"));
    }
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(commodity)),
    )
    .unwrap_or_else(|error| panic!("ambiguous secondary-use selection failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("ambiguous secondary-use selection commit failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("ambiguous secondary-use depletion tick failed: {error}"));
    let input = PlayerDirectControlInput::NEUTRAL
        .with_secondary(SecondaryControlIntent::use_active(positive_x_view()));

    let queued = advance_control_tick_with_direct_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("ambiguous secondary-use queue failed: {error}"));
    assert_eq!(
        queued.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    let outcome = loop {
        let control = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("ambiguous secondary-use continuation failed: {error}"));
        match control.secondary_action() {
            None => {}
            Some(outcome) => break outcome.clone(),
        }
    };
    assert_eq!(
        outcome,
        SecondaryActionOutcome::AmbiguousMaterialStacks { count: 2 }
    );
    assert_eq!(state.player_work().active(), None);
}

#[test]
fn secondary_use_places_selected_storage_body_through_canonical_storage_placement() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WORLD_SEED,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("secondary placement terrain failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("secondary placement terrain commit failed: {error}"));
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(0, *y, 0)) == Some(BLOCK_STATE_STONE))
        .unwrap_or_else(|| panic!("secondary placement terrain has no solid surface"));
    let player_position = VoxelCoord::new(0, surface_y + 1, 0);
    let eye_y = player_position.y() + 1;
    for x in 1..=4 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(x, eye_y, 0),
            BlockStateId::AIR,
        );
    }
    let target = VoxelCoord::new(3, eye_y, 0);
    set_generated_block_for_test(registries.world(), &mut state, target, BLOCK_STATE_STONE);
    let placement = VoxelCoord::new(2, eye_y, 0);
    let carried = admit_player(&registries, &mut state, player_position);
    let definition = registries
        .storage()
        .get(STORAGE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap_or_else(|| panic!("secondary placement storage definition disappeared"));
    let commodity = CommodityKey::new(MATERIAL_WOOD, FORM_ROUGH_BOX_BODY);
    let body_mass = definition.assembly_profile().input_mass();
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        body_mass,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("secondary placement body fixture failed: {error}"));
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(commodity)),
    )
    .unwrap_or_else(|error| panic!("secondary placement selection failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("secondary placement selection commit failed: {error}"));

    let unsupported = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(positive_x_view())),
    )
    .unwrap_or_else(|error| panic!("unsupported storage placement control failed: {error}"));
    assert_eq!(
        unsupported.secondary_action(),
        Some(&SecondaryActionOutcome::StoragePlacementUnsupported {
            position: placement
        })
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.get_mass(commodity)),
        Some(body_mass),
        "unsupported storage placement must not consume its enclosure material"
    );
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(placement.x(), placement.y() - 1, placement.z()),
        BLOCK_STATE_STONE,
    );
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("secondary placement matter baseline failed: {error}"));

    let mut framed = state.clone();
    let bounds = VoxelBounds::new(
        placement,
        VoxelCoord::new(placement.x() + 1, placement.y() + 1, placement.z() + 1),
    )
    .unwrap_or_else(|error| panic!("storage obstruction bounds failed: {error}"));
    let geometry = StructuralElementGeometry::new(
        bounds,
        BASIC_TIMBER_POST_LENGTH,
        BASIC_TIMBER_POST_CROSS_SECTION,
    )
    .unwrap_or_else(|error| panic!("storage obstruction geometry failed: {error}"));
    let frame = add_structural_element(
        &registries,
        &mut framed,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        true,
    )
    .unwrap_or_else(|error| panic!("storage obstruction creation failed: {error}"));
    materialize_structural_element_for_test(&registries, &mut framed, frame, FORM_LOG);
    let obstructed = advance_control_tick_with_direct_input(
        &registries,
        &mut framed,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(positive_x_view())),
    )
    .unwrap_or_else(|error| panic!("structurally obstructed placement control failed: {error}"));
    assert_eq!(
        obstructed.secondary_action(),
        Some(&SecondaryActionOutcome::StoragePlacementLocationOccupied {
            position: placement
        })
    );
    assert_eq!(
        framed
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.get_mass(commodity)),
        Some(body_mass),
        "structural obstruction must not consume enclosure material"
    );

    // The same ordinary pointed placement path must expose actual piled industrial residue as
    // a recoverable foundation problem, without turning all scattered chips into solid walls.
    let residue = CommodityKey::new(MATERIAL_WOOD, FORM_CHIP);
    for (waste_mass, blocked) in [
        (Mass::from_milligrams(10_000_000), false),
        (Mass::from_milligrams(20_000_000), true),
    ] {
        let mut exposed = state.clone();
        let drop = SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            placement,
            residue,
            waste_mass,
            TEMPERATURE,
            MaterialComposition::pure(MATERIAL_WOOD),
        )
        .unwrap_or_else(|error| panic!("exposed footing fixture failed: {error}"));
        insert_surface_resource_for_test(&registries, &mut exposed, drop)
            .unwrap_or_else(|error| panic!("exposed footing generation failed: {error}"));
        let outcome = advance_control_tick_with_direct_input(
            &registries,
            &mut exposed,
            PlayerDirectControlInput::NEUTRAL
                .with_secondary(SecondaryControlIntent::use_active(positive_x_view())),
        )
        .unwrap_or_else(|error| panic!("exposed storage use failed: {error}"));
        if blocked {
            assert_eq!(
                outcome.secondary_action(),
                Some(&SecondaryActionOutcome::StoragePlacementRejected {
                    definition: STORAGE_ROUGH_TIMBER_FIELD_BOX,
                    error: PlayerStoragePlacementError::LooseResidueFooting {
                        position: placement,
                        compact_depth_um: 30_769,
                    },
                }),
                "deep exposed chips must block real targeted enclosure placement"
            );
            assert_eq!(
                exposed
                    .inventory()
                    .get_stockpile(carried)
                    .map(|record| record.get_mass(commodity)),
                Some(body_mass),
                "failed work on a debris-covered site must retain the box body"
            );
        } else {
            assert!(
                matches!(
                    outcome.secondary_action(),
                    Some(SecondaryActionOutcome::StoragePlaced {
                        definition: STORAGE_ROUGH_TIMBER_FIELD_BOX,
                        ..
                    })
                ),
                "thin residue must not arbitrarily prohibit ordinary building"
            );
        }
    }

    let control = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(positive_x_view())),
    )
    .unwrap_or_else(|error| panic!("secondary placement control failed: {error}"));
    let stockpile = match control.secondary_action() {
        Some(SecondaryActionOutcome::StoragePlaced {
            definition,
            stockpile,
        }) if *definition == STORAGE_ROUGH_TIMBER_FIELD_BOX => *stockpile,
        other => panic!("secondary placement returned unexpected outcome: {other:?}"),
    };
    assert_eq!(
        state.logistics().stationary_stockpile_position(stockpile),
        Some(placement)
    );
    let placed = state
        .inventory()
        .get_stockpile(stockpile)
        .unwrap_or_else(|| panic!("secondary placed storage disappeared"));
    assert_eq!(
        placed.enclosure().map(|enclosure| enclosure.definition()),
        Some(STORAGE_ROUGH_TIMBER_FIELD_BOX)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.get_mass(commodity)),
        Some(Mass::ZERO)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("secondary placement matter audit failed: {error}"))
            .total(),
        matter_before.total()
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let occupied = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(positive_x_view())),
    )
    .unwrap_or_else(|error| panic!("occupied secondary placement control failed: {error}"));
    assert_eq!(
        occupied.secondary_action(),
        Some(&SecondaryActionOutcome::StoragePlacementLocationOccupied {
            position: placement,
        })
    );
}

#[test]
fn explicit_multimaterial_spoil_bay_places_in_generated_world_without_losing_material() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WORLD_SEED,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("multi-material storage terrain rejected: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("multi-material storage terrain commit failed: {error}"));
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(0, *y, 0)) == Some(BLOCK_STATE_STONE))
        .unwrap_or_else(|| panic!("multi-material storage terrain has no surface"));
    let actor = VoxelCoord::new(0, surface_y + 1, 0);
    let eye_y = actor.y() + 1;
    for x in 1..=4 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(x, eye_y, 0),
            BlockStateId::AIR,
        );
    }
    let target = VoxelCoord::new(3, eye_y, 0);
    set_generated_block_for_test(registries.world(), &mut state, target, BLOCK_STATE_STONE);
    let placement = VoxelCoord::new(2, eye_y, 0);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(placement.x(), placement.y() - 1, placement.z()),
        BLOCK_STATE_STONE,
    );
    let carried = admit_player(&registries, &mut state, actor);
    let definition = registries
        .storage()
        .get(STORAGE_CLAY_LINED_SPOIL_BAY)
        .unwrap_or_else(|| panic!("authored spoil-bay definition missing"));
    assert_eq!(definition.assembly_profile().inputs().len(), 2);
    for input in definition.assembly_profile().inputs() {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            input.commodity(),
            input.mass(),
            TEMPERATURE,
        )
        .unwrap_or_else(|error| panic!("spoil-bay construction input failed: {error}"));
    }
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("spoil-bay placement baseline failed: {error}"))
        .total();
    let outcome = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL.with_secondary(SecondaryControlIntent::place_storage(
            STORAGE_CLAY_LINED_SPOIL_BAY,
            positive_x_view(),
        )),
    )
    .unwrap_or_else(|error| panic!("spoil-bay direct placement failed: {error}"));
    let stockpile = match outcome.secondary_action() {
        Some(SecondaryActionOutcome::StoragePlaced {
            definition,
            stockpile,
        }) if *definition == STORAGE_CLAY_LINED_SPOIL_BAY => *stockpile,
        other => panic!("multi-input spoil bay did not place: {other:?}"),
    };
    assert_eq!(
        state.logistics().stationary_stockpile_position(stockpile),
        Some(placement)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .unwrap_or_else(|| panic!("pointed spoil-bay stockpile vanished"))
            .enclosure()
            .map(|enclosure| enclosure.definition()),
        Some(STORAGE_CLAY_LINED_SPOIL_BAY)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("spoil-bay placement matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn secondary_storage_placement_rejects_the_player_body_voxel() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WORLD_SEED,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("body-overlap placement terrain failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("body-overlap placement terrain commit failed: {error}"));
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(0, *y, 0)) == Some(BLOCK_STATE_STONE))
        .unwrap_or_else(|| panic!("body-overlap placement terrain has no solid surface"));
    let player_position = VoxelCoord::new(0, surface_y + 1, 0);
    let carried = admit_player(&registries, &mut state, player_position);
    let definition = registries
        .storage()
        .get(STORAGE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap_or_else(|| panic!("body-overlap storage definition disappeared"));
    let commodity = CommodityKey::new(MATERIAL_WOOD, FORM_ROUGH_BOX_BODY);
    let body_mass = definition.assembly_profile().input_mass();
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        body_mass,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("body-overlap placement material failed: {error}"));
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(commodity)),
    )
    .unwrap_or_else(|error| panic!("body-overlap placement selection failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("body-overlap placement selection commit failed: {error}"));

    let control = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(downward_view())),
    )
    .unwrap_or_else(|error| panic!("body-overlap placement control failed: {error}"));
    assert_eq!(
        control.secondary_action(),
        Some(&SecondaryActionOutcome::StoragePlacementIntersectsPlayer {
            position: player_position,
        })
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.get_mass(commodity)),
        Some(body_mass)
    );
    assert_eq!(state.logistics().stockpile_locations().count(), 0);
}

#[test]
fn impossible_self_placement_does_not_prevent_fueling_a_local_campfire() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WORLD_SEED,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(8, *y, 8)) == Some(BLOCK_STATE_STONE))
        .unwrap();
    let position = VoxelCoord::new(8, surface_y + 1, 8);
    let carried = admit_player(&registries, &mut state, position);
    let stone = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    for (commodity, mass) in [
        (stone, Mass::from_milligrams(1_000_000)),
        (timber, CAMPFIRE_FUEL_MASS),
    ] {
        let _ = deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            commodity,
            mass,
            TEMPERATURE,
        )
        .unwrap();
    }
    let mut no_fire = state.clone();
    let fire =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_RING_CAMPFIRE, carried)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(timber)),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let before_matter = calculate_matter_accounting(&state).unwrap().total();
    let direction = ViewDirection::new(0, -VIEW_DIRECTION_PARTS_PER_MILLION, 0).unwrap();
    let target = crate::world::raycast_player_block(&registries, &state, direction)
        .unwrap()
        .unwrap();
    assert_eq!(target.face(), crate::spatial::CubeFace::Top);
    assert_eq!(target.face().adjacent_voxel(target.voxel()), Some(position));

    let queued = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(direction)),
    )
    .unwrap();
    assert_eq!(
        queued.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    let result = loop {
        let control = advance_control_tick(&registries, &mut state).unwrap();
        if control.secondary_action().is_some() {
            break control;
        }
    };
    assert!(
        matches!(
            result.secondary_action(),
            Some(SecondaryActionOutcome::CombustionFuelAdded { store, released })
                if *store == fire && !released.is_zero()
        ),
        "downward timber use yielded {:?}",
        result.secondary_action()
    );
    assert_eq!(state.structures().elements().count(), 0);
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(timber),
        Mass::ZERO
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        before_matter
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    validate_set_active_selection(
        &registries,
        &no_fire,
        Some(PlayerActiveSelection::material(timber)),
    )
    .unwrap()
    .commit(&mut no_fire)
    .unwrap();
    let no_fire_before = calculate_matter_accounting(&no_fire).unwrap().total();
    let queued = advance_control_tick_with_direct_input(
        &registries,
        &mut no_fire,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(direction)),
    )
    .unwrap();
    assert_eq!(
        queued.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    let rejection = loop {
        let control = advance_control_tick(&registries, &mut no_fire).unwrap();
        if let Some(outcome) = control.secondary_action() {
            break outcome.clone();
        }
    };
    assert_eq!(
        rejection,
        SecondaryActionOutcome::StructuralPlacementIntersectsPlayer { position }
    );
    assert_eq!(no_fire.structures().elements().count(), 0);
    assert_eq!(
        no_fire
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(timber),
        CAMPFIRE_FUEL_MASS
    );
    assert_eq!(
        calculate_matter_accounting(&no_fire).unwrap().total(),
        no_fire_before
    );
    assert_eq!(validate_loaded_state(&registries, &no_fire), Ok(()));
}

#[test]
fn secondary_use_sets_one_grounded_timber_post_on_pointed_terrain_top_face() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WORLD_SEED,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("timber-post terrain failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("timber-post terrain commit failed: {error}"));
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(0, *y, 0)) == Some(BLOCK_STATE_STONE))
        .unwrap_or_else(|| panic!("timber-post terrain has no solid surface"));
    let player_position = VoxelCoord::new(0, surface_y + 1, 0);
    let target = VoxelCoord::new(1, surface_y, 0);
    let placement = VoxelCoord::new(1, surface_y + 1, 0);
    set_generated_block_for_test(registries.world(), &mut state, target, BLOCK_STATE_STONE);
    set_generated_block_for_test(registries.world(), &mut state, placement, BlockStateId::AIR);
    let carried = admit_player(&registries, &mut state, player_position);
    let stone = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        stone,
        Mass::from_milligrams(1_000_000),
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("timber-post campfire stone fixture failed: {error}"));
    let local_fire =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_RING_CAMPFIRE, carried)
            .unwrap_or_else(|error| panic!("timber-post local campfire setup failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("timber-post local campfire commit failed: {error}"));
    assert_eq!(
        state
            .energy()
            .get_store(local_fire)
            .map(|store| store.stored()),
        Some(Energy::ZERO)
    );
    let commodity = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let post_mass = Mass::from_milligrams(3_250_000);
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        post_mass,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("timber-post carried material failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("timber-post matter baseline failed: {error}"));
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(commodity)),
    )
    .unwrap_or_else(|error| panic!("timber-post selection failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("timber-post selection commit failed: {error}"));
    let pointed = crate::world::raycast_player_block(&registries, &state, forward_down_view())
        .unwrap_or_else(|error| panic!("timber-post target raycast failed: {error}"))
        .unwrap_or_else(|| panic!("timber-post target raycast missed adjacent terrain"));
    assert_eq!(pointed.face(), crate::spatial::CubeFace::Top);
    assert_eq!(
        pointed.face().adjacent_voxel(pointed.voxel()),
        Some(placement)
    );
    let input = PlayerDirectControlInput::NEUTRAL
        .with_secondary(SecondaryControlIntent::use_active(forward_down_view()));

    let queued = advance_control_tick_with_direct_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("timber-post direct queue failed: {error}"));
    assert_eq!(
        queued.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    let work = loop {
        let control = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("timber-post neutral continuation failed: {error}"));
        match control.secondary_action() {
            Some(SecondaryActionOutcome::StructuralConstructionStarted(work)) => break *work,
            None => {}
            other => panic!("timber-post returned unexpected secondary outcome: {other:?}"),
        }
    };
    let element = work.element();
    let record = state
        .structures()
        .get_element(element)
        .unwrap_or_else(|| panic!("timber-post structural member disappeared"));
    assert!(record.bounds().has_voxel(placement));
    assert_eq!(record.embodied_mass(), post_mass);
    assert_eq!(
        record.lifecycle(),
        crate::structural::StructuralLifecycle::Planned
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.get_mass(commodity)),
        Some(Mass::ZERO)
    );
    assert!(matches!(
        state.player_work().active(),
        Some(crate::labor::PlayerWork::StructuralConstruction { work: active })
            if active.element() == element
    ));
    assert_eq!(
        state
            .energy()
            .get_store(local_fire)
            .map(|store| store.stored()),
        Some(Energy::ZERO),
        "pointed timber placement must not feed a colocated combustion store"
    );

    let mut completions = 0_u32;
    while state.tick() < work.completes_at() {
        let control = advance_control_tick(&registries, &mut state).unwrap_or_else(|error| {
            panic!("timber-post hand-setting control tick failed: {error}")
        });
        if control
            .simulation()
            .is_some_and(|outcome| outcome.structural_construction() == Some(element))
        {
            completions += 1;
        }
    }
    assert_eq!(completions, 1);
    assert_eq!(
        state
            .structures()
            .get_element(element)
            .map(|record| record.lifecycle()),
        Some(crate::structural::StructuralLifecycle::Active)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("timber-post matter audit failed: {error}"))
            .total(),
        matter_before.total()
    );

    let dismantle_input = PlayerDirectControlInput::NEUTRAL
        .with_primary(PrimaryControlIntent::break_target(forward_down_view()));
    let queued = advance_control_tick_with_direct_input(&registries, &mut state, dismantle_input)
        .unwrap_or_else(|error| panic!("timber-post dismantling queue failed: {error}"));
    assert_eq!(
        queued.primary_action(),
        Some(&PrimaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    assert!(state.logistics().pending_primary_break().is_some());

    let mut changed_target = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| {
            panic!("queued timber-post changed-target serialization failed: {error}")
        });
    changed_target["state"]["systems"]["logistics"]["pending_primary_break"]["element"] =
        serde_json::json!(element.value() + 1);
    let changed_target: LoadedSaveEnvelope = serde_json::from_value(changed_target)
        .unwrap_or_else(|error| panic!("queued timber-post changed-target decode failed: {error}"));
    let mut changed_target = changed_target
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("queued timber-post changed-target load failed: {error}"));
    let changed_outcome = loop {
        let control =
            advance_control_tick(&registries, &mut changed_target).unwrap_or_else(|error| {
                panic!("queued timber-post changed-target continuation failed: {error}")
            });
        if let Some(outcome) = control.primary_action() {
            break outcome.clone();
        }
    };
    assert_eq!(
        changed_outcome,
        PrimaryActionOutcome::BufferedContextChanged
    );
    assert_eq!(changed_target.player_work().active(), None);
    assert!(changed_target.structures().get_element(element).is_some());

    let encoded =
        serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("queued timber-post dismantling serialization failed: {error}")
        });
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("queued timber-post dismantling decode failed: {error}"));
    state = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("queued timber-post dismantling load failed: {error}"));

    let dismantling = loop {
        let control = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("timber-post dismantling continuation failed: {error}"));
        match control.primary_action() {
            Some(PrimaryActionOutcome::StructuralDismantlingStarted(start)) => break *start,
            None => {}
            other => panic!("timber-post dismantling returned unexpected outcome: {other:?}"),
        }
    };
    assert_eq!(dismantling.element(), element);
    assert_eq!(dismantling.recovered_mass(), post_mass);
    assert!(matches!(
        state.player_work().active(),
        Some(crate::labor::PlayerWork::StructuralDismantling { work: active })
            if active.element() == element
    ));
    assert!(state.structures().get_element(element).is_some());

    let mut dismantling_completions = 0_u32;
    while state.tick() < dismantling.completes_at() {
        let control = advance_control_tick(&registries, &mut state).unwrap_or_else(|error| {
            panic!("timber-post dismantling completion control failed: {error}")
        });
        if control.simulation().is_some_and(|outcome| {
            outcome
                .structural_dismantling()
                .is_some_and(|completed| completed.element() == element)
        }) {
            dismantling_completions += 1;
        }
    }
    assert_eq!(dismantling_completions, 1);
    assert!(state.structures().get_element(element).is_none());
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.get_mass(commodity)),
        Some(post_mass)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("timber-post recovery matter audit failed: {error}"))
            .total(),
        matter_before.total()
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn active_material_drop_cannot_interrupt_exclusive_gathering_work() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let carried = admit_player(&registries, &mut state, position);
    let commodity = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let mass = Mass::from_milligrams(250_000);
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        mass,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("busy-drop carried material failed: {error}"));
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(commodity)),
    )
    .unwrap_or_else(|error| panic!("busy-drop active selection failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("busy-drop selection commit failed: {error}"));
    let prepared_drop = validate_drop_player_material(&registries, &state, commodity)
        .unwrap_or_else(|error| panic!("busy-drop initial validation failed: {error}"));
    let stone = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Opportunity,
        position,
        stone,
        Mass::from_milligrams(50_000),
        TEMPERATURE,
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap_or_else(|error| panic!("busy-drop opportunity invalid: {error}"));
    let plan = validate_create_surface_resources(&registries, &state, [spec])
        .unwrap_or_else(|error| panic!("busy-drop opportunity creation failed: {error}"));
    let source = plan.first_resource_id();
    plan.commit(&mut state)
        .unwrap_or_else(|error| panic!("busy-drop opportunity commit failed: {error}"));
    validate_start_player_surface_gathering(
        &registries,
        &state,
        SURFACE_GATHERING_HAND_COLLECT_STONE_CHIPS,
        source,
        Mass::from_milligrams(50_000),
    )
    .unwrap_or_else(|error| panic!("busy-drop gathering admission failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("busy-drop gathering commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_drop_player_material(&registries, &state, commodity).err(),
        Some(PlayerMaterialDropError::PlayerBusy)
    );
    assert_eq!(
        prepared_drop.commit(&mut state),
        Err(PlayerMaterialDropCommitError::PlayerBecameBusy)
    );
    assert_eq!(state, before);
    let outcome = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL.with_drop_active_selection(),
    )
    .unwrap_or_else(|error| panic!("busy-drop control failed: {error}"));
    assert_eq!(
        outcome.active_selection_drop(),
        Some(&ActiveSelectionDropOutcome::MaterialRejected(
            PlayerMaterialDropError::PlayerBusy
        ))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.get_mass(commodity)),
        Some(mass)
    );
    assert!(state.player_work().active().is_some());
    let total = calculate_matter_accounting(&before)
        .unwrap_or_else(|error| panic!("busy-drop baseline conservation failed: {error}"))
        .total();
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("busy-drop conservation failed: {error}"))
            .total(),
        total
    );
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("busy-drop serialization failed: {error}"));
    let saved: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("busy-drop decoding failed: {error}"));
    let mut replay = saved
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("busy-drop trusted load failed: {error}"));
    while state.player_work().active().is_some() {
        let next = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("busy-drop completion failed: {error}"));
        let replayed = advance_control_tick(&registries, &mut replay)
            .unwrap_or_else(|error| panic!("busy-drop replay completion failed: {error}"));
        assert_eq!(next, replayed);
    }
    assert_eq!(state, replay);
    let completed_drop = validate_drop_player_material(&registries, &state, commodity)
        .unwrap_or_else(|error| panic!("completed work should release drop: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("completed work drop commit failed: {error}"));
    assert_eq!(completed_drop.dropped_mass(), mass);
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("post-work drop conservation failed: {error}"))
            .total(),
        total
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn active_material_drop_preserves_exact_lot_profiles_and_delays_self_pickup() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let carried = admit_player(&registries, &mut state, position);
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let first_mass = Mass::from_milligrams(120_000);
    let second_mass = Mass::from_milligrams(80_000);
    let first_temperature = Temperature::from_millikelvin(290_000);
    let second_temperature = Temperature::from_millikelvin(310_000);
    for (mass, temperature) in [
        (first_mass, first_temperature),
        (second_mass, second_temperature),
    ] {
        let _ = deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            commodity,
            mass,
            temperature,
        )
        .unwrap_or_else(|error| panic!("active-material drop fixture deposit failed: {error}"));
    }
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("active-material drop matter baseline failed: {error}"));
    let selection = PlayerActiveSelection::material(commodity);
    validate_set_active_selection(&registries, &state, Some(selection))
        .unwrap_or_else(|error| panic!("active-material selection failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("active-material selection commit failed: {error}"));

    let control = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL.with_drop_active_selection(),
    )
    .unwrap_or_else(|error| panic!("active-material direct drop failed: {error}"));
    let dropped = match control.active_selection_drop() {
        Some(ActiveSelectionDropOutcome::MaterialDropped(outcome)) => outcome,
        other => panic!("active-material direct drop returned unexpected outcome: {other:?}"),
    };
    assert_eq!(dropped.commodity(), commodity);
    assert_eq!(
        dropped.dropped_mass(),
        first_mass
            .checked_add(second_mass)
            .unwrap_or_else(|| panic!("active-material fixture mass overflowed"))
    );
    assert_eq!(dropped.position(), position);
    assert_eq!(dropped.resources().len(), 2);
    assert_eq!(state.logistics().active_selection(), Some(selection));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.get_mass(commodity)),
        Some(Mass::ZERO)
    );
    let selection_revision = state.logistics().revision();
    validate_set_active_selection(&registries, &state, Some(selection))
        .unwrap_or_else(|error| panic!("empty selected-material re-selection failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("empty selected-material re-selection commit failed: {error}")
        });
    assert_eq!(state.logistics().revision(), selection_revision);
    let pickup_after = control
        .control_tick()
        .checked_add_steps(super::material_drop::PLAYER_MATERIAL_DROP_PICKUP_DELAY_CONTROL_TICKS)
        .unwrap_or_else(|| panic!("active-material pickup delay overflowed test clock"));
    let mut profiles = dropped
        .resources()
        .iter()
        .map(|resource| {
            let record = state
                .surface()
                .get(*resource)
                .unwrap_or_else(|| panic!("active-material surface parcel disappeared"));
            assert_eq!(record.kind(), SurfaceResourceKind::Drop);
            assert_eq!(record.position(), position);
            assert_eq!(
                record.automatic_pickup_delay_started_at(),
                Some(control.control_tick())
            );
            assert_eq!(
                record
                    .automatic_pickup_delay_started_at()
                    .and_then(|started| started.checked_add_steps(
                        super::material_drop::PLAYER_MATERIAL_DROP_PICKUP_DELAY_CONTROL_TICKS,
                    )),
                Some(pickup_after)
            );
            (record.remaining_mass(), record.temperature())
        })
        .collect::<Vec<_>>();
    profiles.sort_by_key(|(_, temperature)| temperature.millikelvin());
    assert_eq!(
        profiles,
        vec![
            (first_mass, first_temperature),
            (second_mass, second_temperature),
        ]
    );
    let matter_dropped = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("active-material dropped matter audit failed: {error}"));
    assert_eq!(matter_dropped.total(), matter_before.total());

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("active-material drop serialization failed: {error}"));
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("active-material drop decode failed: {error}"));
    let loaded = loaded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("active-material drop trusted load failed: {error}"));
    assert_eq!(loaded, state);

    let first_resource = *dropped
        .resources()
        .first()
        .unwrap_or_else(|| panic!("active-material drop emitted no surface resource"));
    let mut forged =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("active-material forged-delay serialization failed: {error}")
        });
    let resource_key = first_resource.value().to_string();
    forged["state"]["systems"]["surface"]["resources"][resource_key.as_str()]["automatic_pickup_delay_started_at"] =
        serde_json::json!(control.control_tick().value() + 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(forged)
        .unwrap_or_else(|error| panic!("active-material forged-delay decode failed: {error}"));
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Surface(
            SurfaceResourceValidationError::PickupDelayStartedInFuture {
                resource: first_resource,
            }
        )))
    );

    for _ in 1..super::material_drop::PLAYER_MATERIAL_DROP_PICKUP_DELAY_CONTROL_TICKS {
        let outcome = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("active-material pickup-delay tick failed: {error}"));
        assert_eq!(outcome.surface_drop_pickup(), None);
    }
    let pickup = advance_control_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("active-material delayed pickup tick failed: {error}"));
    assert_eq!(pickup.control_tick(), pickup_after);
    assert!(pickup.surface_drop_pickup().is_some());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.get_mass(commodity)),
        Some(first_mass)
    );
    let second_pickup = advance_control_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("active-material second pickup tick failed: {error}"));
    assert!(second_pickup.surface_drop_pickup().is_some());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.get_mass(commodity)),
        Some(
            first_mass
                .checked_add(second_mass)
                .unwrap_or_else(|| panic!("active-material recovered mass overflowed"))
        )
    );
    let matter_after = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("active-material recovered matter audit failed: {error}"));
    assert_eq!(matter_after.total(), matter_before.total());
}

#[test]
fn ordinary_surface_gathering_derives_carried_custody() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let stone = seed_surface(
        &registries,
        &mut state,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(100),
    );
    let carried = admit_player(&registries, &mut state, position);

    validate_start_player_surface_gathering(
        &registries,
        &state,
        SURFACE_GATHERING_HAND_COLLECT_STONE,
        stone,
        Mass::from_milligrams(100),
    )
    .unwrap_or_else(|error| panic!("ordinary surface gathering validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("ordinary surface gathering commit failed: {error}"));

    let work = match state.player_work().active() {
        Some(crate::labor::PlayerWork::SurfaceGathering { work }) => work,
        other => panic!("ordinary surface gathering did not own player work: {other:?}"),
    };
    assert_eq!(work.destination(), carried);
    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("ordinary surface gathering tick failed: {error}"));
    assert_eq!(
        outcome
            .surface_gathering()
            .map(|gathered| gathered.destination()),
        Some(carried)
    );
}

#[test]
fn natural_surface_opportunities_do_not_auto_pick_into_carried_custody() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let mass = Mass::from_milligrams(100_000);
    let stone = seed_surface(
        &registries,
        &mut state,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        mass,
    );
    let carried = admit_player(&registries, &mut state, position);

    let control = advance_control_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("natural-opportunity control tick failed: {error}"));
    assert_eq!(control.surface_drop_pickup(), None);
    let resource = state
        .surface()
        .get(stone)
        .unwrap_or_else(|| panic!("natural surface opportunity disappeared"));
    assert_eq!(resource.kind(), SurfaceResourceKind::Opportunity);
    assert_eq!(resource.remaining_mass(), mass);
    assert_eq!(resource.lifecycle(), SurfaceResourceLifecycle::Available);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO),
        "natural opportunities must still require an explicit gathering action"
    );
}

#[test]
fn stone_chip_surface_matter_is_ordinarily_recoverable() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let mass = Mass::from_milligrams(200_000);
    let chips = seed_surface(
        &registries,
        &mut state,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        mass,
    );
    let carried = admit_player(&registries, &mut state, position);

    validate_start_player_surface_gathering(
        &registries,
        &state,
        SURFACE_GATHERING_HAND_COLLECT_STONE_CHIPS,
        chips,
        mass,
    )
    .unwrap_or_else(|error| panic!("stone-chip gathering validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("stone-chip gathering commit failed: {error}"));
    let completes_at = match state.player_work().active() {
        Some(crate::labor::PlayerWork::SurfaceGathering { work }) => work.completes_at(),
        other => panic!("stone-chip gathering did not own player work: {other:?}"),
    };
    let mut completion = None;
    while state.tick() < completes_at {
        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("stone-chip gathering tick failed: {error}"));
        if let Some(gathered) = outcome.surface_gathering() {
            assert!(completion.replace(gathered.clone()).is_none());
        }
    }
    let completion =
        completion.unwrap_or_else(|| panic!("stone-chip gathering produced no outcome"));
    assert_eq!(completion.resource(), chips);
    assert_eq!(completion.gathered_mass(), mass);
    assert_eq!(completion.remaining_mass(), Mass::ZERO);
    assert!(
        state
            .surface()
            .get(chips)
            .is_some_and(|resource| resource.lifecycle() == SurfaceResourceLifecycle::Depleted)
    );
    let lot = state
        .inventory()
        .get_lot(completion.lot())
        .unwrap_or_else(|| panic!("stone-chip gathering output lot disappeared"));
    assert_eq!(lot.stockpile(), carried);
    assert_eq!(lot.mass(), mass);
    assert_eq!(
        lot.commodity(),
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP)
    );
}

#[test]
fn adjacent_surface_observation_requires_persisted_access_and_hides_exact_state() {
    let registries = build_registries();
    let mut state = AppState::new();
    let player_position = VoxelCoord::new(0, 0, 0);
    let reachable_position = VoxelCoord::new(1, 0, 0);
    let blocked_position = VoxelCoord::new(0, 0, 1);
    let reachable = seed_surface(
        &registries,
        &mut state,
        reachable_position,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(123_456),
    );
    let blocked = seed_surface(
        &registries,
        &mut state,
        blocked_position,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(654_321),
    );
    assert!(
        available_adjacent_surface_resources(&state)
            .next()
            .is_none()
    );
    let _ = admit_player(&registries, &mut state, player_position);
    assert!(
        available_adjacent_surface_resources(&state)
            .next()
            .is_none(),
        "geometric adjacency without persisted traversal authorization must stay hidden"
    );
    add_traversable_edge_for_test(&mut state, player_position, reachable_position);
    assert_eq!(
        available_adjacent_surface_resources(&state)
            .map(|resource| (
                resource.resource(),
                resource.commodity(),
                resource.position()
            ))
            .collect::<Vec<_>>(),
        vec![(
            reachable,
            CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
            reachable_position,
        )]
    );
    assert_ne!(reachable, blocked);
}

#[test]
fn adjacent_surface_observation_preserves_global_identity_order_across_neighbor_indexes() {
    let registries = build_registries();
    let mut state = AppState::new();
    let player = VoxelCoord::new(0, 0, 0);
    let east = VoxelCoord::new(1, 0, 0);
    let west = VoxelCoord::new(-1, 0, 0);
    let far = VoxelCoord::new(100, 0, 100);
    let make = |state: &mut AppState, position| {
        seed_surface(
            &registries,
            state,
            position,
            CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
            Mass::from_milligrams(1_000),
        )
    };
    let first = make(&mut state, east);
    let remote = make(&mut state, far);
    let second = make(&mut state, west);
    let third = make(&mut state, east);
    let _ = admit_player(&registries, &mut state, player);
    // Neighbor coordinates iterate west before east, unlike resource creation order.
    add_traversable_edge_for_test(&mut state, player, west);
    add_traversable_edge_for_test(&mut state, player, east);
    let observed = available_adjacent_surface_resources(&state)
        .map(|resource| (resource.resource(), resource.position()))
        .collect::<Vec<_>>();
    assert_eq!(observed, vec![(first, east), (second, west), (third, east)]);
    assert!(!observed.iter().any(|(id, _)| *id == remote));

    let _ = make(&mut state, far);
    assert_eq!(
        available_adjacent_surface_resources(&state)
            .map(|resource| (resource.resource(), resource.position()))
            .collect::<Vec<_>>(),
        observed,
        "new remote custody must not change nearby choices or their order"
    );
}

fn admit_player(
    registries: &crate::registry::Registries,
    state: &mut AppState,
    position: VoxelCoord,
) -> crate::inventory::StockpileId {
    validate_admit_player(registries, state, position)
        .unwrap_or_else(|error| panic!("surface test player admission failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("surface test player admission commit failed: {error}"))
        .carried_stockpile()
}

fn make_next_tick_fatal(registries: &crate::registry::Registries, state: &mut AppState) {
    let physiology = registries.survival().physiology();
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("surface fatality fixture lost survival player"));
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            Energy::ZERO,
            player.hydration(),
            Vitality::from_parts_per_million_unchecked(
                physiology.starvation_vitality_loss_ppm_per_tick(),
            ),
            player.nutrition(),
            player.vitality_recovery_remainder(),
        ),
    );
}

#[test]
fn fatal_tick_cancels_unfinished_surface_gathering_and_releases_capacity() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let gathered_mass = Mass::from_milligrams(1_000_000);
    let stone = seed_surface(
        &registries,
        &mut state,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        gathered_mass,
    );
    let carried = admit_player(&registries, &mut state, position);
    validate_start_player_surface_gathering(
        &registries,
        &state,
        SURFACE_GATHERING_HAND_COLLECT_STONE,
        stone,
        gathered_mass,
    )
    .unwrap_or_else(|error| panic!("fatal surface gathering start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("fatal surface gathering commit failed: {error}"));
    let work = match state.player_work().active() {
        Some(crate::labor::PlayerWork::SurfaceGathering { work }) => work,
        other => panic!("fatal surface gathering did not own player work: {other:?}"),
    };
    assert!(work.completes_at().value() > state.tick().value() + 1);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.reserved_inbound()),
        Some(gathered_mass)
    );
    make_next_tick_fatal(&registries, &mut state);

    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fatal surface gathering tick failed: {error}"));

    assert!(outcome.surface_gathering().is_none());
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state.survival().player().map(|player| player.vitality()),
        Some(Vitality::ZERO)
    );
    assert_eq!(
        state
            .surface()
            .get(stone)
            .map(|resource| resource.remaining_mass()),
        Some(gathered_mass),
        "canceled gathering must not consume the surface source"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| (record.stored_mass(), record.reserved_inbound())),
        Some((Mass::ZERO, Mass::ZERO)),
        "canceled gathering must release capacity without materializing matter"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn surface_gathering_due_on_fatal_tick_completes_before_attention_release() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let gathered_mass = Mass::from_milligrams(100);
    let stone = seed_surface(
        &registries,
        &mut state,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        gathered_mass,
    );
    let carried = admit_player(&registries, &mut state, position);
    validate_start_player_surface_gathering(
        &registries,
        &state,
        SURFACE_GATHERING_HAND_COLLECT_STONE,
        stone,
        gathered_mass,
    )
    .unwrap_or_else(|error| panic!("fatal due surface gathering start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("fatal due surface gathering commit failed: {error}"));
    let work = match state.player_work().active() {
        Some(crate::labor::PlayerWork::SurfaceGathering { work }) => work,
        other => panic!("fatal due surface gathering did not own player work: {other:?}"),
    };
    assert_eq!(work.completes_at().value(), state.tick().value() + 1);
    make_next_tick_fatal(&registries, &mut state);

    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fatal due surface gathering tick failed: {error}"));

    assert_eq!(
        outcome
            .surface_gathering()
            .map(|gathering| gathering.gathered_mass()),
        Some(gathered_mass)
    );
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state.survival().player().map(|player| player.vitality()),
        Some(Vitality::ZERO)
    );
    assert_eq!(
        state
            .surface()
            .get(stone)
            .map(|resource| resource.remaining_mass()),
        Some(Mass::ZERO)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| (record.stored_mass(), record.reserved_inbound())),
        Some((gathered_mass, Mass::ZERO))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn public_surface_observation_and_start_errors_do_not_reveal_remote_resources() {
    let registries = build_registries();
    let mut state = AppState::new();
    let local_position = VoxelCoord::new(2, 0, -1);
    let remote_position = VoxelCoord::new(8, 0, 4);
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let local = seed_surface(
        &registries,
        &mut state,
        local_position,
        commodity,
        Mass::from_milligrams(100),
    );
    let remote = seed_surface(
        &registries,
        &mut state,
        remote_position,
        commodity,
        Mass::from_milligrams(100),
    );
    let carried = admit_player(&registries, &mut state, local_position);

    let visible = state
        .available_surface_resources()
        .map(SurfaceResourceRecord::id)
        .collect::<Vec<_>>();
    assert_eq!(visible, vec![local]);

    let request = |resource| {
        SurfaceGatheringRequest::new(
            SURFACE_GATHERING_HAND_COLLECT_STONE,
            resource,
            carried,
            Mass::from_milligrams(1),
        )
    };
    assert_eq!(
        validate_start_surface_gathering(&registries, &state, request(remote)).err(),
        Some(SurfaceGatheringError::ResourceUnavailableAtPlayer { resource: remote })
    );
    let unknown = SurfaceResourceId::new(remote.value() + 100);
    assert_eq!(
        validate_start_surface_gathering(&registries, &state, request(unknown)).err(),
        Some(SurfaceGatheringError::ResourceUnavailableAtPlayer { resource: unknown })
    );
}

#[test]
fn surface_gathering_rejects_a_method_authored_for_another_commodity() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let berries_commodity = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let berries = seed_surface(
        &registries,
        &mut state,
        position,
        berries_commodity,
        Mass::from_milligrams(100_000),
    );
    let carried = admit_player(&registries, &mut state, position);
    let before = state.clone();

    assert_eq!(
        validate_start_surface_gathering(
            &registries,
            &state,
            SurfaceGatheringRequest::new(
                SURFACE_GATHERING_HAND_COLLECT_STONE,
                berries,
                carried,
                Mass::from_milligrams(100_000),
            ),
        )
        .err(),
        Some(SurfaceGatheringError::CommodityMismatch {
            method: SURFACE_GATHERING_HAND_COLLECT_STONE,
            expected: CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            actual: berries_commodity,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn trusted_load_rejects_surface_gathering_with_a_method_for_another_commodity() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let stone_commodity = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let stone = seed_surface(
        &registries,
        &mut state,
        position,
        stone_commodity,
        Mass::from_milligrams(100),
    );
    let carried = admit_player(&registries, &mut state, position);
    validate_start_surface_gathering(
        &registries,
        &state,
        SurfaceGatheringRequest::new(
            SURFACE_GATHERING_HAND_COLLECT_STONE,
            stone,
            carried,
            Mass::from_milligrams(100),
        ),
    )
    .unwrap_or_else(|error| panic!("commodity-mismatch load setup failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("commodity-mismatch load commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("commodity-mismatch load serialization failed: {error}"));
    encoded["state"]["systems"]["player_work"]["active"]["SurfaceGathering"]["work"]["method"] =
        serde_json::json!(SURFACE_GATHERING_HAND_FORAGE_BERRIES.value());
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("commodity-mismatch load decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::SurfaceGatheringCommodityMismatch {
                expected: CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
                actual: stone_commodity,
            }
        )))
    );
}

#[test]
fn trusted_load_rejects_remote_surface_gathering_before_source_state() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let stone = seed_surface(
        &registries,
        &mut state,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(100),
    );
    let carried = admit_player(&registries, &mut state, position);
    validate_start_surface_gathering(
        &registries,
        &state,
        SurfaceGatheringRequest::new(
            SURFACE_GATHERING_HAND_COLLECT_STONE,
            stone,
            carried,
            Mass::from_milligrams(100),
        ),
    )
    .unwrap_or_else(|error| panic!("remote trusted-load gathering setup failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("remote trusted-load gathering commit failed: {error}"));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("remote trusted-load gathering serialization failed: {error}")
        });
    encoded["state"]["systems"]["logistics"]["player"]["body"]["foot_position"] = serde_json::json!({
        "x_nanometers": 1_500_000_000_i64,
        "y_nanometers": 0_i64,
        "z_nanometers": 500_000_000_i64
    });
    encoded["state"]["systems"]["player_work"]["active"]["SurfaceGathering"]["work"]["source_mass_before"] =
        serde_json::json!(101_u64);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("remote trusted-load gathering decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::SurfaceGatheringPlayerRemote
        )))
    );
}

#[test]
fn gathered_perishable_retains_ambient_surface_age() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let berries = seed_surface(
        &registries,
        &mut state,
        position,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(100),
    );
    apply_clock_advance(&mut state, SimulationTick::new(10));
    let carried = admit_player(&registries, &mut state, position);

    validate_start_surface_gathering(
        &registries,
        &state,
        SurfaceGatheringRequest::new(
            SURFACE_GATHERING_HAND_FORAGE_BERRIES,
            berries,
            carried,
            Mass::from_milligrams(100),
        ),
    )
    .unwrap_or_else(|error| panic!("aged surface-food gathering validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("aged surface-food gathering commit failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("aged surface-food gathering tick failed: {error}"));

    let lot = state
        .inventory()
        .lot_ids(carried)
        .next()
        .unwrap_or_else(|| panic!("gathered surface food did not enter carried inventory"));
    assert!(matches!(
        assess_food_freshness(&registries, &state, lot),
        Ok(FoodFreshness::Fresh { age, .. }) if age == TickSpan::new(11)
    ));
}

#[test]
fn trusted_load_rejects_active_gathering_into_storage_that_no_longer_accepts_source() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let stone = seed_surface(
        &registries,
        &mut state,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(100),
    );
    let carried = admit_player(&registries, &mut state, position);
    validate_start_surface_gathering(
        &registries,
        &state,
        SurfaceGatheringRequest::new(
            SURFACE_GATHERING_HAND_COLLECT_STONE,
            stone,
            carried,
            Mass::from_milligrams(100),
        ),
    )
    .unwrap_or_else(|error| panic!("trusted-load gathering setup failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("trusted-load gathering setup commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("trusted-load gathering serialization failed: {error}"));
    let storage_profile = &mut encoded["state"]["systems"]["inventory"]["stockpiles"]
        [carried.value().to_string()]["storage_profile"];
    storage_profile["can_store_solid"] = serde_json::json!(false);
    storage_profile["can_store_liquid"] = serde_json::json!(true);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("trusted-load gathering tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::SurfaceGatheringDestinationStorage(
                StockpileStorageError::PhaseNotAccepted {
                    stockpile: carried,
                    phase: MaterialPhase::Solid,
                }
            )
        )))
    );
}
