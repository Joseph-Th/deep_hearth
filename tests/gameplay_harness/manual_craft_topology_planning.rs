//! Topology-only planning for manual production before live inventory exists.

use std::num::NonZeroU64;

use deep_hearth::core::quantity::Mass;
use deep_hearth::crafting::{ManualCraftDefinition, project_manual_craft_hand_work};
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::{ProcessEquipmentRole, Registries};

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
            let batches_nonzero = NonZeroU64::new(batches)
                .unwrap_or_else(|| unreachable!("nonzero output demand yields nonzero batches"));
            let work =
                project_manual_craft_hand_work(registries, definition.process(), batches_nonzero)
                    .unwrap_or_else(|error| {
                        panic!("gameplay harness {context} hand-work projection failed: {error}")
                    });
            let total_input_mg = definition
                .input_mass()
                .milligrams()
                .checked_mul(batches)
                .unwrap_or_else(|| panic!("gameplay harness {context} input cost overflowed"));
            let policy_key = (
                work.duration().value(),
                total_input_mg,
                work.resource_budget().metabolic_energy().nanojoules(),
                work.resource_budget().hydration().microliters(),
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

/// Selects the most attention-efficient equipment-free authored route whose input belongs to the
/// scenario's explicitly disclosed source forms.
pub(super) fn manual_craft_topology_plan_for_output_from_inputs<'a>(
    registries: &'a Registries,
    commodity: CommodityKey,
    required: Mass,
    allowed_inputs: &[CommodityKey],
    context: &'static str,
) -> (&'a ManualCraftDefinition, u64) {
    assert!(
        !allowed_inputs.is_empty(),
        "gameplay harness {context} requires at least one disclosed source form"
    );
    manual_craft_plan_for_output_matching(
        registries,
        commodity,
        required,
        context,
        |definition, _batches| allowed_inputs.contains(&definition.input()),
    )
}
