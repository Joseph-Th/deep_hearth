//! Mining start admission and revision-bound commitment.

use crate::core::quantity::{Mass, Pressure};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::equipment::{
    EquipmentId, EquipmentOccupancy, EquipmentOperationTrace,
    resolve_equipment_provider_with_occupancy,
};
use crate::geology::GeologicalDepositId;
use crate::inventory::{
    InboundReservationError, StockpileId, ValidatedInboundReservation,
    validate_inbound_reservation, validate_stockpile_storage,
    validate_stockpile_support_for_new_inbound,
};
use crate::labor::{PlayerWork, validate_player_work_start};
use crate::logistics::{validate_player_equipment_access, validate_player_stockpile_access};
use crate::maintenance::Condition;
use crate::material::MaterialLotSpec;
use crate::registry::Registries;
use crate::spatial::VoxelBounds;

use super::errors::MiningStartError;
use crate::mining::physics::{MiningPhysicsError, resolve_mining_physics};
use crate::mining::state::{MiningJobIdentity, MiningJobResources, MiningJobSchedule};
use crate::mining::{
    MiningJobId, MiningJobRecord, MiningMethodDefinition, MiningMethodId, MiningTargetRequest,
    MiningTargetResolution, resolve_mining_target,
};

mod commit;

pub use commit::ValidatedMiningStart;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MiningTargetPlan {
    deposit: GeologicalDepositId,
    bounds: VoxelBounds,
    excavation_hardness: Pressure,
    hardness_is_acquired: bool,
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
) -> Result<MiningTargetPlan, MiningStartError> {
    let current = resolve_mining_target(
        state,
        MiningTargetRequest::new(target.region(), target.material()),
    )
    .map_err(|_| MiningStartError::TargetNoLongerResolved)?;
    if !current.has_same_authorization_binding(target) {
        return Err(MiningStartError::TargetNoLongerResolved);
    }
    let deposit = target.deposit;
    let record = state
        .geology()
        .get_deposit(deposit)
        .unwrap_or_else(|| panic!("re-resolved mining target deposit disappeared"));
    let (excavation_hardness, hardness_is_acquired) = current
        .excavation_hardness()
        .map_or((record.excavation_hardness(), false), |estimate| {
            (estimate.upper(), true)
        });
    Ok(MiningTargetPlan {
        deposit,
        bounds: record.bounds(),
        excavation_hardness,
        hardness_is_acquired,
        deposit_mass_before: record.remaining_mass(),
    })
}

