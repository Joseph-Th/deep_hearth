//! Held-tool excavation method choice and amount-sensitive admission.

use crate::capability::CapabilityValue;
use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::{CONTROL_TICKS_PER_SIMULATION_TICK, TickSpan};
use crate::equipment::{EquipmentId, resolve_available_equipment_provider};
use crate::labor::PlayerWorkStartError;
use crate::logistics::{PlayerActiveSelection, validate_player_equipment_access};
use crate::mining::{MiningMethodId, MiningPhysicsError, resolve_mining_physics};
use crate::registry::Registries;
use crate::spatial::ViewDirection;

use super::{
    BlockExcavationError, BlockExcavationRequest, PlayerBlockTarget,
    ValidatedPlayerBlockExcavationStart, raycast_player_block,
    validate_start_player_block_excavation_targeted,
};

#[derive(Clone, Copy)]
struct DirectExcavationCandidate {
    method: MiningMethodId,
    mass: Mass,
    duration: TickSpan,
}

fn candidate_is_faster(
    candidate: DirectExcavationCandidate,
    current: DirectExcavationCandidate,
) -> bool {
    let candidate_rate =
        u128::from(candidate.mass.milligrams()) * u128::from(current.duration.value());
    let current_rate =
        u128::from(current.mass.milligrams()) * u128::from(candidate.duration.value());
    candidate_rate > current_rate
        || (candidate_rate == current_rate && candidate.method < current.method)
}

/// Only retry failures that can be resolved by extracting a smaller amount.
fn batch_size_can_resolve(error: &BlockExcavationError) -> bool {
    matches!(
        error,
        BlockExcavationError::Physics(
            MiningPhysicsError::Duration(_) | MiningPhysicsError::ConditionDuration(_)
        ) | BlockExcavationError::ScheduleOverflow
            | BlockExcavationError::SurfaceRevisionExhausted
            | BlockExcavationError::SurfaceResourceIdExhausted
            | BlockExcavationError::PlayerWork(
                PlayerWorkStartError::MetabolicCostOverflow { .. }
                    | PlayerWorkStartError::HydrationCostOverflow { .. }
                    | PlayerWorkStartError::InsufficientMetabolicEnergy { .. }
                    | PlayerWorkStartError::InsufficientHydration { .. }
                    | PlayerWorkStartError::SurvivalRevisionExhausted { .. }
            )
    )
}

fn largest_authorized_excavation_batch(
    registries: &Registries,
    state: &AppState,
    target: PlayerBlockTarget,
    direction: ViewDirection,
    equipment: EquipmentId,
    method: MiningMethodId,
    maximum_mass: Mass,
) -> Result<
    (
        DirectExcavationCandidate,
        ValidatedPlayerBlockExcavationStart,
    ),
    BlockExcavationError,
> {
    let mut feasible_mass = 0_u64;
    let mut remaining_ceiling = maximum_mass.milligrams();
    let mut trial_mass = remaining_ceiling;
    let mut best = None;
    let mut first_rejection = None;
    loop {
        let mass = Mass::from_milligrams(trial_mass);
        match validate_start_player_block_excavation_targeted(
            registries,
            state,
            BlockExcavationRequest::new(direction, method, equipment, mass),
            target,
        ) {
            Ok(authorized) => {
                let ticks = authorized
                    .work
                    .duration_control_ticks()
                    .unwrap_or_else(|| unreachable!("authorized excavation duration is positive"));
                let duration = TickSpan::new(ticks / CONTROL_TICKS_PER_SIMULATION_TICK);
                feasible_mass = trial_mass;
                best = Some((
                    DirectExcavationCandidate {
                        method,
                        mass,
                        duration,
                    },
                    authorized,
                ));
            }
            Err(error) => {
                let reducible = batch_size_can_resolve(&error);
                first_rejection.get_or_insert(error);
                if !reducible {
                    break;
                }
                remaining_ceiling = trial_mass - 1;
            }
        }
        if feasible_mass >= remaining_ceiling {
            break;
        }
        trial_mass =
            u64::try_from((u128::from(feasible_mass) + u128::from(remaining_ceiling)).div_ceil(2))
                .unwrap_or_else(|_| unreachable!("bounded excavation batch fits u64"));
    }
    best.ok_or_else(|| {
        first_rejection.unwrap_or_else(|| unreachable!("an unadmitted batch has a typed rejection"))
    })
}

