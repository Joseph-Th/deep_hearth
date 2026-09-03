//! Physical execution and conservation audits for the focused foundry scenario.

use super::super::environment::ROOM_TEMPERATURE;
use super::super::production_timing::finish_uninterrupted_production_job;
use super::super::temporal::advance_idle_ticks;
use super::{CastBatchLimit, FoundryIds, PreheatResult, RecoveryCast};
use deep_hearth::content::{ENERGY_THERMAL_SINK, MATERIAL_COPPER};
use deep_hearth::core::quantity::{AggregateMass, Energy, Mass};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::TickSpan;
use deep_hearth::energy::passive_dissipation_ticks_until_empty;
use deep_hearth::inventory::StockpileId;
use deep_hearth::material::MaterialComposition;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::production::validate_start_process;
use deep_hearth::registry::Registries;
use deep_hearth::thermal::{
    ResolvedCasting, ResolvedMelting, calculate_fusion_heat, calculate_sensible_heat,
};

#[derive(Clone, Copy)]
pub(super) struct FoundryInitialAccounting {
    pub(super) matter: AggregateMass,
    pub(super) electrical: Energy,
    pub(super) thermal: Energy,
}

pub(super) fn capture_initial_accounting(
    state: &AppState,
    ids: FoundryIds,
) -> FoundryInitialAccounting {
    let matter = calculate_matter_accounting(state)
        .unwrap_or_else(|error| panic!("foundry initial matter accounting failed: {error}"))
        .total();
    let electrical = state
        .energy()
        .get_store(ids.electrical_buffer)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("foundry electrical buffer disappeared after setup"));
    let thermal = state
        .energy()
        .get_store(ids.heat_sink)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("foundry heat sink disappeared after setup"));
    FoundryInitialAccounting {
        matter,
        electrical,
        thermal,
    }
}

pub(super) fn assert_preheat_partitions_melting_energy(
    registries: &Registries,
    mass: Mass,
    preheat: PreheatResult,
    melt: &ResolvedMelting,
) {
    if !preheat.applied || melt.process_resolution().input_mass() != mass {
        return;
    }
    let melting_point = registries
        .materials()
        .get_material(MATERIAL_COPPER)
        .and_then(|material| material.properties().thermal().melting_point())
        .unwrap_or_else(|| panic!("foundry copper melting point disappeared during replay"));
    let direct_sensible = calculate_sensible_heat(
        registries.materials(),
        mass,
        &MaterialComposition::pure(MATERIAL_COPPER),
        ROOM_TEMPERATURE,
        melting_point,
    )
    .unwrap_or_else(|error| panic!("foundry direct-melt sensible heat failed: {error}"))
    .energy();
    let direct_fusion = calculate_fusion_heat(registries.materials(), mass, MATERIAL_COPPER)
        .unwrap_or_else(|error| panic!("foundry direct-melt fusion heat failed: {error}"))
        .energy();
    let direct_melt_energy = direct_sensible
        .checked_add(direct_fusion)
        .unwrap_or_else(|| panic!("foundry direct-melt energy overflowed"));
    let split_energy = preheat
        .energy
        .checked_add(melt.required_energy())
        .unwrap_or_else(|| panic!("foundry split-heating energy overflowed"));
    assert_eq!(
        split_energy, direct_melt_energy,
        "lossless sensible preheating must partition, not discount or duplicate, direct melting energy"
    );
    assert!(
        melt.required_energy() < direct_melt_energy,
        "successful preheating must reduce the later melting-stage energy requirement"
    );
}

