//! Shared canonical construction helpers for power-provider gameplay comparisons.

use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::energy::{EnergyStoreDefinitionId, EnergyStoreId, validate_assemble_energy_store};
use deep_hearth::equipment::{EquipmentDefinitionId, EquipmentId, validate_assemble_equipment};
use deep_hearth::inventory::StockpileId;
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::Registries;
use deep_hearth::survival::assess_survival;

use super::super::manual_craft_execution::execute_manual_craft_batches;
use super::super::manual_craft_planning::manual_craft_plan_for_available_output;
use super::planning::ShapedBuild;

pub(super) fn stockpile_mass(state: &AppState, stockpile: StockpileId) -> Mass {
    state
        .inventory()
        .get_stockpile(stockpile)
        .unwrap_or_else(|| panic!("power build stockpile disappeared"))
        .stored_mass()
}

fn shape_assembly_inputs(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    shaped: StockpileId,
    inputs: Vec<(CommodityKey, Mass)>,
    context: &'static str,
) -> u64 {
    let mut attention_ticks = 0_u64;
    for (commodity, required) in inputs {
        let available = state
            .inventory()
            .get_stockpile(shaped)
            .map(|stockpile| stockpile.get_mass(commodity))
            .unwrap_or_else(|| panic!("power provider {context} shaped stockpile disappeared"));
        if available >= required {
            continue;
        }
        let missing = required
            .checked_sub(available)
            .unwrap_or_else(|| unreachable!("power provider component availability was checked"));
        let (craft, batches, source) = manual_craft_plan_for_available_output(
            registries,
            state,
            &[raw],
            commodity,
            missing,
            context,
        );
        attention_ticks = attention_ticks
            .checked_add(
                execute_manual_craft_batches(
                    registries,
                    state,
                    craft.process(),
                    source,
                    shaped,
                    batches,
                    context,
                )
                .value(),
            )
            .unwrap_or_else(|| panic!("power provider {context} attention overflowed"));
    }
    attention_ticks
}

pub(super) fn build_provider(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    shaped: StockpileId,
    definition: EquipmentDefinitionId,
    context: &'static str,
) -> (EquipmentId, ShapedBuild) {
    let inputs = registries
        .equipment()
        .get_equipment(definition)
        .and_then(|equipment| equipment.assembly_profile())
        .map(|profile| {
            (
                profile.input_mass(),
                profile
                    .inputs()
                    .iter()
                    .map(|input| (input.commodity(), input.mass()))
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_else(|| {
            panic!(
                "power provider equipment {} lost authored assembly",
                definition.value()
            )
        });
    let raw_before = stockpile_mass(state, raw);
    let survival_before = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("power provider {context} lost survival state before build"));
    let attention_ticks = shape_assembly_inputs(registries, state, raw, shaped, inputs.1, context);
    let input_mass_mg = raw_before
        .checked_sub(stockpile_mass(state, raw))
        .unwrap_or_else(|| panic!("power build must withdraw raw matter"))
        .milligrams();
    assert!(
        input_mass_mg >= inputs.0.milligrams(),
        "raw bill must cover embodied matter"
    );
    let equipment = validate_assemble_equipment(registries, state, definition, shaped)
        .unwrap_or_else(|error| panic!("power provider equipment assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("power provider equipment commit failed: {error}"));
    let survival_after = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("power provider {context} lost survival state after build"));
    (
        equipment,
        ShapedBuild {
            attention_ticks,
            input_mass_mg,
            embodied_mass_mg: inputs.0.milligrams(),
            metabolic_nj: survival_before
                .metabolic_energy()
                .nanojoules()
                .checked_sub(survival_after.metabolic_energy().nanojoules())
                .unwrap_or_else(|| {
                    panic!("power provider {context} build metabolic audit underflowed")
                }),
            hydration_ul: survival_before
                .hydration()
                .microliters()
                .checked_sub(survival_after.hydration().microliters())
                .unwrap_or_else(|| {
                    panic!("power provider {context} build hydration audit underflowed")
                }),
        },
    )
}

pub(super) fn build_flywheel(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    shaped: StockpileId,
    definition: EnergyStoreDefinitionId,
    context: &'static str,
) -> (EnergyStoreId, ShapedBuild) {
    let inputs = registries
        .energy()
        .get_store(definition)
        .and_then(|store| store.assembly_profile())
        .map(|profile| {
            (
                profile.input_mass(),
                profile
                    .inputs()
                    .iter()
                    .map(|input| (input.commodity(), input.mass()))
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_else(|| panic!("power provider flywheel lost authored assembly"));
    let raw_before = stockpile_mass(state, raw);
    let survival_before = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("power provider {context} lost survival state before build"));
    let attention_ticks = shape_assembly_inputs(registries, state, raw, shaped, inputs.1, context);
    let input_mass_mg = raw_before
        .checked_sub(stockpile_mass(state, raw))
        .unwrap_or_else(|| panic!("power build must withdraw raw matter"))
        .milligrams();
    assert!(
        input_mass_mg >= inputs.0.milligrams(),
        "raw bill must cover embodied matter"
    );
    let store = validate_assemble_energy_store(registries, state, definition, shaped)
        .unwrap_or_else(|error| panic!("power provider flywheel assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("power provider flywheel commit failed: {error}"));
    let survival_after = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("power provider {context} lost survival state after build"));
    (
        store,
        ShapedBuild {
            attention_ticks,
            input_mass_mg,
            embodied_mass_mg: inputs.0.milligrams(),
            metabolic_nj: survival_before
                .metabolic_energy()
                .nanojoules()
                .checked_sub(survival_after.metabolic_energy().nanojoules())
                .unwrap_or_else(|| {
                    panic!("power provider {context} build metabolic audit underflowed")
                }),
            hydration_ul: survival_before
                .hydration()
                .microliters()
                .checked_sub(survival_after.hydration().microliters())
                .unwrap_or_else(|| {
                    panic!("power provider {context} build hydration audit underflowed")
                }),
        },
    )
}
