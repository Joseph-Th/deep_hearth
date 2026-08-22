//! Direct player-power transactions; lifecycle owns exclusivity while energy/equipment owners commit completion consequences.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityValue, CapabilityValueKind};
use crate::core::quantity::{Energy, Power};
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::energy::{
    EnergyCarrier, EnergySinkError, EnergyStoreId, EnergyStoreRecord,
    apply_released_energy_outcomes, calculate_power_duration_ceiling, validate_energy_sink,
};
use crate::equipment::{EquipmentId, EquipmentProviderError, resolve_equipment_provider};
use crate::maintenance::{
    ActiveConditionDurationError, calculate_usable_condition_after_active_ticks,
};
use crate::mining::MiningJobId;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::registry::Registries;

use super::power_physics::{
    ManualPowerExertionError, ManualPowerMetabolicDurationError, calculate_metabolic_duration,
    metabolic_output_per_tick, resolve_manual_power_exertion,
};
use super::{
    ManualPowerMethodId, ManualPowerWork, PlayerWork, PlayerWorkCommitError,
    PlayerWorkResourceBudget, PlayerWorkStartError, ValidatedPlayerWorkStart,
    validate_player_work_start,
};

/// Direct-labor request to place an exact quantity of generated work into one finite store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualPowerRequest {
    method: ManualPowerMethodId,
    equipment: EquipmentId,
    destination: EnergyStoreId,
    energy: Energy,
}

impl ManualPowerRequest {
    #[must_use]
    pub const fn new(
        method: ManualPowerMethodId,
        equipment: EquipmentId,
        destination: EnergyStoreId,
        energy: Energy,
    ) -> Self {
        Self {
            method,
            equipment,
            destination,
            energy,
        }
    }
}

/// Failure while resolving one direct player-power work order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualPowerError {
    UnknownMethod {
        method: ManualPowerMethodId,
    },
    Work(PlayerWorkStartError),
    Equipment(EquipmentProviderError),
    EquipmentMounted {
        equipment: EquipmentId,
    },
    EquipmentBusyProduction {
        equipment: EquipmentId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    MissingPowerCapability {
        equipment: EquipmentId,
        capability: crate::capability::CapabilityId,
    },
    PowerCapabilityKindMismatch {
        equipment: EquipmentId,
        capability: crate::capability::CapabilityId,
        found: CapabilityValueKind,
    },
    ZeroEquipmentPower {
        equipment: EquipmentId,
        capability: crate::capability::CapabilityId,
    },
    EnergySink(EnergySinkError),
    WrongCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    ZeroTransferPower {
        equipment: EquipmentId,
        destination: EnergyStoreId,
    },
    PowerDuration {
        energy: Energy,
        power: Power,
    },
    MetabolicConversionTooSmall {
        method: ManualPowerMethodId,
    },
    MetabolicDurationOverflow {
        method: ManualPowerMethodId,
        energy: Energy,
    },
    ExertionResolution {
        method: ManualPowerMethodId,
    },
    ConditionDuration(ActiveConditionDurationError),
    CompletionTickOverflow {
        method: ManualPowerMethodId,
    },
}

impl Display for ManualPowerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMethod { method } => {
                write!(formatter, "unknown manual power method {}", method.value())
            }
            Self::Work(error) => write!(formatter, "manual power labor admission failed: {error}"),
            Self::Equipment(error) => write!(formatter, "manual power equipment failed: {error}"),
            Self::EquipmentMounted { equipment } => write!(
                formatter,
                "manual power equipment {} is mounted and cannot be used for direct player-powered generation",
                equipment.value()
            ),
            Self::EquipmentBusyProduction {
                equipment,
                job,
                release,
            } => write!(
                formatter,
                "manual power equipment {} is occupied by production job {} {release}",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "manual power equipment {} is occupied by mining job {}",
                equipment.value(),
                job.value()
            ),
            Self::MissingPowerCapability {
                equipment,
                capability,
            } => write!(
                formatter,
                "manual power equipment {} lacks authored power capability {}",
                equipment.value(),
                capability.value()
            ),
            Self::PowerCapabilityKindMismatch {
                equipment,
                capability,
                found,
            } => write!(
                formatter,
                "manual power equipment {} capability {} has {found:?} value kind instead of Power",
                equipment.value(),
                capability.value()
            ),
            Self::ZeroEquipmentPower {
                equipment,
                capability,
            } => write!(
                formatter,
                "manual power equipment {} capability {} currently resolves zero output power",
                equipment.value(),
                capability.value()
            ),
            Self::EnergySink(error) => {
                write!(formatter, "manual power destination failed: {error}")
            }
            Self::WrongCarrier { required, provided } => write!(
                formatter,
                "manual power method requires {required:?} storage but destination is {provided:?}"
            ),
            Self::ZeroTransferPower {
                equipment,
                destination,
            } => write!(
                formatter,
                "manual power equipment {} and destination store {} have no common transfer power",
                equipment.value(),
                destination.value()
            ),
            Self::PowerDuration { energy, power } => write!(
                formatter,
                "manual power output of {} nJ at {} pW cannot be transferred within the authoritative tick range",
                energy.nanojoules(),
                power.picowatts()
            ),
            Self::MetabolicConversionTooSmall { method } => write!(
                formatter,
                "manual power method {} metabolic conversion produces less than one nanojoule per active tick",
                method.value()
            ),
            Self::MetabolicDurationOverflow { method, energy } => write!(
                formatter,
                "manual power method {} requires more than the authoritative tick range to generate {} nJ",
                method.value(),
                energy.nanojoules()
            ),
            Self::ExertionResolution { method } => write!(
                formatter,
                "manual power method {} cannot resolve physiological effort for the requested output",
                method.value()
            ),
            Self::ConditionDuration(error) => write!(
                formatter,
                "manual power work exceeds equipment condition lifetime: {error}"
            ),
            Self::CompletionTickOverflow { method } => write!(
                formatter,
                "manual power method {} completion exceeds the world clock range",
                method.value()
            ),
        }
    }
}

