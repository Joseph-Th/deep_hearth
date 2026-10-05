//! Focused pure-copper melt/cast capability probe.

#[path = "foundry_probe/execution.rs"]
mod execution;
#[path = "foundry_probe/reporting.rs"]
mod reporting;

use super::focused_case::{FocusedProbeCase, FocusedProbeRole};
use super::focused_witnesses::FOUNDRY_THERMAL_RECOVERY_COVERAGE_SEED;
use super::foundry_setup::{FoundryIds, setup_foundry_probe};
use super::material_selection::select_stockpile_mass;
use super::production_timing::finish_uninterrupted_production_job;
use deep_hearth::content::{
    PROCESS_CAST_PURE_COPPER, PROCESS_HEAT_MATERIAL_BATCH, PROCESS_MELT_PURE_COPPER,
};

use super::foundry_probe_generation::probe_setup;
use deep_hearth::core::quantity::{Energy, Mass, Temperature};
use deep_hearth::core::state::validate_loaded_state;
use deep_hearth::core::time::TickSpan;
use deep_hearth::energy::EnergySupplyError;
use deep_hearth::inventory::StockpileId;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::production::validate_start_process;
use deep_hearth::registry::Registries;
use deep_hearth::thermal::{
    CastingLotMassConstraint, CastingLotMassRequest, CastingRequest, CastingResolutionError,
    MeltingLotMassConstraint, MeltingLotMassRequest, MeltingRequest, MeltingResolutionError,
    ResolvedCasting, ResolvedMelting, SensibleHeatingRequest, SensibleHeatingResolutionError,
    assess_casting_lot_mass_envelope, assess_melting_lot_mass_envelope, resolve_casting_process,
    resolve_melting_process, resolve_sensible_heating_process,
};
use execution::{
    PrimaryCycleAudit, assert_preheat_partitions_melting_energy, audit_primary_cycle,
    audit_recovery, capture_initial_accounting, classify_foundry_outcome, cool_thermal_sink_until,
    execute_melt, execute_primary_cast, remaining_feed_mass,
};
use reporting::FoundryReport;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MeltBatchLimit {
    OfferedBatch,
    EquipmentCapacity,
    TransferEnergyRange,
    FiniteEnergy,
    ConditionLifetime,
}

impl MeltBatchLimit {
    const fn label(self) -> &'static str {
        match self {
            Self::OfferedBatch => "offered-batch",
            Self::EquipmentCapacity => "equipment-capacity",
            Self::TransferEnergyRange => "transfer-energy-range",
            Self::FiniteEnergy => "finite-energy",
            Self::ConditionLifetime => "condition-lifetime",
        }
    }
}

fn melt_batch_limit(constraint: Option<MeltingLotMassConstraint>) -> MeltBatchLimit {
    match constraint {
        None => MeltBatchLimit::OfferedBatch,
        Some(MeltingLotMassConstraint::EquipmentCapacity) => MeltBatchLimit::EquipmentCapacity,
        Some(MeltingLotMassConstraint::TransferEnergyRange) => MeltBatchLimit::TransferEnergyRange,
        Some(MeltingLotMassConstraint::FiniteEnergy) => MeltBatchLimit::FiniteEnergy,
        Some(MeltingLotMassConstraint::ConditionLifetime) => MeltBatchLimit::ConditionLifetime,
    }
}

fn resolve_melt_for_mass(
    registries: &Registries,
    state: &deep_hearth::core::state::AppState,
    ids: FoundryIds,
    source: StockpileId,
    mass: Mass,
) -> Result<ResolvedMelting, MeltingResolutionError> {
    let selection = select_stockpile_mass(state, source, mass, "foundry melt offer");
    resolve_melting_process(
        registries,
        state,
        MeltingRequest::new(
            PROCESS_MELT_PURE_COPPER,
            source,
            selection.as_slice(),
            ids.furnace,
            ids.electrical_buffer,
        ),
    )
}

#[derive(Clone, Copy, Debug)]
struct PreheatResult {
    source: StockpileId,
    energy: Energy,
    duration: TickSpan,
    applied: bool,
}

