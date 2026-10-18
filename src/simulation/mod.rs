//! Canonical synchronous simulation tick pipeline with active subsystem phases wired in visible order.

mod control;
mod direct_actions;
mod error;
mod outcome;
mod planning;
mod player_death;

pub use control::{PlayerDirectControlInput, PrimaryControlIntent, SecondaryControlIntent};
pub use error::TickError;
pub use outcome::{
    ActiveSelectionDropOutcome, ControlTickOutcome, PrimaryActionOutcome,
    QuickAccessSelectionOutcome, SecondaryActionOutcome, TickOutcome,
};

use crate::core::state::{
    AppState, apply_clock_advance, apply_control_clock_advance, debug_assert_runtime_invariants,
};
use crate::core::time::SimulationTick;
use crate::energy::{apply_passive_energy_dissipation, decide_passive_energy_dissipation};
use crate::equipment::{apply_equipment_maintenance_tick, decide_equipment_maintenance_tick};
use crate::geology::{apply_field_prospecting_tick, decide_field_prospecting_tick};
use crate::inventory::apply_storage_enclosure_dismantling_tick;
use crate::labor::{
    PlayerWork, apply_manual_power_tick, apply_player_work_tick, decide_manual_power_tick,
    decide_player_work_control_tick, decide_player_work_tick, player_work_exertion,
};
use crate::logistics::{
    PlayerControlInput, PlayerOverheadCoverAssessment, PlayerStance, apply_player_body_motion,
    assess_player_overhead_cover, decide_player_body_motion, player_body_is_grounded,
    player_body_uses_generated_world,
};
use crate::mining::{apply_mining_tick, decide_mining_tick};
use crate::production::{CompletionApplication, apply_completion_plan, decide_due_completions};
use crate::registry::Registries;
use crate::structural::{apply_structural_construction_tick, apply_structural_dismantling_tick};
use crate::surface::{
    apply_automatic_surface_drop_pickup, apply_surface_gathering_tick,
    automatic_surface_pickup_requires_control_stepping, decide_automatic_surface_drop_pickup,
};
use crate::survival::{
    SurvivalExertion, ThermalExposure, apply_survival_tick, assess_survival,
    control_exertion_between, decide_control_exertion_settlement,
    decide_survival_tick_with_local_heat,
};
use crate::world::{
    BlockExcavationTickError, apply_block_excavation_tick, decide_block_excavation_completion,
    decide_block_excavation_tick,
};

use direct_actions::{
    attempt_direct_actions, attempt_direct_actions_after_quick_access,
    attempt_pending_primary_break, attempt_pending_secondary_action,
    attempt_quick_access_selection,
};
use planning::{
    decide_automatic_mining_output, decide_storage_dismantling_after_completions,
    decide_structural_construction_after_completions,
    decide_structural_dismantling_after_completions, decide_surface_gathering_after_completions,
    require_revision_capacity,
};
use player_death::{apply_player_death_effects, decide_player_death_effects};

/// Advances one canonical 20 Hz direct-control tick.
///
/// Each control tick advances the high-frequency clock and, when the admitted player's footprint
/// is backed by generated chunks, integrates authoritative body gravity/collision. Every 72nd
/// control tick also executes one complete slow-simulation quantum. Future horizontal movement,
/// direct block interaction, and combat phases belong on this surface rather than forcing slow
/// industrial/survival accounting to 20 Hz.
pub fn advance_control_tick(
    registries: &Registries,
    state: &mut AppState,
) -> Result<ControlTickOutcome, TickError> {
    advance_control_tick_with_direct_input(registries, state, PlayerDirectControlInput::NEUTRAL)
}

/// Advances one canonical 20 Hz direct-control tick with the sampled player input for this step.
pub fn advance_control_tick_with_input(
    registries: &Registries,
    state: &mut AppState,
    input: PlayerControlInput,
) -> Result<ControlTickOutcome, TickError> {
    advance_control_tick_with_direct_input(registries, state, PlayerDirectControlInput::new(input))
}