fn retain_excavation_rejection(
    best_rejection: &mut Option<(DirectExcavationCandidate, BlockExcavationError)>,
    first_physics_failure: &mut Option<MiningPhysicsError>,
    error: BlockExcavationError,
    full_batch_duration: Result<TickSpan, MiningPhysicsError>,
    method: MiningMethodId,
    mass: Mass,
) {
    let duration = match full_batch_duration {
        Ok(duration) => duration,
        Err(error) => {
            first_physics_failure.get_or_insert(error);
            return;
        }
    };
    if let BlockExcavationError::Physics(error) = error {
        first_physics_failure.get_or_insert(error);
        return;
    }
    let candidate = DirectExcavationCandidate {
        method,
        mass,
        duration,
    };
    if best_rejection
        .as_ref()
        .is_none_or(|(current, _)| candidate_is_faster(candidate, *current))
    {
        *best_rejection = Some((candidate, error));
    }
}

/// Resolves one familiar held-tool block action entirely from the active carried equipment and
/// pointed world target. Exact method/equipment selection remains available through
/// [`BlockExcavationRequest`] for planning surfaces that intentionally expose those details.
pub(crate) fn validate_start_player_block_excavation_active(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
) -> Result<ValidatedPlayerBlockExcavationStart, BlockExcavationError> {
    let equipment = match state.logistics().active_selection() {
        Some(PlayerActiveSelection::Equipment(equipment)) => equipment,
        Some(PlayerActiveSelection::Material(commodity)) => {
            return Err(BlockExcavationError::ActiveSelectionNotEquipment { commodity });
        }
        None => return Err(BlockExcavationError::NoActiveEquipment),
    };
    let target = raycast_player_block(registries, state, direction)
        .map_err(BlockExcavationError::Target)?
        .ok_or(BlockExcavationError::NoTarget)?;
    let block = registries
        .world()
        .block_for_state(target.state())
        .unwrap_or_else(|| unreachable!("targeted block state is registered"));
    let excavation = block
        .excavation()
        .ok_or(BlockExcavationError::TargetNotExcavatable {
            target: target.voxel(),
        })?;
    let source_mass = state
        .world()
        .represented_mass_at(registries.world(), target.voxel())
        .unwrap_or_else(|| unreachable!("target ray established generated world truth"));
    validate_player_equipment_access(state, equipment)
        .map_err(BlockExcavationError::EquipmentAccess)?;
    let provider = resolve_available_equipment_provider(registries, state, equipment)
        .map_err(BlockExcavationError::Equipment)?;

    let mut best: Option<(
        DirectExcavationCandidate,
        ValidatedPlayerBlockExcavationStart,
    )> = None;
    let mut best_rejection: Option<(DirectExcavationCandidate, BlockExcavationError)> = None;
    let mut first_physics_failure = None;
    for method in registries.mining().definitions() {
        let Some(CapabilityValue::MassFlow(flow)) =
            provider.get_capability(method.mass_flow_capability())
        else {
            continue;
        };
        let Some(CapabilityValue::Mass(maximum_batch)) =
            provider.get_capability(method.max_batch_mass_capability())
        else {
            continue;
        };
        let Some(CapabilityValue::Pressure(maximum_hardness)) =
            provider.get_capability(method.max_hardness_capability())
        else {
            continue;
        };
        if flow.is_zero() || maximum_batch.is_zero() || maximum_hardness.is_zero() {
            continue;
        }
        let maximum_mass = std::cmp::min(maximum_batch, source_mass);
        // Resolve current batch feasibility without inventing a separate physical or labor
        // model. Only amount-dependent failures cause a bounded search for a smaller batch.
        match largest_authorized_excavation_batch(
            registries,
            state,
            target,
            direction,
            equipment,
            method.id(),
            maximum_mass,
        ) {
            Ok((candidate, authorized)) => {
                if best
                    .as_ref()
                    .is_none_or(|(current, _)| candidate_is_faster(candidate, *current))
                {
                    best = Some((candidate, authorized));
                }
            }
            Err(error) => {
                retain_excavation_rejection(
                    &mut best_rejection,
                    &mut first_physics_failure,
                    error,
                    resolve_mining_physics(
                        registries.core().physical_tick_duration(),
                        method,
                        provider.definition(),
                        provider.condition(),
                        excavation.hardness(),
                        maximum_mass,
                    )
                    .map(|physics| physics.duration()),
                    method.id(),
                    maximum_mass,
                );
            }
        }
    }
    if let Some((_, authorized)) = best {
        return Ok(authorized);
    }
    if let Some((_, error)) = best_rejection {
        return Err(error);
    }
    if let Some(error) = first_physics_failure {
        return Err(BlockExcavationError::Physics(error));
    }
    Err(BlockExcavationError::NoApplicableMethod { equipment })
}
