//! Ordinary personal crafting over the player's carried inventory projection.

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU64;

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::equipment::EquipmentId;
use crate::inventory::{MaterialStackProjection, MaterialStackProjectionError};
use crate::material::CommodityKey;
use crate::production::ProcessId;
use crate::registry::Registries;

use super::{
    ManualCraftInputPlanError, ManualCraftRequest, ManualCraftStartRequest,
    ManualCraftStockpileOption, StartManualCraftError, ValidatedManualCraftStart,
    manual_craft_options_from_stockpile, plan_manual_craft_from_stockpile,
    validate_start_manual_craft,
};

/// Failure while converting one familiar personal-crafting request into canonical manual work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerManualCraftError {
    PlayerNotAdmitted,
    InputPlan(ManualCraftInputPlanError),
    Stack(MaterialStackProjectionError),
    StackCommodityMismatch {
        expected: CommodityKey,
        actual: CommodityKey,
    },
    Start(StartManualCraftError),
}

impl Display for PlayerManualCraftError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => formatter.write_str("player is not admitted"),
            Self::InputPlan(error) => {
                write!(formatter, "personal craft input planning failed: {error}")
            }
            Self::Stack(error) => {
                write!(formatter, "personal craft stack selection failed: {error}")
            }
            Self::StackCommodityMismatch { expected, actual } => write!(
                formatter,
                "personal craft expects material {} form {} but selected stack contains material {} form {}",
                expected.material().value(),
                expected.form().value(),
                actual.material().value(),
                actual.form().value()
            ),
            Self::Start(error) => write!(formatter, "personal craft could not start: {error}"),
        }
    }
}

impl Error for PlayerManualCraftError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PlayerNotAdmitted | Self::StackCommodityMismatch { .. } => None,
            Self::InputPlan(error) => Some(error),
            Self::Stack(error) => Some(error),
            Self::Start(error) => Some(error),
        }
    }
}

fn carried_stockpile(
    state: &AppState,
) -> Result<crate::inventory::StockpileId, PlayerManualCraftError> {
    state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .ok_or(PlayerManualCraftError::PlayerNotAdmitted)
}

fn with_optional_equipment(
    craft: ManualCraftRequest,
    equipment: Option<EquipmentId>,
) -> ManualCraftRequest {
    match equipment {
        Some(equipment) => craft.with_equipment(equipment),
        None => craft,
    }
}

/// Projects the stable manual-recipe catalog against the player's carried inventory only.
///
/// Material availability is current read-side evidence. Starting work still revalidates exact
/// inventory, tool, survival, attention, and output-capacity state through the canonical owners.
pub fn player_manual_craft_options(
    registries: &Registries,
    state: &AppState,
) -> Result<Vec<ManualCraftStockpileOption>, PlayerManualCraftError> {
    let carried = carried_stockpile(state)?;
    manual_craft_options_from_stockpile(registries, state, carried)
        .map_err(PlayerManualCraftError::InputPlan)
}

/// Starts an ordinary recipe/batch choice from carried custody and returns output there in place.
///
/// Durable homogeneous inputs are selected deterministically behind this boundary. Recipes whose
/// inputs have gameplay-relevant stack-local state, currently age-sensitive food, reject here and
/// must use [`validate_start_player_manual_craft_from_stack`] with an explicit projected stack.
pub fn validate_start_player_manual_craft(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    batches: NonZeroU64,
    equipment: Option<EquipmentId>,
) -> Result<ValidatedManualCraftStart, PlayerManualCraftError> {
    let carried = carried_stockpile(state)?;
    let craft = plan_manual_craft_from_stockpile(registries, state, process, carried, batches)
        .map_err(PlayerManualCraftError::InputPlan)?;
    let craft = with_optional_equipment(craft, equipment);
    validate_start_manual_craft(registries, state, ManualCraftStartRequest::in_place(craft))
        .map_err(PlayerManualCraftError::Start)
}

/// Starts personal crafting while routing physical offcuts, chips and other residual forms into
/// an accessible waste stockpile instead of mixing them with carried products.
/// Both destinations must satisfy canonical capacity, locality and revision checks.
pub fn validate_start_player_manual_craft_with_residue_destination(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    batches: NonZeroU64,
    equipment: Option<EquipmentId>,
    residue_destination: crate::inventory::StockpileId,
) -> Result<ValidatedManualCraftStart, PlayerManualCraftError> {
    let carried = carried_stockpile(state)?;
    let craft = plan_manual_craft_from_stockpile(registries, state, process, carried, batches)
        .map_err(PlayerManualCraftError::InputPlan)?;
    let craft = with_optional_equipment(craft, equipment);
    validate_start_manual_craft(
        registries,
        state,
        ManualCraftStartRequest::in_place(craft).with_residue_destination(residue_destination),
    )
    .map_err(PlayerManualCraftError::Start)
}

/// Starts one actor-visible personal-crafting catalog row without exposing recipe identity at the
/// action boundary.
///
/// The option is read-side evidence rather than an authorization token. Current carried matter,
/// equipment, survival, attention, capacity, and owner revisions are re-read by the canonical
/// personal-craft path when work is admitted.
pub fn validate_start_player_manual_craft_option(
    registries: &Registries,
    state: &AppState,
    option: ManualCraftStockpileOption,
    batches: NonZeroU64,
    equipment: Option<EquipmentId>,
) -> Result<ValidatedManualCraftStart, PlayerManualCraftError> {
    validate_start_player_manual_craft(registries, state, option.process(), batches, equipment)
}

/// Starts personal crafting from one explicitly chosen familiar stack.
///
/// This is the player-facing path for freshness-sensitive or otherwise deliberately chosen
/// physical cohorts. The projection remains revision-bound and resolves back to exact lot slices
/// internally; callers never receive or persist lot identity.
pub fn validate_start_player_manual_craft_from_stack(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    batches: NonZeroU64,
    stack: &MaterialStackProjection,
    equipment: Option<EquipmentId>,
) -> Result<ValidatedManualCraftStart, PlayerManualCraftError> {
    let carried = carried_stockpile(state)?;
    let definition = registries.crafting().get_manual(process).ok_or({
        PlayerManualCraftError::InputPlan(ManualCraftInputPlanError::UnknownManualProcess {
            process,
        })
    })?;
    if stack.commodity() != definition.input() {
        return Err(PlayerManualCraftError::StackCommodityMismatch {
            expected: definition.input(),
            actual: stack.commodity(),
        });
    }
    let required = definition
        .input_mass()
        .milligrams()
        .checked_mul(batches.get())
        .map(Mass::from_milligrams)
        .ok_or({
            PlayerManualCraftError::InputPlan(ManualCraftInputPlanError::InputMassOverflow {
                process,
                batches,
            })
        })?;
    let selections = stack
        .selections_for_mass(state, carried, required)
        .map_err(PlayerManualCraftError::Stack)?;
    let craft = with_optional_equipment(
        ManualCraftRequest::new(process, carried, selections),
        equipment,
    );
    validate_start_manual_craft(registries, state, ManualCraftStartRequest::in_place(craft))
        .map_err(PlayerManualCraftError::Start)
}

#[cfg(test)]
#[path = "personal_tests.rs"]
mod tests;
