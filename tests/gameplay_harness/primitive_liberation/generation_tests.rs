//! Cheap contracts keeping primitive-liberation organic worlds materially varied.

use std::collections::BTreeSet;

use super::*;

#[test]
fn organic_liberation_generation_varies_live_feed_and_campaign() {
    let registries = deep_hearth::content::build_registries();
    let worlds = (1_u64..=64)
        .map(|seed| {
            let case = FocusedProbeCase::new(seed, None, FocusedProbeRole::OrganicVariation);
            (
                case,
                primitive_liberation_world_parameters(&registries, case),
            )
        })
        .collect::<Vec<_>>();

    assert!(
        worlds.iter().all(|(case, world)| {
            !world.batch_mass.is_zero()
                && world.batch_mass <= primitive_liberation_batch_ceiling(&registries, *case)
        }),
        "generated liberation batches must remain inside the current complete processing route at the inherited live condition"
    );

    assert!(
        worlds
            .iter()
            .map(|(_, world)| world.batch_mass.milligrams())
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "organic liberation generation collapsed to one representable ore batch"
    );
    assert!(
        worlds
            .iter()
            .map(|(_, world)| world.copper_ppm)
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "organic liberation generation collapsed to one copper assay"
    );
    assert!(
        worlds
            .iter()
            .map(|(_, world)| world.clay_share_ppm)
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "organic liberation generation collapsed to one gangue composition"
    );
    assert!(
        worlds
            .iter()
            .map(|(_, world)| world.planned_batches)
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "organic liberation generation collapsed to one disclosed campaign horizon"
    );
}

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

#[test]
fn liberation_frontier_electrical_power_uses_only_current_acquirable_providers() {
    let registries = deep_hearth::content::build_registries();
    let providers = ordinary_manual_electrical_power_providers(&registries);

    assert!(
        !providers.is_empty(),
        "ordinary liberation-to-foundry continuity requires an authored electrical manual-power provider"
    );
    for (method, equipment, power) in providers {
        let method = registries
            .labor()
            .get_manual_power(method)
            .unwrap_or_else(|| {
                panic!("frontier electrical provider referenced an unknown labor method")
            });
        let equipment = registries
            .equipment()
            .get_equipment(equipment)
            .unwrap_or_else(|| panic!("frontier electrical provider referenced unknown equipment"));
        assert_eq!(
            method.carrier(),
            deep_hearth::energy::EnergyCarrier::Electrical
        );
        assert!(equipment.has_authored_acquisition_edge());
        assert_eq!(
            equipment
                .capabilities()
                .get_capability(method.power_capability()),
            Some(deep_hearth::capability::CapabilityValue::Power(power))
        );
        assert!(!power.is_zero());
    }
}
