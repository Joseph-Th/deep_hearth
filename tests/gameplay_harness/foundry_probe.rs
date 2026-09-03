//! Focused pure-copper melt/cast capability probe.

#[path = "foundry_probe/execution.rs"]
mod execution;
#[path = "foundry_probe/reporting.rs"]
mod reporting;

use super::environment::ROOM_TEMPERATURE;
use super::equipment_support::nominal_equipment_mass_capability;
use super::focused_runner::focused_probe_role_label;
use super::focused_seeds::{FocusedProbeCase, FocusedProbeRole};
use super::foundry_setup::{FoundryIds, FoundrySetup, setup_foundry_probe};
use super::material_selection::select_stockpile_mass;
use super::production_support::varied_healthy_condition;
use super::production_timing::finish_uninterrupted_production_job;
use super::seed::mix64;
use deep_hearth::content::{
    ENERGY_ELECTRICAL_BUFFER, ENERGY_THERMAL_SINK, EQUIPMENT_CASTING_MOLD,
    EQUIPMENT_ELECTRIC_FURNACE, MATERIAL_COPPER, PROCESS_CAST_PURE_COPPER,
    PROCESS_HEAT_MATERIAL_BATCH, PROCESS_MELT_PURE_COPPER,
};

use deep_hearth::core::quantity::{Energy, Mass, Temperature};
use deep_hearth::core::state::validate_loaded_state;
use deep_hearth::core::time::TickSpan;
use deep_hearth::energy::EnergySupplyError;
use deep_hearth::inventory::StockpileId;
use deep_hearth::material::MaterialComposition;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::production::validate_start_process;
use deep_hearth::registry::Registries;
use deep_hearth::thermal::{
    CastingLotMassConstraint, CastingLotMassRequest, CastingRequest, CastingResolutionError,
    MeltingLotMassConstraint, MeltingLotMassRequest, MeltingRequest, MeltingResolutionError,
    ResolvedCasting, ResolvedMelting, SensibleHeatingRequest, SensibleHeatingResolutionError,
    assess_casting_lot_mass_envelope, assess_melting_lot_mass_envelope, calculate_fusion_heat,
    calculate_sensible_heat, resolve_casting_process, resolve_melting_process,
    resolve_sensible_heating_process,
};
use execution::{
    assert_preheat_partitions_melting_energy, audit_primary_cycle, audit_recovery,
    capture_initial_accounting, classify_foundry_outcome, cool_thermal_sink, execute_melt,
    execute_primary_cast, remaining_feed_mass,
};
use reporting::FoundryReport;

