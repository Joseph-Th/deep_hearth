//! Focused pure-copper melt/cast capability probe.

use super::focused_runner::focused_probe_role_label;
use super::focused_seeds::{FocusedProbeCase, FocusedProbeRole};
use super::foundry_setup::{FoundryIds, FoundrySetup, setup_foundry_probe};
use super::production_support::{
    finish_uninterrupted_production_job, select_stockpile_mass, varied_healthy_condition,
};
use super::seed::mix64;
use super::support::{ROOM_TEMPERATURE, nominal_equipment_mass_capability};
use deep_hearth::content::{
    ENERGY_ELECTRICAL_BUFFER, ENERGY_THERMAL_SINK, EQUIPMENT_CASTING_MOLD,
    EQUIPMENT_ELECTRIC_FURNACE, MATERIAL_COPPER, PROCESS_CAST_PURE_COPPER,
    PROCESS_MELT_PURE_COPPER,
};
use deep_hearth::core::quantity::{Energy, Mass, Temperature};
use deep_hearth::core::state::validate_loaded_state;
use deep_hearth::core::time::TickSpan;
use deep_hearth::energy::{EnergySinkError, EnergySupplyError, PowerRemainder, integrate_power};
use deep_hearth::inventory::MaterialLotSelection;
use deep_hearth::material::MaterialComposition;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::production::validate_start_process;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::thermal::{
    CastingRequest, CastingResolutionError, MeltingRequest, MeltingResolutionError,
    ResolvedCasting, ResolvedMelting, calculate_fusion_heat, calculate_sensible_heat,
    resolve_casting_process, resolve_melting_process,
};

pub(super) fn probe_setup(registries: &Registries, seed: u64) -> FoundrySetup {
    let melting = registries
        .thermal()
        .get_melting(PROCESS_MELT_PURE_COPPER)
        .unwrap_or_else(|| panic!("canonical melting definition disappeared"));
    let casting = registries
        .thermal()
        .get_casting(PROCESS_CAST_PURE_COPPER)
        .unwrap_or_else(|| panic!("canonical casting definition disappeared"));
    let melt_maximum = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_ELECTRIC_FURNACE,
        melting.max_batch_mass_capability(),
    );
    let cast_maximum = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_CASTING_MOLD,
        casting.max_batch_mass_capability(),
    );
    let maximum = melt_maximum.milligrams().min(cast_maximum.milligrams());
    assert!(maximum > 0, "foundry probe requires a nonzero legal batch");
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
    let input_temperature = Temperature::from_millikelvin(
        ambient + (mix64(seed ^ 0x5448_4552_4D41_4C49) % (u64::from(preheat_span) + 1)) as u32,
    );
    let composition = MaterialComposition::pure(MATERIAL_COPPER);
    let sensible = calculate_sensible_heat(
        registries.materials(),
        mass,
        &composition,
        input_temperature,
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
    // The seed owns resource pressure so explicit replay recreates the same world regardless of
    // runner role. Generated worlds span under- and over-provisioned electrical budgets.
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
    // distribution keeps the probe focused on both unconstrained throughput and meaningful thermal
    // recovery pressure. The maintained anchor falls in the cool bucket through the mixer; organic
    // seeds commonly begin near saturation.
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
        input_temperature,
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
    FiniteEnergy,
    ConditionLifetime,
}

impl MeltBatchLimit {
    const fn label(self) -> &'static str {
        match self {
            Self::OfferedBatch => "offered-batch",
            Self::EquipmentCapacity => "equipment-capacity",
            Self::FiniteEnergy => "finite-energy",
            Self::ConditionLifetime => "condition-lifetime",
        }
    }
}

fn melt_scaling_limit(error: &MeltingResolutionError) -> Option<MeltBatchLimit> {
    match error {
        MeltingResolutionError::BatchMassExceedsEquipmentCapacity { .. } => {
            Some(MeltBatchLimit::EquipmentCapacity)
        }
        MeltingResolutionError::Energy(EnergySupplyError::InsufficientEnergy { .. }) => {
            Some(MeltBatchLimit::FiniteEnergy)
        }
        MeltingResolutionError::ConditionDuration(_) => Some(MeltBatchLimit::ConditionLifetime),
        _ => None,
    }
}

