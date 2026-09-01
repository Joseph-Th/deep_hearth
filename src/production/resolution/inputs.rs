//! Deterministic binding of authored process inputs to exact inventory matter.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    ConsumedMaterialTrace, ConsumptionSelection, ConsumptionSelectionError,
    ExplicitConsumptionSelectionError, MaterialLotId, MaterialLotSelection, StockpileId,
    validate_consumption_selection, validate_explicit_consumption_selection,
};
use crate::material::CommodityKey;
use crate::registry::Registries;

use crate::production::definitions::{ProcessId, ProcessInputPolicy};

/// Failure while binding one authored process to the exact source matter a resolver will inspect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessInputError {
    UnknownProcess {
        process: ProcessId,
    },
    UnknownStockpile {
        stockpile: StockpileId,
    },
    InsufficientMass {
        stockpile: StockpileId,
        commodity: CommodityKey,
        available: Mass,
        requested: Mass,
    },
    MassOverflow {
        stockpile: StockpileId,
    },
    ExplicitSelectionRequired {
        process: ProcessId,
    },
    FixedInputsRequired {
        process: ProcessId,
    },
    EmptySelection,
    ZeroSelectedMass {
        lot: MaterialLotId,
    },
    DuplicateSelectedLot {
        lot: MaterialLotId,
    },
    UnknownSelectedLot {
        lot: MaterialLotId,
    },
    SelectedLotOwnedElsewhere {
        lot: MaterialLotId,
        requested_source: StockpileId,
        actual_source: StockpileId,
    },
    InsufficientSelectedLotMass {
        lot: MaterialLotId,
        available: Mass,
        requested: Mass,
    },
}

impl Display for ProcessInputError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownProcess { process } => {
                write!(formatter, "unknown process id {}", process.value())
            }
            Self::UnknownStockpile { stockpile } => {
                write!(formatter, "unknown stockpile id {}", stockpile.value())
            }
            Self::InsufficientMass {
                stockpile,
                commodity: _commodity,
                available,
                requested,
            } => write!(
                formatter,
                "stockpile {} has {} mg eligible matter but process requires {} mg",
                stockpile.value(),
                available.milligrams(),
                requested.milligrams()
            ),
            Self::MassOverflow { stockpile } => write!(
                formatter,
                "selected process input mass overflowed in stockpile {}",
                stockpile.value()
            ),
            Self::ExplicitSelectionRequired { process } => write!(
                formatter,
                "process {} requires an explicit runtime material-lot selection",
                process.value()
            ),
            Self::FixedInputsRequired { process } => write!(
                formatter,
                "process {} owns fixed authored inputs and cannot accept an arbitrary selected batch",
                process.value()
            ),
            Self::EmptySelection => formatter.write_str("selected process batch must not be empty"),
            Self::ZeroSelectedMass { lot } => write!(
                formatter,
                "selected material lot {} has zero requested mass",
                lot.value()
            ),
            Self::DuplicateSelectedLot { lot } => write!(
                formatter,
                "material lot {} appears more than once in one selected process batch",
                lot.value()
            ),
            Self::UnknownSelectedLot { lot } => {
                write!(formatter, "unknown selected material lot {}", lot.value())
            }
            Self::SelectedLotOwnedElsewhere {
                lot,
                requested_source,
                actual_source,
            } => write!(
                formatter,
                "material lot {} belongs to stockpile {} rather than selected source {}",
                lot.value(),
                actual_source.value(),
                requested_source.value()
            ),
            Self::InsufficientSelectedLotMass {
                lot,
                available,
                requested,
            } => write!(
                formatter,
                "material lot {} contains {} mg but selected batch requests {} mg",
                lot.value(),
                available.milligrams(),
                requested.milligrams()
            ),
        }
    }
}

impl Error for ProcessInputError {}

/// Validated exact input selection consumed by one physical process resolver.
///
/// The token is intentionally not a production outcome. It exposes read-only physical traces to a
/// resolver, then is consumed when that resolver constructs the operation-specific resolution.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedProcessInputs {
    pub(super) process: ProcessId,
    pub(super) selection: ConsumptionSelection,
}

