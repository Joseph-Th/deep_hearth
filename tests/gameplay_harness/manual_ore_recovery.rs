//! Shared executed hand-breaking and hand-sorting fallback for owned native-copper ore.

use deep_hearth::content::{
    PROCESS_HAND_BREAK_ORE, PROCESS_HAND_SORT_NATIVE_COPPER, PROCESS_SEPARATE_NATIVE_COPPER,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::inventory::StockpileId;
use deep_hearth::ore_processing::{
    ManualComminutionRequest, ManualConstituentSeparationRequest,
    resolve_manual_comminution_process, resolve_manual_constituent_separation_process,
    validate_start_manual_comminution, validate_start_manual_constituent_separation,
};
use deep_hearth::registry::Registries;

use super::material_selection::select_stockpile_mass;
use super::production_timing::finish_uninterrupted_production_job;

#[derive(Clone, Copy)]
pub(super) struct ManualOreRecoveryPlan {
    pub(super) ore_source: StockpileId,
    pub(super) crushed_destination: StockpileId,
    pub(super) native_destination: StockpileId,
    pub(super) residue_destination: StockpileId,
    pub(super) feed_mass: Mass,
}

#[derive(Clone, Copy)]
pub(super) struct ManualOreRecoveryExecution {
    pub(super) attention_ticks: u64,
    pub(super) recovered_native: Mass,
    pub(super) manual_recovery_ppm: u32,
    pub(super) powered_recovery_ppm: u32,
}

pub(super) fn execute_manual_ore_recovery(
    registries: &Registries,
    state: &mut AppState,
    plan: ManualOreRecoveryPlan,
) -> ManualOreRecoveryExecution {
    let breaking = registries
        .ore_processing()
        .get_manual_comminution(PROCESS_HAND_BREAK_ORE)
        .unwrap_or_else(|| panic!("manual ore recovery lost its hand-breaking definition"));
    let sorting = registries
        .ore_processing()
        .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("manual ore recovery lost its hand-sorting definition"));
    let powered_sorting = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("manual ore recovery lost its powered comparison route"));
    assert!(
        state
            .inventory()
            .get_stockpile(plan.ore_source)
            .is_some_and(|stockpile| stockpile.stored_mass() >= plan.feed_mass),
        "manual ore recovery requires its complete feed in the source stockpile"
    );

    let mut attention_ticks = 0_u64;
    let mut remaining = plan.feed_mass;
    while !remaining.is_zero() {
        let batch = remaining.min(breaking.max_batch_mass());
        let selections = select_stockpile_mass(
            state,
            plan.ore_source,
            batch,
            "manual ore recovery breaking feed",
        );
        let resolution = resolve_manual_comminution_process(
            registries,
            state,
            ManualComminutionRequest::new(PROCESS_HAND_BREAK_ORE, plan.ore_source, &selections),
        )
        .unwrap_or_else(|error| panic!("manual ore recovery hand breaking failed: {error}"));
        attention_ticks = attention_ticks
            .checked_add(resolution.duration().value())
            .unwrap_or_else(|| panic!("manual ore recovery attention overflowed"));
        let job = validate_start_manual_comminution(
            registries,
            state,
            &resolution,
            plan.ore_source,
            plan.crushed_destination,
        )
        .unwrap_or_else(|error| panic!("manual ore recovery breaking start failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("manual ore recovery breaking commit failed: {error}"));
        finish_uninterrupted_production_job(registries, state, job, "manual ore recovery breaking");
        remaining = remaining
            .checked_sub(batch)
            .unwrap_or_else(|| unreachable!("manual breaking cannot exceed remaining feed"));
    }

    let mut recovered_native = Mass::ZERO;
    let mut residue_mass = Mass::ZERO;
    remaining = plan.feed_mass;
    while !remaining.is_zero() {
        let batch = remaining.min(sorting.max_batch_mass());
        let selections = select_stockpile_mass(
            state,
            plan.crushed_destination,
            batch,
            "manual ore recovery sorting feed",
        );
        let resolution = resolve_manual_constituent_separation_process(
            registries,
            state,
            ManualConstituentSeparationRequest::new(
                PROCESS_HAND_SORT_NATIVE_COPPER,
                plan.crushed_destination,
                &selections,
            ),
        )
        .unwrap_or_else(|error| panic!("manual ore recovery hand sorting failed: {error}"));
        attention_ticks = attention_ticks
            .checked_add(resolution.duration().value())
            .unwrap_or_else(|| panic!("manual ore recovery attention overflowed"));
        recovered_native = recovered_native
            .checked_add(resolution.target_mass())
            .unwrap_or_else(|| panic!("manual ore recovery native output overflowed"));
        residue_mass = residue_mass
            .checked_add(resolution.residue_mass())
            .unwrap_or_else(|| panic!("manual ore recovery residue overflowed"));
        let job = validate_start_manual_constituent_separation(
            registries,
            state,
            &resolution,
            plan.crushed_destination,
            plan.native_destination,
            plan.residue_destination,
        )
        .unwrap_or_else(|error| panic!("manual ore recovery sorting start failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("manual ore recovery sorting commit failed: {error}"));
        finish_uninterrupted_production_job(registries, state, job, "manual ore recovery sorting");
        remaining = remaining
            .checked_sub(batch)
            .unwrap_or_else(|| unreachable!("manual sorting cannot exceed remaining feed"));
    }
    assert_eq!(
        recovered_native.checked_add(residue_mass),
        Some(plan.feed_mass),
        "manual ore recovery must partition the complete selected feed"
    );
    ManualOreRecoveryExecution {
        attention_ticks,
        recovered_native,
        manual_recovery_ppm: sorting.target_recovery_ppm(),
        powered_recovery_ppm: powered_sorting.target_recovery_ppm(),
    }
}
