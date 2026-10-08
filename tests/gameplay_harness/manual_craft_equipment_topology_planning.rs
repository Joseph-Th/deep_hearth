//! Equipment-aware manual-craft planning from disclosed source forms before live inventory exists.

use std::num::NonZeroU64;

use deep_hearth::core::quantity::Mass;
use deep_hearth::crafting::{ManualCraftDefinition, project_manual_craft_equipment};
use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::maintenance::Condition;
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::{CommoditySource, Registries};

use super::manual_craft_equipment_ranking::{output_batches, select_unique_best};

/// Plans from disclosed source forms using the actual authored tool physics for route ranking.
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
    let handbook = registries
        .commodity_handbook_entry(commodity)
        .unwrap_or_else(|| panic!("gameplay harness {context} requested an unknown commodity"));
    let candidates = handbook
        .sources()
        .iter()
        .filter_map(|source| match *source {
            CommoditySource::ManualCraft { process, .. } => Some(
                registries
                    .crafting()
                    .get_manual(process)
                    .unwrap_or_else(|| {
                        panic!("commodity handbook exposed an unknown manual process")
                    }),
            ),
            CommoditySource::SurfaceGathering { .. }
            | CommoditySource::EquipmentDisassembly { .. }
            | CommoditySource::EnergyStoreDisassembly { .. }
            | CommoditySource::StorageDismantling { .. }
            | CommoditySource::EquipmentMaintenanceSpent { .. }
            | CommoditySource::OreProcessing { .. }
            | CommoditySource::ThermalPhaseChange { .. } => None,
        })
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
