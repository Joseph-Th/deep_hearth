//! Finer tailings regrind and finite secondary copper scavenging stage.

use deep_hearth::content::{
    FORM_EXHAUSTED_TAILINGS, PROCESS_REGRIND_COPPER_TAILINGS, PROCESS_SCAVENGE_COPPER_TAILINGS,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::ore_processing::{
    ComminutionRequest, ConstituentSeparationProcessDefinition, ConstituentSeparationRequest,
    resolve_comminution_process, resolve_constituent_separation_process,
};
use deep_hearth::production::{
    ProcessOutputRoute, validate_start_process, validate_start_process_routed,
};
use deep_hearth::registry::Registries;

use super::super::production_timing::finish_uninterrupted_production_job;
use super::primary::PrimaryLiberationOutcome;
use super::{PrimitiveLiberationScenario, support};

pub(super) struct ScavengingOutcome {
    pub(super) concentrate_mass: Mass,
    pub(super) concentrate_grade_ppm: u32,
    pub(super) additional_recovered_copper_ppm_mg: u128,
    pub(super) exhausted_tailings_mass: Mass,
}

pub(super) fn run(
    registries: &Registries,
    scenario: &mut PrimitiveLiberationScenario,
    primary: &PrimaryLiberationOutcome,
) -> ScavengingOutcome {
    let copper_ppm = scenario.copper_ppm;
    let tailings = scenario.tailings;
    let fine_tailings = scenario.fine_tailings;
    let exhausted_tailings = scenario.exhausted_tailings;
    let concentrate = scenario.concentrate;
    let quern = scenario.quern;
    let separator = scenario.separator;
    let treadle = scenario.treadle;
    let drive = scenario.drive;
    let drive_capacity = scenario.drive_capacity;
    let state = &mut scenario.state;

    support::replenish_primitive_drive(
        registries,
        state,
        treadle,
        drive,
        drive_capacity,
        "primitive liberation tailings-regrind recharge",
    );
    let regrind_feed = support::full_stockpile_selection(state, tailings);
    let regrind = resolve_comminution_process(
        registries,
        state,
        ComminutionRequest::new(
            PROCESS_REGRIND_COPPER_TAILINGS,
            tailings,
            &regrind_feed,
            quern,
            drive,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive tailings regrind failed: {error}"));
    let regrind_job = validate_start_process(
        registries,
        state,
        regrind.process_resolution(),
        tailings,
        fine_tailings,
    )
    .unwrap_or_else(|error| panic!("primitive tailings regrind start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("primitive tailings regrind commit failed: {error}"));
    finish_uninterrupted_production_job(
        registries,
        state,
        regrind_job,
        "primitive tailings regrind",
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(tailings)
            .map(|record| record.stored_mass()),
        Some(Mass::ZERO)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(fine_tailings)
            .map(|record| record.stored_mass()),
        Some(primary.tailings_mass)
    );

    support::replenish_primitive_drive(
        registries,
        state,
        treadle,
        drive,
        drive_capacity,
        "primitive liberation scavenger recharge",
    );
    let scavenger_feed = support::full_stockpile_selection(state, fine_tailings);
    let scavenged = resolve_constituent_separation_process(
        registries,
        state,
        ConstituentSeparationRequest::new(
            PROCESS_SCAVENGE_COPPER_TAILINGS,
            fine_tailings,
            &scavenger_feed,
            separator,
            drive,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive tailings scavenging failed: {error}"));
    let scavenged_concentrate_mass = scavenged.target_mass();
    let scavenged_residue_mass = scavenged.residue_mass();
    let scavenger_job = validate_start_process_routed(
        registries,
        state,
        scavenged.process_resolution(),
        fine_tailings,
        &[
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::TARGET_STREAM,
                concentrate,
            ),
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::RESIDUE_STREAM,
                exhausted_tailings,
            ),
        ],
    )
    .unwrap_or_else(|error| panic!("primitive tailings scavenging start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("primitive tailings scavenging commit failed: {error}"));
    finish_uninterrupted_production_job(
        registries,
        state,
        scavenger_job,
        "primitive tailings scavenging",
    );

    let concentrate_mass = state
        .inventory()
        .get_stockpile(concentrate)
        .map(|record| record.stored_mass())
        .unwrap_or_else(|| panic!("primitive scavenged concentrate stockpile disappeared"));
    let exhausted_tailings_mass = state
        .inventory()
        .get_stockpile(exhausted_tailings)
        .map(|record| record.stored_mass())
        .unwrap_or_else(|| panic!("primitive exhausted-tailings stockpile disappeared"));
    assert_eq!(
        concentrate_mass,
        primary
            .concentrate_mass
            .checked_add(scavenged_concentrate_mass)
            .unwrap_or_else(|| panic!("primitive scavenged concentrate mass overflowed"))
    );
    assert_eq!(exhausted_tailings_mass, scavenged_residue_mass);
    assert!(concentrate_mass > primary.concentrate_mass);
    assert!(exhausted_tailings_mass < primary.tailings_mass);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(fine_tailings)
            .map(|record| record.stored_mass()),
        Some(Mass::ZERO)
    );
    assert!(state.inventory().lot_ids(exhausted_tailings).all(|lot| {
        state
            .inventory()
            .get_lot(lot)
            .is_some_and(|record| record.commodity().form() == FORM_EXHAUSTED_TAILINGS)
    }));
    let concentrate_copper_ppm_mg = support::copper_numerator_ppm_mg(state, concentrate);
    assert!(concentrate_copper_ppm_mg > primary.concentrate_copper_ppm_mg);
    let additional_recovered_copper_ppm_mg = concentrate_copper_ppm_mg
        .checked_sub(primary.concentrate_copper_ppm_mg)
        .unwrap_or_else(|| panic!("primitive scavenger copper accounting underflowed"));
    let concentrate_grade_ppm =
        u32::try_from(concentrate_copper_ppm_mg / u128::from(concentrate_mass.milligrams()))
            .unwrap_or_else(|_| panic!("primitive concentrate grade exceeded normalized range"));
    assert!(concentrate_grade_ppm > copper_ppm);

    ScavengingOutcome {
        concentrate_mass,
        concentrate_grade_ppm,
        additional_recovered_copper_ppm_mg,
        exhausted_tailings_mass,
    }
}
