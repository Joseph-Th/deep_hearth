//! Deterministic slow-tick accounting and cross-owner settlement.

use super::planning::{
    decide_automatic_mining_output, decide_storage_dismantling_after_completions,
    decide_structural_construction_after_completions,
    decide_structural_dismantling_after_completions, decide_surface_gathering_after_completions,
    require_revision_capacity,
};
use super::player_death::{apply_player_death_effects, decide_player_death_effects};
use super::{TickError, TickOutcome, map_block_excavation_tick_error};

use crate::core::state::{AppState, apply_clock_advance, debug_assert_runtime_invariants};
use crate::core::time::SimulationTick;
use crate::energy::{
    apply_mechanical_transmission, apply_passive_energy_dissipation,
    decide_mechanical_transmission, decide_passive_energy_dissipation,
};
use crate::equipment::{apply_equipment_maintenance_tick, decide_equipment_maintenance_tick};
use crate::fluid::apply_natural_water_cycle;
use crate::geology::{apply_field_prospecting_tick, decide_field_prospecting_tick};
use crate::inventory::apply_storage_enclosure_dismantling_tick;
use crate::kobolds::apply_kobold_scout_tick;
use crate::labor::{
    apply_manual_power_tick, apply_player_work_tick, decide_manual_power_tick,
    decide_player_work_tick, player_work_exertion,
};
use crate::logistics::{PlayerOverheadCoverAssessment, assess_player_overhead_cover};
use crate::mining::{apply_mining_tick, decide_mining_tick};
use crate::production::{CompletionApplication, apply_completion_plan, decide_due_completions};
use crate::registry::Registries;
use crate::structural::{apply_structural_construction_tick, apply_structural_dismantling_tick};
use crate::surface::apply_surface_gathering_tick;
use crate::survival::{
    SurvivalExertion, ThermalExposure, apply_survival_tick, assess_survival,
    decide_survival_tick_with_local_heat,
};
use crate::world::{
    apply_block_excavation_tick, apply_natural_forage_cycle, apply_tree_felling_wear,
    assess_player_weather_at, decide_block_excavation_tick, decide_tree_felling_wear,
    validate_due_player_tree_felling,
};

/// Advances the full slow simulation by one 3.6-second physical/accounting quantum.
///
/// Every authoritative subsystem phase is sequenced here. Decisions read the pre-tick state or an
/// explicit same-tick projection when an earlier completion changes a downstream admission fact,
/// then mutations apply in one deterministic order.
pub(super) fn advance_simulation_tick(
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
    let tree_felling_plan = validate_due_player_tree_felling(registries, state, next_tick)
        .map_err(|_| TickError::TreeFellingBlocked)?;
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
    let transmission_plan = decide_mechanical_transmission(
        registries,
        state,
        &passive_energy_plan,
        structural_dismantling_plan
            .as_ref()
            .map(|plan| plan.element()),
    );
    let transmission_revision_steps = transmission_plan.energy_revision_steps();
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
            transmission_revision_steps,
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
    let covered = matches!(
        assess_player_overhead_cover(registries, state),
        Some(PlayerOverheadCoverAssessment::Covered)
    );
    let thermal_exposure = match (
        covered,
        assess_player_weather_at(registries, state, next_tick),
    ) {
        (true, Some(weather)) => ThermalExposure::GeneratedOverheadCover {
            cooling_multiplier_ppm: weather.cooling_multiplier_ppm(),
        },
        (false, Some(weather)) => ThermalExposure::GeneratedOpen {
            cooling_multiplier_ppm: weather.cooling_multiplier_ppm(),
        },
        (true, None) => ThermalExposure::OverheadCover,
        (false, None) => ThermalExposure::Open,
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
    let tree_felling_wear_plan = decide_tree_felling_wear(state, next_tick, player_dead_after_tick);
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
            tree_felling_wear_plan
                .as_ref()
                .map_or(0, |plan| plan.equipment_revision_steps()),
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
    // Settle reserved standing wood before other owner completions can alter surface
    // or structure revisions. This is exact mass transfer, not a second growth tick.
    let tree_felling = tree_felling_plan.map(|validated| {
        validated
            .commit(state)
            .unwrap_or_else(|error| panic!("prechecked due tree felling became stale: {error:?}"))
    });
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
    let mechanical_transmissions = apply_mechanical_transmission(state, transmission_plan);
    let field_prospecting = apply_field_prospecting_tick(state, field_prospecting_plan);
    let storage_enclosure_dismantling =
        apply_storage_enclosure_dismantling_tick(state, storage_enclosure_dismantling_plan);
    apply_player_death_effects(state, player_death_effects);
    // A separate tool can finish production or mining on this tick. Their prevalidated
    // equipment revisions settle first; chopping wears its occupied adze last.
    apply_tree_felling_wear(state, tree_felling_wear_plan);
    apply_player_work_tick(state, player_work_plan);
    // The slow-tick survival decision is already finalized. Install fatality
    // before residents assess a player's actual living threat: a death on this
    // very tick must not make a corpse deter or challenge a neighboring scout.
    let _ = apply_survival_tick(state, survival_plan);
    apply_natural_forage_cycle(registries, state, next_tick);
    // Weather-front changes can move existing water between the exposed pool
    // and its shallow basin before residents choose whether to drink.
    apply_natural_water_cycle(registries, state, next_tick);
    let (kobold_scout, kobold_alarm_pulses) = apply_kobold_scout_tick(registries, state, next_tick);
    if player_dead_after_tick {
        // Raised weapons cannot survive a fatal survival transition or grant a charged respawn hit.
        state.combat_state_mut().clear_preparation_on_death();
        // Buffered actions are only valid for a living player. Clear them inside the slow
        // transaction, before its invariant check and any subsequent save can observe death.
        let _ = crate::logistics::take_player_pending_primary_break(state);
        let _ = crate::logistics::take_player_pending_secondary_use(state);
    }
    apply_clock_advance(state, next_tick);

    // Residents can inflict nonlethal contact injury after basal survival settles.
    // Report the final authoritative vitality, not the earlier physiology plan.
    let survival = assess_survival(registries, state);

    debug_assert_runtime_invariants(registries, state, world_revision_before);
    Ok(TickOutcome {
        tick: next_tick,
        production_availability_changes,
        production_completions,
        mining_output,
        surface_gathering,
        block_excavation,
        tree_felling,
        manual_power,
        mechanical_transmissions,
        equipment_maintenance,
        storage_enclosure_dismantling,
        structural_construction,
        structural_dismantling,
        field_prospecting,
        kobold_scout,
        kobold_alarm_pulses,
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
#[path = "slow_tick_tests.rs"]
mod tests;
