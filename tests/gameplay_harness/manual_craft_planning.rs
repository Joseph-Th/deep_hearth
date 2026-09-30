//! Live-inventory actor planning for manual production routes.

use std::collections::BTreeSet;
use std::num::NonZeroU64;

use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::crafting::{
    ManualCraftDefinition, manual_craft_options_from_stockpile, plan_manual_craft_from_stockpile,
    project_manual_craft_hand_work, resolve_manual_craft,
};
use deep_hearth::inventory::StockpileId;
use deep_hearth::material::CommodityKey;
use deep_hearth::production::ProcessId;
use deep_hearth::registry::{ProcessEquipmentRole, Registries};

pub(super) fn live_manual_craft_catalog_for_output<'a>(
    registries: &'a Registries,
    state: &AppState,
    sources: &[StockpileId],
    commodity: CommodityKey,
    context: &'static str,
) -> Vec<(StockpileId, &'a ManualCraftDefinition, ProcessEquipmentRole)> {
    assert!(
        !sources.is_empty(),
        "gameplay harness {context} requires at least one actor-visible craft source"
    );
    let mut routes = Vec::new();
    for &source in sources {
        let catalog = manual_craft_options_from_stockpile(registries, state, source)
            .unwrap_or_else(|error| {
                panic!(
                    "gameplay harness {context} cannot read the ordinary craft catalog for source {}: {error}",
                    source.value()
                )
            });
        for option in catalog {
            let definition = registries
                .crafting()
                .get_manual(option.process())
                .unwrap_or_else(|| panic!("ordinary craft catalog exposed an unknown process"));
            if definition
                .outputs()
                .iter()
                .any(|output| output.commodity() == commodity)
            {
                routes.push((source, definition, option.equipment_role()));
            }
        }
    }
    routes
}

/// Selects the most attention-efficient equipment-free route whose exact pure homogeneous input is
/// currently present in at least one declared actor-visible source.
///
/// This prevents nominally attractive salvage or conversion recipes from masquerading as available
/// actions merely because their output topology is authored. Every live candidate is resolved
/// canonically before actor policy compares its attention cost, so planning cannot retain a copied
/// duration rule after crafting physics changes. Source order is the explicit actor preference when
/// more than one source can satisfy the same route exactly.
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
    assert!(
        !required.is_zero(),
        "gameplay harness {context} requires nonzero produced mass"
    );
    let mut selected_processes = BTreeSet::<ProcessId>::new();
    let mut candidates = Vec::new();
    for (source, definition, equipment_role) in
        live_manual_craft_catalog_for_output(registries, state, sources, commodity, context)
    {
        if equipment_role == ProcessEquipmentRole::Required
            || selected_processes.contains(&definition.process())
        {
            continue;
        }
        let per_batch = definition
            .outputs()
            .iter()
            .find(|output| output.commodity() == commodity)
            .map(|output| output.mass())
            .unwrap_or_else(|| unreachable!("catalog candidate was filtered by requested output"));
        assert!(
            !per_batch.is_zero(),
            "gameplay harness {context} producer {} has zero requested output",
            definition.process().value()
        );
        let batches = required.milligrams().div_ceil(per_batch.milligrams());
        let batches_nonzero = NonZeroU64::new(batches)
            .unwrap_or_else(|| unreachable!("nonzero output demand yields nonzero batches"));
        let Ok(request) = plan_manual_craft_from_stockpile(
            registries,
            state,
            definition.process(),
            source,
            batches_nonzero,
        ) else {
            continue;
        };
        let Ok(resolution) = resolve_manual_craft(registries, state, &request) else {
            continue;
        };
        let work =
            project_manual_craft_hand_work(registries, definition.process(), batches_nonzero)
                .unwrap_or_else(|error| {
                    panic!("gameplay harness {context} hand-work projection failed: {error}")
                });
        assert_eq!(
            work.duration(),
            resolution.duration(),
            "gameplay harness {context} hand-work projection diverged from canonical craft resolution"
        );
        let total_input_mg = definition
            .input_mass()
            .milligrams()
            .checked_mul(batches)
            .unwrap_or_else(|| panic!("gameplay harness {context} input cost overflowed"));
        let policy_key = (
            resolution.duration().value(),
            total_input_mg,
            work.resource_budget().metabolic_energy().nanojoules(),
            work.resource_budget().hydration().microliters(),
        );
        selected_processes.insert(definition.process());
        candidates.push((definition, batches, source, policy_key));
    }
    let best_key = candidates
        .iter()
        .map(|(_, _, _, policy_key)| *policy_key)
        .min()
        .unwrap_or_else(|| {
            panic!(
                "gameplay harness {context} has no canonically resolvable manual route to commodity {}",
                commodity.value()
            )
        });
    let mut best = candidates
        .into_iter()
        .filter(|(_, _, _, policy_key)| *policy_key == best_key);
    let (definition, batches, source, _) = best
        .next()
        .unwrap_or_else(|| unreachable!("best live manual-production key came from a candidate"));
    assert!(
        best.next().is_none(),
        "gameplay harness {context} has equally efficient canonically resolved manual routes to commodity {}; add an explicit actor preference instead of using process identity",
        commodity.value()
    );
    (definition, batches, source)
}
