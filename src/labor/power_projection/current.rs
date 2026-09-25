//! Exact current-state planning for direct player-powered generation.

use crate::core::quantity::Energy;
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::registry::Registries;

use super::super::ManualPowerMethodId;
use super::super::power_execution::{
    ManualPowerError, ManualPowerRequest, ResolvedManualPowerBindings,
    map_manual_power_schedule_error,
};
use super::super::power_physics::{
    maximum_manual_power_output_for_duration, resolve_manual_power_schedule,
};

mod envelope;
mod target;

pub use envelope::{
    ManualPowerEnergyEnvelope, ManualPowerEnergyEnvelopeRequest,
    assess_manual_power_energy_envelope,
};
pub use target::{
    ManualPowerDestinationTargetAssessment, ManualPowerDestinationTargetBlocker,
    ManualPowerDestinationTargetProjection, ManualPowerDestinationTargetRequest,
    assess_manual_power_destination_target,
};

pub(super) struct CurrentManualPowerContext<'state> {
    pub(super) registries: &'state Registries,
    pub(super) state: &'state AppState,
    pub(super) method: ManualPowerMethodId,
    pub(super) equipment: EquipmentId,
    pub(super) destination: EnergyStoreId,
    pub(super) bindings: ResolvedManualPowerBindings<'state>,
}

impl CurrentManualPowerContext<'_> {
    pub(super) fn current_stored(&self) -> Energy {
        self.state
            .energy()
            .get_store(self.destination)
            .map(|record| record.stored())
            .unwrap_or_else(|| {
                panic!("validated manual-power destination disappeared during read-only planning")
            })
    }

    pub(super) const fn destination_capacity(&self) -> Energy {
        self.bindings.sink().capacity()
    }

    pub(super) fn destination_energy_after(&self, duration: TickSpan, generated: Energy) -> Energy {
        self.bindings
            .sink()
            .stored_after_elapsed(self.registries, duration)
            .checked_add(generated)
            .unwrap_or_else(|| {
                panic!("validated manual-power destination energy overflowed after completion")
            })
    }

    pub(super) fn output_capacity(&self, duration: TickSpan) -> Energy {
        let definition = self.bindings.definition();
        maximum_manual_power_output_for_duration(
            self.bindings.transfer_power(),
            self.registries.core().physical_tick_duration(),
            definition.maximum_exertion(),
            definition.metabolic_efficiency_ppm(),
            duration,
        )
    }

    pub(super) fn schedule_duration(&self, energy: Energy) -> Result<TickSpan, ManualPowerError> {
        let definition = self.bindings.definition();
        resolve_manual_power_schedule(
            energy,
            self.bindings.transfer_power(),
            self.registries.core().physical_tick_duration(),
            definition.maximum_exertion(),
            definition.metabolic_efficiency_ppm(),
        )
        .map(|schedule| schedule.duration())
        .map_err(|error| {
            map_manual_power_schedule_error(
                ManualPowerRequest::new(self.method, self.equipment, self.destination, energy),
                self.bindings.transfer_power(),
                error,
            )
        })
    }
}
