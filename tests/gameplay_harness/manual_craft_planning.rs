//! Registry-derived actor planning for manual production routes.

use deep_hearth::core::quantity::Mass;
use deep_hearth::crafting::ManualCraftDefinition;
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::Registries;

/// Selects the most attention-efficient observable manual production route to one required
/// commodity quantity.
///
/// This is actor policy over registry-derived topology, not simulation authority. Canonical craft
/// resolution still owns the actual operation once the actor has selected a process and current
/// lots. Ties are rejected because process identity is not a player-visible preference.
pub(super) fn manual_craft_plan_for_output<'a>(
    registries: &'a Registries,
    commodity: CommodityKey,
    required: Mass,
    context: &'static str,
) -> (&'a ManualCraftDefinition, u64) {
    assert!(
        !required.is_zero(),
        "gameplay harness {context} requires nonzero produced mass"
    );
    let candidates = registries
        .crafting()
        .manual_producers(commodity)
        .map(|definition| {
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
            (definition, batches, policy_key)
        })
        .collect::<Vec<_>>();
    let best_key = candidates
        .iter()
        .map(|(_, _, policy_key)| *policy_key)
        .min()
        .unwrap_or_else(|| {
            panic!(
                "gameplay harness {context} has no manual route to commodity {}",
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
