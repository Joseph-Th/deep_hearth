//! Powered owned-ore recovery through the processing line earned before first-foundry work.

use deep_hearth::content::{
    ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE, EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
    EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR, MANUAL_POWER_FOOT_TREADLE, PROCESS_CRUSH_ORE,
    PROCESS_SEPARATE_NATIVE_COPPER,
};
use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::core::state::AppState;
use deep_hearth::energy::{EnergyStoreId, calculate_mass_specific_energy};
use deep_hearth::equipment::EquipmentId;
use deep_hearth::inventory::StockpileId;
use deep_hearth::labor::{ManualPowerRequest, validate_start_manual_power};
use deep_hearth::maintenance::Condition;
use deep_hearth::ore_processing::{
    ComminutionRequest, ConstituentSeparationProcessDefinition, ConstituentSeparationRequest,
    assess_powered_ore_mass_envelope, project_powered_ore_replenished_batch_capacity,
    resolve_comminution_process, resolve_constituent_separation_process,
};
use deep_hearth::production::{
    ProcessOutputRoute, validate_start_process, validate_start_process_routed,
};
use deep_hearth::registry::Registries;

use super::super::manual_power_timing::finish_manual_power_work;
use super::super::material_selection::select_stockpile_mass;
use super::super::production_timing::finish_uninterrupted_production_job;

