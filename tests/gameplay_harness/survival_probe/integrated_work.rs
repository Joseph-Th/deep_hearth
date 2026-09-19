//! Integrated provision, prospect, reprovision, and manual-power survival loop.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct IntegratedSurvivalWorkReview {
    pub(super) initial_drink_ticks: u64,
    pub(super) prospecting_ticks: u64,
    pub(super) reprovisioned_after_prospecting: bool,
    pub(super) reprovision_ticks: u64,
    pub(super) manual_power_ticks: u64,
    pub(super) stored_work_nj: u128,
    pub(super) energy_deficit_ppm: u32,
    pub(super) hydration_deficit_ppm: u32,
    pub(super) hydration_warning_safe: bool,
}

pub(super) fn evaluate_integrated_survival_work_loop(
    registries: &Registries,
    seed: u64,
) -> IntegratedSurvivalWorkReview {
    let physiology = registries.survival().physiology();
    let direct = physiology.direct_consumption();
    let mut drinks = registries.survival().drinks().copied().collect::<Vec<_>>();
    drinks.sort_by_key(|drink| drink.fluid());
    let drink = drinks
        .get(
            usize::try_from(mix64(seed ^ 0x494E_5445_4752_4452) % drinks.len().max(1) as u64)
                .unwrap_or_else(|_| unreachable!("integrated survival drink index fits usize")),
        )
        .copied()
        .unwrap_or_else(|| panic!("integrated survival work loop requires one authored drink"));
    let drink_volume = direct.maximum_drink_volume();
    let mut state = AppState::new(WorldSeed::new(seed ^ 0x494E_5445_4752_4154));
    let drink_store = seed_fluid_store(
        registries,
        &mut state,
        drink_volume
            .checked_add(drink_volume)
            .unwrap_or_else(|| panic!("integrated survival drink capacity overflowed")),
        drink.fluid(),
        drink_volume
            .checked_add(drink_volume)
            .unwrap_or_else(|| panic!("integrated survival drink supply overflowed")),
        ROOM_TEMPERATURE,
    );

    let crank_profile = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_HAND_CRANK)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("integrated survival stone crank lost its assembly route"));
    let drive_profile = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("integrated survival stone flywheel lost its assembly route"));
    let component_capacity = crank_profile
        .inputs()
        .iter()
        .chain(drive_profile.inputs())
        .try_fold(Mass::ZERO, |total, input| total.checked_add(input.mass()))
        .unwrap_or_else(|| panic!("integrated survival primitive power component mass overflowed"));
    let component_source = seed_stockpile(
        &mut state,
        component_capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    for input in crank_profile.inputs().iter().chain(drive_profile.inputs()) {
        seed_lot(
            registries,
            &mut state,
            component_source,
            input.commodity(),
            input.mass(),
            ROOM_TEMPERATURE,
        );
    }
    let crank = validate_assemble_equipment(
        registries,
        &state,
        EQUIPMENT_STONE_HAND_CRANK,
        component_source,
    )
    .unwrap_or_else(|error| panic!("integrated survival crank assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("integrated survival crank assembly commit failed: {error}"));
    let drive = validate_assemble_energy_store(
        registries,
        &state,
        ENERGY_STONE_FLYWHEEL_DRIVE,
        component_source,
    )
    .unwrap_or_else(|error| panic!("integrated survival flywheel assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("integrated survival flywheel assembly commit failed: {error}"));
    seed_player_survival_at_hydration_warning_boundary(registries, &mut state);

    let start = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("integrated survival player disappeared at start"));
    assert_eq!(start.hydration(), physiology.thirsty_below());
    let first_drink = validate_drink(registries, &state, drink_store, drink_volume)
        .unwrap_or_else(|error| panic!("integrated survival initial drink failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("integrated survival initial drink commit failed: {error}"));
    let initial_drink_ticks =
        finish_direct_consumption(registries, &mut state, first_drink.completes_at());
    let after_drink = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("integrated survival player disappeared after drinking"));
    assert!(after_drink.hydration() > physiology.thirsty_below());

    let prospecting_method = prospecting_method_for_work_pressure(registries, seed);
    let prospecting_definition = registries
        .labor()
        .get_prospecting(prospecting_method)
        .copied()
        .unwrap_or_else(|| panic!("integrated survival prospecting method disappeared"));
    let region_width = i64::try_from(prospecting_definition.maximum_region_voxels().min(4))
        .unwrap_or_else(|_| unreachable!("bounded integrated prospecting footprint fits i64"));
    let region = VoxelBounds::new(
        VoxelCoord::new(40, -1, 0),
        VoxelCoord::new(40 + region_width, 0, 1),
    )
    .unwrap_or_else(|error| panic!("integrated survival prospecting bounds failed: {error}"));
    let prospecting = validate_start_field_prospecting(
        registries,
        &state,
        FieldProspectingRequest::new(prospecting_method, region, MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("integrated survival prospecting start failed: {error}"));
    let prospecting_work = prospecting.work();
    let prospecting_ticks = prospecting_work
        .completes_at()
        .value()
        .checked_sub(state.tick().value())
        .unwrap_or_else(|| unreachable!("validated prospecting completes after it starts"));
    prospecting
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("integrated survival prospecting commit failed: {error}"));
    let mut observation = None;
    for _ in 0..prospecting_ticks {
        observation = advance_tick(registries, &mut state)
            .unwrap_or_else(|error| panic!("integrated survival prospecting tick failed: {error}"))
            .field_prospecting();
    }
    assert!(
        observation.is_some(),
        "integrated survival prospecting produced no observation"
    );

    let after_prospecting = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("integrated survival player disappeared after prospecting"));
    let reprovisioned_after_prospecting =
        after_prospecting.hydration() < physiology.thirsty_below();
    let reprovision_ticks = if reprovisioned_after_prospecting {
        let drink = validate_drink(registries, &state, drink_store, drink_volume)
            .unwrap_or_else(|error| panic!("integrated survival follow-up drink failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("integrated survival follow-up drink commit failed: {error}")
            });
        finish_direct_consumption(registries, &mut state, drink.completes_at())
    } else {
        0
    };

    let requested_energy = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("integrated survival flywheel definition disappeared"));
    let power = validate_start_manual_power(
        registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested_energy),
    )
    .unwrap_or_else(|error| panic!("integrated survival manual-power start failed: {error}"));
    let work = power.work();
    let manual_power_ticks = work
        .completes_at()
        .value()
        .checked_sub(state.tick().value())
        .unwrap_or_else(|| unreachable!("validated manual power completes after it starts"));
    power
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("integrated survival manual-power commit failed: {error}"));
    assert_eq!(
        finish_manual_power_work(
            registries,
            &mut state,
            work,
            "integrated survival manual power"
        ),
        manual_power_ticks
    );
    assert_eq!(
        state.energy().get_store(drive).map(|store| store.stored()),
        Some(requested_energy)
    );
    let final_survival = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("integrated survival player disappeared after work loop"));
    let energy_deficit_ppm = normalized_energy_deficit_ppm(
        physiology.maximum_metabolic_energy(),
        final_survival.metabolic_energy(),
    );
    let hydration_deficit_ppm = normalized_hydration_deficit_ppm(
        physiology.maximum_hydration(),
        final_survival.hydration(),
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("integrated survival work-loop audit failed: {error}"));

    IntegratedSurvivalWorkReview {
        initial_drink_ticks,
        prospecting_ticks,
        reprovisioned_after_prospecting,
        reprovision_ticks,
        manual_power_ticks,
        stored_work_nj: requested_energy.nanojoules(),
        energy_deficit_ppm,
        hydration_deficit_ppm,
        hydration_warning_safe: final_survival.hydration() >= physiology.thirsty_below(),
    }
}