pub(super) fn execute_melt(
    registries: &Registries,
    state: &mut AppState,
    ids: FoundryIds,
    source: StockpileId,
    melt: &ResolvedMelting,
) -> TickSpan {
    let duration = melt.process_resolution().duration();
    let job = validate_start_process(
        registries,
        state,
        melt.process_resolution(),
        source,
        ids.molten_vessel,
    )
    .unwrap_or_else(|error| panic!("foundry probe melt start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("foundry probe melt commit failed: {error}"));
    finish_uninterrupted_production_job(registries, state, job, duration, "foundry melt");
    duration
}

#[derive(Clone, Copy)]
pub(super) struct PrimaryCastResult {
    pub(super) cast_mass: Mass,
    pub(super) limit: CastBatchLimit,
    pub(super) duration: TickSpan,
    pub(super) released_heat: Energy,
    pub(super) thermal_without_cast: Energy,
    pub(super) molten_remaining: Mass,
}

pub(super) fn execute_primary_cast(
    registries: &Registries,
    state: &mut AppState,
    ids: FoundryIds,
    processed_mass: Mass,
    casting: &ResolvedCasting,
    cast_mass: Mass,
    limit: CastBatchLimit,
) -> PrimaryCastResult {
    let duration = casting.process_resolution().duration();
    let released_heat = casting.released_energy();
    // Compare against the same-duration no-cast branch so canonical scheduling owns passive loss.
    let mut no_cast_baseline = state.clone();
    advance_idle_ticks(
        registries,
        &mut no_cast_baseline,
        duration.value(),
        "foundry no-cast thermal baseline",
    );
    let thermal_without_cast = no_cast_baseline
        .energy()
        .get_store(ids.heat_sink)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("foundry heat sink disappeared from no-cast baseline"));
    let job = validate_start_process(
        registries,
        state,
        casting.process_resolution(),
        ids.molten_vessel,
        ids.cast_storage,
    )
    .unwrap_or_else(|error| panic!("foundry probe casting start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("foundry probe casting commit failed: {error}"));
    finish_uninterrupted_production_job(registries, state, job, duration, "foundry casting");
    let molten_remaining = processed_mass
        .checked_sub(cast_mass)
        .unwrap_or_else(|| unreachable!("cast mass cannot exceed melted mass"));
    PrimaryCastResult {
        cast_mass,
        limit,
        duration,
        released_heat,
        thermal_without_cast,
        molten_remaining,
    }
}

#[derive(Clone, Copy)]
pub(super) struct PrimaryCycleAccounting {
    pub(super) final_electrical: Energy,
    pub(super) final_thermal: Energy,
}

pub(super) fn audit_primary_cycle(
    registries: &Registries,
    state: &AppState,
    ids: FoundryIds,
    initial: FoundryInitialAccounting,
    preheat: PreheatResult,
    melt: &ResolvedMelting,
    thermal_before_cast: Energy,
    cast: PrimaryCastResult,
) -> PrimaryCycleAccounting {
    validate_loaded_state(registries, state)
        .unwrap_or_else(|error| panic!("foundry probe final state audit failed: {error}"));
    let final_matter = calculate_matter_accounting(state)
        .unwrap_or_else(|error| panic!("foundry final matter accounting failed: {error}"))
        .total();
    let final_electrical = state
        .energy()
        .get_store(ids.electrical_buffer)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("foundry electrical buffer disappeared after processing"));
    let final_thermal = state
        .energy()
        .get_store(ids.heat_sink)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("foundry heat sink disappeared after processing"));
    assert_eq!(
        final_matter, initial.matter,
        "foundry melt/cast cycle must conserve represented matter"
    );
    assert_eq!(
        initial
            .electrical
            .checked_sub(preheat.energy)
            .and_then(|remaining| remaining.checked_sub(melt.required_energy())),
        Some(final_electrical),
        "foundry preheat and melt must consume exactly their resolved electrical energy"
    );
    assert_eq!(
        cast.thermal_without_cast.checked_add(cast.released_heat),
        Some(final_thermal),
        "foundry casting must add exactly its resolved released heat above the canonical same-duration passive-cooling baseline"
    );
    assert!(
        thermal_before_cast <= initial.thermal,
        "passive heat rejection during melting must not increase pre-existing sink energy"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(ids.cast_storage)
            .map(|stockpile| stockpile.stored_mass()),
        Some(cast.cast_mass),
        "foundry capability probe must store exactly the mass accepted by canonical casting"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(ids.molten_vessel)
            .map(|stockpile| stockpile.stored_mass()),
        Some(cast.molten_remaining),
        "an adaptive partial cast must leave the uncast molten remainder physically owned"
    );
    PrimaryCycleAccounting {
        final_electrical,
        final_thermal,
    }
}

#[derive(Clone, Copy)]
pub(super) struct CooldownResult {
    pub(super) ticks: u64,
    pub(super) cooled_thermal: Energy,
}

pub(super) fn cool_thermal_sink_until(
    registries: &Registries,
    state: &mut AppState,
    ids: FoundryIds,
    stored: Energy,
    mut recovery_ready: impl FnMut(&AppState) -> bool,
) -> CooldownResult {
    let thermal_sink = registries
        .energy()
        .get_store(ENERGY_THERMAL_SINK)
        .unwrap_or_else(|| panic!("foundry thermal-sink definition disappeared"));
    if stored.is_zero() || recovery_ready(state) {
        return CooldownResult {
            ticks: 0,
            cooled_thermal: stored,
        };
    }
    let maximum_ticks = passive_dissipation_ticks_until_empty(registries, thermal_sink, stored)
        .unwrap_or_else(|| panic!("foundry thermal sink has no finite passive recovery horizon"))
        .value();
    for ticks in 1..=maximum_ticks {
        advance_idle_ticks(registries, state, 1, "foundry thermal cooldown");
        let cooled_thermal = state
            .energy()
            .get_store(ids.heat_sink)
            .map(|store| store.stored())
            .unwrap_or_else(|| panic!("foundry heat sink disappeared during passive cooldown"));
        if recovery_ready(state) || cooled_thermal.is_zero() {
            validate_loaded_state(registries, state).unwrap_or_else(|error| {
                panic!("foundry post-cooldown state audit failed: {error}")
            });
            return CooldownResult {
                ticks,
                cooled_thermal,
            };
        }
    }
    unreachable!("finite passive cooldown must reach either recovery readiness or an empty sink")
}

pub(super) fn audit_recovery(
    registries: &Registries,
    state: &AppState,
    ids: FoundryIds,
    initial_matter: AggregateMass,
    primary_cast_mass: Mass,
    final_molten_remaining: Mass,
    recovery: RecoveryCast,
) {
    validate_loaded_state(registries, state)
        .unwrap_or_else(|error| panic!("foundry recovery-cast state audit failed: {error}"));
    assert_eq!(
        calculate_matter_accounting(state)
            .unwrap_or_else(|error| panic!("foundry recovery-cast matter audit failed: {error}"))
            .total(),
        initial_matter,
        "foundry recovery casting must conserve represented matter"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(ids.cast_storage)
            .map(|stockpile| stockpile.stored_mass()),
        primary_cast_mass.checked_add(recovery.cast_mass),
        "foundry recovery must append exactly the newly cast mass"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(ids.molten_vessel)
            .map(|stockpile| stockpile.stored_mass()),
        Some(final_molten_remaining),
        "foundry recovery must retain any still-uncast molten remainder"
    );
    assert_eq!(
        state
            .energy()
            .get_store(ids.heat_sink)
            .map(|store| store.stored()),
        Some(recovery.thermal_after),
        "foundry recovery audit must observe the exact post-cast sink state"
    );
    assert_eq!(
        recovery
            .thermal_without_cast
            .checked_add(recovery.released_heat),
        Some(recovery.thermal_after),
        "recovery casting must add exactly its released heat above the same-duration passive-cooling baseline"
    );
}

pub(super) fn remaining_feed_mass(state: &AppState, ids: FoundryIds) -> Mass {
    [ids.pure_copper_source, ids.preheated_source]
        .into_iter()
        .fold(Mass::ZERO, |total, stockpile| {
            let stored = state
                .inventory()
                .get_stockpile(stockpile)
                .map(|record| record.stored_mass())
                .unwrap_or_else(|| panic!("foundry feed stockpile disappeared"));
            total
                .checked_add(stored)
                .unwrap_or_else(|| panic!("foundry remaining feed mass overflowed"))
        })
}

pub(super) fn classify_foundry_outcome(
    unmelted_mass: Mass,
    molten_after_first: Mass,
    molten_final: Mass,
) -> &'static str {
    match (
        unmelted_mass.is_zero(),
        molten_after_first.is_zero(),
        molten_final.is_zero(),
    ) {
        (true, true, true) => "full-order-complete",
        (true, false, true) => "full-order-recovered-after-cooldown",
        (true, false, false) => "partial-order-cast-limited",
        (false, _, true) => "partial-order-melt-limited",
        (false, _, false) => "partial-order-melt-and-cast-limited",
        (true, true, false) => {
            unreachable!("no first-cast remainder cannot create a later molten remainder")
        }
    }
}
