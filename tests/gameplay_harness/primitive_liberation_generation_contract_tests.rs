//! Lightweight contracts for primitive-liberation world generation and provider discovery.

use std::collections::BTreeSet;

use super::primitive_liberation_generation::{
    LiberationGenerationRole, ordinary_manual_electrical_power_providers,
    primitive_liberation_batch_ceiling, primitive_liberation_world_parameters,
};

#[test]
fn organic_liberation_generation_varies_live_feed_and_campaign() {
    let registries = deep_hearth::content::build_registries();
    let anchor = primitive_liberation_world_parameters(
        &registries,
        0,
        LiberationGenerationRole::MaintainedAnchor,
        false,
    );
    let fallback = primitive_liberation_world_parameters(
        &registries,
        0,
        LiberationGenerationRole::MaintainedCoverage,
        true,
    );
    assert_eq!(fallback.planned_batches, 1);
    assert!(anchor.planned_batches > fallback.planned_batches);

    let worlds = (1_u64..=64)
        .map(|seed| {
            (
                seed,
                primitive_liberation_world_parameters(
                    &registries,
                    seed,
                    LiberationGenerationRole::Organic,
                    false,
                ),
            )
        })
        .collect::<Vec<_>>();

    assert!(worlds.iter().all(|(seed, world)| {
        !world.batch_mass.is_zero()
            && world.batch_mass
                <= primitive_liberation_batch_ceiling(
                    &registries,
                    *seed,
                    LiberationGenerationRole::Organic,
                )
    }));
    for values in [
        worlds
            .iter()
            .map(|(_, world)| world.batch_mass.milligrams())
            .collect::<BTreeSet<_>>(),
        worlds
            .iter()
            .map(|(_, world)| u64::from(world.copper_ppm))
            .collect::<BTreeSet<_>>(),
        worlds
            .iter()
            .map(|(_, world)| u64::from(world.clay_share_ppm))
            .collect::<BTreeSet<_>>(),
        worlds
            .iter()
            .map(|(_, world)| world.planned_batches)
            .collect::<BTreeSet<_>>(),
    ] {
        assert!(
            values.len() > 1,
            "organic liberation generation stopped varying"
        );
    }
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