pub(super) fn probe_setup(registries: &Registries, seed: u64) -> FoundrySetup {
    let melting = registries
        .thermal()
        .get_melting(PROCESS_MELT_PURE_COPPER)
        .unwrap_or_else(|| panic!("canonical melting definition disappeared"));
    let heating = registries
        .thermal()
        .get_sensible_heating(PROCESS_HEAT_MATERIAL_BATCH)
        .unwrap_or_else(|| panic!("canonical sensible-heating definition disappeared"));
    let casting = registries
        .thermal()
        .get_casting(PROCESS_CAST_PURE_COPPER)
        .unwrap_or_else(|| panic!("canonical casting definition disappeared"));
    let melt_maximum = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_ELECTRIC_FURNACE,
        melting.max_batch_mass_capability(),
    );
    let heat_maximum = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_ELECTRIC_FURNACE,
        heating.max_batch_mass_capability(),
    );
    let cast_maximum = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_CASTING_MOLD,
        casting.max_batch_mass_capability(),
    );
    let maximum = heat_maximum
        .milligrams()
        .min(melt_maximum.milligrams())
        .min(cast_maximum.milligrams());
    assert!(maximum > 0, "foundry probe requires a nonzero legal batch");
    let feed_forms = melting.solid_forms();
    let feed_form_count = u64::try_from(feed_forms.len())
        .unwrap_or_else(|_| panic!("foundry feed-form count exceeded u64"));
    let feed_index = usize::try_from(seed % feed_form_count)
        .unwrap_or_else(|_| panic!("foundry feed-form index exceeded usize"));
    let feed_form = feed_forms[feed_index];
    let minimum = maximum.div_ceil(2);
    let mass = Mass::from_milligrams(minimum + mix64(seed ^ 0xF0A1_DA7A) % (maximum - minimum + 1));
    let melting_point = registries
        .materials()
        .get_material(MATERIAL_COPPER)
        .and_then(|material| material.properties().thermal().melting_point())
        .unwrap_or_else(|| panic!("foundry probe copper melting point disappeared"));
    let ambient = ROOM_TEMPERATURE.millikelvin();
    let melting = melting_point.millikelvin();
    assert!(
        melting > ambient,
        "foundry probe requires copper to melt above room temperature"
    );
    let preheat_span = (melting - ambient) * 3 / 4;
    assert!(preheat_span > 0, "foundry preheat span must be nonzero");
    let preheat_offset =
        u32::try_from(1 + mix64(seed ^ 0x5448_4552_4D41_4C49) % u64::from(preheat_span))
            .unwrap_or_else(|_| panic!("foundry preheat offset exceeded u32"));
    let preheat_target = Temperature::from_millikelvin(ambient + preheat_offset);
    let composition = MaterialComposition::pure(MATERIAL_COPPER);
    let sensible = calculate_sensible_heat(
        registries.materials(),
        mass,
        &composition,
        ROOM_TEMPERATURE,
        melting_point,
    )
    .unwrap_or_else(|error| panic!("foundry probe sensible heating calculation failed: {error}"))
    .energy();
    let fusion = calculate_fusion_heat(registries.materials(), mass, MATERIAL_COPPER)
        .unwrap_or_else(|error| panic!("foundry probe fusion calculation failed: {error}"))
        .energy();
    let required_electrical = sensible
        .checked_add(fusion)
        .unwrap_or_else(|| panic!("foundry probe required electrical energy overflowed"));
    let electrical_capacity = registries
        .energy()
        .get_store(ENERGY_ELECTRICAL_BUFFER)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("foundry probe electrical-buffer definition disappeared"));
    assert!(
        electrical_capacity >= required_electrical,
        "foundry electrical buffer must remain capable of the maintained full-batch contract"
    );
    // Seed-derived resource pressure keeps replay independent of runner role and spans
    // under- and over-provisioned electrical budgets.
    let energy_budget_ppm = 400_000 + (mix64(seed ^ 0x454C_4543_4845_4147) % 950_001) as u32;
    let electrical_budget = Energy::from_nanojoules(
        required_electrical
            .nanojoules()
            .checked_mul(u128::from(energy_budget_ppm))
            .map(|scaled| scaled / 1_000_000)
            .unwrap_or_else(|| panic!("foundry electrical budget scaling overflowed")),
    );
    let electrical_energy = std::cmp::min(electrical_budget, electrical_capacity);
    let thermal_capacity = registries
        .energy()
        .get_store(ENERGY_THERMAL_SINK)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("foundry probe thermal-sink definition disappeared"));
    // Existing heat competes with casting for finite sink capacity. A bimodal cool/saturated
    // distribution covers both unconstrained throughput and meaningful thermal-recovery pressure.
    let thermal_roll = mix64(seed ^ 0x5448_4552_4D53_494F);
    let thermal_pressure_ppm = if thermal_roll.is_multiple_of(5) {
        (thermal_roll % 250_001) as u32
    } else {
        900_000 + ((thermal_roll >> 8) % 100_001) as u32
    };
    let thermal_sink_energy = Energy::from_nanojoules(
        thermal_capacity
            .nanojoules()
            .checked_mul(u128::from(thermal_pressure_ppm))
            .map(|scaled| scaled / 1_000_000)
            .unwrap_or_else(|| panic!("foundry thermal pressure scaling overflowed")),
    );
    FoundrySetup {
        mass,
        feed_form,
        preheat_target,
        furnace_condition: varied_healthy_condition(
            registries,
            EQUIPMENT_ELECTRIC_FURNACE,
            mix64(seed ^ 0x4655_524E_4143_4543),
        ),
        mold_condition: varied_healthy_condition(
            registries,
            EQUIPMENT_CASTING_MOLD,
            mix64(seed ^ 0x4D4F_4C44_434F_4E44),
        ),
        electrical_energy,
        thermal_sink_energy,
    }
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HeatingStrategy {
    Direct,
    Preheat,
}