fn resolve_mining_equipment_plan(
    registries: &Registries,
    state: &AppState,
    method: &MiningMethodDefinition,
    equipment: EquipmentId,
    excavation_hardness: Pressure,
    hardness_is_acquired: bool,
    mass: Mass,
) -> Result<MiningEquipmentPlan, MiningStartError> {
    let (provider, occupancy) =
        resolve_equipment_provider_with_occupancy(registries, state, equipment)
            .map_err(MiningStartError::Equipment)?;
    if state
        .equipment()
        .get_equipment(equipment)
        .is_some_and(|record| record.supported_by().is_some())
    {
        return Err(MiningStartError::EquipmentMounted { equipment });
    }
    validate_player_equipment_access(state, equipment)
        .map_err(MiningStartError::EquipmentAccess)?;
    match occupancy {
        Some(EquipmentOccupancy::Production { job, release }) => {
            return Err(MiningStartError::EquipmentBusyProduction {
                equipment,
                job,
                release,
            });
        }
        Some(EquipmentOccupancy::Mining { job }) => {
            return Err(MiningStartError::EquipmentBusyMining { equipment, job });
        }
        Some(EquipmentOccupancy::ManualPower { .. }) => {
            return Err(MiningStartError::EquipmentBusyManualPower { equipment });
        }
        Some(EquipmentOccupancy::Prospecting { .. } | EquipmentOccupancy::Maintenance { .. })
        | None => {}
    }
    let physics = resolve_mining_physics(
        registries.core().physical_tick_duration(),
        method,
        provider.definition(),
        provider.condition(),
        excavation_hardness,
        mass,
    )
    .map_err(|error| match error {
        MiningPhysicsError::DepositTooHard { maximum, .. } if !hardness_is_acquired => {
            MiningStartError::TargetResistsEquipment { maximum }
        }
        error @ MiningPhysicsError::MissingCapability { .. }
        | error @ MiningPhysicsError::CapabilityKindMismatch { .. }
        | error @ MiningPhysicsError::BatchTooLarge { .. }
        | error @ MiningPhysicsError::DepositTooHard { .. }
        | error @ MiningPhysicsError::ZeroThroughput
        | error @ MiningPhysicsError::Duration(_)
        | error @ MiningPhysicsError::ConditionDuration(_) => MiningStartError::from(error),
    })?;
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
    requested_mass: Mass,
    output_mass: Mass,
) -> Result<MiningDestinationPlan, MiningStartError> {
    validate_player_stockpile_access(state, destination)
        .map_err(MiningStartError::DestinationAccess)?;
    let destination_record = state.inventory().get_stockpile(destination).ok_or(
        MiningStartError::UnknownDestination {
            stockpile: destination,
        },
    )?;
    if state
        .player_work()
        .get_storage_dismantling_stockpile_occupant(destination)
        .is_some_and(|work| work.target() == destination)
    {
        return Err(MiningStartError::DestinationBusyStorageDismantling {
            stockpile: destination,
        });
    }
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
    let requested_reservation =
        validate_inbound_reservation(state.inventory(), destination, requested_mass)
            .map_err(map_inbound_reservation_error)?;
    let reservation = if requested_mass == output_mass {
        requested_reservation
    } else {
        validate_inbound_reservation(state.inventory(), destination, output_mass)
            .map_err(map_inbound_reservation_error)?
    };
    Ok(MiningDestinationPlan {
        reservation,
        expected_structure_revision,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RevisionTransition {
    expected: u64,
    next: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MiningStartRevisions {
    equipment: u64,
    logistics: u64,
    mining: RevisionTransition,
    structure: Option<u64>,
}

fn validate_mining_revision_capacity(
    state: &AppState,
    equipment_plan: MiningEquipmentPlan,
    destination_plan: &MiningDestinationPlan,
) -> Result<MiningStartRevisions, MiningStartError> {
    let expected_equipment_revision = state.equipment().revision();
    if equipment_plan.condition_after != equipment_plan.trace.condition()
        && !state.can_spend_equipment_revisions(1)
    {
        return Err(MiningStartError::EquipmentRevisionExhausted);
    }
    // Admission consumes one inventory revision now and creates one durable output claim that must
    // remain landable later. Preserve both that claim and all previously admitted future work.
    if !state.has_material_lot_id_headroom_from(state.inventory().next_lot_id(), 1) {
        return Err(MiningStartError::MaterialLotIdExhausted);
    }
    if !state.can_spend_inventory_revisions(2) {
        return Err(MiningStartError::InventoryRevisionExhausted);
    }
    let future_structure_steps = u64::from(destination_plan.expected_structure_revision.is_some());
    if !state.can_spend_structure_revisions(future_structure_steps) {
        return Err(MiningStartError::StructureRevisionExhausted);
    }
    state
        .geology()
        .revision()
        .checked_add(1)
        .ok_or(MiningStartError::GeologyRevisionExhausted)?;
    let expected_mining_revision = state.mining().revision();
    // Admission, due transition, and eventual claim retirement each consume one mining revision.
    // Include revisions already owed to retained mining jobs so a new extraction cannot strand
    // older ready output or its own future claim.
    if !state.can_spend_mining_revisions(3) {
        return Err(MiningStartError::MiningRevisionExhausted);
    }
    let next_mining_revision = expected_mining_revision
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("mining revision headroom includes admission"));
    Ok(MiningStartRevisions {
        equipment: expected_equipment_revision,
        logistics: state.logistics().revision(),
        mining: RevisionTransition {
            expected: expected_mining_revision,
            next: next_mining_revision,
        },
        structure: destination_plan.expected_structure_revision,
    })
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
    let target_plan = validate_mining_target(state, target)?;
    if let Some(player) = state.logistics().player().copied()
        && !target_plan.bounds.has_voxel(player.position())
    {
        return Err(MiningStartError::PlayerOutsideDeposit {
            player_position: player.position(),
            deposit: target_plan.deposit,
            bounds: target_plan.bounds,
        });
    }
    let equipment_plan = resolve_mining_equipment_plan(
        registries,
        state,
        method_definition,
        equipment,
        target_plan.excavation_hardness,
        target_plan.hardness_is_acquired,
        mass,
    )?;
    let completes_at = state
        .tick()
        .checked_add_span(equipment_plan.duration)
        .ok_or(MiningStartError::CompletionTickOverflow)?;
    let output_mass = Mass::from_milligrams(
        mass.milligrams()
            .min(target_plan.deposit_mass_before.milligrams()),
    );
    let output = resolve_mining_output(state, target_plan, output_mass)?;
    let destination_plan =
        validate_mining_destination(registries, state, destination, &output, mass, output_mass)?;
    let revisions = validate_mining_revision_capacity(state, equipment_plan, &destination_plan)?;
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

    Ok(ValidatedMiningStart::new(
        target,
        revisions,
        next_mining_job_id,
        destination_plan.reservation,
        work,
        MiningJobRecord::new(
            MiningJobIdentity {
                id: job,
                method,
                deposit: target_plan.deposit,
            },
            MiningJobResources {
                destination,
                equipment_trace: equipment_plan.trace,
                deposit_mass_before: target_plan.deposit_mass_before,
                requested_mass: mass,
                output,
                equipment_condition_after: equipment_plan.condition_after,
            },
            MiningJobSchedule {
                started_at: state.tick(),
                completes_at,
                phase: crate::mining::state::MiningJobPhase::Working,
            },
        ),
    ))
}
