//! Canonical mining start, work completion, and reserved-output claim transactions.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityId, CapabilityValueKind};
use crate::core::quantity::{Mass, Pressure};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::equipment::{
    EquipmentId, EquipmentOperationTrace, EquipmentProviderError, resolve_equipment_provider,
};
use crate::geology::GeologicalDepositId;
use crate::inventory::{
    InboundReservationError, StockpileId, StockpileStorageError, StockpileStructuralLoadError,
    ValidatedInboundReservation, validate_inbound_reservation, validate_stockpile_storage,
    validate_stockpile_support_for_new_inbound,
};
use crate::labor::{
    PlayerWork, PlayerWorkCommitError, PlayerWorkStartError, ValidatedPlayerWorkStart,
    validate_player_work_start,
};
use crate::maintenance::{ActiveConditionDurationError, Condition};
use crate::material::{MaterialLotSpec, MaterialLotSpecError};
use crate::ore_processing::MassFlowDurationError;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::registry::Registries;

use super::physics::{MiningPhysicsError, resolve_mining_physics};
use super::state::{MiningJobIdentity, MiningJobResources, MiningJobSchedule};
use super::{
    MiningJobId, MiningJobRecord, MiningMethodDefinition, MiningMethodId, MiningTargetResolution,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MiningStartError {
    UnknownMethod {
        method: MiningMethodId,
    },
    TargetNoLongerResolved,
    ZeroMass,
    InsufficientTargetMass {
        requested: Mass,
    },
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
    MissingCapability {
        capability: CapabilityId,
    },
    CapabilityKindMismatch {
        capability: CapabilityId,
        expected: CapabilityValueKind,
        found: CapabilityValueKind,
    },
    BatchTooLarge {
        maximum: Mass,
        requested: Mass,
    },
    TargetTooHard {
        maximum: Pressure,
    },
    ZeroThroughput,
    Duration(MassFlowDurationError),
    ConditionDuration(ActiveConditionDurationError),
    CompletionTickOverflow,
    InvalidOutput(MaterialLotSpecError),
    UnknownDestination {
        stockpile: StockpileId,
    },
    DestinationStorage(StockpileStorageError),
    DestinationMassOverflow {
        stockpile: StockpileId,
    },
    DestinationCapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed: Mass,
        requested: Mass,
    },
    InventoryRevisionExhausted,
    DestinationSupport(StockpileStructuralLoadError),
    MiningIdExhausted,
    MiningRevisionExhausted,
    Work(PlayerWorkStartError),
}

