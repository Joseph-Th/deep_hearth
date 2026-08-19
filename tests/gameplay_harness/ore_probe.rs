//! Focused ore-preparation capability probe.

use super::ore_setup::{mixed_ore_composition, setup_ore_preparation_probe};
use super::seed::mix64;
use super::support::nominal_equipment_mass_capability;
use deep_hearth::content::{
    EQUIPMENT_DRY_SCREEN, EQUIPMENT_GRINDING_MILL, EQUIPMENT_JAW_CRUSHER, PROCESS_CRUSH_ORE,
    PROCESS_FINE_GRIND_SCREEN_OVERSIZE, PROCESS_GRIND_CRUSHED_ORE, PROCESS_SCREEN_CRUSHED_ORE,
};
use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::TickSpan;
use deep_hearth::inventory::{MaterialLotId, MaterialLotSelection, StockpileId};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::ore_processing::{
    ComminutionBatchError, ComminutionRequest, ComminutionResolutionError, ScreeningBatchError,
    ScreeningProcessDefinition, ScreeningRequest, ScreeningResolutionError,
    resolve_comminution_process, resolve_screening_process,
};
use deep_hearth::production::{
    ProcessOutputRoute, validate_start_process, validate_start_process_routed,
};
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;

fn greatest_common_divisor(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn finish_operation(registries: &Registries, state: &mut AppState, duration: TickSpan) {
    for _ in 0..duration.value() {
        advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("ore preparation tick failed: {error}"));
    }
}

fn stockpile_first_lot(state: &AppState, stockpile: StockpileId) -> MaterialLotId {
    state
        .inventory()
        .lot_ids(stockpile)
        .next()
        .unwrap_or_else(|| panic!("ore preparation expected output lot is missing"))
}

fn probe_parameters(registries: &Registries, seed: u64) -> (Mass, u32) {
    let crusher = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("canonical crusher definition disappeared"));
    let grinder = registries
        .ore_processing()
        .get_comminution(PROCESS_GRIND_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("canonical grinder definition disappeared"));
    let screening = registries
        .ore_processing()
        .get_screening(PROCESS_SCREEN_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("canonical screen definition disappeared"));
    let fine_grind = registries
        .ore_processing()
        .get_comminution(PROCESS_FINE_GRIND_SCREEN_OVERSIZE)
        .unwrap_or_else(|| panic!("canonical fine-grind definition disappeared"));

    let distribution = grinder.output_particle_size_distribution();
    let aperture = screening.aperture();
    let mut undersize_weight = 0_u64;
    for class in distribution.classes() {
        let range = class.range();
        if range.maximum_diameter() <= aperture {
            undersize_weight += u64::from(class.weight());
        } else if range.minimum_diameter() <= aperture {
            panic!(
                "authored grinder particle class {}..={}um crosses screen aperture {}um",
                range.minimum_diameter().micrometers(),
                range.maximum_diameter().micrometers(),
                aperture.micrometers()
            );
        }
    }
    let total_weight = distribution.total_weight();
    let representable_unit = total_weight / greatest_common_divisor(total_weight, undersize_weight);

    let mut batch_limits = vec![
        nominal_equipment_mass_capability(
            registries,
            EQUIPMENT_JAW_CRUSHER,
            crusher.max_batch_mass_capability(),
        ),
        nominal_equipment_mass_capability(
            registries,
            EQUIPMENT_GRINDING_MILL,
            grinder.max_batch_mass_capability(),
        ),
        nominal_equipment_mass_capability(
            registries,
            EQUIPMENT_DRY_SCREEN,
            screening.max_batch_mass_capability(),
        ),
    ];
    if undersize_weight < total_weight {
        batch_limits.push(nominal_equipment_mass_capability(
            registries,
            EQUIPMENT_GRINDING_MILL,
            fine_grind.max_batch_mass_capability(),
        ));
    }
    let maximum_batch = batch_limits
        .into_iter()
        .map(Mass::milligrams)
        .min()
        .unwrap_or_else(|| panic!("ore preparation probe has no authored batch constraints"));
    let maximum_units = maximum_batch / representable_unit;
    assert!(
        maximum_units > 0,
        "authored screen partition cannot be represented within the equipment batch limits"
    );
    let minimum_units = maximum_units.div_ceil(2);
    let unit_count =
        minimum_units + mix64(seed ^ 0x0AE5_1A5E) % (maximum_units - minimum_units + 1);
    let batch_mass = Mass::from_milligrams(representable_unit * unit_count);
    let copper_ppm = 300_000 + (mix64(seed ^ 0xC0FF_EE11) % 400_001) as u32;
    (batch_mass, copper_ppm)
}

