//! Operation-specific production resolution; exact selected inputs are bound before physical resolvers derive duration and outputs.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::{ConsumedEnergyTrace, ValidatedEnergySupply};
use crate::equipment::{EquipmentOperationTrace, ValidatedEquipmentUse};
use crate::inventory::{
    ConsumedMaterialTrace, ConsumptionSelection, ConsumptionSelectionError,
    ExplicitConsumptionSelectionError, MaterialLotId, MaterialLotSelection, StockpileId,
    validate_consumption_selection, validate_explicit_consumption_selection,
};
use crate::material::{CommodityKey, CompositionError, MaterialId, MaterialLotSpec};
use crate::registry::Registries;

use super::definitions::{ProcessId, ProcessInputPolicy};

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
                available,
                requested,
                ..
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
    process: ProcessId,
    selection: ConsumptionSelection,
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
    pub const fn input_mass(&self) -> Mass {
        self.selection.total_consumed()
    }

    #[cfg(test)]
    pub(crate) fn resolve(
        self,
        duration: TickSpan,
        outputs: Vec<MaterialLotSpec>,
    ) -> Result<ProcessResolution, ProcessResolutionError> {
        self.resolve_inner(duration, outputs, None, None)
    }

    pub(crate) fn resolve_with_energy_and_equipment(
        self,
        duration: TickSpan,
        outputs: Vec<MaterialLotSpec>,
        energy_supply: ValidatedEnergySupply,
        equipment_use: ValidatedEquipmentUse,
    ) -> Result<ProcessResolution, ProcessResolutionError> {
        self.resolve_inner(duration, outputs, Some(energy_supply), Some(equipment_use))
    }

    fn resolve_inner(
        self,
        duration: TickSpan,
        mut outputs: Vec<MaterialLotSpec>,
        energy_supply: Option<ValidatedEnergySupply>,
        equipment_use: Option<ValidatedEquipmentUse>,
    ) -> Result<ProcessResolution, ProcessResolutionError> {
        if duration.is_zero() {
            return Err(ProcessResolutionError::ZeroDuration);
        }
        if outputs.is_empty() {
            return Err(ProcessResolutionError::NoOutputs);
        }
        outputs.sort();
        validate_outputs(&outputs)?;
        if sum_lot_spec_mass(&outputs).is_none() {
            return Err(ProcessResolutionError::OutputMassOverflow);
        }
        Ok(ProcessResolution {
            process: self.process,
            selection: self.selection,
            energy_supply,
            equipment_use,
            duration,
            outputs,
        })
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
    let selection = validate_consumption_selection(state.inventory_state(), source, inputs)
        .map_err(|error| match error {
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
        })?;
    Ok(ValidatedProcessInputs { process, selection })
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
    let selection =
        validate_explicit_consumption_selection(state.inventory_state(), source, selections)
            .map_err(|error| match error {
                ExplicitConsumptionSelectionError::UnknownStockpile { stockpile } => {
                    ProcessInputError::UnknownStockpile { stockpile }
                }
                ExplicitConsumptionSelectionError::EmptySelection => {
                    ProcessInputError::EmptySelection
                }
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

/// Invalid operation-specific output plan produced by a physical resolver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessResolutionError {
    ZeroDuration,
    NoOutputs,
    ZeroOutputMass {
        commodity: CommodityKey,
    },
    InvalidOutputComposition {
        commodity: CommodityKey,
        error: CompositionError,
    },
    OutputCompositionMissingHost {
        commodity: CommodityKey,
        host: MaterialId,
    },
    DuplicateOutputSpecification {
        commodity: CommodityKey,
    },
    OutputMassOverflow,
}

impl Display for ProcessResolutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroDuration => formatter.write_str("resolved process duration must be nonzero"),
            Self::NoOutputs => formatter.write_str("resolved process must own output matter"),
            Self::ZeroOutputMass { commodity } => write!(
                formatter,
                "resolved output material {} form {} has zero mass",
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::InvalidOutputComposition { commodity, error } => write!(
                formatter,
                "resolved output material {} form {} has invalid composition: {error}",
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::OutputCompositionMissingHost { commodity, host } => write!(
                formatter,
                "resolved output material {} form {} composition omits host material {}",
                commodity.material().value(),
                commodity.form().value(),
                host.value()
            ),
            Self::DuplicateOutputSpecification { commodity } => write!(
                formatter,
                "resolved output repeats material {} form {} with identical physical state",
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::OutputMassOverflow => {
                formatter.write_str("resolved process output mass overflows authoritative storage")
            }
        }
    }
}

impl Error for ProcessResolutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidOutputComposition { error, .. } => Some(error),
            Self::ZeroDuration
            | Self::NoOutputs
            | Self::ZeroOutputMass { .. }
            | Self::OutputCompositionMissingHost { .. }
            | Self::DuplicateOutputSpecification { .. }
            | Self::OutputMassOverflow => None,
        }
    }
}