impl Display for MiningStartError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMethod { method } => {
                write!(formatter, "unknown mining method {}", method.value())
            }
            Self::TargetNoLongerResolved => formatter.write_str(
                "resolved mining target is no longer uniquely supported by current local evidence and geology",
            ),
            Self::ZeroMass => formatter.write_str("mining request mass must be nonzero"),
            Self::InsufficientTargetMass { requested } => write!(
                formatter,
                "resolved mining target cannot supply the requested {} mg",
                requested.milligrams()
            ),
            Self::Equipment(error) => write!(formatter, "mining equipment failed: {error}"),
            Self::EquipmentMounted { equipment } => write!(
                formatter,
                "mining equipment {} is mounted and cannot be used for extraction",
                equipment.value()
            ),
            Self::EquipmentBusyProduction {
                equipment,
                job,
                release,
            } => write!(
                formatter,
                "mining equipment {} is occupied by production job {} {release}",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "mining equipment {} is occupied by mining job {}",
                equipment.value(),
                job.value()
            ),
            Self::MissingCapability { capability } => write!(
                formatter,
                "mining equipment lacks required capability {}",
                capability.value()
            ),
            Self::CapabilityKindMismatch {
                capability,
                expected,
                found,
            } => write!(
                formatter,
                "mining capability {} has {found:?} value kind instead of {expected:?}",
                capability.value()
            ),
            Self::BatchTooLarge { maximum, requested } => write!(
                formatter,
                "mining batch {} mg exceeds equipment maximum {} mg",
                requested.milligrams(),
                maximum.milligrams()
            ),
            Self::TargetTooHard { maximum } => write!(
                formatter,
                "resolved mining target exceeds equipment maximum excavation hardness {} Pa",
                maximum.pascals()
            ),
            Self::ZeroThroughput => formatter.write_str("resolved mining throughput is zero"),
            Self::Duration(error) => {
                write!(formatter, "mining duration resolution failed: {error}")
            }
            Self::ConditionDuration(error) => {
                write!(
                    formatter,
                    "mining exceeds equipment condition lifetime: {error}"
                )
            }
            Self::CompletionTickOverflow => {
                formatter.write_str("mining completion exceeds the world clock range")
            }
            Self::InvalidOutput(error) => write!(formatter, "mining output is invalid: {error}"),
            Self::UnknownDestination { stockpile } => write!(
                formatter,
                "unknown mining destination stockpile {}",
                stockpile.value()
            ),
            Self::DestinationStorage(error) => {
                write!(formatter, "mining destination rejects output: {error}")
            }
            Self::DestinationMassOverflow { stockpile } => write!(
                formatter,
                "mining output mass overflows destination stockpile {}",
                stockpile.value()
            ),
            Self::DestinationCapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "stockpile {} capacity {} mg cannot reserve {} mg with {} mg already committed",
                stockpile.value(),
                capacity.milligrams(),
                requested.milligrams(),
                committed.milligrams()
            ),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::DestinationSupport(error) => {
                write!(formatter, "mining destination support failed: {error}")
            }
            Self::MiningIdExhausted => {
                formatter.write_str("mining job identifier space is exhausted")
            }
            Self::MiningRevisionExhausted => {
                formatter.write_str("mining revision space is exhausted")
            }
            Self::Work(error) => write!(formatter, "mining player-work admission failed: {error}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MiningTargetPlan {
    deposit: GeologicalDepositId,
    excavation_hardness: Pressure,
    deposit_mass_before: Mass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MiningEquipmentPlan {
    duration: TickSpan,
    condition_after: Condition,
    trace: EquipmentOperationTrace,
}

struct MiningDestinationPlan {
    reservation: ValidatedInboundReservation,
    expected_structure_revision: Option<u64>,
}

fn validate_mining_target(
    state: &AppState,
    target: MiningTargetResolution,
    mass: Mass,
) -> Result<MiningTargetPlan, MiningStartError> {
    if !target.still_resolves(state) {
        return Err(MiningStartError::TargetNoLongerResolved);
    }
    let deposit = target.deposit;
    let record = state
        .geology()
        .get_deposit(deposit)
        .unwrap_or_else(|| panic!("re-resolved mining target deposit disappeared"));
    if mass > record.remaining_mass() {
        return Err(MiningStartError::InsufficientTargetMass { requested: mass });
    }
    Ok(MiningTargetPlan {
        deposit,
        excavation_hardness: record.excavation_hardness(),
        deposit_mass_before: record.remaining_mass(),
    })
}

fn resolve_mining_equipment_plan(
    registries: &Registries,
    state: &AppState,
    method: &MiningMethodDefinition,
    equipment: EquipmentId,
    excavation_hardness: Pressure,
    mass: Mass,
) -> Result<MiningEquipmentPlan, MiningStartError> {
    let provider = resolve_equipment_provider(registries, state, equipment)
        .map_err(MiningStartError::Equipment)?;
    if state
        .equipment()
        .get_equipment(equipment)
        .is_some_and(|record| record.supported_by().is_some())
    {
        return Err(MiningStartError::EquipmentMounted { equipment });
    }
    if let Some(job) = state.production().get_equipment_occupant(equipment) {
        return Err(MiningStartError::EquipmentBusyProduction {
            equipment,
            job: job.id(),
            release: job.occupancy_release(),
        });
    }
    if let Some(job) = state.mining().get_equipment_occupant(equipment) {
        return Err(MiningStartError::EquipmentBusyMining { equipment, job });
    }
    let physics = resolve_mining_physics(
        registries,
        method,
        provider.definition(),
        provider.condition(),
        excavation_hardness,
        mass,
    )
    .map_err(MiningStartError::from)?;
    Ok(MiningEquipmentPlan {
        duration: physics.duration(),
        condition_after: physics.condition_after(),
        trace: provider.validated_use().trace(),
    })
}

fn resolve_mining_output(
    state: &AppState,
    target: MiningTargetPlan,
    mass: Mass,
) -> Result<MaterialLotSpec, MiningStartError> {
    let record = state
        .geology()
        .get_deposit(target.deposit)
        .unwrap_or_else(|| panic!("validated mining target deposit disappeared"));
    MaterialLotSpec::with_composition(
        record.commodity(),
        mass,
        record.temperature(),
        record.composition().clone(),
    )
    .map_err(MiningStartError::InvalidOutput)
}

fn map_inbound_reservation_error(error: InboundReservationError) -> MiningStartError {
    match error {
        InboundReservationError::UnknownStockpile { stockpile } => {
            MiningStartError::UnknownDestination { stockpile }
        }
        InboundReservationError::MassOverflow { stockpile } => {
            MiningStartError::DestinationMassOverflow { stockpile }
        }
        InboundReservationError::CapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        } => MiningStartError::DestinationCapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        },
        InboundReservationError::RevisionExhausted => MiningStartError::InventoryRevisionExhausted,
    }
}

