//! Final finite-recovery cleaning of rich concentrate into immediately usable native copper.

use deep_hearth::content::{
    FORM_EXHAUSTED_TAILINGS, FORM_NATIVE_METAL, MATERIAL_COPPER,
    PROCESS_CLEAN_NATIVE_COPPER_CONCENTRATE,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::material::CommodityKey;
use deep_hearth::ore_processing::{
    ConstituentSeparationProcessDefinition, ConstituentSeparationRequest,
    resolve_constituent_separation_process,
};
use deep_hearth::production::{ProcessOutputRoute, validate_start_process_routed};
use deep_hearth::registry::Registries;

use super::super::production_timing::finish_uninterrupted_production_job;
use super::{PrimitiveLiberationScenario, support};

#[derive(Debug, PartialEq, Eq)]
pub(super) struct CleanupOutcome {
    pub(super) native_copper_mass: Mass,
    pub(super) cleanup_residue_mass: Mass,
    pub(super) exhausted_tailings_mass: Mass,
    pub(super) target_recovery_ppm: u32,
}

pub(super) fn run(
    registries: &Registries,
    scenario: &mut PrimitiveLiberationScenario,
) -> CleanupOutcome {
    let definition = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_CLEAN_NATIVE_COPPER_CONCENTRATE)
        .unwrap_or_else(|| panic!("primitive concentrate-cleaning definition disappeared"));
    let concentrate = scenario.concentrate;
    let native_copper = scenario.native_copper;
    let exhausted_tailings = scenario.exhausted_tailings;
    let separator = scenario.separator;
    let treadle = scenario.treadle;
    let drive = scenario.drive;
    let state = &mut scenario.state;
    let exhausted_before = state
        .inventory()
        .get_stockpile(exhausted_tailings)
        .map(|record| record.stored_mass())
        .unwrap_or_else(|| panic!("primitive exhausted-tailings stockpile disappeared"));

    scenario.charges.push(support::prepare_stage(
        registries,
        state,
        (
            PROCESS_CLEAN_NATIVE_COPPER_CONCENTRATE,
            separator,
            concentrate,
        ),
        (treadle, drive),
        scenario.charge_policy,
        "primitive concentrate-cleaning recharge",
    ));
    let feed = support::full_stockpile_selection(state, concentrate);
    let resolved = resolve_constituent_separation_process(
        registries,
        state,
        ConstituentSeparationRequest::new(
            PROCESS_CLEAN_NATIVE_COPPER_CONCENTRATE,
            concentrate,
            &feed,
            separator,
            drive,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive concentrate cleaning failed: {error}"));
    let native_mass = resolved.target_mass();
    let cleanup_residue_mass = resolved.residue_mass();
    assert!(native_mass > Mass::ZERO);
    assert!(cleanup_residue_mass > Mass::ZERO);
    let job = validate_start_process_routed(
        registries,
        state,
        resolved.process_resolution(),
        concentrate,
        &[
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::TARGET_STREAM,
                native_copper,
            ),
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::RESIDUE_STREAM,
                exhausted_tailings,
            ),
        ],
    )
    .unwrap_or_else(|error| panic!("primitive concentrate-cleaning start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("primitive concentrate-cleaning commit failed: {error}"));
    finish_uninterrupted_production_job(registries, state, job, "primitive concentrate cleaning");

    assert_eq!(
        state
            .inventory()
            .get_stockpile(concentrate)
            .map(|record| record.stored_mass()),
        Some(Mass::ZERO),
        "ordinary cleanup must consume the rich concentrate rather than duplicate it"
    );
    let native_copper_mass = state
        .inventory()
        .get_stockpile(native_copper)
        .map(|record| record.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL)))
        .unwrap_or_else(|| panic!("primitive native-copper stockpile disappeared"));
    assert_eq!(native_copper_mass, native_mass);
    let exhausted_tailings_mass = state
        .inventory()
        .get_stockpile(exhausted_tailings)
        .map(|record| record.stored_mass())
        .unwrap_or_else(|| panic!("primitive exhausted-tailings stockpile disappeared"));
    assert_eq!(
        exhausted_tailings_mass,
        exhausted_before
            .checked_add(cleanup_residue_mass)
            .unwrap_or_else(|| panic!("primitive cleanup tailings mass overflowed"))
    );
    assert!(state.inventory().lot_ids(exhausted_tailings).all(|lot| {
        state
            .inventory()
            .get_lot(lot)
            .is_some_and(|record| record.commodity().form() == FORM_EXHAUSTED_TAILINGS)
    }));
    CleanupOutcome {
        native_copper_mass,
        cleanup_residue_mass,
        exhausted_tailings_mass,
        target_recovery_ppm: definition.target_recovery_ppm(),
    }
}
