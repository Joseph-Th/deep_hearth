//! Exact post-work destination-energy targeting for current manual-power configurations.

use crate::core::quantity::Energy;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualPowerDestinationTargetRequest {
    method: ManualPowerMethodId,
    equipment: EquipmentId,
    destination: EnergyStoreId,
    target: Energy,
}

impl ManualPowerDestinationTargetRequest {
    #[must_use]
    pub const fn new(
        method: ManualPowerMethodId,
        equipment: EquipmentId,
        destination: EnergyStoreId,
        target: Energy,
    ) -> Self {
        Self {
            method,
            equipment,
            destination,
            target,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManualPowerDestinationTargetBlocker {
    GenerationCapacity,
    DestinationCapacity,
    SurvivalReserve,
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualPowerDestinationTargetProjection {
    generated_energy: Energy,
    destination_energy_after: Energy,
}

impl ManualPowerDestinationTargetProjection {
    #[must_use]
    pub const fn generated_energy(self) -> Energy {
        self.generated_energy
    }

    #[must_use]
    pub const fn destination_energy_after(self) -> Energy {
        self.destination_energy_after
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManualPowerDestinationTargetAssessment {
    AlreadySatisfied { stored: Energy },
    Feasible(ManualPowerDestinationTargetProjection),
    Blocked(ManualPowerDestinationTargetBlocker),
}

enum DurationCandidate {
    GenerationInsufficient,
    DestinationBlocked,
    Ready(Energy),
}

enum CandidateAdmission {
    Feasible(ManualPowerDestinationTargetProjection),
    SurvivalBlocked,
    GenerationBlocked,
    DestinationBlocked,
}

enum TargetSearchStart {
    AlreadySatisfied(Energy),
    Search {
        first_duration: TickSpan,
        last_duration: TickSpan,
    },
    GenerationBlocked,
    DestinationBlocked,
}

#[derive(Default)]
struct TargetSearchState {
    generation_possible: bool,
    destination_blocked: bool,
    survival_blocked: bool,
}

impl TargetSearchState {
    fn blocker(&self) -> ManualPowerDestinationTargetBlocker {
        if self.survival_blocked {
            ManualPowerDestinationTargetBlocker::SurvivalReserve
        } else if self.generation_possible && self.destination_blocked {
            ManualPowerDestinationTargetBlocker::DestinationCapacity
        } else {
            ManualPowerDestinationTargetBlocker::GenerationCapacity
        }
    }
}

fn duration_candidate(
    context: &CurrentManualPowerContext<'_>,
    target: Energy,
    duration: TickSpan,
) -> DurationCandidate {
    let bucket_capacity = context.output_capacity(duration);
    let previous_capacity = duration
        .checked_sub(TickSpan::new(1))
        .map_or(Energy::ZERO, |previous| context.output_capacity(previous));
    let Some(minimum_nanojoules) = previous_capacity.nanojoules().checked_add(1) else {
        return DurationCandidate::GenerationInsufficient;
    };
    let retained = context
        .bindings
        .sink()
        .stored_after_elapsed(context.registries, duration);
    let needed = target.checked_sub(retained).unwrap_or(Energy::ZERO);
    let candidate = Energy::from_nanojoules(minimum_nanojoules.max(needed.nanojoules()));
    if candidate > bucket_capacity {
        return DurationCandidate::GenerationInsufficient;
    }
    if candidate
        > context
            .bindings
            .sink()
            .available_capacity_at_release(context.registries, duration)
    {
        return DurationCandidate::DestinationBlocked;
    }
    DurationCandidate::Ready(candidate)
}

fn admit_candidate(
    context: &CurrentManualPowerContext<'_>,
    duration: TickSpan,
    candidate: Energy,
) -> Result<CandidateAdmission, ManualPowerError> {
    match validate_start_manual_power_with_bindings(
        context.registries,
        context.state,
        ManualPowerRequest::new(
            context.method,
            context.equipment,
            context.destination,
            candidate,
        ),
        context.bindings,
    ) {
        Ok(_start) => Ok(CandidateAdmission::Feasible(
            ManualPowerDestinationTargetProjection {
                generated_energy: candidate,
                destination_energy_after: context.destination_energy_after(duration, candidate),
            },
        )),
        Err(ManualPowerError::Work(
            PlayerWorkStartError::MetabolicCostOverflow { .. }
            | PlayerWorkStartError::InsufficientMetabolicEnergy { .. }
            | PlayerWorkStartError::HydrationCostOverflow { .. }
            | PlayerWorkStartError::InsufficientHydration { .. },
        )) => Ok(CandidateAdmission::SurvivalBlocked),
        Err(ManualPowerError::EnergySink(
            EnergySinkError::CapacityOverflow { .. } | EnergySinkError::InsufficientCapacity { .. },
        )) => Ok(CandidateAdmission::DestinationBlocked),
        Err(ManualPowerError::ConditionDuration(_)) => Ok(CandidateAdmission::GenerationBlocked),
        Err(
            error @ ManualPowerError::Work(PlayerWorkStartError::SurvivalRevisionExhausted {
                ..
            }),
        )
        | Err(error @ ManualPowerError::CompletionTickOverflow { .. }) => Err(error),
        Err(error) => Err(error),
    }
}

fn minimum_duration_for_output(
    context: &CurrentManualPowerContext<'_>,
    required: Energy,
    maximum_duration: TickSpan,
) -> Option<TickSpan> {
    if required.is_zero() {
        return Some(TickSpan::ZERO);
    }
    if maximum_duration.is_zero() || context.output_capacity(maximum_duration) < required {
        return None;
    }

    let mut low = 1_u64;
    let mut high = maximum_duration.value();
    while low < high {
        let midpoint = low + (high - low) / 2;
        if context.output_capacity(TickSpan::new(midpoint)) >= required {
            high = midpoint;
        } else {
            low = midpoint + 1;
        }
    }
    Some(TickSpan::new(low))
}

fn target_search_start(
    context: &CurrentManualPowerContext<'_>,
    target: Energy,
    condition_ticks: TickSpan,
) -> TargetSearchStart {
    let current = context.current_stored();
    if current >= target {
        return TargetSearchStart::AlreadySatisfied(current);
    }
    if target > context.destination_capacity() {
        return TargetSearchStart::DestinationBlocked;
    }
    let initial_gap = target
        .checked_sub(current)
        .unwrap_or_else(|| unreachable!("target above current store has a positive gap"));
    let Some(first_duration) = minimum_duration_for_output(context, initial_gap, condition_ticks)
    else {
        return TargetSearchStart::GenerationBlocked;
    };

    // A least-energy solution cannot occur after the first duration bucket capable of generating
    // the entire target from empty. Beyond that bucket, generated energy must exceed the target
    // while retained preexisting energy can only stay equal or decrease. If condition life ends
    // first, it is already the tighter physical bound.
    let last_duration =
        minimum_duration_for_output(context, target, condition_ticks).unwrap_or(condition_ticks);
    TargetSearchStart::Search {
        first_duration,
        last_duration,
    }
}

fn search_target_duration_range(
    context: &CurrentManualPowerContext<'_>,
    target: Energy,
    first_duration: TickSpan,
    last_duration: TickSpan,
) -> Result<ManualPowerDestinationTargetAssessment, ManualPowerError> {
    let mut search = TargetSearchState::default();
    for ticks in first_duration.value()..=last_duration.value() {
        let duration = TickSpan::new(ticks);
        let candidate = match duration_candidate(context, target, duration) {
            DurationCandidate::GenerationInsufficient => continue,
            DurationCandidate::DestinationBlocked => {
                search.generation_possible = true;
                search.destination_blocked = true;
                continue;
            }
            DurationCandidate::Ready(candidate) => candidate,
        };
        search.generation_possible = true;
        match admit_candidate(context, duration, candidate)? {
            CandidateAdmission::Feasible(projection) => {
                return Ok(ManualPowerDestinationTargetAssessment::Feasible(projection));
            }
            CandidateAdmission::SurvivalBlocked => search.survival_blocked = true,
            CandidateAdmission::GenerationBlocked => {}
            CandidateAdmission::DestinationBlocked => search.destination_blocked = true,
        }
    }
    Ok(ManualPowerDestinationTargetAssessment::Blocked(
        search.blocker(),
    ))
}

fn assess_target(
    context: &CurrentManualPowerContext<'_>,
    target: Energy,
    condition_ticks: TickSpan,
) -> Result<ManualPowerDestinationTargetAssessment, ManualPowerError> {
    match target_search_start(context, target, condition_ticks) {
        TargetSearchStart::AlreadySatisfied(stored) => {
            Ok(ManualPowerDestinationTargetAssessment::AlreadySatisfied { stored })
        }
        TargetSearchStart::GenerationBlocked => {
            Ok(ManualPowerDestinationTargetAssessment::Blocked(
                ManualPowerDestinationTargetBlocker::GenerationCapacity,
            ))
        }
        TargetSearchStart::DestinationBlocked => {
            Ok(ManualPowerDestinationTargetAssessment::Blocked(
                ManualPowerDestinationTargetBlocker::DestinationCapacity,
            ))
        }
        TargetSearchStart::Search {
            first_duration,
            last_duration,
        } => search_target_duration_range(context, target, first_duration, last_duration),
    }
}

/// Assesses the least exact manual-power charge that reaches a requested post-work store level.
pub fn assess_manual_power_destination_target(
    registries: &Registries,
    state: &AppState,
    request: ManualPowerDestinationTargetRequest,
) -> Result<ManualPowerDestinationTargetAssessment, ManualPowerError> {
    let bindings = resolve_manual_power_bindings(
        registries,
        state,
        request.method,
        request.equipment,
        request.destination,
    )?;
    let context = CurrentManualPowerContext {
        registries,
        state,
        method: request.method,
        equipment: request.equipment,
        destination: request.destination,
        bindings,
    };
    let definition = bindings.definition();
    let condition_ticks = maximum_usable_active_ticks(
        definition.condition_wear_ppm_per_active_tick(),
        bindings.provider().condition(),
    );
    assess_target(&context, request.target, condition_ticks)
}
