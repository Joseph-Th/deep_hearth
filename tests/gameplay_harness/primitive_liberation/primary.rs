//! Primary primitive ore liberation, sizing, regrinding, and first-pass concentration stage.

use deep_hearth::content::{
    PROCESS_CONCENTRATE_COPPER, PROCESS_CRUSH_ORE, PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
    PROCESS_GRIND_CRUSHED_ORE, PROCESS_SCREEN_CRUSHED_ORE,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::inventory::MaterialLotSelection;
use deep_hearth::ore_processing::{
    ComminutionRequest, ConstituentSeparationProcessDefinition, ConstituentSeparationRequest,
    ScreeningProcessDefinition, ScreeningRequest, resolve_comminution_process,
    resolve_constituent_separation_process, resolve_screening_process,
};
use deep_hearth::production::{
    ProcessOutputRoute, validate_start_process, validate_start_process_routed,
};
use deep_hearth::registry::Registries;

use super::super::production_timing::finish_uninterrupted_production_job;
use super::{PrimitiveLiberationScenario, support};

#[derive(Debug, PartialEq, Eq)]
pub(super) struct PrimaryLiberationOutcome {
    pub(super) concentrate_mass: Mass,
    pub(super) tailings_mass: Mass,
    pub(super) concentrate_copper_ppm_mg: u128,
    pub(super) concentrate_grade_ppm: u32,
}

pub(super) fn run(
    registries: &Registries,
    scenario: &mut PrimitiveLiberationScenario,
) -> PrimaryLiberationOutcome {
    let batch_mass = scenario.batch_mass;
    let copper_ppm = scenario.copper_ppm;
    let ore = scenario.ore;
    let crushed = scenario.crushed;
    let ground = scenario.ground;
    let undersize = scenario.undersize;
    let oversize = scenario.oversize;
    let concentrate = scenario.concentrate;
    let tailings = scenario.tailings;
    let ore_lot = scenario.ore_lot;
    let crusher = scenario.crusher;
    let quern = scenario.quern;
    let screen = scenario.screen;
    let separator = scenario.separator;
    let treadle = scenario.treadle;
    let drive = scenario.drive;
    let policy = scenario.charge_policy;
    let charges = &mut scenario.charges;
    let state = &mut scenario.state;

    charges.push(support::prepare_stage(
        registries,
        state,
        (PROCESS_CRUSH_ORE, crusher, ore),
        (treadle, drive),
        policy,
        "primitive liberation treadle charge",
    ));
    let crush = resolve_comminution_process(
        registries,
        state,
        ComminutionRequest::new(
            PROCESS_CRUSH_ORE,
            ore,
            &[MaterialLotSelection::new(ore_lot, batch_mass)],
            crusher,
            drive,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive liberation crushing failed: {error}"));
    let crush_job =
        validate_start_process(registries, state, crush.process_resolution(), ore, crushed)
            .unwrap_or_else(|error| panic!("primitive liberation crushing start failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("primitive liberation crushing commit failed: {error}"));
    finish_uninterrupted_production_job(
        registries,
        state,
        crush_job,
        "primitive liberation crushing",
    );

    charges.push(support::prepare_stage(
        registries,
        state,
        (PROCESS_GRIND_CRUSHED_ORE, quern, crushed),
        (treadle, drive),
        policy,
        "primitive liberation post-crush recharge",
    ));
    let ground_feed = support::full_stockpile_selection(state, crushed);
    let grind = resolve_comminution_process(
        registries,
        state,
        ComminutionRequest::new(
            PROCESS_GRIND_CRUSHED_ORE,
            crushed,
            &ground_feed,
            quern,
            drive,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive rotary-quern grinding failed: {error}"));
    let grind_job = validate_start_process(
        registries,
        state,
        grind.process_resolution(),
        crushed,
        ground,
    )
    .unwrap_or_else(|error| panic!("primitive rotary-quern start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("primitive rotary-quern commit failed: {error}"));
    finish_uninterrupted_production_job(
        registries,
        state,
        grind_job,
        "primitive rotary-quern grinding",
    );

    charges.push(support::prepare_stage(
        registries,
        state,
        (PROCESS_SCREEN_CRUSHED_ORE, screen, ground),
        (treadle, drive),
        policy,
        "primitive liberation post-grind recharge",
    ));
    let screen_feed = support::full_stockpile_selection(state, ground);
    let screened = resolve_screening_process(
        registries,
        state,
        ScreeningRequest::new(
            PROCESS_SCREEN_CRUSHED_ORE,
            ground,
            &screen_feed,
            screen,
            drive,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive sizing screen failed: {error}"));
    let screen_job = validate_start_process_routed(
        registries,
        state,
        screened.process_resolution(),
        ground,
        &[
            ProcessOutputRoute::new(ScreeningProcessDefinition::UNDERSIZE_STREAM, undersize),
            ProcessOutputRoute::new(ScreeningProcessDefinition::OVERSIZE_STREAM, oversize),
        ],
    )
    .unwrap_or_else(|error| panic!("primitive sizing-screen start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("primitive sizing-screen commit failed: {error}"));
    finish_uninterrupted_production_job(registries, state, screen_job, "primitive sizing screen");

    charges.push(support::prepare_stage(
        registries,
        state,
        (PROCESS_FINE_GRIND_SCREEN_OVERSIZE, quern, oversize),
        (treadle, drive),
        policy,
        "primitive liberation post-screen recharge",
    ));
    let oversize_mass = state
        .inventory()
        .get_stockpile(oversize)
        .map(|record| record.stored_mass())
        .unwrap_or_else(|| panic!("primitive oversize stockpile disappeared"));
    assert!(!oversize_mass.is_zero());
    let regrind_feed = support::full_stockpile_selection(state, oversize);
    let regrind = resolve_comminution_process(
        registries,
        state,
        ComminutionRequest::new(
            PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
            oversize,
            &regrind_feed,
            quern,
            drive,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive rotary-quern regrinding failed: {error}"));
    let regrind_job = validate_start_process(
        registries,
        state,
        regrind.process_resolution(),
        oversize,
        undersize,
    )
    .unwrap_or_else(|error| panic!("primitive rotary-quern regrind start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("primitive rotary-quern regrind commit failed: {error}"));
    finish_uninterrupted_production_job(
        registries,
        state,
        regrind_job,
        "primitive rotary-quern regrinding",
    );

    charges.push(support::prepare_stage(
        registries,
        state,
        (PROCESS_CONCENTRATE_COPPER, separator, undersize),
        (treadle, drive),
        policy,
        "primitive liberation post-regrind recharge",
    ));
    let concentration_feed = support::full_stockpile_selection(state, undersize);
    let separated = resolve_constituent_separation_process(
        registries,
        state,
        ConstituentSeparationRequest::new(
            PROCESS_CONCENTRATE_COPPER,
            undersize,
            &concentration_feed,
            separator,
            drive,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive concentration failed: {error}"));
    let resolved_concentrate_mass = separated.target_mass();
    let resolved_tailings_mass = separated.residue_mass();
    let separation_job = validate_start_process_routed(
        registries,
        state,
        separated.process_resolution(),
        undersize,
        &[
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::TARGET_STREAM,
                concentrate,
            ),
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::RESIDUE_STREAM,
                tailings,
            ),
        ],
    )
    .unwrap_or_else(|error| panic!("primitive concentration start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("primitive concentration commit failed: {error}"));
    finish_uninterrupted_production_job(
        registries,
        state,
        separation_job,
        "primitive concentration",
    );

    let concentrate_mass = state
        .inventory()
        .get_stockpile(concentrate)
        .map(|record| record.stored_mass())
        .unwrap_or_else(|| panic!("primitive concentrate stockpile disappeared"));
    let tailings_mass = state
        .inventory()
        .get_stockpile(tailings)
        .map(|record| record.stored_mass())
        .unwrap_or_else(|| panic!("primitive tailings stockpile disappeared"));
    assert_eq!(concentrate_mass, resolved_concentrate_mass);
    assert_eq!(tailings_mass, resolved_tailings_mass);
    assert!(!concentrate_mass.is_zero());
    assert!(!tailings_mass.is_zero());
    let concentrate_copper_ppm_mg = support::copper_numerator_ppm_mg(state, concentrate);
    let concentrate_grade_ppm =
        u32::try_from(concentrate_copper_ppm_mg / u128::from(concentrate_mass.milligrams()))
            .unwrap_or_else(|_| {
                panic!("primitive first concentrate grade exceeded normalized range")
            });
    assert!(concentrate_grade_ppm > copper_ppm);

    PrimaryLiberationOutcome {
        concentrate_mass,
        tailings_mass,
        concentrate_copper_ppm_mg,
        concentrate_grade_ppm,
    }
}
