//! Deterministic binding of authored process inputs to exact inventory matter.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    ConsumedMaterialTrace, ConsumptionSelection, ExplicitConsumptionSelectionError, MaterialLotId,
    MaterialLotSelection, StockpileId, validate_explicit_consumption_selection,
};
use crate::registry::Registries;

use crate::production::definitions::ProcessId;

/// Failure while binding one authored process to the exact source matter a resolver will inspect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessInputError {
    UnknownProcess {
        process: ProcessId,
    },
    UnknownStockpile {
        stockpile: StockpileId,
    },
    MassOverflow {
        stockpile: StockpileId,
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
            Self::MassOverflow { stockpile } => write!(
                formatter,
                "selected process input mass overflowed in stockpile {}",
                stockpile.value()
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
pub(crate) struct ValidatedProcessInputs {
    pub(super) process: ProcessId,
    pub(super) selection: ConsumptionSelection,
}

impl ValidatedProcessInputs {
    #[must_use]
    pub(crate) fn consumed_inputs(&self) -> &[ConsumedMaterialTrace] {
        self.selection.consumed_inputs()
    }

    #[must_use]
    pub(crate) fn input_mass(&self) -> Mass {
        self.selection.total_consumed()
    }
}

/// Binds an explicitly selected conserved matter batch for a process whose physical resolver owns
/// batch eligibility and quantity.
pub(crate) fn validate_process_inputs(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    source: StockpileId,
    selections: &[MaterialLotSelection],
) -> Result<ValidatedProcessInputs, ProcessInputError> {
    if registries.production().get_process(process).is_none() {
        return Err(ProcessInputError::UnknownProcess { process });
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
