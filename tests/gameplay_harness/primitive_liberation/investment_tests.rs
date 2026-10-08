//! Maintained primitive-liberation investment-route contracts.

use super::*;

#[test]
fn maintained_liberation_witnesses_pin_both_investment_routes() {
    let registries = deep_hearth::content::build_registries();
    let build = FocusedProbeCase::new(
        super::super::focused_witnesses::PROGRESSION_MAINTAINED_ANCHOR_SEED,
        None,
        FocusedProbeRole::MaintainedAnchor,
    );
    let build_world = primitive_liberation_world_parameters(&registries, build);
    assert_eq!(
        plan_liberation_extension(
            &registries,
            build,
            build_world.batch_mass,
            build_world.planned_batches,
        )
        .choice,
        LiberationExtensionChoice::BuildKit,
        "maintained long liberation campaign lost its extension justification"
    );

    let fallback = FocusedProbeCase::new(
        super::super::focused_witnesses::LIBERATION_MANUAL_FALLBACK_COVERAGE_SEED,
        None,
        FocusedProbeRole::MaintainedCoverage,
    );
    let fallback_world = primitive_liberation_world_parameters(&registries, fallback);
    assert_eq!(fallback_world.planned_batches, 1);
    assert_eq!(
        plan_liberation_extension(
            &registries,
            fallback,
            fallback_world.batch_mass,
            fallback_world.planned_batches,
        )
        .choice,
        LiberationExtensionChoice::ManualFallback,
        "maintained short liberation campaign lost the low-infrastructure fallback"
    );
}
