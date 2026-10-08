//! Maintained primitive-liberation investment-route contracts.

use deep_hearth::content::build_registries;

use super::{LiberationExtensionChoice, plan_liberation_extension};
use crate::focused_witnesses::{
    LIBERATION_MANUAL_FALLBACK_COVERAGE_SEED, PROGRESSION_MAINTAINED_ANCHOR_SEED,
};
use crate::primitive_liberation_generation::{
    LiberationGenerationRole, primitive_liberation_world_parameters,
};

#[test]
fn maintained_liberation_witnesses_pin_both_investment_routes() {
    let registries = build_registries();
    let build_role = LiberationGenerationRole::MaintainedAnchor;
    let build_world = primitive_liberation_world_parameters(
        &registries,
        PROGRESSION_MAINTAINED_ANCHOR_SEED,
        build_role,
        false,
    );
    assert_eq!(
        plan_liberation_extension(
            &registries,
            PROGRESSION_MAINTAINED_ANCHOR_SEED,
            build_role,
            build_world.batch_mass,
            build_world.planned_batches,
        )
        .choice,
        LiberationExtensionChoice::BuildKit,
        "maintained long liberation campaign lost its extension justification"
    );

    let fallback_role = LiberationGenerationRole::MaintainedCoverage;
    let fallback_world = primitive_liberation_world_parameters(
        &registries,
        LIBERATION_MANUAL_FALLBACK_COVERAGE_SEED,
        fallback_role,
        true,
    );
    assert_eq!(fallback_world.planned_batches, 1);
    assert_eq!(
        plan_liberation_extension(
            &registries,
            LIBERATION_MANUAL_FALLBACK_COVERAGE_SEED,
            fallback_role,
            fallback_world.batch_mass,
            fallback_world.planned_batches,
        )
        .choice,
        LiberationExtensionChoice::ManualFallback,
        "maintained short liberation campaign lost the low-infrastructure fallback"
    );
}
