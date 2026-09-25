//! Current-state maximum manual-power generation and destination-energy envelope.

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::{EnergySinkError, EnergyStoreId};
use crate::equipment::EquipmentId;
use crate::maintenance::maximum_usable_active_ticks;
use crate::registry::Registries;

use super::super::super::power_execution::{
    ManualPowerError, ManualPowerRequest, resolve_manual_power_bindings,
    validate_start_manual_power_with_bindings,
};
use super::super::super::{ManualPowerMethodId, PlayerWorkStartError};
use super::CurrentManualPowerContext;

/// Current-state inputs for exact manual-power energy planning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualPowerEnergyEnvelopeRequest {
    method: ManualPowerMethodId,
    equipment: EquipmentId,
    destination: EnergyStoreId,
    energy_limit: Energy,
    minimum_metabolic_energy_after: Energy,
    minimum_hydration_after: Volume,
}

impl ManualPowerEnergyEnvelopeRequest {
    #[must_use]
    pub const fn new(
        method: ManualPowerMethodId,
        equipment: EquipmentId,
        destination: EnergyStoreId,
        energy_limit: Energy,
    ) -> Self {
        Self {
            method,
            equipment,
            destination,
            energy_limit,
            minimum_metabolic_energy_after: Energy::ZERO,
            minimum_hydration_after: Volume::ZERO,
        }
    }

    /// Requires the completed work order to leave at least these player reserves.
    #[must_use]
    pub const fn with_minimum_reserves(
        mut self,
        metabolic_energy: Energy,
        hydration: Volume,
    ) -> Self {
        self.minimum_metabolic_energy_after = metabolic_energy;
        self.minimum_hydration_after = hydration;
        self
    }
}

/// Exact greatest generated energy and destination-store level currently reachable below a limit.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualPowerEnergyEnvelope {
    requested_limit: Energy,
    maximum_energy: Energy,
    maximum_destination_energy: Energy,
    energy_for_maximum_destination: Energy,
}

impl ManualPowerEnergyEnvelope {
    #[must_use]
    pub const fn requested_limit(self) -> Energy {
        self.requested_limit
    }

    #[must_use]
    pub const fn maximum_energy(self) -> Energy {
        self.maximum_energy
    }

    #[must_use]
    pub const fn maximum_destination_energy(self) -> Energy {
        self.maximum_destination_energy
    }

    #[must_use]
    pub const fn energy_for_maximum_destination(self) -> Energy {
        self.energy_for_maximum_destination
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CandidateStatus {
    Feasible,
    TooLarge,
    DurationUnavailable,
}

#[derive(Clone, Copy)]
struct EnvelopeSearchResult {
    maximum_energy: Energy,
    maximum_destination_energy: Energy,
    energy_for_maximum_destination: Energy,
}

struct EnvelopeContext<'state> {
    shared: CurrentManualPowerContext<'state>,
    minimum_metabolic_energy_after: Energy,
    minimum_hydration_after: Volume,
}

impl EnvelopeContext<'_> {
    fn candidate_status(&self, energy: Energy) -> Result<CandidateStatus, ManualPowerError> {
        let start = match validate_start_manual_power_with_bindings(
            self.shared.registries,
            self.shared.state,
            ManualPowerRequest::new(
                self.shared.method,
                self.shared.equipment,
                self.shared.destination,
                energy,
            ),
            self.shared.bindings,
        ) {
            Ok(start) => start,
            Err(ManualPowerError::Work(
                PlayerWorkStartError::MetabolicCostOverflow { .. }
                | PlayerWorkStartError::InsufficientMetabolicEnergy { .. }
                | PlayerWorkStartError::HydrationCostOverflow { .. }
                | PlayerWorkStartError::InsufficientHydration { .. },
            ))
            | Err(ManualPowerError::EnergySink(
                EnergySinkError::CapacityOverflow { .. }
                | EnergySinkError::InsufficientCapacity { .. },
            ))
            | Err(ManualPowerError::ConditionDuration(_)) => return Ok(CandidateStatus::TooLarge),
            Err(ManualPowerError::Work(PlayerWorkStartError::SurvivalRevisionExhausted {
                ..
            }))
            | Err(ManualPowerError::CompletionTickOverflow { .. }) => {
                return Ok(CandidateStatus::DurationUnavailable);
            }
            Err(error) => return Err(error),
        };
        let Some(player) = self.shared.state.survival().player() else {
            return Err(ManualPowerError::Work(
                PlayerWorkStartError::SurvivalNotInitialized,
            ));
        };
        let budget = start.resource_budget();
        let preserves_reserves = player
            .metabolic_energy()
            .checked_sub(budget.metabolic_energy())
            .is_some_and(|remaining| remaining >= self.minimum_metabolic_energy_after)
            && player
                .hydration()
                .checked_sub(budget.hydration())
                .is_some_and(|remaining| remaining >= self.minimum_hydration_after);
        Ok(if preserves_reserves {
            CandidateStatus::Feasible
        } else {
            CandidateStatus::TooLarge
        })
    }

