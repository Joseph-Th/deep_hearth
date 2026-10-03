//! Whole-package projection through recursive manual construction topology.

use std::collections::BTreeMap;

use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::inventory::StockpileId;
use deep_hearth::material::{CommodityKey, MaterialAssemblyProfile};
use deep_hearth::registry::Registries;

use super::manual_construction_planning::manual_construction_route_from_roots;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ManualConstructionAssemblyProjection {
    pub(super) attention_ticks: u64,
    pub(super) input_mass_mg: u64,
    pub(super) embodied_mass_mg: u64,
    pub(super) metabolic_nj: u128,
    pub(super) hydration_ul: u64,
}

/// Projects a complete assembly package through the same recursive equipment-free manual topology
/// used for live construction, starting from the scenario's disclosed raw-material frontier.
pub(super) fn project_manual_construction_package_from_roots(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    destination: StockpileId,
    profiles: &[&MaterialAssemblyProfile],
    raw_roots: &[CommodityKey],
    context: &'static str,
) -> ManualConstructionAssemblyProjection {
    assert!(
        !profiles.is_empty(),
        "gameplay harness {context} requires at least one assembly profile"
    );
    let raw_record = state
        .inventory()
        .get_stockpile(raw)
        .unwrap_or_else(|| panic!("gameplay harness {context} raw stockpile disappeared"));
    let destination_record = state
        .inventory()
        .get_stockpile(destination)
        .unwrap_or_else(|| panic!("gameplay harness {context} destination stockpile disappeared"));
    let mut required_by_commodity = BTreeMap::<CommodityKey, Mass>::new();
    let mut embodied_mass = Mass::ZERO;
    for profile in profiles {
        embodied_mass = embodied_mass
            .checked_add(profile.input_mass())
            .unwrap_or_else(|| panic!("gameplay harness {context} embodied mass overflowed"));
        for input in profile.inputs() {
            let entry = required_by_commodity
                .entry(input.commodity())
                .or_insert(Mass::ZERO);
            *entry = entry.checked_add(input.mass()).unwrap_or_else(|| {
                panic!("gameplay harness {context} component demand overflowed")
            });
        }
    }

    let mut attention_ticks = 0_u64;
    let mut input_mass = Mass::ZERO;
    let mut metabolic_nj = 0_u128;
    let mut hydration_ul = 0_u64;
    let mut raw_requirements = BTreeMap::<CommodityKey, Mass>::new();
    for (commodity, required) in required_by_commodity {
        let available = destination_record.get_mass(commodity);
        if available >= required {
            continue;
        }
        let missing = required
            .checked_sub(available)
            .unwrap_or_else(|| unreachable!("assembly projection checked component availability"));
        let route = manual_construction_route_from_roots(
            registries,
            commodity,
            missing,
            raw_roots,
            context,
        )
        .unwrap_or_else(|| {
            panic!(
                "gameplay harness {context} has no ordinary manual route from disclosed roots to component {}",
                commodity.value()
            )
        });
        assert!(
            !route.steps.is_empty(),
            "gameplay harness {context} assembly consumes disclosed raw commodity {} directly; projection requires an explicit custody transfer",
            commodity.value()
        );
        attention_ticks = attention_ticks
            .checked_add(route.attention_ticks)
            .unwrap_or_else(|| panic!("gameplay harness {context} attention overflowed"));
        input_mass = input_mass
            .checked_add(route.raw_mass)
            .unwrap_or_else(|| panic!("gameplay harness {context} raw input total overflowed"));
        metabolic_nj = metabolic_nj
            .checked_add(route.metabolic_energy_nj)
            .unwrap_or_else(|| panic!("gameplay harness {context} metabolic budget overflowed"));
        hydration_ul = hydration_ul
            .checked_add(route.hydration_ul)
            .unwrap_or_else(|| panic!("gameplay harness {context} hydration budget overflowed"));
        let entry = raw_requirements
            .entry(route.raw_commodity)
            .or_insert(Mass::ZERO);
        *entry = entry
            .checked_add(route.raw_mass)
            .unwrap_or_else(|| panic!("gameplay harness {context} raw requirement overflowed"));
    }
    for (commodity, required) in raw_requirements {
        assert!(
            raw_record.get_mass(commodity) >= required,
            "gameplay harness {context} package projection overbooks disclosed raw commodity {}",
            commodity.value()
        );
    }

    ManualConstructionAssemblyProjection {
        attention_ticks,
        input_mass_mg: input_mass.milligrams(),
        embodied_mass_mg: embodied_mass.milligrams(),
        metabolic_nj,
        hydration_ul,
    }
}