/// Advances one canonical 20 Hz control tick with locomotion and held primary-action intent.
pub fn advance_control_tick_with_direct_input(
    registries: &Registries,
    state: &mut AppState,
    input: PlayerDirectControlInput,
) -> Result<ControlTickOutcome, TickError> {
    let world_revision_before = state.world().revision();
    let current = state.control_tick();
    let next_control = current
        .checked_next()
        .ok_or(TickError::ControlClockExhausted { current })?;
    if next_control.simulation_tick() == state.tick() {
        let body_plan = decide_player_body_motion(registries, state, input.movement(), false)
            .map_err(TickError::PlayerBodyMotion)?;
        let control_excavation = match state.player_work().active() {
            Some(PlayerWork::BlockExcavation { work }) if work.completes_at() == next_control => {
                let boundary = crate::core::time::ControlTick::from_simulation_tick(state.tick())
                    .unwrap_or_else(|| unreachable!("current slow tick must map to control time"));
                let elapsed_before = boundary
                    .checked_steps_since(work.started_at())
                    .unwrap_or_else(|| unreachable!("excavation completion cannot precede start"));
                let elapsed_after = next_control
                    .checked_steps_since(work.started_at())
                    .unwrap_or_else(|| unreachable!("excavation completion cannot precede start"));
                let method = registries
                    .mining()
                    .get_method(work.method())
                    .unwrap_or_else(|| {
                        panic!("runtime invariant broken: excavation method disappeared")
                    });
                let tail_exertion =
                    control_exertion_between(method.exertion(), elapsed_before, elapsed_after)
                        .unwrap_or_else(|| panic!("excavation control-rate exertion overflowed"));
                let survival_plan =
                    decide_control_exertion_settlement(registries, state, tail_exertion).map_err(
                        |error| match error {
                            crate::survival::SurvivalTickError::RevisionExhausted => {
                                TickError::SurvivalRevisionExhausted
                            }
                            crate::survival::SurvivalTickError::EnergyCostOverflow => {
                                TickError::SurvivalEnergyCostOverflow
                            }
                            crate::survival::SurvivalTickError::HydrationCostOverflow => {
                                TickError::SurvivalHydrationCostOverflow
                            }
                        },
                    )?;
                let excavation_plan = decide_block_excavation_completion(
                    registries,
                    state,
                    state.inventory(),
                    next_control,
                    state.tick(),
                )
                .map_err(map_block_excavation_tick_error)?
                .unwrap_or_else(|| {
                    panic!("runtime invariant broken: due control-rate excavation produced no completion plan")
                });
                let player_work_plan = decide_player_work_control_tick(state, next_control)
                    .map_err(|_| TickError::PlayerWorkRevisionExhausted)?;
                Some((excavation_plan, survival_plan, player_work_plan))
            }
            _ => None,
        };
        let body_motion = apply_player_body_motion(state, body_plan);
        let block_excavation =
            if let Some((excavation_plan, survival_plan, player_work_plan)) = control_excavation {
                let outcome = apply_block_excavation_tick(registries, state, Some(excavation_plan));
                let _ = apply_survival_tick(state, survival_plan);
                apply_player_work_tick(state, player_work_plan);
                outcome
            } else {
                None
            };
        apply_control_clock_advance(state, next_control);
        let surface_drop_pickup = decide_automatic_surface_drop_pickup(registries, state)
            .map(|plan| apply_automatic_surface_drop_pickup(state, plan));
        let direct_actions = attempt_direct_actions(registries, state, input, None, None);
        debug_assert_runtime_invariants(registries, state, world_revision_before);
        return Ok(ControlTickOutcome::new(
            next_control,
            None,
            body_motion,
            surface_drop_pickup,
            block_excavation,
            direct_actions,
        ));
    }
    debug_assert!(next_control.is_simulation_boundary());
    let settlement_plan =
        decide_player_body_motion(registries, state, PlayerControlInput::NEUTRAL, true)
            .map_err(TickError::PlayerBodyMotion)?;
    let locomotion_exertion = settlement_plan
        .as_ref()
        .map_or(SurvivalExertion::REST, |plan| {
            plan.settled_locomotion_exertion()
        });
    let fall_vitality_loss_ppm = settlement_plan
        .as_ref()
        .map_or(0, |plan| plan.settled_fall_vitality_loss_ppm());
    // The slow tick may be fatal. Record buffered intent before the death transition clears it:
    // the outer control step still owes the actor an observable rejection.
    let has_pending_primary = state.logistics().pending_primary_break().is_some();
    let has_pending_secondary = state.logistics().pending_secondary_use().is_some();
    let simulation = advance_simulation_tick(
        registries,
        state,
        next_control.simulation_tick(),
        locomotion_exertion,
        fall_vitality_loss_ppm,
    )?;
    let player_dead = state
        .survival()
        .player()
        .is_some_and(|player| player.vitality() == crate::survival::Vitality::ZERO);
    let preserve_pending_context = (has_pending_primary && input.primary().is_none())
        || (has_pending_secondary && input.secondary().is_none());
    let body_input = if preserve_pending_context {
        PlayerControlInput::NEUTRAL
    } else {
        input.movement()
    };
    let body_plan = decide_player_body_motion(registries, state, body_input, true)
        .map_err(TickError::PlayerBodyMotion)?;
    let body_motion = apply_player_body_motion(state, body_plan);
    let surface_drop_pickup = decide_automatic_surface_drop_pickup(registries, state)
        .map(|plan| apply_automatic_surface_drop_pickup(state, plan));
    let quick_access = attempt_quick_access_selection(state, input.quick_access_slot());
    let pending_primary = if has_pending_primary && player_dead {
        input
            .primary()
            .is_none()
            .then_some(PrimaryActionOutcome::PlayerDead)
    } else if has_pending_primary && input.primary().is_some() {
        let _ = crate::logistics::take_player_pending_primary_break(state);
        None
    } else {
        attempt_pending_primary_break(registries, state)
    };
    let pending_secondary = if has_pending_secondary && player_dead {
        input
            .secondary()
            .is_none()
            .then_some(SecondaryActionOutcome::PlayerDead)
    } else if has_pending_secondary && input.secondary().is_some() {
        let _ = crate::logistics::take_player_pending_secondary_use(state);
        None
    } else {
        attempt_pending_secondary_action(registries, state)
    };
    let direct_actions = attempt_direct_actions_after_quick_access(
        registries,
        state,
        input,
        quick_access,
        pending_primary,
        pending_secondary,
    );
    let block_excavation = simulation.block_excavation();
    debug_assert_runtime_invariants(registries, state, world_revision_before);
    debug_assert_eq!(state.control_tick(), next_control);
    Ok(ControlTickOutcome::new(
        next_control,
        Some(simulation),
        body_motion,
        surface_drop_pickup,
        block_excavation,
        direct_actions,
    ))
}

