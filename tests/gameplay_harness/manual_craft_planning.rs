//! Registry-derived actor planning for manual production routes.

use std::collections::BTreeMap;

use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::crafting::{ManualCraftDefinition, resolve_manual_craft};
use deep_hearth::inventory::StockpileId;
use deep_hearth::material::{CommodityKey, MaterialAssemblyProfile};
use deep_hearth::registry::{ProcessEquipmentRole, Registries};

use super::manual_craft_selection::{
    first_sufficient_pure_temperature, select_manual_craft_request,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ManualAssemblyProjection {
    pub(super) attention_ticks: u64,
    pub(super) input_mass_mg: u64,
    pub(super) embodied_mass_mg: u64,
}

fn has_selectable_manual_craft_input(
    state: &AppState,
    source: StockpileId,
    definition: &ManualCraftDefinition,
    batches: u64,
) -> bool {
    let Some(required_mg) = definition.input_mass().milligrams().checked_mul(batches) else {
        return false;
    };
    first_sufficient_pure_temperature(
        state,
        source,
        definition.input(),
        Mass::from_milligrams(required_mg),
        "manual-craft availability",
    )
    .is_some()
}

fn manual_craft_plan_for_output_matching<'a>(
    registries: &'a Registries,
    commodity: CommodityKey,
    required: Mass,
    context: &'static str,
    mut input_is_available: impl FnMut(&ManualCraftDefinition, u64) -> bool,
) -> (&'a ManualCraftDefinition, u64) {
    assert!(
        !required.is_zero(),
        "gameplay harness {context} requires nonzero produced mass"
    );
    let candidates = registries
        .crafting()
        .manual_producers(commodity)
        .filter(|definition| {
            registries
                .process_topology(definition.process())
                .unwrap_or_else(|| panic!("manual producer lost process topology"))
                .equipment_role()
                != ProcessEquipmentRole::Required
        })
        .filter_map(|definition| {
            let per_batch = definition
                .outputs()
                .iter()
                .find(|output| output.commodity() == commodity)
                .map(|output| output.mass())
                .unwrap_or_else(|| {
                    panic!(
                        "gameplay harness {context} producer {} lost requested commodity {}",
                        definition.process().value(),
                        commodity.value()
                    )
                });
            assert!(
                !per_batch.is_zero(),
                "gameplay harness {context} producer {} has zero requested output",
                definition.process().value()
            );
            let batches = required.milligrams().div_ceil(per_batch.milligrams());
            if !input_is_available(definition, batches) {
                return None;
            }
            let total_ticks = definition
                .duration()
                .value()
                .checked_mul(batches)
                .unwrap_or_else(|| panic!("gameplay harness {context} attention cost overflowed"));
            let total_input_mg = definition
                .input_mass()
                .milligrams()
                .checked_mul(batches)
                .unwrap_or_else(|| panic!("gameplay harness {context} input cost overflowed"));
            let exertion = definition.exertion();
            let policy_key = (
                total_ticks,
                total_input_mg,
                exertion.energy_cost_per_tick().nanojoules(),
                exertion.hydration_loss_per_tick().microliters(),
            );
            Some((definition, batches, policy_key))
        })
        .collect::<Vec<_>>();
    let best_key = candidates
        .iter()
        .map(|(_, _, policy_key)| *policy_key)
        .min()
        .unwrap_or_else(|| {
            panic!(
                "gameplay harness {context} has no eligible manual route to commodity {}",
                commodity.value()
            )
        });
    let mut best = candidates
        .into_iter()
        .filter(|(_, _, policy_key)| *policy_key == best_key);
    let (definition, batches, _) = best
        .next()
        .unwrap_or_else(|| unreachable!("best manual-production key came from a candidate"));
    assert!(
        best.next().is_none(),
        "gameplay harness {context} has equally efficient observable manual routes to commodity {}; add an explicit actor preference instead of using process identity",
        commodity.value()
    );
    (definition, batches)
}

/// Selects the most attention-efficient equipment-free authored route without considering current
/// inventory. Use this only for pre-episode requirement topology where current inventory
/// deliberately does not exist yet.
pub(super) fn manual_craft_topology_plan_for_output<'a>(
    registries: &'a Registries,
    commodity: CommodityKey,
    required: Mass,
    context: &'static str,
) -> (&'a ManualCraftDefinition, u64) {
    manual_craft_plan_for_output_matching(
        registries,
        commodity,
        required,
        context,
        |_definition, _batches| true,
    )
}

/// Selects the most attention-efficient equipment-free route whose exact pure homogeneous input is
/// currently present in at least one declared actor-visible source.
///
/// This prevents nominally attractive salvage or conversion recipes from masquerading as available
/// actions merely because their output topology is authored. Canonical craft resolution remains the
/// legality proof after actor policy chooses the route. Source order is the explicit actor preference
/// when more than one source can satisfy the selected route exactly.
pub(super) fn manual_craft_plan_for_available_output<'a>(
    registries: &'a Registries,
    state: &AppState,
    sources: &[StockpileId],
    commodity: CommodityKey,
    required: Mass,
    context: &'static str,
) -> (&'a ManualCraftDefinition, u64, StockpileId) {
    assert!(
        !sources.is_empty(),
        "gameplay harness {context} requires at least one actor-visible craft source"
    );
    let (definition, batches) = manual_craft_plan_for_output_matching(
        registries,
        commodity,
        required,
        context,
        |definition, batches| {
            sources
                .iter()
                .copied()
                .any(|source| has_selectable_manual_craft_input(state, source, definition, batches))
        },
    );
    let source = sources
        .iter()
        .copied()
        .find(|source| has_selectable_manual_craft_input(state, *source, definition, batches))
        .unwrap_or_else(|| {
            unreachable!("available manual-craft route must retain a proven source")
        });
    (definition, batches, source)
}

/// Projects the complete manual shaping bill for one future assembly package from actor-visible
/// inventory without mutating runtime state.
///
/// Requirements are aggregated before route selection so craft surplus can satisfy later inputs in
/// the same package exactly as it can during execution. Source use is then aggregated again and
/// checked against one homogeneous pure temperature group, preventing several individually valid
/// projections from silently overbooking the same visible material.
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
    }
}