impl Error for ManualPowerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Work(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::EnergySink(error) => Some(error),
            Self::ConditionDuration(error) => Some(error),
            Self::UnknownMethod { method: _ }
            | Self::EquipmentMounted { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::MissingPowerCapability { .. }
            | Self::PowerCapabilityKindMismatch { .. }
            | Self::ZeroEquipmentPower { .. }
            | Self::WrongCarrier { .. }
            | Self::ZeroTransferPower { .. }
            | Self::PowerDuration { .. }
            | Self::MetabolicConversionTooSmall { .. }
            | Self::MetabolicDurationOverflow { .. }
            | Self::ExertionResolution { .. }
            | Self::CompletionTickOverflow { .. } => None,
        }
    }
}

/// Commit-time conflict for a resolved direct player-power start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualPowerCommitError {
    Work(PlayerWorkCommitError),
    StaleEquipmentRevision {
        expected: u64,
        actual: u64,
    },
    StaleEnergyRevision {
        expected: u64,
        actual: u64,
    },
    EquipmentBusyProduction {
        equipment: EquipmentId,
        job: ProductionJobId,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    EnergyBusyProduction {
        store: EnergyStoreId,
        job: ProductionJobId,
    },
}

impl Display for ManualPowerCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Work(error) => write!(
                formatter,
                "manual power labor changed after validation: {error}"
            ),
            Self::StaleEquipmentRevision { expected, actual } => write!(
                formatter,
                "manual power expected equipment revision {expected} but current revision is {actual}"
            ),
            Self::StaleEnergyRevision { expected, actual } => write!(
                formatter,
                "manual power expected energy revision {expected} but current revision is {actual}"
            ),
            Self::EquipmentBusyProduction { equipment, job } => write!(
                formatter,
                "manual power equipment {} became occupied by production job {} after validation",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "manual power equipment {} became occupied by mining job {} after validation",
                equipment.value(),
                job.value()
            ),
            Self::EnergyBusyProduction { store, job } => write!(
                formatter,
                "manual power destination store {} became occupied by production job {} after validation",
                store.value(),
                job.value()
            ),
        }
    }
}

impl Error for ManualPowerCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Work(error) => Some(error),
            Self::StaleEquipmentRevision { .. }
            | Self::StaleEnergyRevision { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EnergyBusyProduction { .. } => None,
        }
    }
}

/// Revision-bound admission token for direct player-powered generation.
#[must_use]
pub struct ValidatedManualPowerStart {
    work_start: ValidatedPlayerWorkStart,
    work: ManualPowerWork,
    resource_budget: PlayerWorkResourceBudget,
    expected_equipment_revision: u64,
    expected_energy_revision: u64,
}

impl ValidatedManualPowerStart {
    #[must_use]
    pub const fn work(&self) -> ManualPowerWork {
        self.work
    }

    /// Returns the authoritative survival reserve consumed if this work runs to completion.
    #[must_use]
    pub const fn resource_budget(&self) -> PlayerWorkResourceBudget {
        self.resource_budget
    }