fn map_block_excavation_tick_error(error: BlockExcavationTickError) -> TickError {
    match error {
        BlockExcavationTickError::MaterialLotId => TickError::MaterialLotIdExhausted,
        BlockExcavationTickError::InventoryRevision => TickError::InventoryRevisionExhausted,
        BlockExcavationTickError::SurfaceResourceId => TickError::SurfaceResourceIdExhausted,
        BlockExcavationTickError::SurfaceRevision => TickError::SurfaceRevisionExhausted,
        BlockExcavationTickError::WorldRevision => TickError::WorldRevisionExhausted,
        BlockExcavationTickError::EquipmentRevision => TickError::EquipmentRevisionExhausted,
    }
}

fn control_stepping_required(registries: &Registries, state: &AppState) -> bool {
    if state.logistics().has_pending_direct_action()
        || matches!(
            state.player_work().active(),
            Some(PlayerWork::BlockExcavation { .. })
        )
        || automatic_surface_pickup_requires_control_stepping(state)
    {
        return true;
    }
    let locomotion = registries.survival().physiology().locomotion();
    if !player_body_uses_generated_world(state, locomotion) {
        return false;
    }
    let player =
        state.logistics().player().copied().unwrap_or_else(|| {
            unreachable!("generated-world body requires admitted player logistics")
        });
    let body = player.body();
    let horizontal = body.horizontal_velocity();
    player.pending_fall_vitality_loss_ppm() != 0
        || body.stance() != PlayerStance::Standing
        || horizontal.x_nanometers_per_second() != 0
        || horizontal.z_nanometers_per_second() != 0
        || body.vertical_velocity().nanometers_per_second() != 0
        || !player_body_is_grounded(registries, state)
        || player.locomotion_exertion().pending() != SurvivalExertion::REST
}

