//! Automatic loose-matter pickup and neutral slow-step eligibility contracts.

use super::{
    SurfaceResourceKind, SurfaceResourceSpec, insert_surface_resource_for_test,
    validate_drop_player_material,
};
use crate::content::{FORM_CHIP, FORM_LOG, MATERIAL_STONE, MATERIAL_WOOD, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::core::time::{CONTROL_TICKS_PER_SIMULATION_TICK, ControlTick, SimulationTick};
use crate::inventory::deposit_lot_for_test;
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::{TickError, advance_control_tick, advance_tick};
use crate::spatial::VoxelCoord;
use crate::survival::{Vitality, player_record_with_thermal_comfort};

#[test]
fn automatic_pickup_respects_tool_weight_across_fast_forward_and_recovery() {
    use crate::content::{EQUIPMENT_STONE_PICK, FORM_HANDLE, FORM_TOOL};
    use crate::equipment::validate_assemble_equipment;
    use crate::inventory::add_solid_stockpile_for_test;
    use crate::logistics::{
        assess_player_carrying, place_equipment_for_fixture, validate_drop_equipment,
        validate_pickup_equipment,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let material = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    let dropped_mass = Mass::from_milligrams(25_000_000);
    let source =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000)).unwrap();
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    let tool = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, source)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    place_equipment_for_fixture(&mut state, tool, position);
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        position,
        material,
        dropped_mass,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let drop = insert_surface_resource_for_test(&registries, &mut state, spec).unwrap();
    let carried = validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    validate_pickup_equipment(&registries, &state, tool)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let carrying = assess_player_carrying(&state).unwrap();
    assert_eq!(carrying.capacity(), dropped_mass);
    assert_eq!(carrying.equipment(), Mass::from_milligrams(1_000_000));
    assert_eq!(carrying.available(), Mass::from_milligrams(24_000_000));
    let original_matter = calculate_matter_accounting(&state).unwrap().total();

    let pickup = advance_control_tick(&registries, &mut state)
        .unwrap()
        .surface_drop_pickup()
        .unwrap();
    assert_eq!(pickup.resource(), drop);
    assert_eq!(pickup.picked_mass(), Mass::from_milligrams(24_000_000));
    assert_eq!(
        state.surface().get(drop).unwrap().remaining_mass(),
        Mass::from_milligrams(1_000_000)
    );
    assert_eq!(
        assess_player_carrying(&state).unwrap().available(),
        Mass::ZERO
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        original_matter
    );

    // A full pack must not force 71 redundant 20 Hz pickup samples. The
    // skipped neutral interval must match exactly replaying those samples.
    let mut replay = state.clone();
    let slow = advance_tick(&registries, &mut state).unwrap();
    let mut last_control = None;
    for _ in 1..CONTROL_TICKS_PER_SIMULATION_TICK {
        last_control = Some(advance_control_tick(&registries, &mut replay).unwrap());
    }
    assert_eq!(slow, *last_control.unwrap().simulation().unwrap());
    assert_eq!(state, replay);
    assert_eq!(
        state.surface().get(drop).unwrap().remaining_mass(),
        Mass::from_milligrams(1_000_000)
    );

    validate_drop_equipment(&registries, &state, tool)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let carrying = assess_player_carrying(&state).unwrap();
    assert_eq!(carrying.capacity(), dropped_mass);
    assert_eq!(carrying.equipment(), Mass::ZERO);
    assert_eq!(carrying.available(), Mass::from_milligrams(1_000_000));
    let rest = advance_control_tick(&registries, &mut state)
        .unwrap()
        .surface_drop_pickup()
        .unwrap();
    assert_eq!(rest.resource(), drop);
    assert_eq!(rest.picked_mass(), Mass::from_milligrams(1_000_000));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(material),
        dropped_mass
    );
    assert_eq!(
        assess_player_carrying(&state).unwrap().available(),
        Mass::ZERO
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        original_matter
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, restored);
}