fn validate_mining_destination(
    registries: &Registries,
    state: &AppState,
    destination: StockpileId,
    output: &MaterialLotSpec,
    mass: Mass,
) -> Result<MiningDestinationPlan, MiningStartError> {
    let destination_record = state.inventory().get_stockpile(destination).ok_or(
        MiningStartError::UnknownDestination {
            stockpile: destination,
        },
    )?;
    validate_stockpile_storage(
        registries,
        destination_record,
        destination,
        output.commodity(),
        output.composition(),
        output.temperature(),
        output.particle_size_distribution(),
    )
    .map_err(MiningStartError::DestinationStorage)?;
    let expected_structure_revision =
        validate_stockpile_support_for_new_inbound(state, destination)
            .map_err(MiningStartError::DestinationSupport)?;
    let reservation = validate_inbound_reservation(state.inventory(), destination, mass)
        .map_err(map_inbound_reservation_error)?;
    Ok(MiningDestinationPlan {
        reservation,
        expected_structure_revision,
    })
}

impl Error for MiningStartError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Equipment(error) => Some(error),
            Self::Duration(error) => Some(error),
            Self::ConditionDuration(error) => Some(error),
            Self::InvalidOutput(error) => Some(error),
            Self::DestinationStorage(error) => Some(error),
            Self::DestinationSupport(error) => Some(error),
            Self::Work(error) => Some(error),
            Self::UnknownMethod { .. }
            | Self::TargetNoLongerResolved
            | Self::ZeroMass
            | Self::InsufficientTargetMass { .. }
            | Self::EquipmentMounted { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::MissingCapability { .. }
            | Self::CapabilityKindMismatch { .. }
            | Self::BatchTooLarge { .. }
            | Self::TargetTooHard { .. }
            | Self::ZeroThroughput
            | Self::CompletionTickOverflow
            | Self::UnknownDestination { .. }
            | Self::DestinationMassOverflow { .. }
            | Self::DestinationCapacityExceeded { .. }
            | Self::InventoryRevisionExhausted
            | Self::MiningIdExhausted
            | Self::MiningRevisionExhausted => None,
        }
    }
}