fn resolve_melt_for_mass(
    registries: &Registries,
    state: &deep_hearth::core::state::AppState,
    ids: FoundryIds,
    mass: Mass,
) -> Result<ResolvedMelting, MeltingResolutionError> {
    let selection = [MaterialLotSelection::new(ids.pure_copper_lot, mass)];
    resolve_melting_process(
        registries,
        state,
        MeltingRequest::new(
            PROCESS_MELT_PURE_COPPER,
            ids.pure_copper_source,
            &selection,
            ids.furnace,
            ids.electrical_buffer,
        ),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CastBatchLimit {
    OfferedBatch,
    EquipmentCapacity,
    ThermalSinkCapacity,
    ConditionLifetime,
}

impl CastBatchLimit {
    const fn label(self) -> &'static str {
        match self {
            Self::OfferedBatch => "offered-batch",
            Self::EquipmentCapacity => "equipment-capacity",
            Self::ThermalSinkCapacity => "thermal-sink-capacity",
            Self::ConditionLifetime => "condition-lifetime",
        }
    }
}

fn cast_scaling_limit(error: &CastingResolutionError) -> Option<CastBatchLimit> {
    match error {
        CastingResolutionError::BatchMassExceedsEquipmentCapacity { .. } => {
            Some(CastBatchLimit::EquipmentCapacity)
        }
        CastingResolutionError::EnergySink(EnergySinkError::InsufficientCapacity { .. }) => {
            Some(CastBatchLimit::ThermalSinkCapacity)
        }
        CastingResolutionError::ConditionDuration(_) => Some(CastBatchLimit::ConditionLifetime),
        _ => None,
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
    match resolve_cast_for_mass(registries, state, ids, offered) {
        Ok(resolved) => return Some((resolved, offered, CastBatchLimit::OfferedBatch)),
        Err(error) if cast_scaling_limit(&error).is_some() => {}
        Err(error) => {
            panic!("foundry offered-batch casting resolution failed unexpectedly: {error}")
        }
    }

    let mut low = 1_u64;
    let mut high = offered.milligrams();
    let mut best = 0_u64;
    while low <= high {
        let mid = low + (high - low) / 2;
        match resolve_cast_for_mass(registries, state, ids, Mass::from_milligrams(mid)) {
            Ok(_) => {
                best = mid;
                low = mid + 1;
            }
            Err(error) if cast_scaling_limit(&error).is_some() => {
                high = mid - 1;
            }
            Err(error) => {
                panic!("foundry adaptive casting resolution failed unexpectedly: {error}")
            }
        }
    }
    if best == 0 {
        return None;
    }
    let processed = Mass::from_milligrams(best);
    let resolved = resolve_cast_for_mass(registries, state, ids, processed)
        .unwrap_or_else(|error| panic!("foundry selected feasible cast became invalid: {error}"));
    let offered_error = resolve_cast_for_mass(registries, state, ids, offered)
        .err()
        .unwrap_or_else(|| unreachable!("offered cast was already known to be constrained"));
    let limit = cast_scaling_limit(&offered_error)
        .unwrap_or_else(|| unreachable!("offered cast constraint must remain scale-related"));
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
    offered: Mass,
) -> Option<(ResolvedMelting, Mass, MeltBatchLimit)> {
    match resolve_melt_for_mass(registries, state, ids, offered) {
        Ok(resolved) => return Some((resolved, offered, MeltBatchLimit::OfferedBatch)),
        Err(error) if melt_scaling_limit(&error).is_some() => {}
        Err(error) => panic!("foundry offered-batch melt resolution failed unexpectedly: {error}"),
    }

    let mut low = 1_u64;
    let mut high = offered.milligrams();
    let mut best = 0_u64;
    while low <= high {
        let mid = low + (high - low) / 2;
        match resolve_melt_for_mass(registries, state, ids, Mass::from_milligrams(mid)) {
            Ok(_) => {
                best = mid;
                low = mid + 1;
            }
            Err(error) if melt_scaling_limit(&error).is_some() => {
                high = mid - 1;
            }
            Err(error) => panic!("foundry adaptive melt resolution failed unexpectedly: {error}"),
        }
    }
    if best == 0 {
        return None;
    }
    let processed = Mass::from_milligrams(best);
    let resolved = resolve_melt_for_mass(registries, state, ids, processed)
        .unwrap_or_else(|error| panic!("foundry selected feasible melt became invalid: {error}"));
    let offered_error = resolve_melt_for_mass(registries, state, ids, offered)
        .err()
        .unwrap_or_else(|| unreachable!("offered batch was already known to be constrained"));
    let limit = melt_scaling_limit(&offered_error)
        .unwrap_or_else(|| unreachable!("offered batch constraint must remain scale-related"));
    Some((resolved, processed, limit))
}

pub(super) fn run_foundry_capability_probe(registries: &Registries, case: FocusedProbeCase) {
    let seed = case.seed();
    let setup = probe_setup(registries, seed);
    let mass = setup.mass;
    let input_temperature = setup.input_temperature;
    let initial_furnace_condition = setup.furnace_condition;
    let initial_mold_condition = setup.mold_condition;
    let (mut state, ids) = setup_foundry_probe(registries, seed, setup);
    let initial_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("foundry initial matter accounting failed: {error}"))
        .total();
    let initial_electrical = state
        .energy()
        .get_store(ids.electrical_buffer)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("foundry electrical buffer disappeared after setup"));
    let initial_thermal = state
        .energy()
        .get_store(ids.heat_sink)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("foundry heat sink disappeared after setup"));
    let Some((melt, processed_mass, melt_limit)) =
        resolve_largest_feasible_melt(registries, &state, ids, mass)
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
            initial_matter
        );
        std::println!(
            "FOUNDRY REVIEW seed=0x{seed:016X} sample={} role=capability-only outcome=stopped stage=melt blocker=no-feasible-batch electrical={}nJ matter=conserved",
            focused_probe_role_label(case.role()),
            initial_electrical.nanojoules(),
        );
        return;
    };
    if case.role() == FocusedProbeRole::MaintainedAnchor {
        assert_eq!(
            processed_mass, mass,
            "maintained foundry anchor must preserve the full offered-batch capability contract"
        );
    }
    let melt_duration = melt.process_resolution().duration();
    let melt_job = validate_start_process(
        registries,
        &state,
        melt.process_resolution(),
        ids.pure_copper_source,
        ids.molten_vessel,
    )
    .unwrap_or_else(|error| panic!("foundry probe melt start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("foundry probe melt commit failed: {error}"));
    finish_uninterrupted_production_job(
        registries,
        &mut state,
        melt_job,
        melt_duration,
        "foundry melt",
    );
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
            initial_matter
        );
        std::println!(
            "FOUNDRY REVIEW seed=0x{seed:016X} sample={} role=capability-only outcome=stopped stage=cast blocker=no-feasible-batch melted={}mg molten={}mg matter=conserved",
            focused_probe_role_label(case.role()),
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
    let cast_duration = casting.process_resolution().duration();
    let released_heat = casting.released_energy();
    // Compare the actual cast against a same-duration branch with no cast started. This uses the
    // canonical tick scheduler to account for passive heat rejection instead of reimplementing its
    // timing in the harness. The branch is created only after the cast choice has been resolved.
    let mut no_cast_baseline = state.clone();
    for _ in 0..cast_duration.value() {
        advance_tick(registries, &mut no_cast_baseline).unwrap_or_else(|error| {
            panic!("foundry no-cast thermal baseline tick failed: {error}")
        });
    }
    let thermal_without_cast = no_cast_baseline
        .energy()
        .get_store(ids.heat_sink)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("foundry heat sink disappeared from no-cast baseline"));
    let cast_job = validate_start_process(
        registries,
        &state,
        casting.process_resolution(),
        ids.molten_vessel,
        ids.cast_storage,
    )
    .unwrap_or_else(|error| panic!("foundry probe casting start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("foundry probe casting commit failed: {error}"));
    finish_uninterrupted_production_job(
        registries,
        &mut state,
        cast_job,
        cast_duration,
        "foundry casting",
    );
    let molten_remaining = processed_mass
        .checked_sub(cast_mass)
        .unwrap_or_else(|| unreachable!("cast mass cannot exceed melted mass"));

    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("foundry probe final state audit failed: {error}"));
    let final_matter = calculate_matter_accounting(&state)
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
        final_matter, initial_matter,
        "foundry melt/cast cycle must conserve represented matter"
    );
    assert_eq!(
        initial_electrical.checked_sub(melt.required_energy()),
        Some(final_electrical),
        "foundry melt must consume exactly its resolved electrical energy"
    );
    assert_eq!(
        thermal_without_cast.checked_add(released_heat),
        Some(final_thermal),
        "foundry casting must add exactly its resolved released heat above the canonical same-duration passive-cooling baseline"
    );
    assert!(
        thermal_before_cast <= initial_thermal,
        "passive heat rejection during melting must not increase pre-existing sink energy"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(ids.cast_storage)
            .map(|stockpile| stockpile.stored_mass()),
        Some(cast_mass),
        "foundry capability probe must store exactly the mass accepted by canonical casting"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(ids.molten_vessel)
            .map(|stockpile| stockpile.stored_mass()),
        Some(molten_remaining),
        "an adaptive partial cast must leave the uncast molten remainder physically owned"
    );
    let thermal_sink = registries
        .energy()
        .get_store(ENERGY_THERMAL_SINK)
        .unwrap_or_else(|| panic!("foundry thermal-sink definition disappeared"));
    let passive_dissipation = integrate_power(
        thermal_sink.passive_dissipation_power(),
        TickSpan::new(1),
        registries.core().physical_tick_duration(),
        PowerRemainder::ZERO,
    )
    .unwrap_or_else(|error| panic!("foundry passive thermal dissipation failed: {error}"));
    assert_eq!(
        passive_dissipation.remainder(),
        PowerRemainder::ZERO,
        "foundry thermal sink must dissipate exact whole nanojoules per tick"
    );
    assert!(
        !passive_dissipation.energy().is_zero(),
        "foundry thermal sink must have a nonzero passive cooling route"
    );
    let cooling_ticks = final_thermal
        .nanojoules()
        .div_ceil(passive_dissipation.energy().nanojoules());
    let cooling_ticks = u64::try_from(cooling_ticks)
        .unwrap_or_else(|_| panic!("foundry thermal cooling duration exceeded tick range"));
    for _ in 0..cooling_ticks {
        advance_tick(registries, &mut state)
            .unwrap_or_else(|error| panic!("foundry thermal cooldown tick failed: {error}"));
    }
    let cooled_thermal = state
        .energy()
        .get_store(ids.heat_sink)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("foundry heat sink disappeared during passive cooldown"));
    assert_eq!(
        cooled_thermal,
        Energy::ZERO,
        "foundry thermal sink must recover its full casting capacity without player micromanagement"
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("foundry post-cooldown state audit failed: {error}"));
    let recovery = if molten_remaining.is_zero() {
        None
    } else {
        execute_recovery_cast(registries, &mut state, ids, molten_remaining)
    };
    let recovered_cast_mass = recovery.map_or(Mass::ZERO, |recovery| recovery.cast_mass);
    let final_molten_remaining =
        recovery.map_or(molten_remaining, |recovery| recovery.remaining_mass);
    let recovery_limit = recovery.map_or("not-needed", |recovery| recovery.limit.label());
    let recovery_ticks = recovery.map_or(0, |recovery| recovery.duration.value());
    let recovery_heat = recovery.map_or(Energy::ZERO, |recovery| recovery.released_heat);
    if recovery.is_some() {
        validate_loaded_state(registries, &state)
            .unwrap_or_else(|error| panic!("foundry recovery-cast state audit failed: {error}"));
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!(
                    "foundry recovery-cast matter audit failed: {error}"
                ))
                .total(),
            initial_matter,
            "foundry recovery casting must conserve represented matter"
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(ids.cast_storage)
                .map(|stockpile| stockpile.stored_mass()),
            cast_mass.checked_add(recovered_cast_mass),
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
            Some(recovery_heat),
            "a recovery cast started from an empty sink must capture exactly its released heat"
        );
    }
    let unmelted_mass = mass.checked_sub(processed_mass).unwrap_or_else(|| {
        unreachable!("adaptive melt cannot process more than the offered batch")
    });
    assert_eq!(
        state
            .inventory()
            .get_stockpile(ids.pure_copper_source)
            .map(|stockpile| stockpile.stored_mass()),
        Some(unmelted_mass),
        "adaptive melting must leave the unprocessed portion of the offered order physically owned"
    );
    let outcome = match (
        unmelted_mass.is_zero(),
        molten_remaining.is_zero(),
        final_molten_remaining.is_zero(),
    ) {
        (true, true, true) => "full-order-complete",
        (true, false, true) => "full-order-recovered-after-cooldown",
        (true, false, false) => "partial-order-cast-limited",
        (false, _, true) => "partial-order-melt-limited",
        (false, _, false) => "partial-order-melt-and-cast-limited",
        (true, true, false) => {
            unreachable!("no first-cast remainder cannot create a later molten remainder")
        }
    };
    if case.role() == FocusedProbeRole::MaintainedCoverage {
        assert_eq!(case.seed(), 2, "unknown maintained foundry coverage seed");
        assert_eq!(
            cast_limit,
            CastBatchLimit::ThermalSinkCapacity,
            "foundry coverage seed 2 must preserve thermal-sink-limited first casting"
        );
        assert!(
            !molten_remaining.is_zero(),
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
    if std::env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some() {
        std::println!(
            "CAPABILITY FOUNDRY seed=0x{seed:016X} sample={} outcome={outcome} reachability=bootstrapped-industrial installation=required+structurally-supported role=capability-evidence player-loop=not-claimed system-depth=[phase-change,finite-electrical-input,finite-thermal-recovery,passive-heat-rejection,wear] offered={}mg melted={}mg unmelted={}mg melt-limit={} first-cast={}mg cast-limit={} molten-after-first={}mg recovery-cast={}mg recovery-limit={} molten-final={}mg input={}mK initial-condition=[furnace:{} mold:{}ppm] electrical=[initial:{}nJ melt:{}nJ remaining:{}nJ] thermal=[initial:{}nJ pre-cast:{}nJ no-cast-baseline:{}nJ released:{}nJ captured:{}nJ cooled:{}nJ cooldown:{}t recovery-heat:{}nJ] durations=[melt:{}t cast:{}t recovery-cast:{}t] matter=conserved",
            focused_probe_role_label(case.role()),
            mass.milligrams(),
            processed_mass.milligrams(),
            unmelted_mass.milligrams(),
            melt_limit.label(),
            cast_mass.milligrams(),
            cast_limit.label(),
            molten_remaining.milligrams(),
            recovered_cast_mass.milligrams(),
            recovery_limit,
            final_molten_remaining.milligrams(),
            input_temperature.millikelvin(),
            initial_furnace_condition.parts_per_million(),
            initial_mold_condition.parts_per_million(),
            initial_electrical.nanojoules(),
            melt.required_energy().nanojoules(),
            final_electrical.nanojoules(),
            initial_thermal.nanojoules(),
            thermal_before_cast.nanojoules(),
            thermal_without_cast.nanojoules(),
            released_heat.nanojoules(),
            final_thermal.nanojoules(),
            cooled_thermal.nanojoules(),
            cooling_ticks,
            recovery_heat.nanojoules(),
            melt_duration.value(),
            cast_duration.value(),
            recovery_ticks,
        );
    } else {
        std::println!(
            "FOUNDRY REVIEW seed=0x{seed:016X} sample={} role=capability-only outcome={outcome} pipeline=heat->melt->cast->passive-cool->retry offered={}mg melted={}mg unmelted={}mg melt-limit={} first-cast={}mg cast-limit={} molten-after-first={}mg recovery-cast={}mg recovery-limit={} molten-final={}mg input={}mK electrical=[used:{}nJ remaining:{}nJ] thermal=[initial:{}nJ pre-cast:{}nJ no-cast-baseline:{}nJ captured:{}nJ cooldown:{}t cooled:{}nJ recovery-heat:{}nJ] durations=[melt:{}t cast:{}t recovery-cast:{}t] matter=conserved",
            focused_probe_role_label(case.role()),
            mass.milligrams(),
            processed_mass.milligrams(),
            unmelted_mass.milligrams(),
            melt_limit.label(),
            cast_mass.milligrams(),
            cast_limit.label(),
            molten_remaining.milligrams(),
            recovered_cast_mass.milligrams(),
            recovery_limit,
            final_molten_remaining.milligrams(),
            input_temperature.millikelvin(),
            melt.required_energy().nanojoules(),
            final_electrical.nanojoules(),
            initial_thermal.nanojoules(),
            thermal_before_cast.nanojoules(),
            thermal_without_cast.nanojoules(),
            final_thermal.nanojoules(),
            cooling_ticks,
            cooled_thermal.nanojoules(),
            recovery_heat.nanojoules(),
            melt_duration.value(),
            cast_duration.value(),
            recovery_ticks,
        );
    }
}
