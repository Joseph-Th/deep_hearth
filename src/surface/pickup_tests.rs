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
