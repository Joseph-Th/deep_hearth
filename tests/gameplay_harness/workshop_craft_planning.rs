//! Live-inventory actor planning for established workshops with already-owned equipment.

use std::num::NonZeroU64;

use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::crafting::{
    ManualCraftRequest, manual_craft_options_from_stockpile, plan_manual_craft_from_stockpile,
    resolve_manual_craft,
};
use deep_hearth::equipment::EquipmentId;
use deep_hearth::inventory::StockpileId;
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::Registries;
use deep_hearth::survival::project_survival_resource_budget;

/// Selects the best live manual-production action from explicit actor-visible sources and tools.
///
/// Candidate generation is harness policy, while legality, schedule, equipment wear, inputs,
/// outputs, and body cost come from production owners. Source/tool order is explicit actor
/// preference only after physical and bodily costs tie; runtime identity and registry order never
/// decide a tie.
pub(super) fn manual_craft_plan_with_available_equipment(
    registries: &Registries,
    state: &AppState,
    sources: &[StockpileId],
    equipment: &[EquipmentId],
    commodity: CommodityKey,
    required: Mass,
    context: &'static str,
) -> (ManualCraftRequest, u64) {
    assert!(
        !sources.is_empty(),
        "gameplay harness {context} requires at least one actor-visible craft source"
    );
    assert!(
        !required.is_zero(),
        "gameplay harness {context} requires nonzero produced mass"
    );

    let mut candidates = Vec::new();
    for (source_rank, &source) in sources.iter().enumerate() {
        let catalog = manual_craft_options_from_stockpile(registries, state, source)
            .unwrap_or_else(|error| {
                panic!(
                    "gameplay harness {context} cannot read the ordinary craft catalog for source {}: {error}",
                    source.value()
                )
            });
        for option in catalog {
            let Some(definition) = registries.crafting().get_manual(option.process()) else {
                panic!("ordinary craft catalog exposed an unknown process");
            };
            let Some(per_batch) = definition
                .outputs()
                .iter()
                .find(|output| output.commodity() == commodity)
                .map(|output| output.mass())
            else {
                continue;
            };
            assert!(
                !per_batch.is_zero(),
                "gameplay harness {context} producer {} has zero requested output",
                definition.process().value()
            );
            let batches = required.milligrams().div_ceil(per_batch.milligrams());
            let batches_nonzero = NonZeroU64::new(batches)
                .unwrap_or_else(|| unreachable!("nonzero output demand yields nonzero batches"));
            let total_input_mg = definition
                .input_mass()
                .milligrams()
                .checked_mul(batches)
                .unwrap_or_else(|| panic!("gameplay harness {context} input cost overflowed"));
            let Ok(base) = plan_manual_craft_from_stockpile(
                registries,
                state,
                definition.process(),
                source,
                batches_nonzero,
            ) else {
                continue;
            };
            for (equipment_rank, request) in std::iter::once(base.clone())
                .chain(
                    equipment
                        .iter()
                        .map(|&tool| base.clone().with_equipment(tool)),
                )
                .enumerate()
            {
                let Ok(resolution) = resolve_manual_craft(registries, state, &request) else {
                    continue;
                };
                let body = project_survival_resource_budget(
                    registries.survival().physiology(),
                    definition.exertion(),
                    resolution.duration(),
                )
                .unwrap_or_else(|error| {
                    panic!("gameplay harness {context} survival projection failed: {error:?}")
                });
                let policy_key = (
                    resolution.duration().value(),
                    total_input_mg,
                    body.metabolic_energy().nanojoules(),
                    body.hydration().microliters(),
                    source_rank,
                    equipment_rank,
                );
                candidates.push((request, batches, policy_key));
            }
        }
    }

    let best_key = candidates
        .iter()
        .map(|(_, _, key)| *key)
        .min()
        .unwrap_or_else(|| {
            panic!(
                "gameplay harness {context} has no canonically resolvable route to commodity {} with the actor's visible workshop",
                commodity.value()
            )
        });
    let mut best = candidates
        .into_iter()
        .filter(|(_, _, key)| *key == best_key);
    let (request, batches, _) = best
        .next()
        .unwrap_or_else(|| unreachable!("best live workshop-production key came from a candidate"));
    assert!(
        best.next().is_none(),
        "gameplay harness {context} has equally efficient canonically resolved workshop routes to commodity {}; add an explicit actor preference instead of using process/catalog order",
        commodity.value()
    );
    (request, batches)
}
