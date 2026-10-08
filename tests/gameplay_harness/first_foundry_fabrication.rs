//! Live manual component fabrication for the first-foundry episode.

use deep_hearth::content::{
    EQUIPMENT_TIMBER_FRAME_SAW_BENCH, EQUIPMENT_TIMBER_TREADLE_HAMMER, MATERIAL_COPPER,
    MATERIAL_STONE, MATERIAL_WOOD,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::equipment::EquipmentId;
use deep_hearth::inventory::StockpileId;
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::Registries;

use super::first_foundry_planning::foundry_component_requirements;
use super::manual_craft_execution::execute_manual_craft;
use super::material_selection::select_stockpile_commodity_mass;
use super::workshop_craft_planning::manual_craft_plan_with_available_equipment;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct FoundryFabrication {
    pub(super) total_ticks: u64,
    pub(super) stone_ticks: u64,
    pub(super) wood_ticks: u64,
    pub(super) copper_ticks: u64,
    pub(super) hand_ticks: u64,
    pub(super) frame_saw_ticks: u64,
    pub(super) treadle_hammer_ticks: u64,
}

pub(super) fn select_commodity_mass(
    state: &AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
    context: &'static str,
) -> Vec<deep_hearth::inventory::MaterialLotSelection> {
    select_stockpile_commodity_mass(state, stockpile, commodity, mass, context)
}

pub(super) fn craft_foundry_components(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    parts: StockpileId,
    workshop_tools: &[EquipmentId],
) -> FoundryFabrication {
    let mut fabrication = FoundryFabrication::default();
    for (commodity, required) in foundry_component_requirements(registries) {
        let available = state
            .inventory()
            .get_stockpile(parts)
            .map(|stockpile| stockpile.get_mass(commodity))
            .unwrap_or_else(|| panic!("first foundry parts stockpile disappeared"));
        if available >= required {
            continue;
        }
        let missing = required
            .checked_sub(available)
            .unwrap_or_else(|| unreachable!("component shortfall was established"));
        let (request, _batches) = manual_craft_plan_with_available_equipment(
            registries,
            state,
            &[raw],
            workshop_tools,
            commodity,
            missing,
            "first foundry component shaping",
        );
        let equipment = request.equipment();
        let ticks = execute_manual_craft(
            registries,
            state,
            request,
            parts,
            "first foundry component shaping",
        )
        .value();
        fabrication.total_ticks = fabrication
            .total_ticks
            .checked_add(ticks)
            .unwrap_or_else(|| panic!("first foundry fabrication attention overflowed"));
        let material_ticks = match commodity.material() {
            MATERIAL_STONE => &mut fabrication.stone_ticks,
            MATERIAL_WOOD => &mut fabrication.wood_ticks,
            MATERIAL_COPPER => &mut fabrication.copper_ticks,
            other => panic!(
                "first foundry component fabrication unexpectedly targets material {}",
                other.value()
            ),
        };
        *material_ticks = material_ticks
            .checked_add(ticks)
            .unwrap_or_else(|| panic!("first foundry material fabrication attention overflowed"));
        match equipment.and_then(|equipment| {
            state
                .equipment()
                .get_equipment(equipment)
                .map(|record| record.definition())
        }) {
            Some(EQUIPMENT_TIMBER_FRAME_SAW_BENCH) => {
                fabrication.frame_saw_ticks = fabrication
                    .frame_saw_ticks
                    .checked_add(ticks)
                    .unwrap_or_else(|| panic!("first foundry frame-saw attention overflowed"));
            }
            Some(EQUIPMENT_TIMBER_TREADLE_HAMMER) => {
                fabrication.treadle_hammer_ticks = fabrication
                    .treadle_hammer_ticks
                    .checked_add(ticks)
                    .unwrap_or_else(|| panic!("first foundry treadle-hammer attention overflowed"));
            }
            Some(other) => panic!(
                "first foundry component planner selected undisclosed workshop equipment {}",
                other.value()
            ),
            None => {
                fabrication.hand_ticks =
                    fabrication
                        .hand_ticks
                        .checked_add(ticks)
                        .unwrap_or_else(|| {
                            panic!("first foundry hand fabrication attention overflowed")
                        });
            }
        }
    }
    assert_eq!(
        fabrication.stone_ticks + fabrication.wood_ticks + fabrication.copper_ticks,
        fabrication.total_ticks,
        "first foundry material-family fabrication accounting must cover every component job"
    );
    assert_eq!(
        fabrication.hand_ticks + fabrication.frame_saw_ticks + fabrication.treadle_hammer_ticks,
        fabrication.total_ticks,
        "first foundry tool-route fabrication accounting must cover every component job"
    );
    fabrication
}
