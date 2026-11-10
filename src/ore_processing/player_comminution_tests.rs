//! Player-owned powered ore choices retain exact custody and live authorization.

use super::*;

use crate::content::{
    ENERGY_MECHANICAL_SMALL_DRIVE, EQUIPMENT_STONE_CRUSHER, FORM_ORE, MATERIAL_CASSITERITE,
    MATERIAL_GALENA, PROCESS_CRUSH_ORE, build_registries,
};
use crate::core::quantity::Temperature;
use crate::core::state::validate_loaded_state;
use crate::energy::add_energy_store_with_initial_for_fixture;
use crate::equipment::validate_assemble_equipment;
use crate::inventory::{
    add_solid_stockpile_for_test, deposit_lot_for_test, project_material_stacks,
};
use crate::logistics::{
    place_energy_store_for_fixture, place_equipment_for_fixture, validate_place_ground_stockpile,
};
use crate::material::CommodityKey;
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;
use crate::survival::{Vitality, player_record_with_thermal_comfort};

#[test]
fn dead_actor_cannot_inspect_a_retained_powered_ore_choice_before_authorization() {
    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let ore = CommodityKey::new(MATERIAL_GALENA, FORM_ORE);
    let mass = Mass::from_milligrams(100_000);
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        ore,
        mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap()
        .into_iter()
        .find(|stack| stack.commodity() == ore)
        .unwrap();
    // The original stack and observed quantity are real custody. Unused machine
    // identities are not inspected when the player is no longer alive.
    let option = PlayerComminutionOption {
        stack,
        process: PROCESS_CRUSH_ORE,
        equipment: EquipmentId::new(98_101),
        store: EnergyStoreId::new(98_102),
        destination: carried,
        maximum_mass: mass,
        duration: TickSpan::new(1),
        required_energy: Energy::ZERO,
        deferred_until_boundary: false,
    };
    let player = state.survival().player().copied().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
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
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        ore,
        mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let before = state.clone();
    for requested in [Mass::ZERO, mass, Mass::from_milligrams(100_001)] {
        assert!(matches!(
            validate_start_player_comminution_option(&registries, &state, &option, requested),
            Err(PlayerComminutionOptionError::Start(
                StartPlayerProcessError::PlayerDead
            ))
        ));
    }
    assert_eq!(state, before);
}

#[test]
fn separate_ore_stacks_share_provider_limits_without_merging_choices_or_custody() {
    let registries = build_registries();
    let mut state = AppState::new();
    let at = VoxelCoord::new(0, 0, 0);
    let temperature = Temperature::from_millikelvin(293_150);
    let profile = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_CRUSHER)
        .unwrap()
        .assembly_profile()
        .unwrap();
    let assembly = add_solid_stockpile_for_test(&mut state, profile.input_mass()).unwrap();
    for component in profile.inputs() {
        deposit_lot_for_test(
            &registries,
            &mut state,
            assembly,
            component.commodity(),
            component.mass(),
            temperature,
        )
        .unwrap();
    }
    let crusher =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_CRUSHER, assembly)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    place_equipment_for_fixture(&mut state, crusher, at);
    let power = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_MECHANICAL_SMALL_DRIVE,
        Energy::from_nanojoules(500_000_000_000),
    )
    .unwrap();
    place_energy_store_for_fixture(&mut state, power, at);
    let mut feeds = Vec::new();
    for material in [MATERIAL_GALENA, MATERIAL_CASSITERITE] {
        let source =
            add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000)).unwrap();
        deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(material, FORM_ORE),
            Mass::from_milligrams(100_000),
            temperature,
        )
        .unwrap();
        validate_place_ground_stockpile(&state, source, at)
            .unwrap()
            .commit(&mut state)
            .unwrap();
        feeds.push(source);
    }
    let output = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(60_000)).unwrap();
    validate_place_ground_stockpile(&state, output, at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_admit_player(&registries, &state, at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let before = state.clone();
    let choices = player_local_powered_comminution_options(&registries, &state).unwrap();
    assert_eq!(
        choices,
        player_local_powered_comminution_options(&registries, &state).unwrap(),
        "the read-only catalog is deterministic"
    );
    assert_eq!(
        state, before,
        "menu inspection must not reserve any provider or lot"
    );
    let relevant = choices
        .iter()
        .filter(|choice| {
            choice.process() == PROCESS_CRUSH_ORE
                && choice.equipment() == crusher
                && choice.energy_store() == power
                && choice.destination() == output
        })
        .collect::<Vec<_>>();
    assert_eq!(
        relevant.len(),
        2,
        "both distinct ore sources remain visible"
    );
    for (source, material) in feeds
        .into_iter()
        .zip([MATERIAL_GALENA, MATERIAL_CASSITERITE])
    {
        let matching = relevant
            .iter()
            .filter(|choice| {
                choice.source() == source && choice.feed() == CommodityKey::new(material, FORM_ORE)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            matching.len(),
            1,
            "distinct source and material must not be conflated"
        );
        assert_eq!(matching[0].maximum_mass(), Mass::from_milligrams(60_000));
        assert!(!matching[0].deferred_until_boundary());
    }
    let first = relevant[0];
    let second = relevant[1];
    let job =
        validate_start_player_comminution_option(&registries, &state, first, first.maximum_mass())
            .unwrap()
            .commit(&mut state)
            .unwrap();
    assert!(state.production().get_job(job).is_some());
    assert!(matches!(
        validate_start_player_comminution_option(
            &registries,
            &state,
            second,
            second.maximum_mass(),
        ),
        Err(PlayerComminutionOptionError::Stack(
            MaterialStackProjectionError::StaleInventoryRevision { .. }
        ))
    ));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