/// Advances atomically to the next slow-simulation boundary when no control-rate consequence can
/// occur during the skipped neutral-control interval.
///
/// Headless/process-only states, players outside generated body truth, and a standing grounded
/// motionless generated-world player with settled locomotion may use this fast path. Buffered
/// input, unsettled fall injury, crouch/air/motion, control-rate excavation, and local drops that
/// can enter the player's pack still require exact 50 ms stepping and return
/// `TickError::ControlSteppingRequired`.
pub fn advance_tick(
    registries: &Registries,
    state: &mut AppState,
) -> Result<TickOutcome, TickError> {
    if control_stepping_required(registries, state) {
        return Err(TickError::ControlSteppingRequired {
            current: state.control_tick(),
        });
    }
    let current = state.tick();
    let next_tick = SimulationTick::new(current.value().checked_add(1).ok_or(
        TickError::ControlClockExhausted {
            current: state.control_tick(),
        },
    )?);
    if crate::core::time::ControlTick::from_simulation_tick(next_tick).is_none() {
        return Err(TickError::ControlClockExhausted {
            current: state.control_tick(),
        });
    }
    advance_simulation_tick(registries, state, next_tick, SurvivalExertion::REST, 0)
}

/// Advances the full slow simulation by one 3.6-second physical/accounting quantum.
///
/// Every authoritative subsystem phase is sequenced here. Decisions read the pre-tick state or an
/// explicit same-tick projection when an earlier completion changes a downstream admission fact,
/// then mutations apply in one deterministic order.
fn advance_simulation_tick(
    registries: &Registries,
    state: &mut AppState,
    next_tick: SimulationTick,
    locomotion_exertion: SurvivalExertion,
    fall_vitality_loss_ppm: u32,
) -> Result<TickOutcome, TickError> {
    let world_revision_before = state.world().revision();
    debug_assert_eq!(next_tick.value(), state.tick().value() + 1);

    // Due jobs are indexed by exact authoritative tick. Downstream enclosure dismantling and
    // structural activation use projected owner state because production completion commits first.
    // Snapshot isolation is intentional: every other phase decides against the pre-tick snapshot
    // and defers same-tick completion effects by one tick. Revision-budget prechecks make that
    // deferral fail-closed (a newly freed store, tool, or calorie is denied this tick rather than
    // admitted against stale facts), so no additional projection is needed for those phases.
    let mut completion_plan = decide_due_completions(registries, state, next_tick)?;
    let structural_construction_plan = decide_structural_construction_after_completions(
        registries,
        state,
        &completion_plan,
        next_tick,
    );
    let structural_dismantling_plan = decide_structural_dismantling_after_completions(
        registries,
        state,
        &completion_plan,
        next_tick,
    )?;
    let surface_gathering_plan =
        decide_surface_gathering_after_completions(registries, state, &completion_plan, next_tick)?;
    let storage_enclosure_dismantling_plan = decide_storage_dismantling_after_completions(
        registries,
        state,
        &completion_plan,
        next_tick,
    )?;
    let projected_inventory_after_completions =
        completion_plan.project_inventory_after_deposits(state.inventory());
    let block_excavation_plan = decide_block_excavation_tick(
        registries,
        state,
        projected_inventory_after_completions.as_ref(),
        next_tick,
    )
    .map_err(map_block_excavation_tick_error)?;
    let equipment_maintenance_plan = decide_equipment_maintenance_tick(state, next_tick);
    let field_prospecting_plan = decide_field_prospecting_tick(registries, state, next_tick)?;
    let manual_power_plan = decide_manual_power_tick(state, next_tick);
    let mining_plan = decide_mining_tick(state, next_tick);
    require_revision_capacity(
        state.mining().revision(),
        [mining_plan
            .as_ref()
            .map_or(0, |plan| plan.mining_revision_steps())],
        TickError::MiningRevisionExhausted,
    )?;
    require_revision_capacity(
        state.geology().revision(),
        [mining_plan
            .as_ref()
            .map_or(0, |plan| plan.geology_revision_steps())],
        TickError::GeologyRevisionExhausted,
    )?;
    let passive_energy_plan = decide_passive_energy_dissipation(registries, state);
    let passive_energy_revision_steps = passive_energy_plan.energy_revision_steps();
    // Production releases and manual-power completion already own future energy revisions through
    // durable resident state. Passive loss is an incidental same-tick mutation, so it must fit in
    // addition to those obligations rather than consuming capacity promised to them.
    if passive_energy_revision_steps != 0
        && !state.can_spend_energy_revisions(passive_energy_revision_steps)
    {
        return Err(TickError::EnergyRevisionExhausted);
    }
    require_revision_capacity(
        state.energy().revision(),
        [
            completion_plan.energy_revision_steps(),
            manual_power_plan
                .as_ref()
                .map_or(0, |plan| plan.energy_revision_steps()),
            passive_energy_revision_steps,
        ],
        TickError::EnergyRevisionExhausted,
    )?;
    let maximum_relevant_warmth = registries
        .survival()
        .physiology()
        .thermal_comfort()
        .local_heat_for_maximum_gain();
    let mut local_warmth = crate::core::quantity::Energy::ZERO;
    for store in state.available_local_energy_stores().filter(|store| {
        registries
            .energy()
            .get_store(store.definition())
            .is_some_and(|definition| definition.is_local_warmth_source())
    }) {
        if local_warmth == maximum_relevant_warmth {
            break;
        }
        local_warmth = accumulate_relevant_local_warmth(
            local_warmth,
            passive_energy_plan.dissipated_for_store(store.id()),
            maximum_relevant_warmth,
        );
    }
    let work_exertion =
        player_work_exertion(registries, state, completion_plan.availability_changes());
    let exertion = SurvivalExertion::new(
        work_exertion
            .energy_cost_per_tick()
            .checked_add(locomotion_exertion.energy_cost_per_tick())
            .ok_or(TickError::SurvivalEnergyCostOverflow)?,
        work_exertion
            .hydration_loss_per_tick()
            .checked_add(locomotion_exertion.hydration_loss_per_tick())
            .ok_or(TickError::SurvivalHydrationCostOverflow)?,
    );
    let thermal_exposure = match assess_player_overhead_cover(registries, state) {
        Some(PlayerOverheadCoverAssessment::Covered) => ThermalExposure::OverheadCover,
        Some(
            PlayerOverheadCoverAssessment::Open | PlayerOverheadCoverAssessment::WorldUnavailable,
        )
        | None => ThermalExposure::Open,
    };
    let survival_plan = decide_survival_tick_with_local_heat(
        registries,
        state,
        exertion,
        next_tick,
        local_warmth,
        thermal_exposure,
        fall_vitality_loss_ppm,
    )?;
    // Player attention uses the projected survival result so fatality, work release, and direct-
    // consumption cancellation become authoritative on the same tick without reading partially
    // mutated state.
    let player_dead_after_tick = survival_plan
        .as_ref()
        .is_some_and(|plan| plan.player_dead_after_tick());
    let player_death_effects = decide_player_death_effects(
        registries,
        state,
        next_tick,
        player_dead_after_tick,
        &mut completion_plan,
    )?;
    let inventory_owner_busy = completion_plan.has_inventory_deposits()
        || block_excavation_plan
            .as_ref()
            .is_some_and(|plan| plan.mutates_inventory())
        || surface_gathering_plan.is_some()
        || storage_enclosure_dismantling_plan.is_some()
        || structural_dismantling_plan.is_some()
        || player_death_effects
            .as_ref()
            .is_some_and(|plan| plan.mutates_inventory());
    let automatic_mining_output = decide_automatic_mining_output(
        registries,
        state,
        next_tick,
        mining_plan,
        inventory_owner_busy,
        structural_construction_plan.is_some() || structural_dismantling_plan.is_some(),
    )?;
    require_revision_capacity(
        state.equipment().revision(),
        [
            completion_plan.equipment_revision_steps(),
            mining_plan
                .as_ref()
                .map_or(0, |plan| plan.equipment_revision_steps()),
            manual_power_plan
                .as_ref()
                .map_or(0, |plan| plan.equipment_revision_steps()),
            equipment_maintenance_plan
                .as_ref()
                .map_or(0, |plan| plan.equipment_revision_steps()),
            field_prospecting_plan
                .as_ref()
                .map_or(0, |plan| plan.equipment_revision_steps()),
            u64::from(block_excavation_plan.is_some()),
            player_death_effects
                .as_ref()
                .map_or(0, |plan| plan.equipment_revision_steps()),
        ],
        TickError::EquipmentRevisionExhausted,
    )?;
    let player_work_plan = decide_player_work_tick(
        registries,
        state,
        next_tick,
        completion_plan.availability_changes(),
        player_dead_after_tick,
    )
    .map_err(|_error| TickError::PlayerWorkRevisionExhausted)?;
    let CompletionApplication {
        completions: production_completions,
        availability_changes: production_availability_changes,
    } = apply_completion_plan(state, completion_plan)?;
    let structural_construction =
        apply_structural_construction_tick(state, structural_construction_plan);
    let structural_dismantling =
        apply_structural_dismantling_tick(state, structural_dismantling_plan);
    let block_excavation = apply_block_excavation_tick(registries, state, block_excavation_plan);
    let _completed_mining_job = apply_mining_tick(state, mining_plan);
    let mining_output =
        automatic_mining_output.map(|settlement| settlement.apply_prechecked(state));
    let surface_gathering = apply_surface_gathering_tick(state, surface_gathering_plan);
    let manual_power = apply_manual_power_tick(state, manual_power_plan);
    let equipment_maintenance = apply_equipment_maintenance_tick(state, equipment_maintenance_plan);
    apply_passive_energy_dissipation(state, passive_energy_plan);
    let field_prospecting = apply_field_prospecting_tick(state, field_prospecting_plan);
    let storage_enclosure_dismantling =
        apply_storage_enclosure_dismantling_tick(state, storage_enclosure_dismantling_plan);
    apply_player_death_effects(state, player_death_effects);
    apply_player_work_tick(state, player_work_plan);
    let survival =
        apply_survival_tick(state, survival_plan).or_else(|| assess_survival(registries, state));
    if player_dead_after_tick {
        // Buffered actions are only valid for a living player. Clear them inside the slow
        // transaction, before its invariant check and any subsequent save can observe death.
        let _ = crate::logistics::take_player_pending_primary_break(state);
        let _ = crate::logistics::take_player_pending_secondary_use(state);
    }
    apply_clock_advance(state, next_tick);

    debug_assert_runtime_invariants(registries, state, world_revision_before);
    Ok(TickOutcome {
        tick: next_tick,
        production_availability_changes,
        production_completions,
        mining_output,
        surface_gathering,
        block_excavation,
        manual_power,
        equipment_maintenance,
        storage_enclosure_dismantling,
        structural_construction,
        structural_dismantling,
        field_prospecting,
        survival,
    })
}

fn accumulate_relevant_local_warmth(
    total: crate::core::quantity::Energy,
    contribution: crate::core::quantity::Energy,
    maximum_relevant: crate::core::quantity::Energy,
) -> crate::core::quantity::Energy {
    if total >= maximum_relevant {
        return maximum_relevant;
    }
    let remaining = maximum_relevant
        .checked_sub(total)
        .unwrap_or_else(|| unreachable!("sub-maximum local warmth leaves positive headroom"));
    total
        .checked_add(contribution.min(remaining))
        .unwrap_or_else(|| unreachable!("local warmth capped to represented energy range"))
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
