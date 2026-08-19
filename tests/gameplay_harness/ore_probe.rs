//! Focused ore-preparation capability probe.

use super::ore_setup::{OrePreparationSetup, mixed_ore_composition, setup_ore_preparation_probe};
use super::production_support::{
    finish_production_job, only_lot_in_stockpile, varied_healthy_condition,
};
use super::seed::mix64;
use super::support::nominal_equipment_mass_capability;
use deep_hearth::content::{
    ENERGY_MECHANICAL_LARGE_DRIVE, EQUIPMENT_DRY_SCREEN, EQUIPMENT_GRINDING_MILL,
    EQUIPMENT_JAW_CRUSHER, PROCESS_CRUSH_ORE, PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
    PROCESS_GRIND_CRUSHED_ORE, PROCESS_SCREEN_CRUSHED_ORE,
};
use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::core::state::validate_loaded_state;
use deep_hearth::energy::calculate_mass_specific_energy;
use deep_hearth::inventory::MaterialLotSelection;
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

fn greatest_common_divisor(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn probe_parameters(registries: &Registries, seed: u64) -> OrePreparationSetup {
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
    let required_energy_upper_bound = [
        crusher.specific_energy(),
        grinder.specific_energy(),
        screening.specific_energy(),
        fine_grind.specific_energy(),
    ]
    .into_iter()
    .map(|specific| calculate_mass_specific_energy(batch_mass, specific))
    .try_fold(Energy::ZERO, |total, energy| total.checked_add(energy))
    .unwrap_or_else(|| panic!("ore preparation chain energy upper bound overflowed"));
    let drive_capacity = registries
        .energy()
        .get_store(ENERGY_MECHANICAL_LARGE_DRIVE)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("ore preparation drive definition disappeared"));
    let slack = drive_capacity
        .checked_sub(required_energy_upper_bound)
        .unwrap_or_else(|| {
            panic!(
                "authored industrial drive cannot power the complete legal ore-preparation chain"
            )
        });
    let headroom_ppm = 50_000 + (mix64(seed ^ 0x454E_4552_4759_4844) % 550_001) as u32;
    let varied_headroom = Energy::from_nanojoules(
        slack
            .nanojoules()
            .checked_mul(u128::from(headroom_ppm))
            .map(|scaled| scaled / 1_000_000)
            .unwrap_or_else(|| panic!("ore preparation energy headroom scaling overflowed")),
    );
    let drive_energy = required_energy_upper_bound
        .checked_add(varied_headroom)
        .unwrap_or_else(|| panic!("ore preparation initial energy overflowed"));
    OrePreparationSetup {
        batch_mass,
        copper_ppm,
        crusher_condition: varied_healthy_condition(
            registries,
            EQUIPMENT_JAW_CRUSHER,
            mix64(seed ^ 0x4352_5553_4843_4F4E),
        ),
        grinder_condition: varied_healthy_condition(
            registries,
            EQUIPMENT_GRINDING_MILL,
            mix64(seed ^ 0x4752_494E_4443_4F4E),
        ),
        screen_condition: varied_healthy_condition(
            registries,
            EQUIPMENT_DRY_SCREEN,
            mix64(seed ^ 0x5343_5245_454E_434F),
        ),
        drive_energy,
    }
}

pub(super) fn run_ore_preparation_capability_probe(registries: &Registries, seed: u64) {
    let setup = probe_parameters(registries, seed);
    let batch_mass = setup.batch_mass;
    let copper_ppm = setup.copper_ppm;
    let initial_crusher_condition = setup.crusher_condition;
    let initial_grinder_condition = setup.grinder_condition;
    let initial_screen_condition = setup.screen_condition;
    let (mut state, ids) = setup_ore_preparation_probe(registries, seed, setup);
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
    let crush_job = validate_start_process(
        registries,
        &state,
        crushed.process_resolution(),
        ids.ore_source,
        ids.crushed_storage,
    )
    .unwrap_or_else(|error| panic!("ore preparation crushing start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("ore preparation crushing commit failed: {error}"));
    finish_production_job(
        registries,
        &mut state,
        crush_job,
        crush_duration,
        "ore crushing",
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("ore preparation post-crush audit failed: {error}"));

    let crushed_lot = only_lot_in_stockpile(&state, ids.crushed_storage, "crushed ore output");
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
    let grind_job = validate_start_process(
        registries,
        &state,
        ground.process_resolution(),
        ids.crushed_storage,
        ids.ground_storage,
    )
    .unwrap_or_else(|error| panic!("ore preparation grinding start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("ore preparation grinding commit failed: {error}"));
    finish_production_job(
        registries,
        &mut state,
        grind_job,
        grind_duration,
        "ore grinding",
    );
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

    let ground_lot = only_lot_in_stockpile(&state, ids.ground_storage, "ground ore output");
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
    let screen_job = validate_start_process_routed(
        registries,
        &state,
        screened.process_resolution(),
        ids.ground_storage,
        &routes,
    )
    .unwrap_or_else(|error| panic!("ore preparation screening start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("ore preparation screening commit failed: {error}"));
    finish_production_job(
        registries,
        &mut state,
        screen_job,
        screen_duration,
        "ore screening",
    );
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
            let oversize_lot =
                only_lot_in_stockpile(&state, ids.oversize_storage, "screen oversize output");
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
            let fine_job = validate_start_process(
                registries,
                &state,
                fine_ground.process_resolution(),
                ids.oversize_storage,
                ids.undersize_storage,
            )
            .unwrap_or_else(|error| panic!("ore preparation fine-grinding start failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("ore preparation fine-grinding commit failed: {error}"));
            finish_production_job(
                registries,
                &mut state,
                fine_job,
                fine_duration,
                "ore fine grinding",
            );
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
        "CAPABILITY ORE_PREP seed=0x{seed:016X} reachability=bootstrapped-industrial batch={}mg copper={}ppm initial-condition=[crusher:{} grinder:{} screen:{}ppm] stored-work=[initial:{}nJ consumed:{}nJ remaining:{}nJ] stages=[crush:{}t grind:{}t screen:{}t regrind:{}t] matter=conserved energy=resolved",
        batch_mass.milligrams(),
        copper_ppm,
        initial_crusher_condition.parts_per_million(),
        initial_grinder_condition.parts_per_million(),
        initial_screen_condition.parts_per_million(),
        initial_energy.nanojoules(),
        consumed_energy.nanojoules(),
        final_energy.nanojoules(),
        crush_duration.value(),
        grind_duration.value(),
        screen_duration.value(),
        fine_duration_ticks,
    );
}