impl From<MiningPhysicsError> for MiningStartError {
    fn from(error: MiningPhysicsError) -> Self {
        match error {
            MiningPhysicsError::MissingCapability { capability } => {
                Self::MissingCapability { capability }
            }
            MiningPhysicsError::CapabilityKindMismatch {
                capability,
                expected,
                found,
            } => Self::CapabilityKindMismatch {
                capability,
                expected,
                found,
            },
            MiningPhysicsError::BatchTooLarge { maximum, requested } => {
                Self::BatchTooLarge { maximum, requested }
            }
            MiningPhysicsError::DepositTooHard {
                hardness: _hardness,
                maximum,
            } => Self::TargetTooHard { maximum },
            MiningPhysicsError::ZeroThroughput => Self::ZeroThroughput,
            MiningPhysicsError::Duration(error) => Self::Duration(error),
            MiningPhysicsError::ConditionDuration(error) => Self::ConditionDuration(error),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MiningStartCommitError {
    TargetNoLongerResolved,
    TargetMassChanged {
        expected: Mass,
        actual: Mass,
    },
    StaleInventory {
        expected: u64,
        actual: u64,
    },
    StaleEquipment {
        expected: u64,
        actual: u64,
    },
    StaleMining {
        expected: u64,
        actual: u64,
    },
    StaleStructure {
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
    Work(PlayerWorkCommitError),
}

impl Display for MiningStartCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TargetNoLongerResolved => formatter.write_str(
                "validated mining target is no longer uniquely supported by current local evidence and geology",
            ),
            Self::TargetMassChanged { expected, actual } => write!(
                formatter,
                "validated mining target source mass changed from {} mg to {} mg before commit",
                expected.milligrams(),
                actual.milligrams()
            ),
            Self::StaleInventory { expected, actual } => write!(
                formatter,
                "validated mining start expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleEquipment { expected, actual } => write!(
                formatter,
                "validated mining start expected equipment revision {expected} but current revision is {actual}"
            ),
            Self::StaleMining { expected, actual } => write!(
                formatter,
                "validated mining start expected mining revision {expected} but current revision is {actual}"
            ),
            Self::StaleStructure { expected, actual } => write!(
                formatter,
                "validated mining start expected structural revision {expected} but current revision is {actual}"
            ),
            Self::EquipmentBusyProduction { equipment, job } => write!(
                formatter,
                "validated mining start equipment {} became occupied by production job {}",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "validated mining start equipment {} became occupied by mining job {}",
                equipment.value(),
                job.value()
            ),
            Self::Work(error) => write!(
                formatter,
                "validated mining start player-work state changed: {error}"
            ),
        }
    }
}

impl Error for MiningStartCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Work(error) => Some(error),
            Self::TargetNoLongerResolved
            | Self::TargetMassChanged { .. }
            | Self::StaleInventory { .. }
            | Self::StaleEquipment { .. }
            | Self::StaleMining { .. }
            | Self::StaleStructure { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. } => None,
        }
    }
}

#[must_use]
pub struct ValidatedMiningStart {
    target: MiningTargetResolution,
    revisions: MiningStartRevisions,
    next_mining_job_id: u64,
    reservation: ValidatedInboundReservation,
    work: ValidatedPlayerWorkStart,
    record: MiningJobRecord,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RevisionTransition {
    expected: u64,
    next: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MiningStartRevisions {
    equipment: u64,
    mining: RevisionTransition,
    structure: Option<u64>,
}

impl ValidatedMiningStart {
    fn precheck_target(&self, state: &AppState) -> Result<(), MiningStartCommitError> {
        if !self.target.still_resolves(state) {
            return Err(MiningStartCommitError::TargetNoLongerResolved);
        }
        let record = state
            .geology()
            .get_deposit(self.record.deposit())
            .unwrap_or_else(|| panic!("re-resolved mining target deposit disappeared"));
        if record.remaining_mass() != self.record.deposit_mass_before() {
            return Err(MiningStartCommitError::TargetMassChanged {
                expected: self.record.deposit_mass_before(),
                actual: record.remaining_mass(),
            });
        }
        Ok(())
    }

    fn precheck_owner_revisions(&self, state: &AppState) -> Result<(), MiningStartCommitError> {
        if state.inventory().revision() != self.reservation.expected_revision() {
            return Err(MiningStartCommitError::StaleInventory {
                expected: self.reservation.expected_revision(),
                actual: state.inventory().revision(),
            });
        }
        if state.equipment().revision() != self.revisions.equipment {
            return Err(MiningStartCommitError::StaleEquipment {
                expected: self.revisions.equipment,
                actual: state.equipment().revision(),
            });
        }
        if state.mining().revision() != self.revisions.mining.expected {
            return Err(MiningStartCommitError::StaleMining {
                expected: self.revisions.mining.expected,
                actual: state.mining().revision(),
            });
        }
        if let Some(expected) = self.revisions.structure
            && state.structures().revision() != expected
        {
            return Err(MiningStartCommitError::StaleStructure {
                expected,
                actual: state.structures().revision(),
            });
        }
        Ok(())
    }