#[test]
fn exhausted_lot_ids_skip_incompatible_food_and_collect_later_mergeable_drop() {
    use crate::content::{FORM_FOOD, MATERIAL_BERRIES};

    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let commodity = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let ordinary = Temperature::from_millikelvin(293_150);
    let differently_warm = Temperature::from_millikelvin(294_150);
    let drop_mass = Mass::from_milligrams(30_000);
    let blocked = insert_surface_resource_for_test(
        &registries,
        &mut state,
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            position,
            commodity,
            drop_mass,
            differently_warm,
            MaterialComposition::pure(MATERIAL_BERRIES),
        )
        .unwrap(),
    )
    .unwrap();
    let aged = insert_surface_resource_for_test(
        &registries,
        &mut state,
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            position,
            commodity,
            drop_mass,
            ordinary,
            MaterialComposition::pure(MATERIAL_BERRIES),
        )
        .unwrap(),
    )
    .unwrap();
    // Food cohorts with equal temperature still cannot coalesce after
    // accumulating different outdoor exposure histories.
    for _ in 0..2 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    let mergeable = insert_surface_resource_for_test(
        &registries,
        &mut state,
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            position,
            commodity,
            drop_mass,
            ordinary,
            MaterialComposition::pure(MATERIAL_BERRIES),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(blocked < aged && aged < mergeable);
    let carried = validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let original_lot = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        Mass::from_milligrams(20_000),
        ordinary,
    )
    .unwrap();
    let conserved = calculate_matter_accounting(&state).unwrap().total();

    // No new lot can be allocated. Exact food-temperature and storage
    // exposure cohorts still permit merging the later parcel in place.
    let mut serialized = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    serialized["state"]["systems"]["inventory"]["next_lot_id"] = serde_json::json!(u64::MAX);
    let mut loaded = serde_json::from_value::<LoadedSaveEnvelope>(serialized)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &loaded), Ok(()));
    let before = loaded.clone();
    assert_eq!(
        advance_tick(&registries, &mut loaded),
        Err(TickError::ControlSteppingRequired {
            current: before.control_tick(),
        })
    );
    assert_eq!(loaded, before);

    let pickup = advance_control_tick(&registries, &mut loaded)
        .unwrap()
        .surface_drop_pickup()
        .unwrap();
    assert_eq!(pickup.resource(), mergeable);
    assert_eq!(pickup.lot(), original_lot);
    assert_eq!(pickup.picked_mass(), drop_mass);
    assert_eq!(
        loaded.surface().get(blocked).unwrap().remaining_mass(),
        drop_mass
    );
    assert_eq!(
        loaded.surface().get(aged).unwrap().remaining_mass(),
        drop_mass
    );
    assert_eq!(
        loaded.surface().get(mergeable).unwrap().remaining_mass(),
        Mass::ZERO
    );
    assert_eq!(
        loaded
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(commodity),
        Mass::from_milligrams(50_000)
    );
    assert_eq!(
        calculate_matter_accounting(&loaded).unwrap().total(),
        conserved
    );

    // Once the older parcels cannot merge, skipping control samples
    // is legal and bit-identical to their explicit neutral replay.
    let mut replay = loaded.clone();
    let fast = advance_tick(&registries, &mut loaded).unwrap();
    let mut boundary = None;
    for _ in 1..CONTROL_TICKS_PER_SIMULATION_TICK {
        boundary = Some(advance_control_tick(&registries, &mut replay).unwrap());
    }
    assert_eq!(fast, *boundary.unwrap().simulation().unwrap());
    assert_eq!(loaded, replay);
    assert_eq!(
        loaded.surface().get(blocked).unwrap().remaining_mass(),
        drop_mass
    );
    assert_eq!(
        loaded.surface().get(aged).unwrap().remaining_mass(),
        drop_mass
    );
    assert_eq!(
        calculate_matter_accounting(&loaded).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &loaded), Ok(()));
    let json = serde_json::to_vec(&SaveEnvelope::new(&registries, &loaded)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&json)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, loaded);
}

