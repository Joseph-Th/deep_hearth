//! Canonical mining start, work completion, and reserved-output claim transactions.

use crate::core::quantity::{Mass, Pressure};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::equipment::{EquipmentId, EquipmentOperationTrace, resolve_equipment_provider};
use crate::geology::GeologicalDepositId;
use crate::inventory::{
    InboundReservationError, StockpileId, ValidatedInboundReservation,
    validate_inbound_reservation, validate_stockpile_storage,
    validate_stockpile_support_for_new_inbound,
};
use crate::labor::{PlayerWork, ValidatedPlayerWorkStart, validate_player_work_start};
use crate::maintenance::Condition;
use crate::material::MaterialLotSpec;
use crate::registry::Registries;

use super::physics::resolve_mining_physics;
use super::state::{MiningJobIdentity, MiningJobResources, MiningJobSchedule};
use super::{
    MiningJobId, MiningJobRecord, MiningMethodDefinition, MiningMethodId, MiningTargetResolution,
};

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
        self.reservation.assert_matches_state(state.inventory());
        state.mining().assert_job_insertable(
            &self.record,
            self.next_mining_job_id,
            self.revisions.mining.next,
        );
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
mod errors;
mod tick;

pub use claim::{
    MiningClaimCommitError, MiningClaimError, ValidatedMiningClaim, validate_claim_mining_output,
};
pub use errors::{MiningStartCommitError, MiningStartError};
pub(crate) use tick::{MiningTickError, apply_mining_tick, decide_mining_tick};

#[cfg(test)]
#[path = "execution_tests.rs"]
mod tests;