    fn precheck_equipment_occupancy(&self, state: &AppState) -> Result<(), MiningStartCommitError> {
        if let Some(job) = state
            .production()
            .get_equipment_occupant(self.record.equipment())
        {
            return Err(MiningStartCommitError::EquipmentBusyProduction {
                equipment: self.record.equipment(),
                job: job.id(),
            });
        }
        if let Some(job) = state
            .mining()
            .get_equipment_occupant(self.record.equipment())
        {
            return Err(MiningStartCommitError::EquipmentBusyMining {
                equipment: self.record.equipment(),
                job,
            });
        }
        Ok(())
    }

    pub fn commit(self, state: &mut AppState) -> Result<MiningJobId, MiningStartCommitError> {
        self.work
            .precheck(state)
            .map_err(MiningStartCommitError::Work)?;
        self.precheck_target(state)?;
        self.precheck_owner_revisions(state)?;
        self.precheck_equipment_occupancy(state)?;
        let id = self.record.id();
        self.reservation.apply(state.inventory_state_mut());
        state.mining_state_mut().insert_job(
            self.record,
            self.next_mining_job_id,
            self.revisions.mining.next,
        );
        self.work.apply(state);
        Ok(id)
    }
}

/// Resolves one finite geological slice against a real hand tool and reserves its eventual output.
pub fn validate_start_mining(
    registries: &Registries,
    state: &AppState,
    method: MiningMethodId,
    target: MiningTargetResolution,
    destination: StockpileId,
    equipment: EquipmentId,
    mass: Mass,
) -> Result<ValidatedMiningStart, MiningStartError> {
    if mass.is_zero() {
        return Err(MiningStartError::ZeroMass);
    }
    let method_definition = registries
        .mining()
        .get_method(method)
        .ok_or(MiningStartError::UnknownMethod { method })?;
    let target_plan = validate_mining_target(state, target, mass)?;
    let equipment_plan = resolve_mining_equipment_plan(
        registries,
        state,
        method_definition,
        equipment,
        target_plan.excavation_hardness,
        mass,
    )?;
    let completes_at = state
        .tick()
        .checked_add_span(equipment_plan.duration)
        .ok_or(MiningStartError::CompletionTickOverflow)?;
    let output = resolve_mining_output(state, target_plan, mass)?;
    let destination_plan =
        validate_mining_destination(registries, state, destination, &output, mass)?;

    let expected_equipment_revision = state.equipment().revision();
    let expected_mining_revision = state.mining().revision();
    let next_mining_revision = expected_mining_revision
        .checked_add(1)
        .ok_or(MiningStartError::MiningRevisionExhausted)?;
    let job_value = state.mining().next_job_id();
    let next_mining_job_id = job_value
        .checked_add(1)
        .ok_or(MiningStartError::MiningIdExhausted)?;
    let job = MiningJobId::new(job_value);
    let work = validate_player_work_start(
        registries,
        state,
        PlayerWork::Mining { job },
        equipment_plan.duration,
        method_definition.exertion(),
    )
    .map_err(MiningStartError::Work)?;

    Ok(ValidatedMiningStart {
        target,
        revisions: MiningStartRevisions {
            equipment: expected_equipment_revision,
            mining: RevisionTransition {
                expected: expected_mining_revision,
                next: next_mining_revision,
            },
            structure: destination_plan.expected_structure_revision,
        },
        next_mining_job_id,
        reservation: destination_plan.reservation,
        work,
        record: MiningJobRecord::new(
            MiningJobIdentity {
                id: job,
                method,
                deposit: target_plan.deposit,
            },
            MiningJobResources {
                destination,
                equipment_trace: equipment_plan.trace,
                deposit_mass_before: target_plan.deposit_mass_before,
                output,
                equipment_condition_after: equipment_plan.condition_after,
            },
            MiningJobSchedule {
                started_at: state.tick(),
                completes_at,
                phase: super::state::MiningJobPhase::Working,
            },
        ),
    })
}

mod claim;
mod tick;

pub use claim::{
    MiningClaimCommitError, MiningClaimError, ValidatedMiningClaim, validate_claim_mining_output,
};
pub(crate) use tick::{MiningTickError, apply_mining_tick, decide_mining_tick};

#[cfg(test)]
#[path = "execution_tests.rs"]
mod tests;
