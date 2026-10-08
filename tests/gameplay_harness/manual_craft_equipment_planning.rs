//! Equipment-aware manual-craft planning from actor-visible live inventory.

use std::collections::BTreeSet;
use std::num::NonZeroU64;

use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::crafting::{
    ManualCraftDefinition, project_manual_craft_equipment, resolve_manual_craft,
};
use deep_hearth::equipment::EquipmentId;
use deep_hearth::inventory::StockpileId;
use deep_hearth::material::CommodityKey;
use deep_hearth::production::ProcessId;
use deep_hearth::registry::Registries;

use super::manual_craft_equipment_ranking::{output_batches, select_unique_best};
use super::manual_craft_planning::live_manual_craft_catalog_for_output;
use super::manual_craft_selection::plan_manual_craft_request;

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
    let mut selected_processes = BTreeSet::<ProcessId>::new();
    let mut candidates = Vec::new();
    for (source, definition, _equipment_role) in
        live_manual_craft_catalog_for_output(registries, state, sources, commodity, context)
    {
        if selected_processes.contains(&definition.process()) {
            continue;
        }
        let batches = output_batches(definition, commodity, required, context);
        let Some(required_input_mg) = definition.input_mass().milligrams().checked_mul(batches)
        else {
            continue;
        };
        let Ok(request) =
            plan_manual_craft_request(registries, state, definition.process(), source, batches)
        else {
            continue;
        };
        let request = request.with_equipment(equipment);
        let Ok(resolution) = resolve_manual_craft(registries, state, &request) else {
            continue;
        };
        let Some(batches_nonzero) = NonZeroU64::new(batches) else {
            continue;
        };
        let Ok(work) = project_manual_craft_equipment(
            registries,
            definition.process(),
            batches_nonzero,
            equipment_record.definition(),
            equipment_record.condition(),
        ) else {
            continue;
        };
        assert_eq!(
            resolution.duration(),
            work.duration(),
            "gameplay harness {context} equipment projection diverged from canonical resolution"
        );
        selected_processes.insert(definition.process());
        candidates.push((
            definition,
            batches,
            source,
            (resolution.duration().value(), required_input_mg),
        ));
    }
    select_unique_best(candidates, commodity, context)
}