/// Immutable outcome of physical process resolution for one exact selected input snapshot.
///
/// There is no public arbitrary constructor. Physical subsystem resolvers consume
/// `ValidatedProcessInputs` and create this value through the crate-private resolution boundary.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessResolution {
    process: ProcessId,
    selection: ConsumptionSelection,
    energy_supply: Option<ValidatedEnergySupply>,
    equipment_use: Option<ValidatedEquipmentUse>,
    duration: TickSpan,
    outputs: Vec<MaterialLotSpec>,
}

impl ProcessResolution {
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
    pub const fn input_mass(&self) -> Mass {
        self.selection.total_consumed()
    }

    /// Returns the exact finite energy input bound by the physical resolver, if one is required.
    #[must_use]
    pub fn energy_input(&self) -> Option<ConsumedEnergyTrace> {
        self.energy_supply.map(ValidatedEnergySupply::trace)
    }

    /// Returns the exact equipment-provider snapshot bound by the physical resolver, if any.
    #[must_use]
    pub fn equipment_input(&self) -> Option<EquipmentOperationTrace> {
        self.equipment_use.map(ValidatedEquipmentUse::trace)
    }

    #[must_use]
    pub const fn duration(&self) -> TickSpan {
        self.duration
    }

    #[must_use]
    pub fn outputs(&self) -> &[MaterialLotSpec] {
        &self.outputs
    }

    pub(crate) const fn selection(&self) -> &ConsumptionSelection {
        &self.selection
    }

    pub(crate) const fn energy_supply(&self) -> Option<ValidatedEnergySupply> {
        self.energy_supply
    }

    pub(crate) const fn equipment_use(&self) -> Option<ValidatedEquipmentUse> {
        self.equipment_use
    }
}

fn validate_outputs(outputs: &[MaterialLotSpec]) -> Result<(), ProcessResolutionError> {
    let mut seen = BTreeSet::new();
    for output in outputs {
        let commodity = output.commodity();
        if output.mass().is_zero() {
            return Err(ProcessResolutionError::ZeroOutputMass { commodity });
        }
        output.composition().validate().map_err(|error| {
            ProcessResolutionError::InvalidOutputComposition { commodity, error }
        })?;
        let host = commodity.material();
        if output.composition().parts_per_million(host) == 0 {
            return Err(ProcessResolutionError::OutputCompositionMissingHost { commodity, host });
        }
        if !seen.insert(output.clone()) {
            return Err(ProcessResolutionError::DuplicateOutputSpecification { commodity });
        }
    }
    Ok(())
}

pub(crate) fn sum_lot_spec_mass(entries: &[MaterialLotSpec]) -> Option<Mass> {
    super::definitions::sum_lot_spec_mass(entries)
}

#[cfg(test)]
pub(crate) fn make_test_process_resolution(
    inputs: ValidatedProcessInputs,
    duration_ticks: u64,
    outputs: Vec<MaterialLotSpec>,
) -> ProcessResolution {
    match inputs.resolve(TickSpan::new(duration_ticks), outputs) {
        Ok(resolution) => resolution,
        Err(error) => panic!("test process resolution fixture failed: {error}"),
    }
}
