//! Equipment-aware manual-craft planning for harness actors.

use std::num::NonZeroU64;

use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::crafting::{
    ManualCraftDefinition, project_manual_craft_equipment, resolve_manual_craft,
};
use deep_hearth::equipment::{EquipmentDefinitionId, EquipmentId};
use deep_hearth::inventory::StockpileId;
use deep_hearth::maintenance::Condition;
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::Registries;

use super::manual_craft_selection::{
    first_sufficient_pure_temperature, select_manual_craft_request,
};

fn output_batches(
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

fn select_unique_best<'a, T: Copy>(
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

/// Plans from disclosed source forms before live inventory exists, using the actual authored tool
/// physics for route ranking.
pub(super) fn manual_craft_topology_plan_with_equipment<'a>(
    registries: &'a Registries,
    commodity: CommodityKey,
    required: Mass,
    allowed_inputs: &[CommodityKey],
    equipment: EquipmentDefinitionId,
    condition: Condition,
    context: &'static str,
) -> (&'a ManualCraftDefinition, u64) {
    assert!(!allowed_inputs.is_empty());
    assert!(!required.is_zero());
    let candidates = registries
        .crafting()
        .manual_producers(commodity)
        .filter_map(|definition| {
            if !allowed_inputs.contains(&definition.input()) {
                return None;
            }
            let batches = output_batches(definition, commodity, required, context);
            let work = project_manual_craft_equipment(
                registries,
                definition.process(),
                NonZeroU64::new(batches)?,
                equipment,
                condition,
            )
            .ok()?;
            let input_mg = definition.input_mass().milligrams().checked_mul(batches)?;
            Some((definition, batches, (), (work.duration().value(), input_mg)))
        })
        .collect::<Vec<_>>();
    let (definition, batches, ()) = select_unique_best(candidates, commodity, context);
    (definition, batches)
}

/// Plans from actor-visible live inventory through the canonical resolver using the actual equipment
/// instance and current condition.
pub(super) fn manual_craft_plan_with_equipment<'a>(
    registries: &'a Registries,
    state: &AppState,
    sources: &[StockpileId],
    commodity: CommodityKey,
    required: Mass,
    equipment: EquipmentId,
    context: &'static str,
) -> (&'a ManualCraftDefinition, u64, StockpileId) {
    assert!(!sources.is_empty());
    assert!(!required.is_zero());
    let equipment_record = state
        .equipment()
        .get_equipment(equipment)
        .unwrap_or_else(|| panic!("gameplay harness {context} equipment disappeared"));
    let candidates = registries
        .crafting()
        .manual_producers(commodity)
        .filter_map(|definition| {
            let batches = output_batches(definition, commodity, required, context);
            let required_input =
                Mass::from_milligrams(definition.input_mass().milligrams().checked_mul(batches)?);
            let source = sources.iter().copied().find(|source| {
                first_sufficient_pure_temperature(
                    state,
                    *source,
                    definition.input(),
                    required_input,
                    context,
                )
                .is_some()
            })?;
            let request = select_manual_craft_request(
                registries,
                state,
                definition.process(),
                source,
                batches,
                context,
            )
            .with_equipment(equipment);
            let resolution = resolve_manual_craft(registries, state, &request).ok()?;
            let work = project_manual_craft_equipment(
                registries,
                definition.process(),
                NonZeroU64::new(batches)?,
                equipment_record.definition(),
                equipment_record.condition(),
            )
            .ok()?;
            assert_eq!(
                resolution.duration(),
                work.duration(),
                "gameplay harness {context} equipment projection diverged from canonical resolution"
            );
            Some((
                definition,
                batches,
                source,
                (resolution.duration().value(), required_input.milligrams()),
            ))
        })
        .collect::<Vec<_>>();
    select_unique_best(candidates, commodity, context)
}
