//! Familiar recipe-input planning over exact inventory lots.

use std::collections::BTreeMap;
use std::num::NonZeroU64;

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{MaterialLotSelection, StockpileId};
use crate::material::{CommodityKey, MaterialComposition};
use crate::production::ProcessId;
use crate::registry::Registries;

use super::ManualCraftRequest;

mod availability;
mod errors;

use availability::{CompatibleInputInventory, scan_compatible_input};
pub use availability::{
    ManualCraftInputAvailability, ManualCraftInputMode, ManualCraftStockpileOption,
};
pub use errors::ManualCraftInputPlanError;

/// Builds a deterministic manual-recipe catalog for one inventory custody location.
///
/// Entries are ordered by stable process ID through the crafting registry. The catalog reports
/// material availability and the static tool relationship separately: a recipe with enough matter
/// and `Required` equipment is not claimed to be executable until a runtime equipment instance is
/// chosen and validated by the normal crafting path.
pub fn manual_craft_options_from_stockpile(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
) -> Result<Vec<ManualCraftStockpileOption>, ManualCraftInputPlanError> {
    if state.inventory().get_stockpile(source).is_none() {
        return Err(ManualCraftInputPlanError::UnknownStockpile { stockpile: source });
    }
    let mut options = Vec::new();
    let mut compatible_inputs = BTreeMap::<CommodityKey, CompatibleInputInventory>::new();
    for definition in registries.crafting().definitions() {
        let process = definition.process();
        let input_mode = if input_requires_explicit_stack_choice(registries, definition.input()) {
            ManualCraftInputMode::ExplicitStackChoice {
                input: definition.input(),
                batch_mass: definition.input_mass(),
            }
        } else {
            let input = definition.input();
            let inventory = compatible_inputs
                .entry(input)
                .or_insert_with(|| scan_compatible_input(state, source, input));
            let availability = inventory.availability(input, definition.input_mass());
            if availability.craftable_temperature_groups() > 1 {
                ManualCraftInputMode::TemperatureChoice(availability)
            } else {
                ManualCraftInputMode::Automatic(availability)
            }
        };
        let topology = registries.process_topology(process).unwrap_or_else(|| {
            panic!(
                "registered manual craft {} lost its process topology",
                process.value()
            )
        });
        options.push(ManualCraftStockpileOption {
            process,
            input_mode,
            equipment_role: topology.equipment_role(),
        });
    }
    Ok(options)
}

pub(super) fn input_requires_explicit_stack_choice(
    registries: &Registries,
    input: CommodityKey,
) -> bool {
    registries.survival().get_food(input).is_some()
}

fn resolve_recipe_input(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    source: StockpileId,
) -> Result<(ManualCraftInputAvailability, CompatibleInputInventory), ManualCraftInputPlanError> {
    let definition = registries
        .crafting()
        .get_manual(process)
        .ok_or(ManualCraftInputPlanError::UnknownManualProcess { process })?;
    if state.inventory().get_stockpile(source).is_none() {
        return Err(ManualCraftInputPlanError::UnknownStockpile { stockpile: source });
    }
    let input = definition.input();
    if input_requires_explicit_stack_choice(registries, input) {
        return Err(
            ManualCraftInputPlanError::AgeSensitiveInputRequiresExplicitSelection { input },
        );
    }
    let inventory = scan_compatible_input(state, source, input);
    let availability = inventory.availability(input, definition.input_mass());
    Ok((availability, inventory))
}

/// Reports how many complete batches one stockpile can supply without mixing input temperatures.
pub fn assess_manual_craft_inputs(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    source: StockpileId,
) -> Result<ManualCraftInputAvailability, ManualCraftInputPlanError> {
    resolve_recipe_input(registries, state, process, source)
        .map(|(availability, _inventory)| availability)
}

/// Converts a recipe-and-batch choice into exact deterministic lot slices.
///
/// This is the player/UI convenience boundary for ordinary fixed-input crafting. It deliberately
/// stops before resolution/admission, so all existing crafting physics, equipment checks, labor,
/// reservations, and commit semantics remain authoritative. Age-sensitive food stays explicit
/// because freshness is a meaningful stack-level decision.
pub fn plan_manual_craft_from_stockpile(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    source: StockpileId,
    batches: NonZeroU64,
) -> Result<ManualCraftRequest, ManualCraftInputPlanError> {
    let (availability, inventory) = resolve_recipe_input(registries, state, process, source)?;
    let required = availability
        .batch_mass()
        .milligrams()
        .checked_mul(batches.get())
        .map(Mass::from_milligrams)
        .ok_or(ManualCraftInputPlanError::InputMassOverflow { process, batches })?;
    if availability.total_eligible_mass() < required {
        return Err(ManualCraftInputPlanError::InsufficientInput {
            input: availability.input(),
            available: availability.total_eligible_mass(),
            required,
        });
    }
    let mut candidates = inventory
        .groups()
        .iter()
        .filter(|group| group.mass() >= required);
    let group = candidates
        .next()
        .ok_or(ManualCraftInputPlanError::SplitTemperatureInput {
            input: availability.input(),
            available: availability.total_eligible_mass(),
            largest_compatible: availability.largest_compatible_mass(),
            required,
        })?;
    if candidates.next().is_some() {
        let temperatures = inventory
            .groups()
            .iter()
            .filter(|group| group.mass() >= required)
            .map(|group| group.temperature())
            .collect::<Vec<_>>();
        return Err(
            ManualCraftInputPlanError::MultipleCompatibleInputTemperatures {
                input: availability.input(),
                required,
                temperatures,
            },
        );
    }

    let expected_composition = MaterialComposition::pure(availability.input().material());
    let mut remaining = required;
    let mut selections = Vec::new();
    for lot_id in state
        .inventory()
        .lot_ids_for_commodity(source, availability.input())
    {
        if remaining.is_zero() {
            break;
        }
        let lot = state.inventory().get_lot(lot_id).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: stockpile {} indexes missing lot {}",
                source.value(),
                lot_id.value()
            )
        });
        if lot.composition() != &expected_composition || lot.temperature() != group.temperature() {
            continue;
        }
        let selected = lot.mass().min(remaining);
        selections.push(MaterialLotSelection::new(lot_id, selected));
        remaining = remaining.checked_sub(selected).unwrap_or_else(|| {
            unreachable!("selected manual-craft input cannot exceed remaining mass")
        });
    }
    assert!(
        remaining.is_zero(),
        "selected compatible group must satisfy requested batch mass"
    );
    Ok(ManualCraftRequest::new(process, source, selections))
}