#[test]
fn carried_material_drop_cannot_reuse_an_earlier_control_sample_for_pickup_delay() {
    use crate::surface::available_player_world_drops;

    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    let mass = Mass::from_milligrams(65_000);
    let carried = validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let old_token = validate_drop_player_material(&registries, &state, commodity).unwrap();
    let before = calculate_matter_accounting(&state).unwrap().total();
    for _ in 0..20 {
        assert!(
            advance_control_tick(&registries, &mut state)
                .unwrap()
                .surface_drop_pickup()
                .is_none()
        );
    }
    assert_eq!(state.tick(), SimulationTick::ZERO);
    assert_eq!(state.control_tick(), ControlTick::new(20));
    let unchanged = state.clone();
    assert_eq!(
        old_token.commit(&mut state),
        Err(super::PlayerMaterialDropCommitError::ControlTickChanged {
            expected: ControlTick::ZERO,
            actual: ControlTick::new(20),
        })
    );
    assert_eq!(state, unchanged);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let fresh = validate_drop_player_material(&registries, &state, commodity).unwrap();
    let dropped = fresh.commit(&mut state).unwrap();
    let resource = dropped.resources()[0];
    assert_eq!(
        state
            .surface()
            .get(resource)
            .unwrap()
            .automatic_pickup_delay_started_at(),
        Some(ControlTick::new(20))
    );
    let projection = available_player_world_drops(&state).next().unwrap();
    assert_eq!(
        projection.automatic_pickup_available_at(),
        Some(ControlTick::new(60))
    );
    for _ in 0..39 {
        assert!(
            advance_control_tick(&registries, &mut state)
                .unwrap()
                .surface_drop_pickup()
                .is_none()
        );
    }
    assert_eq!(state.control_tick(), ControlTick::new(59));
    assert_eq!(
        state.surface().get(resource).unwrap().remaining_mass(),
        mass
    );
    let pickup = advance_control_tick(&registries, &mut state)
        .unwrap()
        .surface_drop_pickup()
        .unwrap();
    assert_eq!(pickup.resource(), resource);
    assert_eq!(pickup.picked_mass(), mass);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(commodity),
        mass
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, loaded);
}

#[test]
fn prepared_strike_blocks_automatic_hand_pickup_until_cancelled() {
    use crate::combat::{validate_cancel_primitive_preparation, validate_prepare_primitive_strike};
    use crate::content::EQUIPMENT_WOODEN_CLUB;
    use crate::equipment::validate_assemble_equipment;
    use crate::logistics::{
        PlayerActiveSelection, validate_pickup_equipment, validate_set_active_selection,
    };
    use crate::spatial::ViewDirection;
    use crate::surface::available_player_world_drops;

    let registries = build_registries();
    let position = VoxelCoord::new(0, 0, 0);
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    let mass = Mass::from_milligrams(75_000);
    let mut state = AppState::new();
    let drop = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        position,
        commodity,
        mass,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let resource = insert_surface_resource_for_test(&registries, &mut state, drop).unwrap();
    let carried = validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let assembly = registries
        .equipment()
        .get_equipment(EQUIPMENT_WOODEN_CLUB)
        .unwrap()
        .assembly_profile()
        .unwrap()
        .clone();
    for input in assembly.inputs() {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            input.commodity(),
            input.mass(),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    let club = validate_assemble_equipment(&registries, &state, EQUIPMENT_WOODEN_CLUB, carried)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_pickup_equipment(&registries, &state, club)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::equipment(club)),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let _ = validate_prepare_primitive_strike(
        &registries,
        &state,
        ViewDirection::new(1_000_000, 0, 0).unwrap(),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();

    let matter_before = calculate_matter_accounting(&state).unwrap().total();
    assert!(state.combat().has_prepared_strike());
    assert!(
        !available_player_world_drops(&state)
            .next()
            .unwrap()
            .automatic_pickup_preflight_ready()
    );
    assert_eq!(
        advance_control_tick(&registries, &mut state)
            .unwrap()
            .surface_drop_pickup(),
        None
    );
    assert_eq!(
        state.surface().get(resource).unwrap().remaining_mass(),
        mass
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(commodity),
        Mass::ZERO
    );
    let _ = advance_tick(&registries, &mut state).unwrap();
    assert_eq!(
        state.surface().get(resource).unwrap().remaining_mass(),
        mass
    );
    assert!(state.combat().has_prepared_strike());
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let _ = validate_cancel_primitive_preparation(&state)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(
        available_player_world_drops(&state)
            .next()
            .unwrap()
            .automatic_pickup_preflight_ready()
    );
    let pickup = advance_control_tick(&registries, &mut state)
        .unwrap()
        .surface_drop_pickup()
        .unwrap();
    assert_eq!(pickup.resource(), resource);
    assert_eq!(pickup.picked_mass(), mass);
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

    let serialized = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&serialized)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
}