    pub fn commit(self, state: &mut AppState) -> Result<ManualPowerWork, ManualPowerCommitError> {
        self.work_start
            .precheck(state)
            .map_err(ManualPowerCommitError::Work)?;
        if state.equipment().revision() != self.expected_equipment_revision {
            return Err(ManualPowerCommitError::StaleEquipmentRevision {
                expected: self.expected_equipment_revision,
                actual: state.equipment().revision(),
            });
        }
        if state.energy().revision() != self.expected_energy_revision {
            return Err(ManualPowerCommitError::StaleEnergyRevision {
                expected: self.expected_energy_revision,
                actual: state.energy().revision(),
            });
        }
        if let Some(job) = state
            .production()
            .get_equipment_occupant(self.work.equipment())
        {
            return Err(ManualPowerCommitError::EquipmentBusyProduction {
                equipment: self.work.equipment(),
                job: job.id(),
            });
        }
        if let Some(job) = state.mining().get_equipment_occupant(self.work.equipment()) {
            return Err(ManualPowerCommitError::EquipmentBusyMining {
                equipment: self.work.equipment(),
                job,
            });
        }
        if let Some(job) = state
            .production()
            .get_energy_occupant(self.work.destination())
        {
            return Err(ManualPowerCommitError::EnergyBusyProduction {
                store: self.work.destination(),
                job,
            });
        }
        self.work_start.apply(state);
        Ok(self.work)
    }
}

/// Resolves and admits a direct player-power work order without creating energy before work finishes.
pub fn validate_start_manual_power(
    registries: &Registries,
    state: &AppState,
    request: ManualPowerRequest,
) -> Result<ValidatedManualPowerStart, ManualPowerError> {
    let definition = registries
        .labor()
        .get_manual_power(request.method)
        .copied()
        .ok_or(ManualPowerError::UnknownMethod {
            method: request.method,
        })?;
    if state
        .equipment()
        .get_equipment(request.equipment)
        .is_some_and(|equipment| equipment.supported_by().is_some())
    {
        return Err(ManualPowerError::EquipmentMounted {
            equipment: request.equipment,
        });
    }
    let provider = resolve_equipment_provider(registries, state, request.equipment)
        .map_err(ManualPowerError::Equipment)?;
    if let Some(job) = state.production().get_equipment_occupant(request.equipment) {
        return Err(ManualPowerError::EquipmentBusyProduction {
            equipment: request.equipment,
            job: job.id(),
            release: job.occupancy_release(),
        });
    }
    if let Some(job) = state.mining().get_equipment_occupant(request.equipment) {
        return Err(ManualPowerError::EquipmentBusyMining {
            equipment: request.equipment,
            job,
        });
    }
    let power_value = provider
        .get_capability(definition.power_capability())
        .ok_or(ManualPowerError::MissingPowerCapability {
            equipment: request.equipment,
            capability: definition.power_capability(),
        })?;
    let CapabilityValue::Power(equipment_power) = power_value else {
        return Err(ManualPowerError::PowerCapabilityKindMismatch {
            equipment: request.equipment,
            capability: definition.power_capability(),
            found: power_value.kind(),
        });
    };
    if equipment_power.is_zero() {
        return Err(ManualPowerError::ZeroEquipmentPower {
            equipment: request.equipment,
            capability: definition.power_capability(),
        });
    }
    let sink = validate_energy_sink(registries, state, request.destination, request.energy)
        .map_err(ManualPowerError::EnergySink)?;
    if sink.trace().carrier() != definition.carrier() {
        return Err(ManualPowerError::WrongCarrier {
            required: definition.carrier(),
            provided: sink.trace().carrier(),
        });
    }
    let transfer_power = std::cmp::min(equipment_power, sink.max_input_power());
    if transfer_power == Power::ZERO {
        return Err(ManualPowerError::ZeroTransferPower {
            equipment: request.equipment,
            destination: request.destination,
        });
    }
    let power_duration = calculate_power_duration_ceiling(
        transfer_power,
        request.energy,
        registries.core().physical_tick_duration(),
    )
    .map_err(|_error| ManualPowerError::PowerDuration {
        energy: request.energy,
        power: transfer_power,
    })?;
    let metabolic_output = metabolic_output_per_tick(
        definition.maximum_exertion().energy_cost_per_tick(),
        definition.metabolic_efficiency_ppm(),
    );
    let metabolic_duration = calculate_metabolic_duration(request.energy, metabolic_output)
        .map_err(|error| match error {
            ManualPowerMetabolicDurationError::ZeroOutput => {
                ManualPowerError::MetabolicConversionTooSmall {
                    method: request.method,
                }
            }
            ManualPowerMetabolicDurationError::DurationOverflow => {
                ManualPowerError::MetabolicDurationOverflow {
                    method: request.method,
                    energy: request.energy,
                }
            }
        })?;
    let duration = std::cmp::max(power_duration, metabolic_duration);
    let exertion = resolve_manual_power_exertion(
        request.energy,
        duration,
        definition.maximum_exertion(),
        definition.metabolic_efficiency_ppm(),
    )
    .map_err(|error| match error {
        ManualPowerExertionError::EnergyOverflow
        | ManualPowerExertionError::HydrationOverflow
        | ManualPowerExertionError::ExceedsAuthoredMaximum => {
            ManualPowerError::ExertionResolution {
                method: request.method,
            }
        }
    })?;
    let completes_at = state.tick().checked_add_span(duration).ok_or(
        ManualPowerError::CompletionTickOverflow {
            method: request.method,
        },
    )?;
    let equipment_use = provider.validated_use();
    let condition_after = calculate_usable_condition_after_active_ticks(
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
        duration,
    )
    .map_err(ManualPowerError::ConditionDuration)?;
    let work = ManualPowerWork::new(
        request.method,
        equipment_use.trace(),
        condition_after,
        sink.trace(),
        state.tick(),
        completes_at,
    );
    let work_start = validate_player_work_start(
        registries,
        state,
        PlayerWork::ManualPower { work },
        duration,
        exertion,
    )
    .map_err(ManualPowerError::Work)?;
    let resource_budget = work_start.resource_budget();
    Ok(ValidatedManualPowerStart {
        work_start,
        work,
        resource_budget,
        expected_equipment_revision: equipment_use.expected_equipment_revision(),
        expected_energy_revision: state.energy().revision(),
    })
}

