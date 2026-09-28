//! Whole-package manual assembly projection from actor-visible inventory.

use std::collections::BTreeMap;
use std::num::NonZeroU64;

use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::crafting::{project_manual_craft_hand_work, resolve_manual_craft};
use deep_hearth::inventory::StockpileId;
use deep_hearth::material::{CommodityKey, MaterialAssemblyProfile};
use deep_hearth::registry::Registries;

use super::manual_craft_planning::manual_craft_plan_for_available_output;
use super::manual_craft_selection::{
    first_sufficient_pure_temperature, select_manual_craft_request,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ManualAssemblyProjection {
    pub(super) attention_ticks: u64,
    pub(super) input_mass_mg: u64,
    pub(super) embodied_mass_mg: u64,
    pub(super) metabolic_nj: u128,
    pub(super) hydration_ul: u64,
}

/// Projects the complete manual shaping bill for one future assembly package without mutating
/// runtime state. Repeated commodity demand shares whole-batch craft surplus, and aggregate source
/// demand must fit one homogeneous pure temperature group.
pub(super) fn project_manual_assembly_package(
    registries: &Registries,
    state: &AppState,
    sources: &[StockpileId],
    destination: StockpileId,
    profiles: &[&MaterialAssemblyProfile],
    context: &'static str,
) -> ManualAssemblyProjection {
    assert!(
        !profiles.is_empty(),
        "gameplay harness {context} requires at least one assembly profile"
    );
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
    let mut source_requirements = BTreeMap::<(StockpileId, CommodityKey), Mass>::new();
    for (commodity, required) in required_by_commodity {
        let available = destination_record.get_mass(commodity);
        if available >= required {
            continue;
        }
        let missing = required
            .checked_sub(available)
            .unwrap_or_else(|| unreachable!("assembly projection checked component availability"));
        let (craft, batches, source) = manual_craft_plan_for_available_output(
            registries, state, sources, commodity, missing, context,
        );
        let request = select_manual_craft_request(
            registries,
            state,
            craft.process(),
            source,
            batches,
            context,
        );
        let resolution =
            resolve_manual_craft(registries, state, &request).unwrap_or_else(|error| {
                panic!("gameplay harness {context} projection failed: {error}")
            });
        attention_ticks = attention_ticks
            .checked_add(resolution.duration().value())
            .unwrap_or_else(|| panic!("gameplay harness {context} attention overflowed"));
        let batches_nonzero = NonZeroU64::new(batches)
            .unwrap_or_else(|| unreachable!("nonzero component demand yields nonzero batches"));
        let work = project_manual_craft_hand_work(registries, craft.process(), batches_nonzero)
            .unwrap_or_else(|error| {
                panic!("gameplay harness {context} assembly hand-work projection failed: {error}")
            });
        assert_eq!(
            work.duration(),
            resolution.duration(),
            "gameplay harness {context} assembly body-cost projection diverged from craft resolution"
        );
        metabolic_nj = metabolic_nj
            .checked_add(work.resource_budget().metabolic_energy().nanojoules())
            .unwrap_or_else(|| panic!("gameplay harness {context} metabolic budget overflowed"));
        hydration_ul = hydration_ul
            .checked_add(work.resource_budget().hydration().microliters())
            .unwrap_or_else(|| panic!("gameplay harness {context} hydration budget overflowed"));
        let consumed = Mass::from_milligrams(
            craft
                .input_mass()
                .milligrams()
                .checked_mul(batches)
                .unwrap_or_else(|| panic!("gameplay harness {context} input mass overflowed")),
        );
        input_mass = input_mass
            .checked_add(consumed)
            .unwrap_or_else(|| panic!("gameplay harness {context} raw input total overflowed"));
        let entry = source_requirements
            .entry((source, craft.input()))
            .or_insert(Mass::ZERO);
        *entry = entry
            .checked_add(consumed)
            .unwrap_or_else(|| panic!("gameplay harness {context} source demand overflowed"));
    }

    for ((source, commodity), required) in source_requirements {
        assert!(
            first_sufficient_pure_temperature(state, source, commodity, required, context)
                .is_some(),
            "gameplay harness {context} package projection overbooks homogeneous pure commodity {} in source {}",
            commodity.value(),
            source.value()
        );
    }

    ManualAssemblyProjection {
        attention_ticks,
        input_mass_mg: input_mass.milligrams(),
        embodied_mass_mg: embodied_mass.milligrams(),
        metabolic_nj,
        hydration_ul,
    }
}