#[test]
fn surface_drop_pickup_delay_and_depletion_fields_must_be_explicit() {
    let registries = build_registries();
    let mut state = AppState::new();
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        VoxelCoord::new(0, 0, 0),
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(20_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let id = insert_surface_resource_for_test(&registries, &mut state, spec).unwrap();
    let resource = state.surface().get(id).unwrap();
    let encoded = serde_json::to_value(resource).unwrap();
    assert_eq!(
        serde_json::from_value::<super::SurfaceResourceRecord>(encoded.clone()).unwrap(),
        *resource
    );
    for field in ["automatic_pickup_delay_started_at", "depleted_at"] {
        let mut incomplete = encoded.clone();
        assert!(incomplete.as_object_mut().unwrap().remove(field).is_some());
        assert!(
            serde_json::from_value::<super::SurfaceResourceRecord>(incomplete).is_err(),
            "omitted {field} must not change physical loose-matter history"
        );
    }
}

#[test]
fn local_drop_only_requires_control_stepping_when_pickup_can_transfer_matter() {
    let registries = build_registries();
    let position = VoxelCoord::new(0, 0, 0);
    let mut state = AppState::new();
    let drop_mass = Mass::from_milligrams(200_000);
    let drop = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        drop_mass,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap_or_else(|error| panic!("pickup-drop fixture invalid: {error}"));
    let resource = insert_surface_resource_for_test(&registries, &mut state, drop)
        .unwrap_or_else(|error| panic!("pickup-drop insertion failed: {error}"));
    let carried = validate_admit_player(&registries, &state, position)
        .unwrap_or_else(|error| panic!("pickup-drop admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("pickup-drop admission commit failed: {error}"))
        .carried_stockpile();

    let mut with_room = state.clone();
    assert_eq!(
        advance_tick(&registries, &mut with_room),
        Err(TickError::ControlSteppingRequired {
            current: ControlTick::new(0),
        })
    );
    let pickup = advance_control_tick(&registries, &mut with_room)
        .unwrap_or_else(|error| panic!("control-rate pickup failed: {error}"))
        .surface_drop_pickup()
        .unwrap_or_else(|| panic!("available drop did not transfer on control tick"));
    assert_eq!(pickup.resource(), resource);
    assert_eq!(pickup.picked_mass(), drop_mass);
    assert_eq!(
        with_room
            .inventory()
            .get_stockpile(carried)
            .map(|pile| pile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_CHIP))),
        Some(drop_mass)
    );

    let mut dead = state.clone();
    let player = dead
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("pickup player disappeared"));
    let revision = dead.survival().revision();
    dead.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            player.metabolic_energy(),
            player.hydration(),
            Vitality::ZERO,
            player.nutrition(),
            player.vitality_recovery_remainder(),
            player.thermal_comfort_ppm(),
        ),
    );
    let _ = advance_tick(&registries, &mut dead)
        .unwrap_or_else(|error| panic!("dead player should allow slow stepping: {error}"));
    assert_eq!(
        dead.surface()
            .get(resource)
            .map(|record| record.remaining_mass()),
        Some(drop_mass)
    );
    assert_eq!(validate_loaded_state(&registries, &dead), Ok(()));

    let full_mass = registries.survival().physiology().carried_mass_capacity();
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        full_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("full-pack pickup fixture failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|pile| pile.available_capacity()),
        Some(Mass::ZERO)
    );
    let conserved = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("full-pack matter baseline failed: {error}"))
        .total();
    let tick = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("full pack should allow slow stepping: {error}"));
    assert_eq!(tick.tick(), SimulationTick::new(1));
    assert_eq!(
        state.control_tick().value(),
        CONTROL_TICKS_PER_SIMULATION_TICK
    );
    assert_eq!(
        state
            .surface()
            .get(resource)
            .map(|record| record.remaining_mass()),
        Some(drop_mass)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("full-pack matter audit failed: {error}"))
            .total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn exhausted_lot_id_only_requires_control_stepping_for_mergeable_drops() {
    for mergeable in [false, true] {
        let registries = build_registries();
        let position = VoxelCoord::new(0, 0, 0);
        let commodity = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
        let drop_mass = Mass::from_milligrams(50_000);
        let mut state = AppState::new();
        let spec = SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            position,
            commodity,
            drop_mass,
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap();
        let drop = insert_surface_resource_for_test(&registries, &mut state, spec).unwrap();
        let carried = validate_admit_player(&registries, &state, position)
            .unwrap()
            .commit(&mut state)
            .unwrap()
            .carried_stockpile();
        if mergeable {
            deposit_lot_for_test(
                &registries,
                &mut state,
                carried,
                commodity,
                Mass::from_milligrams(20_000),
                Temperature::from_millikelvin(293_150),
            )
            .unwrap();
        }
        let mut saved = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
        saved["state"]["systems"]["inventory"]["next_lot_id"] = serde_json::json!(u64::MAX);
        let mut loaded = serde_json::from_value::<LoadedSaveEnvelope>(saved)
            .unwrap()
            .into_state(&registries)
            .unwrap();
        let matter_before = calculate_matter_accounting(&loaded).unwrap().total();
        let before = loaded.clone();
        if mergeable {
            assert_eq!(
                advance_tick(&registries, &mut loaded),
                Err(TickError::ControlSteppingRequired {
                    current: ControlTick::new(0),
                }),
                "a coalescible parcel still requires the first control tick"
            );
            assert_eq!(loaded, before);
            let pickup = advance_control_tick(&registries, &mut loaded)
                .unwrap()
                .surface_drop_pickup()
                .unwrap();
            assert_eq!(pickup.resource(), drop);
            assert_eq!(pickup.picked_mass(), drop_mass);
            assert_eq!(
                loaded
                    .inventory()
                    .get_stockpile(carried)
                    .unwrap()
                    .get_mass(commodity),
                Mass::from_milligrams(70_000)
            );
        } else {
            assert_eq!(
                advance_tick(&registries, &mut loaded).unwrap().tick(),
                SimulationTick::new(1),
                "an unallocatable parcel must not force pointless control-rate steps"
            );
            assert_eq!(
                loaded.surface().get(drop).unwrap().remaining_mass(),
                drop_mass
            );
            assert_eq!(
                loaded
                    .inventory()
                    .get_stockpile(carried)
                    .unwrap()
                    .stored_mass(),
                Mass::ZERO
            );
        }
        assert_eq!(
            calculate_matter_accounting(&loaded).unwrap().total(),
            matter_before
        );
        assert_eq!(validate_loaded_state(&registries, &loaded), Ok(()));
        let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &loaded)).unwrap();
        let replay = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
            .unwrap()
            .into_state(&registries)
            .unwrap();
        assert_eq!(replay, loaded);
    }
}

