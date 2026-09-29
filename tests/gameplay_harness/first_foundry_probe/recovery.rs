//! Powered owned-ore recovery through the processing line earned before first-foundry work.

use deep_hearth::content::{
    ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE, EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
    EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR, MANUAL_POWER_FOOT_TREADLE, PROCESS_CRUSH_ORE,
    PROCESS_SEPARATE_NATIVE_COPPER,
};
use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::core::state::AppState;
use deep_hearth::energy::{
    EnergyStoreId, calculate_mass_specific_energy, calculate_mass_specific_energy_capacity,
};
use deep_hearth::equipment::EquipmentId;
use deep_hearth::inventory::StockpileId;
use deep_hearth::labor::{ManualPowerRequest, validate_start_manual_power};
use deep_hearth::ore_processing::{
    ComminutionRequest, ConstituentSeparationProcessDefinition, ConstituentSeparationRequest,
    resolve_comminution_process, resolve_constituent_separation_process,
};
use deep_hearth::production::{
    ProcessOutputRoute, validate_start_process, validate_start_process_routed,
};
use deep_hearth::registry::Registries;

use super::super::equipment_support::nominal_equipment_mass_capability;
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
struct RecoveryBatch {
    feed: Mass,
    target: Mass,
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
}

fn inherited_processing_batch_limit(registries: &Registries) -> Mass {
    let crusher = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("first foundry inherited crusher process disappeared"));
    let separator = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("first foundry inherited separator process disappeared"));
    let drive = registries
        .energy()
        .get_store(ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE)
        .unwrap_or_else(|| panic!("first foundry inherited processing flywheel disappeared"));
    let crusher_batch = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
        crusher.max_batch_mass_capability(),
    );
    let separator_batch = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
        separator.max_batch_mass_capability(),
    );
    let crusher_energy_batch =
        calculate_mass_specific_energy_capacity(drive.capacity(), crusher.specific_energy());
    let separator_energy_batch =
        calculate_mass_specific_energy_capacity(drive.capacity(), separator.specific_energy());
    crusher_batch
        .min(separator_batch)
        .min(crusher_energy_batch)
        .min(separator_energy_batch)
}

fn largest_target_for_one_batch(
    registries: &Registries,
    copper_ppm: u32,
    batch_limit: Mass,
) -> Option<Mass> {
    let separator = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)?;
    let mut lower = 0_u64;
    let mut upper = batch_limit.milligrams();
    while lower < upper {
        let midpoint = lower + (upper - lower).div_ceil(2);
        let target = Mass::from_milligrams(midpoint);
        let fits = separator
            .minimum_homogeneous_feed_mass_for_target_recovery(target, copper_ppm)
            .is_some_and(|feed| feed <= batch_limit);
        if fits {
            lower = midpoint;
        } else {
            upper = midpoint - 1;
        }
    }
    (lower > 0).then(|| Mass::from_milligrams(lower))
}

fn recovery_batches_for_target(
    registries: &Registries,
    target: Mass,
    copper_ppm: u32,
) -> Option<Vec<RecoveryBatch>> {
    if target.is_zero() {
        return Some(Vec::new());
    }
    let separator = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)?;
    let batch_limit = inherited_processing_batch_limit(registries);
    let target_per_batch = largest_target_for_one_batch(registries, copper_ppm, batch_limit)?;
    let mut remaining = target;
    let mut batches = Vec::new();
    while !remaining.is_zero() {
        let batch_target = remaining.min(target_per_batch);
        let feed = separator
            .minimum_homogeneous_feed_mass_for_target_recovery(batch_target, copper_ppm)?;
        if feed.is_zero() || feed > batch_limit {
            return None;
        }
        batches.push(RecoveryBatch {
            feed,
            target: batch_target,
        });
        remaining = remaining.checked_sub(batch_target)?;
    }
    Some(batches)
}

pub(super) fn minimum_powered_ore_feed_for_target_recovery(
    registries: &Registries,
    target: Mass,
    copper_ppm: u32,
) -> Option<Mass> {
    recovery_batches_for_target(registries, target, copper_ppm)?
        .into_iter()
        .try_fold(Mass::ZERO, |total, batch| total.checked_add(batch.feed))
}

fn charge_processing_drive(
    registries: &Registries,
    state: &mut AppState,
    line: InheritedProcessingLine,
    energy: Energy,
    context: &'static str,
) -> u64 {
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
    .unwrap_or_else(|error| panic!("first foundry {context} charge failed: {error}"));
    let work = start.work();
    start
        .commit(state)
        .unwrap_or_else(|error| panic!("first foundry {context} charge commit failed: {error}"));
    finish_manual_power_work(registries, state, work, context)
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
    let batches = recovery_batches_for_target(registries, plan.target, plan.copper_ppm)
        .unwrap_or_else(|| {
            panic!("first foundry inherited processing cannot recover target copper")
        });
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

    for batch in &batches {
        let crush_energy = calculate_mass_specific_energy(batch.feed, crusher.specific_energy());
        attention_ticks = attention_ticks
            .checked_add(charge_processing_drive(
                registries,
                state,
                plan.line,
                crush_energy,
                "owned-ore crushing",
            ))
            .unwrap_or_else(|| panic!("first foundry recovery attention overflowed"));
        let ore_selection = select_stockpile_mass(
            state,
            plan.ore_source,
            batch.feed,
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

        let separation_energy =
            calculate_mass_specific_energy(batch.feed, separator.specific_energy());
        attention_ticks = attention_ticks
            .checked_add(charge_processing_drive(
                registries,
                state,
                plan.line,
                separation_energy,
                "owned-ore separation",
            ))
            .unwrap_or_else(|| panic!("first foundry recovery attention overflowed"));
        let crushed_selection = select_stockpile_mass(
            state,
            plan.crushed_destination,
            batch.feed,
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
            separation.target_mass() >= batch.target,
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
        feed_mass = feed_mass
            .checked_add(batch.feed)
            .unwrap_or_else(|| panic!("first foundry recovery feed accounting overflowed"));
        recovered_native = recovered_native
            .checked_add(separation.target_mass())
            .unwrap_or_else(|| panic!("first foundry recovered copper accounting overflowed"));
    }

    assert!(
        recovered_native >= plan.target,
        "inherited processing selected to close the foundry shortfall must recover enough copper"
    );
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
        batches: u64::try_from(batches.len())
            .unwrap_or_else(|_| panic!("first foundry recovery batch count exceeds u64")),
    }
}
