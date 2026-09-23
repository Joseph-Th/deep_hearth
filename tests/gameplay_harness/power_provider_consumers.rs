//! Real productive consumers used to exercise stored work in power-provider comparisons.

use deep_hearth::content::{
    EQUIPMENT_STONE_CRUSHER, EQUIPMENT_TIMBER_SASH_SAWMILL, FORM_LOG, MATERIAL_WOOD,
    PROCESS_CRUSH_ORE, PROCESS_POWER_SAW_WOOD_BOARDS,
};
use deep_hearth::core::quantity::Energy;
use deep_hearth::core::state::AppState;
use deep_hearth::crafting::{PoweredCraftRequest, validate_start_powered_craft};
use deep_hearth::energy::EnergyStoreId;
use deep_hearth::equipment::EquipmentId;
use deep_hearth::inventory::{MaterialLotSelection, StockpileId};
use deep_hearth::material::CommodityKey;
use deep_hearth::ore_processing::{ComminutionRequest, resolve_comminution_process};
use deep_hearth::production::validate_start_process;
use deep_hearth::registry::Registries;

use super::super::material_selection::select_stockpile_mass;
use super::super::production_timing::finish_uninterrupted_production_job;
use super::build::build_provider;

fn select_stockpile_commodity_mass(
    state: &AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass: deep_hearth::core::quantity::Mass,
    context: &'static str,
) -> Vec<MaterialLotSelection> {
    assert!(
        !mass.is_zero(),
        "power provider {context} requires a positive material selection"
    );
    let mut remaining = mass;
    let mut selections = Vec::new();
    for lot in state.inventory().lot_ids(stockpile) {
        if remaining.is_zero() {
            break;
        }
        let record = state
            .inventory()
            .get_lot(lot)
            .unwrap_or_else(|| panic!("power provider {context} material lot disappeared"));
        if record.commodity() != commodity {
            continue;
        }
        let selected = deep_hearth::core::quantity::Mass::from_milligrams(
            record.mass().milligrams().min(remaining.milligrams()),
        );
        if selected.is_zero() {
            continue;
        }
        selections.push(MaterialLotSelection::new(lot, selected));
        remaining = remaining.checked_sub(selected).unwrap_or_else(|| {
            unreachable!("selected commodity mass is bounded by remaining demand")
        });
    }
    assert!(
        remaining.is_zero(),
        "power provider {context} is missing {}mg of commodity {}",
        remaining.milligrams(),
        commodity.value()
    );
    selections
}

#[derive(Clone, Copy)]
pub(super) struct PrimitivePowerConsumer {
    source: StockpileId,
    destination: StockpileId,
    crusher: EquipmentId,
}

pub(super) fn build_primitive_power_consumer(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    shaped: StockpileId,
    source: StockpileId,
    destination: StockpileId,
) -> PrimitivePowerConsumer {
    let (crusher, _) = build_provider(
        registries,
        state,
        raw,
        shaped,
        EQUIPMENT_STONE_CRUSHER,
        "power provider shared stone crusher",
    );
    PrimitivePowerConsumer {
        source,
        destination,
        crusher,
    }
}