    fn maximum_feasible_in_bucket(
        &self,
        minimum: Energy,
        maximum: Energy,
    ) -> Result<Option<Energy>, ManualPowerError> {
        match self.candidate_status(minimum)? {
            CandidateStatus::Feasible => {}
            CandidateStatus::TooLarge | CandidateStatus::DurationUnavailable => return Ok(None),
        }
        if self.candidate_status(maximum)? == CandidateStatus::Feasible {
            return Ok(Some(maximum));
        }
        let mut low = minimum.nanojoules();
        let mut high = maximum.nanojoules();
        while low < high {
            let midpoint = low + (high - low).div_ceil(2);
            match self.candidate_status(Energy::from_nanojoules(midpoint))? {
                CandidateStatus::Feasible => low = midpoint,
                CandidateStatus::TooLarge | CandidateStatus::DurationUnavailable => {
                    high = midpoint - 1;
                }
            }
        }
        Ok(Some(Energy::from_nanojoules(low)))
    }

    fn search_duration_buckets(
        &self,
        upper: Energy,
        mut duration: TickSpan,
    ) -> Result<EnvelopeSearchResult, ManualPowerError> {
        let mut result = EnvelopeSearchResult {
            maximum_energy: Energy::ZERO,
            maximum_destination_energy: self.shared.current_stored(),
            energy_for_maximum_destination: Energy::ZERO,
        };
        loop {
            let bucket_capacity = self.shared.output_capacity(duration);
            let previous_capacity = duration
                .checked_sub(TickSpan::new(1))
                .map_or(Energy::ZERO, |previous| {
                    self.shared.output_capacity(previous)
                });
            let Some(minimum_nanojoules) = previous_capacity.nanojoules().checked_add(1) else {
                return Ok(result);
            };
            let minimum = Energy::from_nanojoules(minimum_nanojoules);
            let candidate = upper.min(bucket_capacity).min(
                self.shared
                    .bindings
                    .sink()
                    .available_capacity_at_release(self.shared.registries, duration),
            );
            if candidate >= minimum
                && let Some(maximum) = self.maximum_feasible_in_bucket(minimum, candidate)?
            {
                result.maximum_energy = result.maximum_energy.max(maximum);
                let destination = self.shared.destination_energy_after(duration, maximum);
                if destination > result.maximum_destination_energy
                    || (destination == result.maximum_destination_energy
                        && maximum < result.energy_for_maximum_destination)
                {
                    result.maximum_destination_energy = destination;
                    result.energy_for_maximum_destination = maximum;
                }
            }
            if duration.is_zero() {
                return Ok(result);
            }
            duration = duration
                .checked_sub(TickSpan::new(1))
                .unwrap_or_else(|| unreachable!("nonzero duration has a predecessor"));
        }
    }
}

fn envelope(
    requested_limit: Energy,
    maximum_energy: Energy,
    maximum_destination_energy: Energy,
    energy_for_maximum_destination: Energy,
) -> ManualPowerEnergyEnvelope {
    ManualPowerEnergyEnvelope {
        requested_limit,
        maximum_energy,
        maximum_destination_energy,
        energy_for_maximum_destination,
    }
}

/// Assesses exact current generation and destination-energy bounds without mutation.
pub fn assess_manual_power_energy_envelope(
    registries: &Registries,
    state: &AppState,
    request: ManualPowerEnergyEnvelopeRequest,
) -> Result<ManualPowerEnergyEnvelope, ManualPowerError> {
    let bindings = resolve_manual_power_bindings(
        registries,
        state,
        request.method,
        request.equipment,
        request.destination,
    )?;
    let shared = CurrentManualPowerContext {
        registries,
        state,
        method: request.method,
        equipment: request.equipment,
        destination: request.destination,
        bindings,
    };
    let current_stored = shared.current_stored();
    if request.energy_limit.is_zero() {
        return Ok(envelope(
            request.energy_limit,
            Energy::ZERO,
            current_stored,
            Energy::ZERO,
        ));
    }
    let Some(player) = state.survival().player() else {
        return Err(ManualPowerError::Work(
            PlayerWorkStartError::SurvivalNotInitialized,
        ));
    };
    if player.metabolic_energy() < request.minimum_metabolic_energy_after
        || player.hydration() < request.minimum_hydration_after
    {
        return Ok(envelope(
            request.energy_limit,
            Energy::ZERO,
            current_stored,
            Energy::ZERO,
        ));
    }

    let context = EnvelopeContext {
        shared,
        minimum_metabolic_energy_after: request.minimum_metabolic_energy_after,
        minimum_hydration_after: request.minimum_hydration_after,
    };
    let definition = bindings.definition();
    let condition_ticks = maximum_usable_active_ticks(
        definition.condition_wear_ppm_per_active_tick(),
        bindings.provider().condition(),
    );
    let upper = request
        .energy_limit
        .min(context.shared.output_capacity(condition_ticks))
        .min(
            bindings
                .sink()
                .available_capacity_at_release(registries, condition_ticks),
        );
    if upper.is_zero() {
        return Ok(envelope(
            request.energy_limit,
            Energy::ZERO,
            current_stored,
            Energy::ZERO,
        ));
    }
    let duration = context.shared.schedule_duration(upper)?;
    let result = context.search_duration_buckets(upper, duration)?;
    Ok(envelope(
        request.energy_limit,
        result.maximum_energy,
        result.maximum_destination_energy,
        result.energy_for_maximum_destination,
    ))
}