/// Observable completion of one direct player-powered generation work order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualPowerOutcome {
    method: ManualPowerMethodId,
    equipment: EquipmentId,
    destination: EnergyStoreId,
    energy: Energy,
}

impl ManualPowerOutcome {
    #[must_use]
    pub const fn method(self) -> ManualPowerMethodId {
        self.method
    }
    #[must_use]
    pub const fn equipment(self) -> EquipmentId {
        self.equipment
    }
    #[must_use]
    pub const fn destination(self) -> EnergyStoreId {
        self.destination
    }
    #[must_use]
    pub const fn energy(self) -> Energy {
        self.energy
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ManualPowerTickError {
    EnergyRevisionExhausted,
    EquipmentRevisionExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ManualPowerTickPlan {
    work: ManualPowerWork,
    stored_before: Energy,
}

impl ManualPowerTickPlan {
    pub(crate) const fn equipment_revision_steps(&self) -> u64 {
        1
    }

    pub(crate) const fn energy_revision_steps(&self) -> u64 {
        1
    }
}

pub(crate) fn decide_manual_power_tick(
    state: &AppState,
    next_tick: SimulationTick,
) -> Result<Option<ManualPowerTickPlan>, ManualPowerTickError> {
    let Some(PlayerWork::ManualPower { work }) = state.player_work().active() else {
        return Ok(None);
    };
    if work.completes_at() != next_tick {
        return Ok(None);
    }
    state
        .energy()
        .revision()
        .checked_add(1)
        .ok_or(ManualPowerTickError::EnergyRevisionExhausted)?;
    state
        .equipment()
        .revision()
        .checked_add(1)
        .ok_or(ManualPowerTickError::EquipmentRevisionExhausted)?;
    let stored_before = state
        .energy()
        .get_store(work.destination())
        .unwrap_or_else(|| panic!("runtime invariant broken: manual power destination disappeared"))
        .stored();
    Ok(Some(ManualPowerTickPlan {
        work,
        stored_before,
    }))
}

pub(crate) fn apply_manual_power_tick(
    state: &mut AppState,
    plan: Option<ManualPowerTickPlan>,
) -> Option<ManualPowerOutcome> {
    let plan = plan?;
    let work = plan.work;
    let equipment = state
        .equipment()
        .get_equipment(work.equipment())
        .unwrap_or_else(|| panic!("runtime invariant broken: manual power equipment disappeared"));
    assert_eq!(
        equipment.condition(),
        work.equipment_trace().condition(),
        "manual power occupancy must prevent equipment condition mutation while work is active"
    );
    assert_eq!(
        state
            .energy()
            .get_store(work.destination())
            .map(EnergyStoreRecord::stored),
        Some(plan.stored_before),
        "manual power occupancy must prevent destination mutation while work is active"
    );

    let energy_revision = state.energy().revision();
    let next_energy_revision = energy_revision
        .checked_add(1)
        .unwrap_or_else(|| panic!("prevalidated manual power energy revision exhausted"));
    apply_released_energy_outcomes(
        state.energy_state_mut(),
        energy_revision,
        next_energy_revision,
        &[work.output()],
    );

    let equipment_revision = state.equipment().revision();
    let next_equipment_revision = equipment_revision
        .checked_add(1)
        .unwrap_or_else(|| panic!("prevalidated manual power equipment revision exhausted"));
    state.equipment_state_mut().apply_condition_change(
        work.equipment(),
        work.equipment_trace().condition(),
        work.condition_after(),
        next_equipment_revision,
    );

    Some(ManualPowerOutcome {
        method: work.method(),
        equipment: work.equipment(),
        destination: work.destination(),
        energy: work.output().energy(),
    })
}

#[cfg(test)]
#[path = "power_execution_tests.rs"]
mod tests;
