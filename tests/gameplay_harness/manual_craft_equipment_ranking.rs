//! Shared ranking primitives for equipment-assisted manual-craft planning.

use deep_hearth::core::quantity::Mass;
use deep_hearth::crafting::ManualCraftDefinition;
use deep_hearth::material::CommodityKey;

pub(super) fn output_batches(
    definition: &ManualCraftDefinition,
    commodity: CommodityKey,
    required: Mass,
    context: &'static str,
) -> u64 {
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
    assert!(!per_batch.is_zero());
    required.milligrams().div_ceil(per_batch.milligrams())
}

pub(super) fn select_unique_best<'a, T: Copy>(
    candidates: Vec<(&'a ManualCraftDefinition, u64, T, (u64, u64))>,
    commodity: CommodityKey,
    context: &'static str,
) -> (&'a ManualCraftDefinition, u64, T) {
    let best_key = candidates
        .iter()
        .map(|(_, _, _, key)| *key)
        .min()
        .unwrap_or_else(|| {
            panic!(
                "gameplay harness {context} has no equipment-assisted route to commodity {}",
                commodity.value()
            )
        });
    let mut best = candidates
        .into_iter()
        .filter(|(_, _, _, key)| *key == best_key);
    let (definition, batches, source, _) = best
        .next()
        .unwrap_or_else(|| unreachable!("best equipment-assisted key came from a candidate"));
    assert!(
        best.next().is_none(),
        "gameplay harness {context} has equally efficient equipment-assisted routes to commodity {}; add an explicit actor preference instead of using process identity",
        commodity.value()
    );
    (definition, batches, source)
}