#[test]
fn delayed_player_drop_only_requires_control_steps_when_pickup_is_due_before_next_boundary() {
    let registries = build_registries();
    let position = VoxelCoord::new(0, 0, 0);
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    let mass = Mass::from_milligrams(80_000);
    for (drop_at, requires_stepping) in [(32, true), (60, false)] {
        let mut state = AppState::new();
        let carried = validate_admit_player(&registries, &state, position)
            .unwrap()
            .commit(&mut state)
            .unwrap()
            .carried_stockpile();
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
        for _ in 0..drop_at {
            let _ = advance_control_tick(&registries, &mut state).unwrap();
        }
        let dropped = validate_drop_player_material(&registries, &state, commodity)
            .unwrap()
            .commit(&mut state)
            .unwrap();
        assert_eq!(dropped.dropped_mass(), mass);
        let resource = dropped.resources()[0];
        let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
        let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
            .unwrap()
            .into_state(&registries)
            .unwrap();
        assert_eq!(state, loaded);

        let remaining = CONTROL_TICKS_PER_SIMULATION_TICK - drop_at;
        let mut replay = loaded;
        for _ in 0..remaining {
            let _ = advance_control_tick(&registries, &mut replay).unwrap();
        }
        if requires_stepping {
            assert_eq!(
                advance_tick(&registries, &mut state),
                Err(TickError::ControlSteppingRequired {
                    current: ControlTick::new(drop_at),
                })
            );
            assert_eq!(state.control_tick(), ControlTick::new(drop_at));
        } else {
            let _ = advance_tick(&registries, &mut state).unwrap();
            assert_eq!(
                state, replay,
                "safe slow stepping must match control replay"
            );
            assert_eq!(
                state.surface().get(resource).unwrap().remaining_mass(),
                mass
            );
            assert_eq!(
                advance_tick(&registries, &mut state),
                Err(TickError::ControlSteppingRequired {
                    current: ControlTick::new(CONTROL_TICKS_PER_SIMULATION_TICK),
                }),
                "the next interval reaches the pickup deadline"
            );
        }
        for control in replay.control_tick().value()..(drop_at + 40) {
            let result = advance_control_tick(&registries, &mut replay).unwrap();
            if control + 1 == drop_at + 40 {
                assert_eq!(result.surface_drop_pickup().unwrap().picked_mass(), mass);
            }
        }
        assert_eq!(
            replay
                .inventory()
                .get_stockpile(carried)
                .unwrap()
                .get_mass(commodity),
            mass
        );
        assert_eq!(
            replay.surface().get(resource).unwrap().remaining_mass(),
            Mass::ZERO
        );
        assert_eq!(validate_loaded_state(&registries, &replay), Ok(()));
    }
}

#[test]
fn remote_drop_does_not_block_neutral_slow_stepping() {
    let registries = build_registries();
    let position = VoxelCoord::new(0, 0, 0);
    let remote = VoxelCoord::new(4, 0, 0);
    let mut state = AppState::new();
    let mass = Mass::from_milligrams(100_000);
    let drop = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        remote,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        mass,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap_or_else(|error| panic!("remote drop fixture invalid: {error}"));
    let resource = insert_surface_resource_for_test(&registries, &mut state, drop)
        .unwrap_or_else(|error| panic!("remote drop insertion failed: {error}"));
    let carried = validate_admit_player(&registries, &state, position)
        .unwrap_or_else(|error| panic!("remote drop admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote drop admission commit failed: {error}"))
        .carried_stockpile();

    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|pile| pile.available_capacity().is_zero()),
        Some(false)
    );
    let advanced = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("remote drop should not block slow stepping: {error}"));
    assert_eq!(advanced.tick(), SimulationTick::new(1));
    assert_eq!(
        state
            .surface()
            .get(resource)
            .map(|record| record.remaining_mass()),
        Some(mass)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|pile| pile.stored_mass()),
        Some(Mass::ZERO)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