#[derive(Clone, Copy)]
pub(super) struct InheritedProcessingLine {
    pub(super) crusher: EquipmentId,
    pub(super) separator: EquipmentId,
    pub(super) drive: EnergyStoreId,
    pub(super) provider: EquipmentId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PoweredOreRecoveryStop {
    TargetRecovered,
    OwnedOreExhausted,
    ProcessingLineUnavailable,
    PlayerPowerUnavailable,
}

impl PoweredOreRecoveryStop {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::TargetRecovered => "target-recovered",
            Self::OwnedOreExhausted => "owned-ore-exhausted",
            Self::ProcessingLineUnavailable => "processing-line-unavailable",
            Self::PlayerPowerUnavailable => "player-power-unavailable",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PoweredOreRecoveryExecution {
    pub(super) feed_mass: Mass,
    pub(super) recovered_native: Mass,
    pub(super) attention_ticks: u64,
    pub(super) autonomous_ticks: u64,
    pub(super) elapsed_ticks: u64,
    pub(super) recovery_ppm: u32,
    pub(super) batches: u64,
    pub(super) stop: PoweredOreRecoveryStop,
}

pub(super) fn projected_inherited_processing_batch_limit(
    registries: &Registries,
    crusher_condition: Condition,
    separator_condition: Condition,
) -> Option<Mass> {
    let crusher = project_powered_ore_replenished_batch_capacity(
        registries,
        PROCESS_CRUSH_ORE,
        EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
        crusher_condition,
        ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
    )
    .ok()?;
    let separator = project_powered_ore_replenished_batch_capacity(
        registries,
        PROCESS_SEPARATE_NATIVE_COPPER,
        EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
        separator_condition,
        ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
    )
    .ok()?;
    Some(crusher.min(separator))
}

pub(super) fn current_processing_batch_limit(
    registries: &Registries,
    state: &AppState,
    line: InheritedProcessingLine,
) -> Option<Mass> {
    let crusher = assess_powered_ore_mass_envelope(
        registries,
        state,
        PROCESS_CRUSH_ORE,
        line.crusher,
        line.drive,
    )
    .ok()?;
    let separator = assess_powered_ore_mass_envelope(
        registries,
        state,
        PROCESS_SEPARATE_NATIVE_COPPER,
        line.separator,
        line.drive,
    )
    .ok()?;
    Some(
        crusher
            .maximum_mass_with_replenished_energy()
            .min(separator.maximum_mass_with_replenished_energy()),
    )
}

pub(super) fn minimum_powered_ore_feed_for_target_recovery(
    registries: &Registries,
    target: Mass,
    copper_ppm: u32,
    processing_batch_limit: Mass,
) -> Option<Mass> {
    let separator = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)?;
    separator.minimum_batched_homogeneous_feed_mass_for_target_recovery(
        target,
        copper_ppm,
        processing_batch_limit,
    )
}

pub(super) fn minimum_current_powered_ore_feed_for_target_recovery(
    registries: &Registries,
    state: &AppState,
    line: InheritedProcessingLine,
    target: Mass,
    copper_ppm: u32,
) -> Option<Mass> {
    minimum_powered_ore_feed_for_target_recovery(
        registries,
        target,
        copper_ppm,
        current_processing_batch_limit(registries, state, line)?,
    )
}

fn charge_processing_drive(
    registries: &Registries,
    state: &mut AppState,
    line: InheritedProcessingLine,
    energy: Energy,
    context: &'static str,
) -> Option<u64> {
    assert_eq!(
        state
            .energy()
            .get_store(line.drive)
            .map(|store| store.stored()),
        Some(Energy::ZERO),
        "first foundry {context} must begin from an empty processing flywheel"
    );
    let start = validate_start_manual_power(
        registries,
        state,
        ManualPowerRequest::new(MANUAL_POWER_FOOT_TREADLE, line.provider, line.drive, energy),
    )
    .ok()?;
    let work = start.work();
    start
        .commit(state)
        .unwrap_or_else(|error| panic!("first foundry {context} charge commit failed: {error}"));
    Some(finish_manual_power_work(registries, state, work, context))
}

pub(super) struct PoweredOreRecoveryPlan {
    pub(super) ore_source: StockpileId,
    pub(super) crushed_destination: StockpileId,
    pub(super) native_destination: StockpileId,
    pub(super) residue_destination: StockpileId,
    pub(super) target: Mass,
    pub(super) copper_ppm: u32,
    pub(super) line: InheritedProcessingLine,
}

pub(super) fn execute_powered_ore_recovery(
    registries: &Registries,
    state: &mut AppState,
    plan: PoweredOreRecoveryPlan,
) -> PoweredOreRecoveryExecution {
    let crusher = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("first foundry inherited crusher process disappeared"));
    let separator = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("first foundry inherited separator process disappeared"));
    let started_at = state.tick().value();
    let mut feed_mass = Mass::ZERO;
    let mut recovered_native = Mass::ZERO;
    let mut attention_ticks = 0_u64;
    let mut autonomous_ticks = 0_u64;
    let mut batches = 0_u64;
    let mut stop = PoweredOreRecoveryStop::TargetRecovered;

    while recovered_native < plan.target {
        let available_feed = state
            .inventory()
            .get_stockpile(plan.ore_source)
            .map(|stockpile| stockpile.stored_mass())
            .unwrap_or_else(|| panic!("first foundry owned-ore stockpile disappeared"));
        if available_feed.is_zero() {
            stop = PoweredOreRecoveryStop::OwnedOreExhausted;
            break;
        }
        let Some(live_limit) = current_processing_batch_limit(registries, state, plan.line) else {
            stop = PoweredOreRecoveryStop::ProcessingLineUnavailable;
            break;
        };
        let batch_limit = live_limit.min(available_feed);
        let target_capacity = separator
            .maximum_homogeneous_target_recovery_from_feed(batch_limit, plan.copper_ppm)
            .unwrap_or_else(|| panic!("first foundry powered recovery projection overflowed"));
        if target_capacity.is_zero() {
            stop = if batch_limit == available_feed {
                PoweredOreRecoveryStop::OwnedOreExhausted
            } else {
                PoweredOreRecoveryStop::ProcessingLineUnavailable
            };
            break;
        }
        let remaining_target = plan
            .target
            .checked_sub(recovered_native)
            .unwrap_or_else(|| unreachable!("recovery cannot exceed its target before this batch"));
        let batch_target = remaining_target.min(target_capacity);
        let feed = separator
            .minimum_homogeneous_feed_mass_for_target_recovery(batch_target, plan.copper_ppm)
            .unwrap_or_else(|| panic!("first foundry live recovery feed projection overflowed"));
        assert!(
            !feed.is_zero() && feed <= batch_limit,
            "live recovery feed must fit the current processing envelope"
        );

        let crush_energy = calculate_mass_specific_energy(feed, crusher.specific_energy());
        let Some(crush_attention) = charge_processing_drive(
            registries,
            state,
            plan.line,
            crush_energy,
            "owned-ore crushing",
        ) else {
            stop = PoweredOreRecoveryStop::PlayerPowerUnavailable;
            break;
        };
        attention_ticks = attention_ticks
            .checked_add(crush_attention)
            .unwrap_or_else(|| panic!("first foundry recovery attention overflowed"));
        let ore_selection = select_stockpile_mass(
            state,
            plan.ore_source,
            feed,
            "first foundry inherited crusher feed",
        );
        let crushing = resolve_comminution_process(
            registries,
            state,
            ComminutionRequest::new(
                PROCESS_CRUSH_ORE,
                plan.ore_source,
                ore_selection.as_slice(),
                plan.line.crusher,
                plan.line.drive,
            ),
        )
        .unwrap_or_else(|error| panic!("first foundry inherited crushing failed: {error}"));
        assert_eq!(crushing.required_energy(), crush_energy);
        let crush_ticks = crushing.process_resolution().duration().value();
        let job = validate_start_process(
            registries,
            state,
            crushing.process_resolution(),
            plan.ore_source,
            plan.crushed_destination,
        )
        .unwrap_or_else(|error| panic!("first foundry inherited crushing start failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("first foundry inherited crushing commit failed: {error}"));
        finish_uninterrupted_production_job(
            registries,
            state,
            job,
            "first foundry inherited crushing",
        );
        autonomous_ticks = autonomous_ticks
            .checked_add(crush_ticks)
            .unwrap_or_else(|| panic!("first foundry recovery autonomous time overflowed"));
        feed_mass = feed_mass
            .checked_add(feed)
            .unwrap_or_else(|| panic!("first foundry recovery feed accounting overflowed"));

        let separation_energy = calculate_mass_specific_energy(feed, separator.specific_energy());
        let Some(separation_attention) = charge_processing_drive(
            registries,
            state,
            plan.line,
            separation_energy,
            "owned-ore separation",
        ) else {
            stop = PoweredOreRecoveryStop::PlayerPowerUnavailable;
            break;
        };
        attention_ticks = attention_ticks
            .checked_add(separation_attention)
            .unwrap_or_else(|| panic!("first foundry recovery attention overflowed"));
        let crushed_selection = select_stockpile_mass(
            state,
            plan.crushed_destination,
            feed,
            "first foundry inherited separator feed",
        );
        let separation = resolve_constituent_separation_process(
            registries,
            state,
            ConstituentSeparationRequest::new(
                PROCESS_SEPARATE_NATIVE_COPPER,
                plan.crushed_destination,
                crushed_selection.as_slice(),
                plan.line.separator,
                plan.line.drive,
            ),
        )
        .unwrap_or_else(|error| panic!("first foundry inherited separation failed: {error}"));
        assert_eq!(separation.required_energy(), separation_energy);
        assert!(
            separation.target_mass() >= batch_target,
            "canonical powered recovery must meet the batch target used to size its feed"
        );
        let separation_ticks = separation.process_resolution().duration().value();
        let job = validate_start_process_routed(
            registries,
            state,
            separation.process_resolution(),
            plan.crushed_destination,
            &[
                ProcessOutputRoute::new(
                    ConstituentSeparationProcessDefinition::TARGET_STREAM,
                    plan.native_destination,
                ),
                ProcessOutputRoute::new(
                    ConstituentSeparationProcessDefinition::RESIDUE_STREAM,
                    plan.residue_destination,
                ),
            ],
        )
        .unwrap_or_else(|error| panic!("first foundry inherited separation start failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| {
            panic!("first foundry inherited separation commit failed: {error}")
        });
        finish_uninterrupted_production_job(
            registries,
            state,
            job,
            "first foundry inherited separation",
        );
        autonomous_ticks = autonomous_ticks
            .checked_add(separation_ticks)
            .unwrap_or_else(|| panic!("first foundry recovery autonomous time overflowed"));
        recovered_native = recovered_native
            .checked_add(separation.target_mass())
            .unwrap_or_else(|| panic!("first foundry recovered copper accounting overflowed"));
        batches = batches
            .checked_add(1)
            .unwrap_or_else(|| panic!("first foundry recovery batch count overflowed"));
    }
    let elapsed_ticks = state
        .tick()
        .value()
        .checked_sub(started_at)
        .unwrap_or_else(|| unreachable!("first foundry recovery cannot move time backward"));
    assert_eq!(
        elapsed_ticks,
        attention_ticks
            .checked_add(autonomous_ticks)
            .unwrap_or_else(|| panic!("first foundry recovery elapsed accounting overflowed")),
        "first foundry powered recovery must account for every charging and autonomous process tick"
    );
    PoweredOreRecoveryExecution {
        feed_mass,
        recovered_native,
        attention_ticks,
        autonomous_ticks,
        elapsed_ticks,
        recovery_ppm: separator.target_recovery_ppm(),
        batches,
        stop,
    }
}