impl PreheatResult {
    const fn skipped(source: StockpileId) -> Self {
        Self {
            source,
            energy: Energy::ZERO,
            duration: TickSpan::new(0),
            applied: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct HeatingRouteEvidence {
    pub(super) processed_mass: Mass,
    pub(super) total_duration: TickSpan,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct HeatingDecision {
    pub(super) direct: Option<HeatingRouteEvidence>,
    pub(super) preheated: Option<HeatingRouteEvidence>,
}

fn route_evidence(
    resolved: &ResolvedMelting,
    processed_mass: Mass,
    prior_duration: TickSpan,
) -> HeatingRouteEvidence {
    HeatingRouteEvidence {
        processed_mass,
        total_duration: TickSpan::new(
            prior_duration
                .value()
                .checked_add(resolved.process_resolution().duration().value())
                .unwrap_or_else(|| panic!("foundry heating-route duration overflowed")),
        ),
    }
}

pub(super) fn choose_heating_strategy(
    registries: &Registries,
    state: &deep_hearth::core::state::AppState,
    ids: FoundryIds,
    mass: Mass,
    target: Temperature,
) -> HeatingDecision {
    let direct =
        resolve_largest_feasible_melt(registries, state, ids, ids.pure_copper_source, mass)
            .map(|(resolved, processed, _)| route_evidence(&resolved, processed, TickSpan::new(0)));
    // Same-furnace, same-source preheat is diagnostic only: it partitions the sensible heat that
    // direct melting already owns, so it cannot make an otherwise infeasible order affordable.
    // Keep the alternate on a clone to detect future content that creates a real physical tradeoff.
    let mut preheated_state = state.clone();
    let preheat = execute_optional_preheat(registries, &mut preheated_state, ids, mass, target);
    let preheated = preheat
        .applied
        .then(|| {
            resolve_largest_feasible_melt(registries, &preheated_state, ids, preheat.source, mass)
                .map(|(resolved, processed, _)| {
                    assert_preheat_partitions_melting_energy(registries, mass, preheat, &resolved);
                    route_evidence(&resolved, processed, preheat.duration)
                })
        })
        .flatten();

    if let (Some(direct), Some(preheated)) = (direct, preheated) {
        assert!(
            !(preheated.processed_mass > direct.processed_mass
                || (preheated.processed_mass == direct.processed_mass
                    && preheated.total_duration < direct.total_duration)),
            "same-source preheat gained a real physical advantage; promote it from diagnostic evidence to a player-visible strategy"
        );
    }

    HeatingDecision { direct, preheated }
}

fn execute_optional_preheat(
    registries: &Registries,
    state: &mut deep_hearth::core::state::AppState,
    ids: FoundryIds,
    mass: Mass,
    target: Temperature,
) -> PreheatResult {
    let selection = select_stockpile_mass(
        state,
        ids.pure_copper_source,
        mass,
        "foundry sensible-preheat offer",
    );
    let resolved = match resolve_sensible_heating_process(
        registries,
        state,
        SensibleHeatingRequest::new(
            PROCESS_HEAT_MATERIAL_BATCH,
            ids.pure_copper_source,
            selection.as_slice(),
            ids.furnace,
            ids.electrical_buffer,
            target,
        ),
    ) {
        Ok(resolved) => resolved,
        Err(SensibleHeatingResolutionError::Energy(EnergySupplyError::InsufficientEnergy {
            ..
        }))
        | Err(SensibleHeatingResolutionError::ConditionDuration(_)) => {
            return PreheatResult::skipped(ids.pure_copper_source);
        }
        Err(error) => panic!("foundry sensible-preheat resolution failed unexpectedly: {error}"),
    };
    let energy = resolved.required_energy();
    let duration = resolved.process_resolution().duration();
    let job = validate_start_process(
        registries,
        state,
        resolved.process_resolution(),
        ids.pure_copper_source,
        ids.preheated_source,
    )
    .unwrap_or_else(|error| panic!("foundry sensible-preheat start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("foundry sensible-preheat commit failed: {error}"));
    finish_uninterrupted_production_job(registries, state, job, "foundry sensible preheat");
    PreheatResult {
        source: ids.preheated_source,
        energy,
        duration,
        applied: true,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CastBatchLimit {
    OfferedBatch,
    EquipmentCapacity,
    TransferEnergyRange,
    ThermalSinkCapacity,
    ConditionLifetime,
}

impl CastBatchLimit {
    const fn label(self) -> &'static str {
        match self {
            Self::OfferedBatch => "offered-batch",
            Self::EquipmentCapacity => "equipment-capacity",
            Self::TransferEnergyRange => "transfer-energy-range",
            Self::ThermalSinkCapacity => "thermal-sink-capacity",
            Self::ConditionLifetime => "condition-lifetime",
        }
    }
}

fn cast_batch_limit(constraint: Option<CastingLotMassConstraint>) -> CastBatchLimit {
    match constraint {
        None => CastBatchLimit::OfferedBatch,
        Some(CastingLotMassConstraint::EquipmentCapacity) => CastBatchLimit::EquipmentCapacity,
        Some(CastingLotMassConstraint::TransferEnergyRange) => CastBatchLimit::TransferEnergyRange,
        Some(CastingLotMassConstraint::ConditionLifetime) => CastBatchLimit::ConditionLifetime,
        Some(CastingLotMassConstraint::ThermalSinkCapacity) => CastBatchLimit::ThermalSinkCapacity,
    }
}

fn resolve_cast_for_mass(
    registries: &Registries,
    state: &deep_hearth::core::state::AppState,
    ids: FoundryIds,
    mass: Mass,
) -> Result<ResolvedCasting, CastingResolutionError> {
    let selection = select_stockpile_mass(state, ids.molten_vessel, mass, "foundry molten offer");
    resolve_casting_process(
        registries,
        state,
        CastingRequest::new(
            PROCESS_CAST_PURE_COPPER,
            ids.molten_vessel,
            selection.as_slice(),
            ids.mold,
            ids.heat_sink,
        ),
    )
}

pub(super) fn resolve_largest_feasible_cast(
    registries: &Registries,
    state: &deep_hearth::core::state::AppState,
    ids: FoundryIds,
    offered: Mass,
) -> Option<(ResolvedCasting, Mass, CastBatchLimit)> {
    let selection = select_stockpile_mass(state, ids.molten_vessel, offered, "foundry cast offer");
    let [selection] = selection.as_slice() else {
        panic!("foundry casting projection requires one homogeneous molten lot")
    };
    let envelope = assess_casting_lot_mass_envelope(
        registries,
        state,
        CastingLotMassRequest::new(
            PROCESS_CAST_PURE_COPPER,
            ids.molten_vessel,
            *selection,
            ids.mold,
            ids.heat_sink,
        ),
    )
    .unwrap_or_else(|error| panic!("foundry casting mass projection failed unexpectedly: {error}"));
    let processed = envelope.maximum_mass();
    if processed.is_zero() {
        return None;
    }
    let resolved = resolve_cast_for_mass(registries, state, ids, processed)
        .unwrap_or_else(|error| panic!("foundry selected feasible cast became invalid: {error}"));
    let limit = cast_batch_limit(envelope.limiting_constraint());
    Some((resolved, processed, limit))
}

#[derive(Clone, Copy, Debug)]
struct RecoveryCast {
    cast_mass: Mass,
    remaining_mass: Mass,
    limit: CastBatchLimit,
    duration: TickSpan,
    released_heat: Energy,
    thermal_without_cast: Energy,
    thermal_after: Energy,
}

fn execute_recovery_cast(
    registries: &Registries,
    state: &mut deep_hearth::core::state::AppState,
    ids: FoundryIds,
    offered: Mass,
) -> Option<RecoveryCast> {
    let (casting, cast_mass, limit) =
        resolve_largest_feasible_cast(registries, state, ids, offered)?;
    let duration = casting.process_resolution().duration();
    let released_heat = casting.released_energy();
    let mut no_cast_baseline = state.clone();
    super::temporal::advance_idle_ticks(
        registries,
        &mut no_cast_baseline,
        duration.value(),
        "foundry recovery no-cast thermal baseline",
    );
    let thermal_without_cast = no_cast_baseline
        .energy()
        .get_store(ids.heat_sink)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("foundry heat sink disappeared from recovery baseline"));
    let job = validate_start_process(
        registries,
        state,
        casting.process_resolution(),
        ids.molten_vessel,
        ids.cast_storage,
    )
    .unwrap_or_else(|error| panic!("foundry recovery casting start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("foundry recovery casting commit failed: {error}"));
    finish_uninterrupted_production_job(registries, state, job, "foundry recovery casting");
    let remaining_mass = offered
        .checked_sub(cast_mass)
        .unwrap_or_else(|| unreachable!("recovery cast cannot exceed its offered molten mass"));
    let thermal_after = state
        .energy()
        .get_store(ids.heat_sink)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("foundry heat sink disappeared after recovery cast"));
    Some(RecoveryCast {
        cast_mass,
        remaining_mass,
        limit,
        duration,
        released_heat,
        thermal_without_cast,
        thermal_after,
    })
}

pub(super) fn resolve_largest_feasible_melt(
    registries: &Registries,
    state: &deep_hearth::core::state::AppState,
    ids: FoundryIds,
    source: StockpileId,
    offered: Mass,
) -> Option<(ResolvedMelting, Mass, MeltBatchLimit)> {
    let selection = select_stockpile_mass(state, source, offered, "foundry melt offer");
    let [selection] = selection.as_slice() else {
        panic!("foundry melting projection requires one homogeneous feed lot")
    };
    let envelope = assess_melting_lot_mass_envelope(
        registries,
        state,
        MeltingLotMassRequest::new(
            PROCESS_MELT_PURE_COPPER,
            source,
            *selection,
            ids.furnace,
            ids.electrical_buffer,
        ),
    )
    .unwrap_or_else(|error| panic!("foundry melting mass projection failed unexpectedly: {error}"));
    let processed = envelope.maximum_mass();
    if processed.is_zero() {
        return None;
    }
    let resolved = resolve_melt_for_mass(registries, state, ids, source, processed)
        .unwrap_or_else(|error| panic!("foundry selected feasible melt became invalid: {error}"));
    let limit = melt_batch_limit(envelope.limiting_constraint());
    Some((resolved, processed, limit))
}

pub(super) fn run_foundry_capability_probe(registries: &Registries, case: FocusedProbeCase) {
    let seed = case.seed();
    let setup = probe_setup(registries, seed);
    let mass = setup.mass;
    let feed_form = setup.feed_form;
    let preheat_target = setup.preheat_target;
    let initial_furnace_condition = setup.furnace_condition;
    let initial_mold_condition = setup.mold_condition;
    let (mut state, ids) = setup_foundry_probe(registries, setup);
    let initial = capture_initial_accounting(&state, ids);
    let heating = choose_heating_strategy(registries, &state, ids, mass, preheat_target);
    let preheat = PreheatResult::skipped(ids.pure_copper_source);
    let Some((melt, processed_mass, melt_limit)) =
        resolve_largest_feasible_melt(registries, &state, ids, preheat.source, mass)
    else {
        assert!(
            case.role() != FocusedProbeRole::MaintainedAnchor,
            "maintained foundry anchor must always admit a nonzero melt batch"
        );
        validate_loaded_state(registries, &state)
            .unwrap_or_else(|error| panic!("foundry no-work stop-state audit failed: {error}"));
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!("foundry no-work matter audit failed: {error}"))
                .total(),
            initial.matter
        );
        reviewln!(
            "FOUNDRY REVIEW seed=0x{seed:016X} sample={} role=capability-only outcome=stopped stage=melt feed-form={} heating-strategy=direct-melt same-source-preheat=counterfactual-only blocker=no-feasible-batch electrical={}nJ matter=conserved",
            case.role().label(),
            feed_form.value(),
            initial.electrical.nanojoules(),
        );
        return;
    };
    if case.role() == FocusedProbeRole::MaintainedAnchor {
        assert_eq!(
            processed_mass, mass,
            "maintained foundry anchor must preserve the full offered-batch capability contract"
        );
    }
    assert_preheat_partitions_melting_energy(registries, mass, preheat, &melt);
    let melt_duration = execute_melt(registries, &mut state, ids, preheat.source, &melt);
    let thermal_before_cast = state
        .energy()
        .get_store(ids.heat_sink)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("foundry heat sink disappeared before casting"));

    let Some((casting, cast_mass, cast_limit)) =
        resolve_largest_feasible_cast(registries, &state, ids, processed_mass)
    else {
        assert!(
            case.role() != FocusedProbeRole::MaintainedAnchor,
            "maintained foundry anchor must always admit a nonzero cast batch"
        );
        validate_loaded_state(registries, &state)
            .unwrap_or_else(|error| panic!("foundry cast-stop state audit failed: {error}"));
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!("foundry cast-stop matter audit failed: {error}"))
                .total(),
            initial.matter
        );
        reviewln!(
            "FOUNDRY REVIEW seed=0x{seed:016X} sample={} role=capability-only outcome=stopped stage=cast feed-form={} blocker=no-feasible-batch melted={}mg molten={}mg matter=conserved",
            case.role().label(),
            feed_form.value(),
            processed_mass.milligrams(),
            processed_mass.milligrams(),
        );
        return;
    };
    if case.role() == FocusedProbeRole::MaintainedAnchor {
        assert_eq!(
            cast_mass, processed_mass,
            "maintained foundry anchor must cast the full melted batch"
        );
    }
    let primary_cast = execute_primary_cast(
        registries,
        &mut state,
        ids,
        processed_mass,
        &casting,
        cast_mass,
        cast_limit,
    );
    let cycle = audit_primary_cycle(
        registries,
        &state,
        &melt,
        PrimaryCycleAudit {
            ids,
            initial,
            preheat,
            thermal_before_cast,
            cast: primary_cast,
        },
    );
    let recovery_target = primary_cast.molten_remaining;
    let cooldown = cool_thermal_sink_until(
        registries,
        &mut state,
        ids,
        cycle.final_thermal,
        |candidate_state| {
            recovery_target.is_zero()
                || resolve_largest_feasible_cast(registries, candidate_state, ids, recovery_target)
                    .is_some_and(|(_, feasible_mass, _)| feasible_mass == recovery_target)
        },
    );
    let recovery = if primary_cast.molten_remaining.is_zero() {
        None
    } else {
        execute_recovery_cast(registries, &mut state, ids, primary_cast.molten_remaining)
    };
    if let Some(recovery) = recovery {
        assert_eq!(
            recovery.cast_mass, primary_cast.molten_remaining,
            "foundry cooldown must stop at the first tick where the complete retained molten batch is castable"
        );
    }
    let recovered_cast_mass = recovery.map_or(Mass::ZERO, |recovery| recovery.cast_mass);
    let final_molten_remaining = recovery.map_or(primary_cast.molten_remaining, |recovery| {
        recovery.remaining_mass
    });
    let recovery_limit = recovery.map_or("not-needed", |recovery| recovery.limit.label());
    let recovery_duration = recovery.map_or(TickSpan::new(0), |recovery| recovery.duration);
    let recovery_heat = recovery.map_or(Energy::ZERO, |recovery| recovery.released_heat);
    if let Some(recovery) = recovery {
        audit_recovery(
            registries,
            &state,
            ids,
            initial.matter,
            primary_cast.cast_mass,
            final_molten_remaining,
            recovery,
        );
    }
    let unmelted_mass = mass.checked_sub(processed_mass).unwrap_or_else(|| {
        unreachable!("adaptive melt cannot process more than the offered batch")
    });
    assert_eq!(
        remaining_feed_mass(&state, ids),
        unmelted_mass,
        "adaptive melting must leave the unprocessed portion of the offered order physically owned"
    );
    let outcome = classify_foundry_outcome(
        unmelted_mass,
        primary_cast.molten_remaining,
        final_molten_remaining,
    );
    if case.role() == FocusedProbeRole::MaintainedCoverage {
        assert_eq!(case.seed(), FOUNDRY_THERMAL_RECOVERY_COVERAGE_SEED);
        assert_eq!(
            primary_cast.limit,
            CastBatchLimit::ThermalSinkCapacity,
            "maintained foundry recovery coverage must preserve thermal-sink-limited first casting"
        );
        assert!(
            !primary_cast.molten_remaining.is_zero(),
            "thermal-limited first casting must retain a physical molten remainder"
        );
        assert!(
            recovery.is_some(),
            "thermal coverage must exercise cooldown recovery"
        );
        assert_eq!(
            final_molten_remaining,
            Mass::ZERO,
            "thermal coverage must recover the complete retained molten batch after cooldown"
        );
    }
    FoundryReport {
        seed,
        sample: case.role().label(),
        outcome,
        feed_form,
        offered: mass,
        melted: processed_mass,
        unmelted: unmelted_mass,
        melt_limit: melt_limit.label(),
        first_cast: primary_cast.cast_mass,
        cast_limit: primary_cast.limit.label(),
        molten_after_first: primary_cast.molten_remaining,
        recovery_cast: recovered_cast_mass,
        recovery_limit,
        molten_final: final_molten_remaining,
        heating_strategy: "direct-melt",
        direct_heating_mass: heating
            .direct
            .map_or(Mass::ZERO, |route| route.processed_mass),
        direct_heating_duration: heating
            .direct
            .map_or(TickSpan::new(0), |route| route.total_duration),
        preheated_mass: heating
            .preheated
            .map_or(Mass::ZERO, |route| route.processed_mass),
        preheated_duration: heating
            .preheated
            .map_or(TickSpan::new(0), |route| route.total_duration),
        preheat_applied: preheat.applied,
        preheat_target,
        preheat_energy: preheat.energy,
        preheat_duration: preheat.duration,
        furnace_condition: initial_furnace_condition,
        mold_condition: initial_mold_condition,
        initial_electrical: initial.electrical,
        melt_energy: melt.required_energy(),
        final_electrical: cycle.final_electrical,
        initial_thermal: initial.thermal,
        thermal_before_cast,
        thermal_without_cast: primary_cast.thermal_without_cast,
        released_heat: primary_cast.released_heat,
        final_thermal: cycle.final_thermal,
        thermal_after_cooldown: cooldown.thermal_after_cooldown,
        cooldown_ticks: cooldown.ticks,
        recovery_heat,
        melt_duration,
        cast_duration: primary_cast.duration,
        recovery_duration,
    }
    .print();
}