pub(super) fn consume_primitive_charge(
    registries: &Registries,
    state: &mut AppState,
    consumer: PrimitivePowerConsumer,
    drive: EnergyStoreId,
    capacity_nj: u128,
) -> u64 {
    let definition = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("power-provider primitive consumer process disappeared"));
    let mass = deep_hearth::energy::calculate_mass_specific_energy_capacity(
        Energy::from_nanojoules(capacity_nj),
        definition.specific_energy(),
    );
    assert_eq!(
        deep_hearth::energy::calculate_mass_specific_energy(mass, definition.specific_energy()),
        Energy::from_nanojoules(capacity_nj),
        "primitive power consumer must convert one full flywheel charge into an exact ore batch"
    );
    let selections = select_stockpile_mass(
        state,
        consumer.source,
        mass,
        "power provider primitive consumer feed",
    );
    let resolved = resolve_comminution_process(
        registries,
        state,
        ComminutionRequest::new(
            PROCESS_CRUSH_ORE,
            consumer.source,
            selections.as_slice(),
            consumer.crusher,
            drive,
        ),
    )
    .unwrap_or_else(|error| panic!("power-provider primitive consumer resolution failed: {error}"));
    assert_eq!(
        resolved.required_energy(),
        Energy::from_nanojoules(capacity_nj)
    );
    let ticks = resolved.process_resolution().duration().value();
    let job = validate_start_process(
        registries,
        state,
        resolved.process_resolution(),
        consumer.source,
        consumer.destination,
    )
    .unwrap_or_else(|error| panic!("power-provider primitive consumer start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("power-provider primitive consumer commit failed: {error}"));
    finish_uninterrupted_production_job(
        registries,
        state,
        job,
        "power provider primitive consumer",
    );
    assert_eq!(
        state.energy().get_store(drive).map(|store| store.stored()),
        Some(Energy::ZERO),
        "primitive consumer must use the entire matched flywheel charge"
    );
    ticks
}

#[derive(Clone, Copy)]
pub(super) struct SettlementPowerConsumer {
    source: StockpileId,
    destination: StockpileId,
    sawmill: EquipmentId,
}

pub(super) fn build_settlement_power_consumer(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    shaped: StockpileId,
    source: StockpileId,
    destination: StockpileId,
) -> SettlementPowerConsumer {
    let (sawmill, _) = build_provider(
        registries,
        state,
        raw,
        shaped,
        EQUIPMENT_TIMBER_SASH_SAWMILL,
        "power provider shared sash sawmill",
    );
    SettlementPowerConsumer {
        source,
        destination,
        sawmill,
    }
}

pub(super) fn consume_settlement_charge(
    registries: &Registries,
    state: &mut AppState,
    consumer: SettlementPowerConsumer,
    drive: EnergyStoreId,
    capacity_nj: u128,
) -> u64 {
    let definition = registries
        .crafting()
        .get_powered(PROCESS_POWER_SAW_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("power-provider settlement saw process disappeared"));
    let input_mass = deep_hearth::energy::calculate_mass_specific_energy_capacity(
        Energy::from_nanojoules(capacity_nj),
        definition.specific_energy(),
    );
    assert_eq!(
        deep_hearth::energy::calculate_mass_specific_energy(
            input_mass,
            definition.specific_energy(),
        ),
        Energy::from_nanojoules(capacity_nj),
        "settlement power consumer must convert one full bank charge into an exact lumber batch"
    );
    let commodity = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let selection = select_stockpile_commodity_mass(
        state,
        consumer.source,
        commodity,
        input_mass,
        "power provider settlement consumer feed",
    );
    assert_eq!(
        selection.len(),
        1,
        "settlement consumer setup should retain one contiguous lumber work lot"
    );
    let request = PoweredCraftRequest::single(
        PROCESS_POWER_SAW_WOOD_BOARDS,
        consumer.source,
        MaterialLotSelection::new(selection[0].lot(), input_mass),
        consumer.sawmill,
        drive,
    );
    let start = validate_start_powered_craft(registries, state, request, consumer.destination)
        .unwrap_or_else(|error| panic!("power-provider settlement consumer start failed: {error}"));
    let job = start.commit(state).unwrap_or_else(|error| {
        panic!("power-provider settlement consumer commit failed: {error}")
    });
    let record = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("power-provider settlement consumer job disappeared"));
    assert_eq!(
        record
            .consumed_energy()
            .map(|trace| trace.energy().nanojoules()),
        Some(capacity_nj),
        "settlement consumer must use the entire matched flywheel-bank charge"
    );
    let ticks = record.active_duration().value();
    finish_uninterrupted_production_job(
        registries,
        state,
        job,
        "power provider settlement consumer",
    );
    assert_eq!(
        state.energy().get_store(drive).map(|store| store.stored()),
        Some(Energy::ZERO),
        "settlement consumer must leave the matched flywheel bank empty"
    );
    ticks
}
