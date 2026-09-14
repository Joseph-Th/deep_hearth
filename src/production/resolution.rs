//! Binds exact process inputs and derives operation-specific duration, outputs, energy, and equipment effects.

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::time::TickSpan;
use crate::energy::{ConsumedEnergyTrace, ValidatedEnergySink, ValidatedEnergySupply};
use crate::equipment::{EquipmentOperationTrace, ValidatedEquipmentUse};
use crate::inventory::{ConsumedMaterialTrace, ConsumptionSelection, StockpileId};
use crate::maintenance::Condition;
use crate::material::MaterialLotSpec;

use super::definitions::ProcessId;

mod errors;
mod inputs;
mod resources;

pub use errors::ProcessResolutionError;
pub use inputs::{
    ProcessInputError, ValidatedProcessInputs, validate_process_inputs,
    validate_selected_process_inputs,
};
use resources::ProcessResourceResolution;

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

impl ValidatedProcessInputs {
    pub(crate) fn resolve_with_equipment(
        self,
        duration: TickSpan,
        outputs: Vec<MaterialLotSpec>,
        equipment_use: ValidatedEquipmentUse,
        equipment_condition_after: Condition,
    ) -> Result<ProcessResolution, ProcessResolutionError> {
        self.resolve_inner(
            duration,
            vec![ProcessOutputStream::new(
                ProcessOutputStreamId::PRIMARY,
                outputs,
            )],
            ProcessResourceResolution::with_equipment(equipment_use, equipment_condition_after),
        )
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
            ProcessResourceResolution::none(),
        )
    }

    pub(crate) fn resolve_without_resources_routed(
        self,
        duration: TickSpan,
        output_streams: Vec<ProcessOutputStream>,
    ) -> Result<ProcessResolution, ProcessResolutionError> {
        self.resolve_inner(duration, output_streams, ProcessResourceResolution::none())
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
        let (output_streams, output_mass) = validate_and_order_output_streams(output_streams)?;
        let input_mass = self.selection.total_consumed();
        if output_mass != input_mass {
            return Err(ProcessResolutionError::MatterBalanceMismatch {
                input_mass,
                output_mass,
            });
        }
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
) -> Result<(Vec<ProcessOutputStream>, Mass), ProcessResolutionError> {
    if output_streams.is_empty() {
        return Err(ProcessResolutionError::NoOutputs);
    }
    for stream in &mut output_streams {
        if stream.id.value() == 0 {
            return Err(ProcessResolutionError::ZeroOutputStreamId);
        }
        if stream.outputs.is_empty() {
            return Err(ProcessResolutionError::EmptyOutputStream);
        }
        stream.outputs.sort();
        validate_outputs(&stream.outputs)?;
    }
    output_streams.sort_by_key(|stream| stream.id);
    if let Some(duplicate) = output_streams
        .windows(2)
        .find(|pair| pair[0].id == pair[1].id)
    {
        return Err(ProcessResolutionError::DuplicateOutputStreamId {
            stream: duplicate[0].id,
        });
    }
    let output_mass = sum_output_stream_mass(&output_streams)
        .ok_or(ProcessResolutionError::OutputMassOverflow)?;
    Ok((output_streams, output_mass))
}

/// Immutable outcome of physical process resolution for one exact selected input snapshot.
///
/// There is no public arbitrary constructor. Physical subsystem resolvers consume
/// `ValidatedProcessInputs` and create this value through the crate-private resolution boundary.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
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
    pub fn input_mass(&self) -> Mass {
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
    if let Some(duplicate) = outputs.windows(2).find(|pair| pair[0] == pair[1]) {
        return Err(ProcessResolutionError::DuplicateOutputSpecification {
            commodity: duplicate[0].commodity(),
        });
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