impl HeatingStrategy {
    const fn label(self) -> &'static str {
        match self {
            Self::Direct => "direct-melt",
            Self::Preheat => "preheat-then-melt",
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
    pub(super) strategy: HeatingStrategy,
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

pub(super) fn heating_route_is_better(
    candidate: HeatingRouteEvidence,
    current: HeatingRouteEvidence,
) -> bool {
    candidate.processed_mass > current.processed_mass
        || (candidate.processed_mass == current.processed_mass
            && candidate.total_duration < current.total_duration)
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

    let strategy = match (direct, preheated) {
        (None, Some(_)) => HeatingStrategy::Preheat,
        (Some(direct), Some(preheated)) if heating_route_is_better(preheated, direct) => {
            HeatingStrategy::Preheat
        }
        _ => HeatingStrategy::Direct,
    };
    HeatingDecision {
        strategy,
        direct,
        preheated,
    }
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
    finish_uninterrupted_production_job(
        registries,
        state,
        job,
        duration,
        "foundry sensible preheat",
    );
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
    finish_uninterrupted_production_job(
        registries,
        state,
        job,
        duration,
        "foundry recovery casting",
    );
    let remaining_mass = offered
        .checked_sub(cast_mass)
        .unwrap_or_else(|| unreachable!("recovery cast cannot exceed its offered molten mass"));
    Some(RecoveryCast {
        cast_mass,
        remaining_mass,
        limit,
        duration,
        released_heat,
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
    let (mut state, ids) = setup_foundry_probe(registries, seed, setup);
    let initial = capture_initial_accounting(&state, ids);
    let heating = choose_heating_strategy(registries, &state, ids, mass, preheat_target);
    let preheat = match heating.strategy {
        HeatingStrategy::Direct => PreheatResult::skipped(ids.pure_copper_source),
        HeatingStrategy::Preheat => {
            execute_optional_preheat(registries, &mut state, ids, mass, preheat_target)
        }
    };
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
            "FOUNDRY REVIEW seed=0x{seed:016X} sample={} role=capability-only outcome=stopped stage=melt feed-form={} heating-strategy={} preheat=[applied:{} target:{}mK energy:{}nJ duration:{}t] blocker=no-feasible-batch electrical={}nJ matter=conserved",
            focused_probe_role_label(case.role()),
            feed_form.value(),
            heating.strategy.label(),
            preheat.applied,
            preheat_target.millikelvin(),
            preheat.energy.nanojoules(),
            preheat.duration.value(),
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
            focused_probe_role_label(case.role()),
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
        ids,
        initial,
        preheat,
        &melt,
        thermal_before_cast,
        primary_cast,
    );
    let cooldown = cool_thermal_sink(registries, &mut state, ids, cycle.final_thermal);
    let recovery = if primary_cast.molten_remaining.is_zero() {
        None
    } else {
        execute_recovery_cast(registries, &mut state, ids, primary_cast.molten_remaining)
    };
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
        assert_eq!(case.seed(), 2, "unknown maintained foundry coverage seed");
        assert_eq!(
            primary_cast.limit,
            CastBatchLimit::ThermalSinkCapacity,
            "foundry coverage seed 2 must preserve thermal-sink-limited first casting"
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
        sample: focused_probe_role_label(case.role()),
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
        heating_strategy: heating.strategy.label(),
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
        cooled_thermal: cooldown.cooled_thermal,
        cooldown_ticks: cooldown.ticks,
        recovery_heat,
        melt_duration,
        cast_duration: primary_cast.duration,
        recovery_duration,
    }
    .print();
}