pub(super) fn run_ore_preparation_capability_probe(registries: &Registries, seed: u64) {
    let (batch_mass, copper_ppm) = probe_parameters(registries, seed);
    let (mut state, ids) = setup_ore_preparation_probe(registries, seed, batch_mass, copper_ppm);
    let crusher_definition = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("canonical crusher definition disappeared"));
    let grinder_definition = registries
        .ore_processing()
        .get_comminution(PROCESS_GRIND_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("canonical grinder definition disappeared"));
    let screen_definition = registries
        .ore_processing()
        .get_screening(PROCESS_SCREEN_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("canonical screen definition disappeared"));
    let fine_grind_definition = registries
        .ore_processing()
        .get_comminution(PROCESS_FINE_GRIND_SCREEN_OVERSIZE)
        .unwrap_or_else(|| panic!("canonical fine-grind definition disappeared"));
    let initial_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("ore preparation initial matter accounting failed: {error}"))
        .total();
    let initial_energy = state
        .energy()
        .get_store(ids.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("ore preparation drive disappeared"));

    let crush_selection = [MaterialLotSelection::new(ids.ore_lot, batch_mass)];
    let crushed = resolve_comminution_process(
        registries,
        &state,
        ComminutionRequest::new(
            PROCESS_CRUSH_ORE,
            ids.ore_source,
            &crush_selection,
            ids.crusher,
            ids.drive,
        ),
    )
    .unwrap_or_else(|error| panic!("canonical crushing probe resolution failed: {error}"));
    let crush_duration = crushed.process_resolution().duration();
    let crush_energy = crushed.required_energy();
    let crusher_condition = crushed.condition_after();
    validate_start_process(
        registries,
        &state,
        crushed.process_resolution(),
        ids.ore_source,
        ids.crushed_storage,
    )
    .unwrap_or_else(|error| panic!("ore preparation crushing start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("ore preparation crushing commit failed: {error}"));
    finish_operation(registries, &mut state, crush_duration);
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("ore preparation post-crush audit failed: {error}"));

    let crushed_lot = stockpile_first_lot(&state, ids.crushed_storage);
    let crushed_distribution = state
        .inventory()
        .get_lot(crushed_lot)
        .and_then(|lot| lot.particle_size_distribution())
        .unwrap_or_else(|| panic!("canonical crushing output lost particle-size state"));
    let crusher_output_matches_authoring =
        crushed_distribution == crusher_definition.output_particle_size_distribution();
    let direct_screen_selection = [MaterialLotSelection::new(crushed_lot, batch_mass)];
    match resolve_screening_process(
        registries,
        &state,
        ScreeningRequest::new(
            PROCESS_SCREEN_CRUSHED_ORE,
            ids.crushed_storage,
            &direct_screen_selection,
            ids.screen,
            ids.drive,
        ),
    ) {
        Ok(_)
        | Err(ScreeningResolutionError::Batch(ScreeningBatchError::UnresolvedParticleClass {
            ..
        })) => {}
        Err(error) => panic!("direct-screen route failed unexpectedly: {error}"),
    }
    match resolve_comminution_process(
        registries,
        &state,
        ComminutionRequest::new(
            PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
            ids.crushed_storage,
            &direct_screen_selection,
            ids.grinder,
            ids.drive,
        ),
    ) {
        Ok(_)
        | Err(ComminutionResolutionError::Batch(
            ComminutionBatchError::InputParticleSizeOutsideOperatingRange { .. },
        )) => {}
        Err(error) => panic!("direct fine-grind route failed unexpectedly: {error}"),
    }

    let grind_selection = [MaterialLotSelection::new(crushed_lot, batch_mass)];
    let ground = resolve_comminution_process(
        registries,
        &state,
        ComminutionRequest::new(
            PROCESS_GRIND_CRUSHED_ORE,
            ids.crushed_storage,
            &grind_selection,
            ids.grinder,
            ids.drive,
        ),
    )
    .unwrap_or_else(|error| panic!("canonical grinding probe resolution failed: {error}"));
    let grind_duration = ground.process_resolution().duration();
    let grind_energy = ground.required_energy();
    let grinder_condition = ground.condition_after();
    validate_start_process(
        registries,
        &state,
        ground.process_resolution(),
        ids.crushed_storage,
        ids.ground_storage,
    )
    .unwrap_or_else(|error| panic!("ore preparation grinding start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("ore preparation grinding commit failed: {error}"));
    finish_operation(registries, &mut state, grind_duration);
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("ore preparation post-grind audit failed: {error}"));
    assert_eq!(
        state
            .equipment()
            .get_equipment(ids.grinder)
            .map(|equipment| equipment.condition()),
        Some(grinder_condition),
        "grinder condition must match the resolved wear projection"
    );

    let ground_lot = stockpile_first_lot(&state, ids.ground_storage);
    let ground_distribution = state
        .inventory()
        .get_lot(ground_lot)
        .and_then(|lot| lot.particle_size_distribution())
        .cloned()
        .unwrap_or_else(|| panic!("canonical grinding output lost particle-size state"));
    let ground_classes = ground_distribution.classes();
    let grinding_matches_authoring =
        &ground_distribution == grinder_definition.output_particle_size_distribution();
    let grinding_resolved_screen_cut = ground_classes.iter().all(|class| {
        class.range().maximum_diameter() <= screen_definition.aperture()
            || class.range().minimum_diameter() > screen_definition.aperture()
    });

    let screen_selection = [MaterialLotSelection::new(ground_lot, batch_mass)];
    let screened = resolve_screening_process(
        registries,
        &state,
        ScreeningRequest::new(
            PROCESS_SCREEN_CRUSHED_ORE,
            ids.ground_storage,
            &screen_selection,
            ids.screen,
            ids.drive,
        ),
    )
    .unwrap_or_else(|error| panic!("canonical screening probe resolution failed: {error}"));
    let screen_duration = screened.process_resolution().duration();
    let screen_energy = screened.required_energy();
    let screen_condition = screened.condition_after();
    let screened_undersize_mass = screened.undersize_mass();
    let screened_oversize_mass = screened.oversize_mass();
    let mut routes = Vec::with_capacity(2);
    if !screened_undersize_mass.is_zero() {
        routes.push(ProcessOutputRoute::new(
            ScreeningProcessDefinition::UNDERSIZE_STREAM,
            ids.undersize_storage,
        ));
    }
    if !screened_oversize_mass.is_zero() {
        routes.push(ProcessOutputRoute::new(
            ScreeningProcessDefinition::OVERSIZE_STREAM,
            ids.oversize_storage,
        ));
    }
    validate_start_process_routed(
        registries,
        &state,
        screened.process_resolution(),
        ids.ground_storage,
        &routes,
    )
    .unwrap_or_else(|error| panic!("ore preparation screening start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("ore preparation screening commit failed: {error}"));
    finish_operation(registries, &mut state, screen_duration);
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("ore preparation post-screen audit failed: {error}"));

    let output_composition = mixed_ore_composition(copper_ppm);
    let fine_output_fits_undersize = fine_grind_definition
        .output_particle_size_distribution()
        .classes()
        .iter()
        .all(|class| class.range().maximum_diameter() <= screen_definition.aperture());
    let (fine_energy, fine_duration_ticks, final_grinder_projection, oversize_profile_is_preserved) =
        if screened_oversize_mass.is_zero() {
            (Energy::ZERO, 0, grinder_condition, true)
        } else {
            let oversize_lot = stockpile_first_lot(&state, ids.oversize_storage);
            let oversize_before_regrind = state
                .inventory()
                .get_lot(oversize_lot)
                .unwrap_or_else(|| panic!("ore preparation oversize lot disappeared"));
            let oversize_profile_is_preserved = oversize_before_regrind.composition()
                == &output_composition
                && oversize_before_regrind
                    .particle_size_distribution()
                    .is_some_and(|distribution| {
                        distribution.classes().iter().all(|class| {
                            class.range().minimum_diameter() > screen_definition.aperture()
                                && ground_classes.contains(class)
                        })
                    });
            let fine_selection = [MaterialLotSelection::new(
                oversize_lot,
                screened_oversize_mass,
            )];
            let fine_ground = resolve_comminution_process(
                registries,
                &state,
                ComminutionRequest::new(
                    PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
                    ids.oversize_storage,
                    &fine_selection,
                    ids.grinder,
                    ids.drive,
                ),
            )
            .unwrap_or_else(|error| {
                panic!("canonical fine-grinding probe resolution failed: {error}")
            });
            let fine_duration = fine_ground.process_resolution().duration();
            let fine_energy = fine_ground.required_energy();
            let final_grinder_projection = fine_ground.condition_after();
            validate_start_process(
                registries,
                &state,
                fine_ground.process_resolution(),
                ids.oversize_storage,
                ids.undersize_storage,
            )
            .unwrap_or_else(|error| panic!("ore preparation fine-grinding start failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("ore preparation fine-grinding commit failed: {error}"));
            finish_operation(registries, &mut state, fine_duration);
            validate_loaded_state(registries, &state).unwrap_or_else(|error| {
                panic!("ore preparation post-regrind audit failed: {error}")
            });
            (
                fine_energy,
                fine_duration.value(),
                final_grinder_projection,
                oversize_profile_is_preserved,
            )
        };

    let final_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("ore preparation final matter accounting failed: {error}"))
        .total();
    let final_energy = state
        .energy()
        .get_store(ids.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("ore preparation drive disappeared after completion"));
    let final_crusher_condition = state
        .equipment()
        .get_equipment(ids.crusher)
        .map(|equipment| equipment.condition())
        .unwrap_or_else(|| panic!("ore preparation crusher disappeared after completion"));
    let final_grinder_condition = state
        .equipment()
        .get_equipment(ids.grinder)
        .map(|equipment| equipment.condition())
        .unwrap_or_else(|| panic!("ore preparation grinder disappeared after completion"));
    let final_screen_condition = state
        .equipment()
        .get_equipment(ids.screen)
        .map(|equipment| equipment.condition())
        .unwrap_or_else(|| panic!("ore preparation screen disappeared after completion"));
    let undersize_mass = state
        .inventory()
        .get_stockpile(ids.undersize_storage)
        .map(|stockpile| stockpile.stored_mass())
        .unwrap_or_else(|| panic!("ore preparation undersize storage disappeared"));
    let oversize_mass = state
        .inventory()
        .get_stockpile(ids.oversize_storage)
        .map(|stockpile| stockpile.stored_mass())
        .unwrap_or_else(|| panic!("ore preparation oversize storage disappeared"));
    let composition_preserved = state.inventory().lot_ids(ids.undersize_storage).all(|lot| {
        state
            .inventory()
            .get_lot(lot)
            .is_some_and(|lot| lot.composition() == &output_composition)
    });
    let final_distribution_is_fine = state.inventory().lot_ids(ids.undersize_storage).all(|lot| {
        state
            .inventory()
            .get_lot(lot)
            .and_then(|lot| lot.particle_size_distribution())
            .is_some_and(|distribution| {
                distribution
                    .classes()
                    .iter()
                    .all(|class| class.range().maximum_diameter() <= screen_definition.aperture())
            })
    });
    let consumed_energy = crush_energy
        .checked_add(grind_energy)
        .and_then(|energy| energy.checked_add(screen_energy))
        .and_then(|energy| energy.checked_add(fine_energy))
        .unwrap_or_else(|| panic!("ore preparation consumed energy overflowed"));

    assert_eq!(
        final_matter, initial_matter,
        "ore preparation must conserve world matter"
    );
    assert_eq!(
        initial_energy.checked_sub(consumed_energy),
        Some(final_energy),
        "ore preparation must consume exactly the resolved work energy"
    );
    assert_eq!(
        final_crusher_condition, crusher_condition,
        "crusher condition must match resolved wear"
    );
    assert_eq!(
        final_grinder_condition, final_grinder_projection,
        "grinder condition must match resolved wear"
    );
    assert_eq!(
        final_screen_condition, screen_condition,
        "screen condition must match resolved wear"
    );
    assert_eq!(
        undersize_mass, batch_mass,
        "all prepared mass must finish in undersize storage"
    );
    assert_eq!(
        oversize_mass,
        Mass::ZERO,
        "oversize storage must be empty after regrind"
    );

    let qualitative_requirements = [
        (
            "crusher output matches authored particle state",
            crusher_output_matches_authoring,
        ),
        (
            "grinder output matches authored particle state",
            grinding_matches_authoring,
        ),
        (
            "grinder output resolves the authored screen cut",
            grinding_resolved_screen_cut,
        ),
        (
            "fine-grind output fits the authored screen undersize",
            screened_oversize_mass.is_zero() || fine_output_fits_undersize,
        ),
        (
            "screen oversize preserves its particle profile",
            oversize_profile_is_preserved,
        ),
        (
            "ore preparation preserves composition",
            composition_preserved,
        ),
        (
            "final product satisfies the fine size range",
            final_distribution_is_fine,
        ),
    ];
    for (name, observed) in qualitative_requirements {
        assert!(
            observed,
            "ore-preparation capability contract failed: {name}"
        );
    }

    std::println!(
        "CAPABILITY ORE_PREP seed=0x{seed:016X} reachability=bootstrapped-industrial batch={}mg copper={}ppm stages=[crush:{}t grind:{}t screen:{}t regrind:{}t] matter=conserved energy=resolved",
        batch_mass.milligrams(),
        copper_ppm,
        crush_duration.value(),
        grind_duration.value(),
        screen_duration.value(),
        fine_duration_ticks,
    );
}
