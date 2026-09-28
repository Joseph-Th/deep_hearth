//! Live-inventory actor planning for manual production routes.

use std::num::NonZeroU64;

use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::crafting::{
    ManualCraftDefinition, plan_manual_craft_from_stockpile, project_manual_craft_hand_work,
    resolve_manual_craft,
};
use deep_hearth::inventory::StockpileId;
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::{ProcessEquipmentRole, Registries};

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
            let batches_nonzero = NonZeroU64::new(batches)
                .unwrap_or_else(|| unreachable!("nonzero output demand yields nonzero batches"));
            let (source, request) = sources.iter().copied().find_map(|source| {
                plan_manual_craft_from_stockpile(
                    registries,
                    state,
                    definition.process(),
                    source,
                    batches_nonzero,
                )
                .ok()
                .map(|request| (source, request))
            })?;
            let resolution = resolve_manual_craft(registries, state, &request).ok()?;
            let work = project_manual_craft_hand_work(registries, definition.process(), batches_nonzero)
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
            Some((definition, batches, source, policy_key))
        })
        .collect::<Vec<_>>();
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
