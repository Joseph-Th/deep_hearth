//! Material opportunity, capital demand, and manual component preparation for the first foundry.

use std::collections::BTreeMap;

use deep_hearth::content::{
    ENERGY_COPPER_PLATE_ELECTRICAL_BUFFER, EQUIPMENT_FOUR_CAVITY_STONE_INGOT_MOLD,
    EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE, EQUIPMENT_STONE_INGOT_MOLD,
    EQUIPMENT_TIMBER_TREADLE_DYNAMO, FORM_INGOT, FORM_LUMP, MATERIAL_COPPER, MATERIAL_STONE,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::inventory::{MaterialLotSelection, StockpileId};
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::Registries;

use super::super::focused_seeds::{FocusedProbeCase, FocusedProbeRole};
use super::super::manual_craft_batches::execute_manual_craft_batches;
use super::super::manual_craft_planning::manual_craft_plan_for_available_output;
use super::super::seed::mix64;

pub(super) const FOUNDRY_STONE_OPPORTUNITY: Mass = Mass::from_milligrams(12_000_000);
pub(super) const FOUNDRY_WOOD_OPPORTUNITY: Mass = Mass::from_milligrams(12_000_000);

pub(super) fn native_copper_opportunity(case: FocusedProbeCase) -> Mass {
    let milligrams = match case.role() {
        FocusedProbeRole::MaintainedAnchor => 280_000,
        FocusedProbeRole::MaintainedCoverage if case.seed().is_multiple_of(2) => 240_000,
        FocusedProbeRole::MaintainedCoverage => 300_000,
        FocusedProbeRole::OrganicVariation | FocusedProbeRole::ExplicitReplay => {
            220_000 + mix64(case.seed() ^ 0x464F_554E_4452_5943) % 100_001
        }
    };
    Mass::from_milligrams(milligrams)
}

pub(super) fn select_commodity_mass(
    state: &AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
    context: &'static str,
) -> Vec<MaterialLotSelection> {
    let mut remaining = mass;
    let mut selections = Vec::new();
    for lot in state.inventory().lot_ids(stockpile) {
        if remaining.is_zero() {
            break;
        }
        let record = state
            .inventory()
            .get_lot(lot)
            .unwrap_or_else(|| panic!("first foundry {context} lot disappeared"));
        if record.commodity() != commodity {
            continue;
        }
        let selected =
            Mass::from_milligrams(record.mass().milligrams().min(remaining.milligrams()));
        if selected.is_zero() {
            continue;
        }
        selections.push(MaterialLotSelection::new(lot, selected));
        remaining = remaining
            .checked_sub(selected)
            .unwrap_or_else(|| unreachable!("selected commodity mass is bounded by demand"));
    }
    assert!(
        remaining.is_zero(),
        "first foundry {context} is missing {}mg of commodity {}",
        remaining.milligrams(),
        commodity.value(),
    );
    selections
}

fn foundry_component_requirements(registries: &Registries) -> BTreeMap<CommodityKey, Mass> {
    let mut requirements = BTreeMap::<CommodityKey, Mass>::new();
    for equipment in [
        EQUIPMENT_TIMBER_TREADLE_DYNAMO,
        EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE,
        EQUIPMENT_STONE_INGOT_MOLD,
    ] {
        let profile = registries
            .equipment()
            .get_equipment(equipment)
            .and_then(|definition| definition.assembly_profile())
            .unwrap_or_else(|| {
                panic!("first foundry equipment lost its ordinary assembly profile")
            });
        for input in profile.inputs() {
            let entry = requirements.entry(input.commodity()).or_insert(Mass::ZERO);
            *entry = entry
                .checked_add(input.mass())
                .unwrap_or_else(|| panic!("first foundry component demand overflowed"));
        }
    }
    let electrical = registries
        .energy()
        .get_store(ENERGY_COPPER_PLATE_ELECTRICAL_BUFFER)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("first foundry electrical buffer lost assembly"));
    for input in electrical.inputs() {
        let entry = requirements.entry(input.commodity()).or_insert(Mass::ZERO);
        *entry = entry
            .checked_add(input.mass())
            .unwrap_or_else(|| panic!("first foundry electrical component demand overflowed"));
    }
    requirements
}

pub(super) fn craft_foundry_components(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    parts: StockpileId,
) -> u64 {
    let mut ticks = 0_u64;
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
        let (definition, batches, source) = manual_craft_plan_for_available_output(
            registries,
            state,
            &[raw],
            commodity,
            missing,
            "first foundry component shaping",
        );
        ticks = ticks
            .checked_add(
                execute_manual_craft_batches(
                    registries,
                    state,
                    definition.process(),
                    source,
                    parts,
                    batches,
                    "first foundry component shaping",
                )
                .value(),
            )
            .unwrap_or_else(|| panic!("first foundry fabrication attention overflowed"));
    }
    ticks
}

pub(super) fn foundry_capital_copper(registries: &Registries) -> Mass {
    foundry_component_requirements(registries)
        .into_iter()
        .filter(|(commodity, _)| commodity.material() == MATERIAL_COPPER)
        .map(|(_, mass)| mass)
        .try_fold(Mass::ZERO, Mass::checked_add)
        .unwrap_or_else(|| panic!("first foundry copper capital overflowed"))
}

fn settlement_mold_addition_mass(
    registries: &Registries,
    commodity: CommodityKey,
    context: &'static str,
) -> Mass {
    registries
        .equipment()
        .get_equipment(EQUIPMENT_FOUR_CAVITY_STONE_INGOT_MOLD)
        .and_then(|definition| definition.upgrade_profile())
        .and_then(|upgrade| {
            upgrade
                .additions()
                .inputs()
                .iter()
                .find(|input| input.commodity() == commodity)
                .map(|input| input.mass())
        })
        .unwrap_or_else(|| panic!("settlement foundry mold lost its {context} upgrade input"))
}

pub(super) fn settlement_mold_ingot_requirement(registries: &Registries) -> Mass {
    settlement_mold_addition_mass(
        registries,
        CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
        "cast-ingot bootstrap",
    )
}

pub(super) fn settlement_mold_stone_requirement(registries: &Registries) -> Mass {
    settlement_mold_addition_mass(
        registries,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        "stone",
    )
}
