//! Operation-specific production resolution; exact selected inputs are bound before physical resolvers derive duration and outputs.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::{ConsumedEnergyTrace, ValidatedEnergySink, ValidatedEnergySupply};
use crate::equipment::{EquipmentOperationTrace, ValidatedEquipmentUse};
use crate::inventory::{
    ConsumedMaterialTrace, ConsumptionSelection, ConsumptionSelectionError,
    ExplicitConsumptionSelectionError, MaterialLotId, MaterialLotSelection, StockpileId,
    validate_consumption_selection, validate_explicit_consumption_selection,
};
use crate::maintenance::Condition;
use crate::material::{
    CommodityKey, CompositionError, MaterialId, MaterialInputSpec, MaterialInputSpecError,
    MaterialLotSpec,
};
use crate::registry::Registries;

use super::definitions::{ProcessId, ProcessInputPolicy};

/// Operation-local identity for one physically distinct output stream.
///
/// IDs are stable within a resolved process family and persisted with in-flight jobs so routing is
/// never dependent on vector position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProcessOutputStreamId(u16);

impl ProcessOutputStreamId {
    pub const PRIMARY: Self = Self(1);

    #[must_use]
    pub const fn new(value: u16) -> Self {
        assert!(value != 0, "process output stream id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

/// One physically inseparable material stream produced by a resolved operation.
///
/// A stream may contain multiple homogeneous lot specifications, but routing is assigned to the
/// stream as a whole. This prevents logistics code from inventing a separation that the physical
/// resolver did not perform.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessOutputStream {
    id: ProcessOutputStreamId,
    outputs: Vec<MaterialLotSpec>,
}

impl ProcessOutputStream {
    pub(crate) fn new(id: ProcessOutputStreamId, outputs: Vec<MaterialLotSpec>) -> Self {
        Self { id, outputs }
    }

    #[must_use]
    pub const fn id(&self) -> ProcessOutputStreamId {
        self.id
    }

    /// Returns the homogeneous lots that jointly make up this inseparable stream.
    #[must_use]
    pub fn outputs(&self) -> &[MaterialLotSpec] {
        &self.outputs
    }
}

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
    RepeatedInputMassOverflow {
        process: ProcessId,
        commodity: CommodityKey,
        repetitions: NonZeroU64,
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
            Self::RepeatedInputMassOverflow {
                process,
                commodity,
                repetitions,
            } => write!(
                formatter,
                "repeating process {} input material {} form {} {} times exceeds authoritative mass range",
                process.value(),
                commodity.material().value(),
                commodity.form().value(),
                repetitions.get()
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcessResourceResolution {
    None,
    SupplyAndEquipment {
        energy_supply: ValidatedEnergySupply,
        equipment: ProcessEquipmentResolution,
    },
    SinkAndEquipment {
        energy_sink: ValidatedEnergySink,
        equipment: ProcessEquipmentResolution,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProcessEquipmentResolution {
    equipment_use: ValidatedEquipmentUse,
    condition_after: Condition,
}

struct ResolvedProcessResources {
    energy_supply: Option<ValidatedEnergySupply>,
    energy_sink: Option<ValidatedEnergySink>,
    equipment_use: Option<ValidatedEquipmentUse>,
    equipment_condition_after: Option<Condition>,
}

impl ProcessEquipmentResolution {
    fn validate(self) -> Result<Self, ProcessResolutionError> {
        let before = self.equipment_use.trace().condition();
        if self.condition_after > before {
            return Err(ProcessResolutionError::EquipmentConditionImproved {
                before,
                after: self.condition_after,
            });
        }
        Ok(self)
    }
}

impl ProcessResourceResolution {
    const NONE: Self = Self::None;

    const fn with_supply_and_equipment(
        energy_supply: ValidatedEnergySupply,
        equipment_use: ValidatedEquipmentUse,
        condition_after: Condition,
    ) -> Self {
        Self::SupplyAndEquipment {
            energy_supply,
            equipment: ProcessEquipmentResolution {
                equipment_use,
                condition_after,
            },
        }
    }

    const fn with_sink_and_equipment(
        energy_sink: ValidatedEnergySink,
        equipment_use: ValidatedEquipmentUse,
        condition_after: Condition,
    ) -> Self {
        Self::SinkAndEquipment {
            energy_sink,
            equipment: ProcessEquipmentResolution {
                equipment_use,
                condition_after,
            },
        }
    }

    fn resolve(self) -> Result<ResolvedProcessResources, ProcessResolutionError> {
        match self {
            Self::None => Ok(ResolvedProcessResources {
                energy_supply: None,
                energy_sink: None,
                equipment_use: None,
                equipment_condition_after: None,
            }),
            Self::SupplyAndEquipment {
                energy_supply,
                equipment,
            } => {
                let equipment = equipment.validate()?;
                Ok(ResolvedProcessResources {
                    energy_supply: Some(energy_supply),
                    energy_sink: None,
                    equipment_use: Some(equipment.equipment_use),
                    equipment_condition_after: Some(equipment.condition_after),
                })
            }
            Self::SinkAndEquipment {
                energy_sink,
                equipment,
            } => {
                let equipment = equipment.validate()?;
                Ok(ResolvedProcessResources {
                    energy_supply: None,
                    energy_sink: Some(energy_sink),
                    equipment_use: Some(equipment.equipment_use),
                    equipment_condition_after: Some(equipment.condition_after),
                })
            }
        }
    }
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

    pub(crate) fn resolve_without_resources(
        self,
        duration: TickSpan,
        outputs: Vec<MaterialLotSpec>,
    ) -> Result<ProcessResolution, ProcessResolutionError> {
        self.resolve_inner(
            duration,
            vec![ProcessOutputStream::new(
                ProcessOutputStreamId::PRIMARY,
                outputs,
            )],
            ProcessResourceResolution::NONE,
        )
    }

    pub(crate) fn resolve_without_resources_routed(
        self,
        duration: TickSpan,
        output_streams: Vec<ProcessOutputStream>,
    ) -> Result<ProcessResolution, ProcessResolutionError> {
        self.resolve_inner(duration, output_streams, ProcessResourceResolution::NONE)
    }

    pub(crate) fn resolve_with_energy_and_equipment(
        self,
        duration: TickSpan,
        output_streams: Vec<ProcessOutputStream>,
        energy_supply: ValidatedEnergySupply,
        equipment_use: ValidatedEquipmentUse,
        equipment_condition_after: Condition,
    ) -> Result<ProcessResolution, ProcessResolutionError> {
        self.resolve_inner(
            duration,
            output_streams,
            ProcessResourceResolution::with_supply_and_equipment(
                energy_supply,
                equipment_use,
                equipment_condition_after,
            ),
        )
    }

    pub(crate) fn resolve_with_equipment_and_energy_release(
        self,
        duration: TickSpan,
        output_streams: Vec<ProcessOutputStream>,
        energy_sink: ValidatedEnergySink,
        equipment_use: ValidatedEquipmentUse,
        equipment_condition_after: Condition,
    ) -> Result<ProcessResolution, ProcessResolutionError> {
        self.resolve_inner(
            duration,
            output_streams,
            ProcessResourceResolution::with_sink_and_equipment(
                energy_sink,
                equipment_use,
                equipment_condition_after,
            ),
        )
    }

    fn resolve_inner(
        self,
        duration: TickSpan,
        output_streams: Vec<ProcessOutputStream>,
        resources: ProcessResourceResolution,
    ) -> Result<ProcessResolution, ProcessResolutionError> {
        if duration.is_zero() {
            return Err(ProcessResolutionError::ZeroDuration);
        }
        let output_streams = validate_and_order_output_streams(output_streams)?;
        let resources = resources.resolve()?;
        Ok(ProcessResolution {
            process: self.process,
            selection: self.selection,
            energy_supply: resources.energy_supply,
            energy_sink: resources.energy_sink,
            equipment_use: resources.equipment_use,
            equipment_condition_after: resources.equipment_condition_after,
            duration,
            output_streams,
        })
    }
}

fn validate_and_order_output_streams(
    mut output_streams: Vec<ProcessOutputStream>,
) -> Result<Vec<ProcessOutputStream>, ProcessResolutionError> {
    if output_streams.is_empty() {
        return Err(ProcessResolutionError::NoOutputs);
    }
    let mut stream_ids = BTreeSet::new();
    for stream in &mut output_streams {
        if stream.id.value() == 0 {
            return Err(ProcessResolutionError::ZeroOutputStreamId);
        }
        if !stream_ids.insert(stream.id) {
            return Err(ProcessResolutionError::DuplicateOutputStreamId { stream: stream.id });
        }
        if stream.outputs.is_empty() {
            return Err(ProcessResolutionError::EmptyOutputStream);
        }
        stream.outputs.sort();
        validate_outputs(&stream.outputs)?;
    }
    output_streams.sort_by_key(|stream| stream.id);
    if sum_output_stream_mass(&output_streams).is_none() {
        return Err(ProcessResolutionError::OutputMassOverflow);
    }
    Ok(output_streams)
}

#[cfg(test)]
pub(crate) fn make_test_process_resolution_with_streams(
    inputs: ValidatedProcessInputs,
    duration_ticks: u64,
    output_streams: Vec<(ProcessOutputStreamId, Vec<MaterialLotSpec>)>,
) -> ProcessResolution {
    let output_streams = output_streams
        .into_iter()
        .map(|(id, outputs)| ProcessOutputStream::new(id, outputs))
        .collect();
    match inputs.resolve_inner(
        TickSpan::new(duration_ticks),
        output_streams,
        ProcessResourceResolution::NONE,
    ) {
        Ok(resolution) => resolution,
        Err(error) => panic!("multi-stream test process resolution fixture failed: {error}"),
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

/// Deterministically binds an integral number of identical fixed-input process batches.
///
/// This remains a fixed-input operation: every authored input mass and composition constraint is
/// scaled by the same repeat count, so callers cannot turn a fixed recipe into an arbitrary batch.
pub(crate) fn validate_repeated_process_inputs(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    source: StockpileId,
    repetitions: NonZeroU64,
) -> Result<ValidatedProcessInputs, ProcessInputError> {
    if repetitions == NonZeroU64::MIN {
        return validate_process_inputs(registries, state, process, source);
    }
    let Some(definition) = registries.production().get_process(process) else {
        return Err(ProcessInputError::UnknownProcess { process });
    };
    let ProcessInputPolicy::Fixed { inputs, .. } = definition.input_policy() else {
        return Err(ProcessInputError::ExplicitSelectionRequired { process });
    };
    let scaled_inputs = inputs
        .iter()
        .map(|input| scale_input(process, input, repetitions))
        .collect::<Result<Vec<_>, _>>()?;
    let selection = validate_consumption_selection(state.inventory(), source, &scaled_inputs)
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

fn scale_input(
    process: ProcessId,
    input: &MaterialInputSpec,
    repetitions: NonZeroU64,
) -> Result<MaterialInputSpec, ProcessInputError> {
    let mass = input
        .mass()
        .milligrams()
        .checked_mul(repetitions.get())
        .map(Mass::from_milligrams)
        .ok_or(ProcessInputError::RepeatedInputMassOverflow {
            process,
            commodity: input.commodity(),
            repetitions,
        })?;
    match MaterialInputSpec::with_constraints(input.commodity(), mass, input.constraints().to_vec())
    {
        Ok(input) => Ok(input),
        Err(MaterialInputSpecError::ZeroMass) => {
            unreachable!("nonzero authored input repeated a nonzero number of times")
        }
        Err(MaterialInputSpecError::DuplicateConstraint {
            material: _material,
        }) => {
            unreachable!("authored input constraints were validated before repeat scaling")
        }
        Err(MaterialInputSpecError::HostExcluded { host: _ }) => {
            unreachable!("authored input host feasibility was validated before repeat scaling")
        }
        Err(MaterialInputSpecError::ImpossibleMinimumTotal { total_ppm: _ }) => {
            unreachable!(
                "authored input constraint feasibility was validated before repeat scaling"
            )
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

/// Invalid operation-specific output plan produced by a physical resolver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessResolutionError {
    ZeroDuration,
    NoOutputs,
    ZeroOutputStreamId,
    DuplicateOutputStreamId {
        stream: ProcessOutputStreamId,
    },
    EmptyOutputStream,
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
    EquipmentConditionImproved {
        before: Condition,
        after: Condition,
    },
}

impl Display for ProcessResolutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroDuration => formatter.write_str("resolved process duration must be nonzero"),
            Self::NoOutputs => formatter.write_str("resolved process must own output matter"),
            Self::ZeroOutputStreamId => {
                formatter.write_str("resolved process output stream id must be nonzero")
            }
            Self::DuplicateOutputStreamId { stream } => write!(
                formatter,
                "resolved process contains duplicate output stream id {}",
                stream.value()
            ),
            Self::EmptyOutputStream => {
                formatter.write_str("resolved process output stream must own material")
            }
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
            Self::EquipmentConditionImproved { before, after } => write!(
                formatter,
                "production operation cannot improve equipment condition from {} ppm to {} ppm",
                before.parts_per_million(),
                after.parts_per_million()
            ),
        }
    }
}

impl Error for ProcessResolutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidOutputComposition {
                commodity: _commodity,
                error,
            } => Some(error),
            Self::ZeroDuration
            | Self::NoOutputs
            | Self::ZeroOutputStreamId
            | Self::EmptyOutputStream
            | Self::OutputMassOverflow => None,
            Self::DuplicateOutputStreamId { stream: _stream } => None,
            Self::ZeroOutputMass {
                commodity: _commodity,
            }
            | Self::DuplicateOutputSpecification {
                commodity: _commodity,
            } => None,
            Self::OutputCompositionMissingHost {
                commodity: _commodity,
                host: _host,
            } => None,
            Self::EquipmentConditionImproved {
                before: _before,
                after: _after,
            } => None,
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
    energy_sink: Option<ValidatedEnergySink>,
    equipment_use: Option<ValidatedEquipmentUse>,
    equipment_condition_after: Option<Condition>,
    duration: TickSpan,
    output_streams: Vec<ProcessOutputStream>,
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

    /// Returns the exact equipment condition committed when this operation completes, if any.
    #[must_use]
    pub const fn equipment_condition_after(&self) -> Option<Condition> {
        self.equipment_condition_after
    }

    #[must_use]
    pub const fn duration(&self) -> TickSpan {
        self.duration
    }

    pub fn output_streams(&self) -> &[ProcessOutputStream] {
        &self.output_streams
    }

    /// Returns the sole stream for processes whose physics guarantees exactly one output stream.
    #[must_use]
    pub fn single_output_stream(&self) -> Option<&ProcessOutputStream> {
        let [stream] = self.output_streams.as_slice() else {
            return None;
        };
        Some(stream)
    }

    pub(crate) const fn selection(&self) -> &ConsumptionSelection {
        &self.selection
    }

    pub(crate) const fn energy_supply(&self) -> Option<ValidatedEnergySupply> {
        self.energy_supply
    }

    pub(crate) const fn energy_sink(&self) -> Option<ValidatedEnergySink> {
        self.energy_sink
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

pub(crate) fn sum_output_stream_mass(entries: &[ProcessOutputStream]) -> Option<Mass> {
    let mut total = Mass::ZERO;
    for stream in entries {
        total = total.checked_add(sum_lot_spec_mass(stream.outputs())?)?;
    }
    Some(total)
}

#[cfg(test)]
pub(crate) fn make_test_process_resolution(
    inputs: ValidatedProcessInputs,
    duration_ticks: u64,
    outputs: Vec<MaterialLotSpec>,
) -> ProcessResolution {
    match inputs.resolve_without_resources(TickSpan::new(duration_ticks), outputs) {
        Ok(resolution) => resolution,
        Err(error) => panic!("test process resolution fixture failed: {error}"),
    }
}