impl ValidatedProcessInputs {
    #[must_use]
    pub const fn process(&self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn source(&self) -> StockpileId {
        self.selection.source()
    }

    #[must_use]
    pub fn consumed_inputs(&self) -> &[ConsumedMaterialTrace] {
        self.selection.consumed_inputs()
    }

    #[must_use]
    pub fn input_mass(&self) -> Mass {
        self.selection.total_consumed()
    }
}

/// Validates process existence and deterministically binds its material requirements to source lots.
pub fn validate_process_inputs(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    source: StockpileId,
) -> Result<ValidatedProcessInputs, ProcessInputError> {
    let Some(definition) = registries.production().get_process(process) else {
        return Err(ProcessInputError::UnknownProcess { process });
    };
    let ProcessInputPolicy::Fixed { inputs, .. } = definition.input_policy() else {
        return Err(ProcessInputError::ExplicitSelectionRequired { process });
    };
    let selection = validate_consumption_selection(state.inventory(), source, inputs)
        .map_err(map_consumption_selection_error)?;
    Ok(ValidatedProcessInputs { process, selection })
}

fn map_consumption_selection_error(error: ConsumptionSelectionError) -> ProcessInputError {
    match error {
        ConsumptionSelectionError::UnknownStockpile { stockpile } => {
            ProcessInputError::UnknownStockpile { stockpile }
        }
        ConsumptionSelectionError::InsufficientMass {
            stockpile,
            commodity,
            available,
            requested,
        } => ProcessInputError::InsufficientMass {
            stockpile,
            commodity,
            available,
            requested,
        },
        ConsumptionSelectionError::MassOverflow { stockpile } => {
            ProcessInputError::MassOverflow { stockpile }
        }
    }
}

/// Binds an explicitly selected conserved matter batch for a process whose physical resolver owns
/// batch eligibility and quantity.
pub fn validate_selected_process_inputs(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    source: StockpileId,
    selections: &[MaterialLotSelection],
) -> Result<ValidatedProcessInputs, ProcessInputError> {
    let Some(definition) = registries.production().get_process(process) else {
        return Err(ProcessInputError::UnknownProcess { process });
    };
    if !matches!(definition.input_policy(), ProcessInputPolicy::SelectedBatch) {
        return Err(ProcessInputError::FixedInputsRequired { process });
    }
    let selection = validate_explicit_consumption_selection(state.inventory(), source, selections)
        .map_err(|error| match error {
            ExplicitConsumptionSelectionError::UnknownStockpile { stockpile } => {
                ProcessInputError::UnknownStockpile { stockpile }
            }
            ExplicitConsumptionSelectionError::EmptySelection => ProcessInputError::EmptySelection,
            ExplicitConsumptionSelectionError::ZeroMass { lot } => {
                ProcessInputError::ZeroSelectedMass { lot }
            }
            ExplicitConsumptionSelectionError::DuplicateLot { lot } => {
                ProcessInputError::DuplicateSelectedLot { lot }
            }
            ExplicitConsumptionSelectionError::UnknownLot { lot } => {
                ProcessInputError::UnknownSelectedLot { lot }
            }
            ExplicitConsumptionSelectionError::LotOwnedElsewhere {
                lot,
                requested_source,
                actual_source,
            } => ProcessInputError::SelectedLotOwnedElsewhere {
                lot,
                requested_source,
                actual_source,
            },
            ExplicitConsumptionSelectionError::InsufficientLotMass {
                lot,
                available,
                requested,
            } => ProcessInputError::InsufficientSelectedLotMass {
                lot,
                available,
                requested,
            },
            ExplicitConsumptionSelectionError::MassOverflow { stockpile } => {
                ProcessInputError::MassOverflow { stockpile }
            }
        })?;
    Ok(ValidatedProcessInputs { process, selection })
}
