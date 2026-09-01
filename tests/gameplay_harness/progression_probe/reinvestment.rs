//! Mature primitive reinvestment after the first pick/crank convergence.
//!
//! This subepisode branches from an ordinary player-visible decision state and spends only matter,
//! evidence, equipment, and stored work already owned there. It proves that later copper is not a
//! flat stat bump: crusher throughput, accumulator capacity, residual stored work, survival cost,
//! and batch sizing constrain one another through the canonical runtime APIs.

use super::*;

fn run_reinvestment_separation(
    registries: &Registries,
    state: &mut AppState,
    plan: PrimitiveSeparationPlan,
    context: &'static str,
) -> PrimitiveSeparationWork {
    let PrimitiveSeparationPlan {
        crushed_storage,
        native_storage,
        residue_storage,
        machine,
        feed_mass,
        expected_target: _,
    } = plan;
    let definition = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("primitive reinvestment separator process disappeared"));
    let required_energy = calculate_mass_specific_energy(feed_mass, definition.specific_energy());
    let charge_ticks = fill_primitive_accumulator(registries, state, machine, required_energy)
        .unwrap_or_else(|error| panic!("primitive reinvestment {context} charge failed: {error}"));
    let selections = select_stockpile_mass(state, crushed_storage, feed_mass, context);
    let native = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
    let target_before = state
        .inventory()
        .get_stockpile(native_storage)
        .map(|stockpile| stockpile.get_mass(native))
        .unwrap_or_else(|| {
            panic!("primitive reinvestment native storage disappeared before {context}")
        });
    let resolved = resolve_constituent_separation_process(
        registries,
        state,
        ConstituentSeparationRequest::new(
            PROCESS_SEPARATE_NATIVE_COPPER,
            crushed_storage,
            selections.as_slice(),
            machine.separator,
            machine.drive,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive reinvestment {context} resolution failed: {error}"));
    assert_eq!(resolved.required_energy(), required_energy);
    let ticks = resolved.process_resolution().duration().value();
    let job = validate_start_process_routed(
        registries,
        state,
        resolved.process_resolution(),
        crushed_storage,
        &[
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::TARGET_STREAM,
                native_storage,
            ),
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::RESIDUE_STREAM,
                residue_storage,
            ),
        ],
    )
    .unwrap_or_else(|error| panic!("primitive reinvestment {context} start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("primitive reinvestment {context} commit failed: {error}"));
    finish_uninterrupted_production_job(
        registries,
        state,
        job,
        resolved.process_resolution().duration(),
        context,
    );
    let target_after = state
        .inventory()
        .get_stockpile(native_storage)
        .map(|stockpile| stockpile.get_mass(native))
        .unwrap_or_else(|| {
            panic!("primitive reinvestment native storage disappeared after {context}")
        });
    assert_eq!(
        target_after.checked_sub(target_before),
        Some(resolved.target_mass())
    );
    PrimitiveSeparationWork {
        feed_mass,
        target_mass: resolved.target_mass(),
        residue_mass: resolved.residue_mass(),
        required_energy,
        charge_ticks,
        ticks,
    }
}

pub(super) fn evaluate_mature_reinvestment(
    registries: &Registries,
    decision_state: &AppState,
    plan: MatureReinvestmentPlan,
) -> PrimitiveReinvestmentOutcome {
    let MatureReinvestmentPlan {
        raw,
        shaped,
        ore_storage,
        crushed_storage,
        native_storage,
        residue_storage,
        machine,
        pick,
        mining_target,
        primary_batch_mass,
        separation_feed_mass,
        reinforcement_mass,
    } = plan;
    let mut state = decision_state.clone();
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("primitive reinvestment matter setup failed: {error}"))
        .total();
    let survival_before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("primitive reinvestment player disappeared at decision point"));
    let base_drive_capacity = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("primitive reinvestment base flywheel disappeared"));
    let upgraded_drive_capacity = registries
        .energy()
        .get_store(ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("primitive reinvestment upgraded flywheel disappeared"));
    assert!(upgraded_drive_capacity > base_drive_capacity);
    let separation_definition = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("primitive reinvestment separator process disappeared"));
    let base_separator_batch_capacity = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_STONE_SEPARATOR,
        separation_definition.max_batch_mass_capability(),
    );
    let upgraded_separator_batch_capacity = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
        separation_definition.max_batch_mass_capability(),
    );
    assert!(upgraded_separator_batch_capacity > base_separator_batch_capacity);
    let expanded_batch_energy = Energy::from_nanojoules(
        base_drive_capacity
            .nanojoules()
            .checked_add(
                upgraded_drive_capacity
                    .nanojoules()
                    .checked_sub(base_drive_capacity.nanojoules())
                    .unwrap_or_else(|| unreachable!("upgraded flywheel has larger capacity"))
                    / 2,
            )
            .unwrap_or_else(|| panic!("primitive reinvestment expanded charge overflowed")),
    );
    assert!(
        expanded_batch_energy > base_drive_capacity
            && expanded_batch_energy <= upgraded_drive_capacity
    );
    let expanded_batch_mass = crush_mass_for_exact_energy(registries, expanded_batch_energy);
    let maximum_drain_mass = crush_mass_for_exact_energy(registries, base_drive_capacity);
    let prepared_ore = primary_batch_mass
        .checked_add(maximum_drain_mass)
        .and_then(|mass| mass.checked_add(expanded_batch_mass))
        .unwrap_or_else(|| panic!("primitive reinvestment prepared ore mass overflowed"));
    match try_mine_total_and_claim(
        registries,
        &mut state,
        mining_target,
        ore_storage,
        pick,
        prepared_ore,
        reinforced_pick_mining_batch_limit(registries),
    ) {
        Ok(_) => {}
        Err(
            MiningStartError::TargetNoLongerResolved
            | MiningStartError::InsufficientTargetMass { .. },
        ) => return PrimitiveReinvestmentOutcome::TargetSupplyLimited,
        Err(error) => panic!("primitive mature reinvestment mining failed: {error}"),
    }

    let primary_energy = calculate_mass_specific_energy(
        primary_batch_mass,
        registries
            .ore_processing()
            .get_comminution(PROCESS_CRUSH_ORE)
            .unwrap_or_else(|| panic!("primitive reinvestment crusher process disappeared"))
            .specific_energy(),
    );
    fill_primitive_accumulator(registries, &mut state, machine, primary_energy)
        .unwrap_or_else(|error| panic!("primitive reinvestment baseline charge failed: {error}"));
    let base_crush_ticks = resolve_crush_ticks(
        registries,
        &state,
        ore_storage,
        machine,
        primary_batch_mass,
        primary_energy,
        "base crusher comparison",
    );

    let remaining_primary_crushed = state
        .inventory()
        .get_stockpile(crushed_storage)
        .map(|stockpile| stockpile.stored_mass())
        .unwrap_or_else(|| panic!("primitive reinvestment crushed stockpile disappeared"));
    assert!(
        multiply_mass(separation_feed_mass, 2, "reinvestment separation feed")
            <= remaining_primary_crushed,
        "primary crushed output must retain enough same-feed material for two later reinvestments"
    );
    let first_recovery = separate_native_copper(
        registries,
        &mut state,
        PrimitiveSeparationPlan {
            crushed_storage,
            native_storage,
            residue_storage,
            machine,
            feed_mass: separation_feed_mass,
            expected_target: reinforcement_mass,
        },
    );
    craft_for_profile(
        registries,
        &mut state,
        raw,
        native_storage,
        shaped,
        equipment_upgrade_additions(registries, EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER),
    );
    let crusher_condition = state
        .equipment()
        .get_equipment(machine.crusher)
        .unwrap_or_else(|| panic!("primitive reinvestment crusher disappeared"))
        .condition();
    validate_upgrade_equipment(
        registries,
        &state,
        machine.crusher,
        EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
        shaped,
    )
    .unwrap_or_else(|error| panic!("primitive reinvestment crusher upgrade failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| {
        panic!("primitive reinvestment crusher upgrade commit failed: {error}")
    });
    assert_eq!(
        state
            .equipment()
            .get_equipment(machine.crusher)
            .unwrap_or_else(|| panic!("primitive reinvestment upgraded crusher disappeared"))
            .condition(),
        crusher_condition,
        "productive reinvestment must preserve accumulated crusher wear"
    );

    let second_recovery = separate_native_copper(
        registries,
        &mut state,
        PrimitiveSeparationPlan {
            crushed_storage,
            native_storage,
            residue_storage,
            machine,
            feed_mass: separation_feed_mass,
            expected_target: reinforcement_mass,
        },
    );
    let first_two_reinvestment_copper = first_recovery
        .target_mass
        .checked_add(second_recovery.target_mass)
        .unwrap_or_else(|| panic!("primitive reinvestment recovered copper overflowed"));
    assert_eq!(
        first_two_reinvestment_copper,
        reinforcement_mass
            .checked_add(reinforcement_mass)
            .unwrap_or_else(|| panic!("primitive reinvestment reinforcement mass overflowed"))
    );

    fill_primitive_accumulator(registries, &mut state, machine, primary_energy).unwrap_or_else(
        |error| panic!("primitive reinvestment upgraded crusher charge failed: {error}"),
    );
    let reinforced_crush_ticks = run_uninterrupted_crush(
        registries,
        &mut state,
        ore_storage,
        crushed_storage,
        machine,
        primary_batch_mass,
        primary_energy,
        "reinforced crusher comparison",
    );
    assert!(
        reinforced_crush_ticks < base_crush_ticks,
        "crusher reinforcement must reduce actual machine time on the same represented batch"
    );
    let crusher_time_reduction_ppm = u32::try_from(
        u128::from(base_crush_ticks - reinforced_crush_ticks) * 1_000_000
            / u128::from(base_crush_ticks),
    )
    .unwrap_or_else(|_| unreachable!("primitive crusher time reduction fits u32"));

    let base_separator_work = run_reinvestment_separation(
        registries,
        &mut state,
        PrimitiveSeparationPlan {
            crushed_storage,
            native_storage,
            residue_storage,
            machine,
            feed_mass: separation_feed_mass,
            expected_target: reinforcement_mass,
        },
        "base separator reinvestment comparison",
    );
    assert!(
        !base_separator_work.target_mass.is_zero(),
        "base separator comparison must recover real copper from the represented feed"
    );
    craft_for_profile(
        registries,
        &mut state,
        raw,
        native_storage,
        shaped,
        equipment_upgrade_additions(registries, EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR),
    );
    let separator_condition = state
        .equipment()
        .get_equipment(machine.separator)
        .unwrap_or_else(|| panic!("primitive reinvestment separator disappeared"))
        .condition();
    validate_upgrade_equipment(
        registries,
        &state,
        machine.separator,
        EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
        shaped,
    )
    .unwrap_or_else(|error| panic!("primitive reinvestment separator upgrade failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| {
        panic!("primitive reinvestment separator upgrade commit failed: {error}")
    });
    assert_eq!(
        state
            .equipment()
            .get_equipment(machine.separator)
            .unwrap_or_else(|| panic!("primitive reinvestment upgraded separator disappeared"))
            .condition(),
        separator_condition,
        "productive reinvestment must preserve accumulated separator wear"
    );
    let reinforced_separator_work = run_reinvestment_separation(
        registries,
        &mut state,
        PrimitiveSeparationPlan {
            crushed_storage,
            native_storage,
            residue_storage,
            machine,
            feed_mass: separation_feed_mass,
            expected_target: reinforcement_mass,
        },
        "reinforced separator comparison",
    );
    assert!(
        !reinforced_separator_work.target_mass.is_zero(),
        "reinforced separator comparison must recover real copper from the represented feed"
    );
    assert!(
        reinforced_separator_work.ticks < base_separator_work.ticks,
        "separator reinforcement must reduce actual machine time on the same represented feed"
    );
    let separator_time_reduction_ppm = u32::try_from(
        u128::from(base_separator_work.ticks - reinforced_separator_work.ticks) * 1_000_000
            / u128::from(base_separator_work.ticks),
    )
    .unwrap_or_else(|_| unreachable!("primitive separator time reduction fits u32"));

    let residual_energy = state
        .energy()
        .get_store(machine.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| {
            panic!("primitive reinvestment flywheel disappeared after crusher work")
        });
    if !residual_energy.is_zero() {
        let crusher_process = registries
            .ore_processing()
            .get_comminution(PROCESS_CRUSH_ORE)
            .unwrap_or_else(|| panic!("primitive reinvestment crusher process disappeared"));
        let per_milligram =
            u128::from(crusher_process.specific_energy().nanojoules_per_milligram());
        let productive_milligrams = residual_energy.nanojoules() / per_milligram;
        if productive_milligrams > 0 {
            let drain_mass =
                Mass::from_milligrams(u64::try_from(productive_milligrams).unwrap_or_else(|_| {
                    panic!("primitive reinvestment drain mass exceeds authoritative range")
                }));
            let drain_energy =
                calculate_mass_specific_energy(drain_mass, crusher_process.specific_energy());
            run_uninterrupted_crush(
                registries,
                &mut state,
                ore_storage,
                crushed_storage,
                machine,
                drain_mass,
                drain_energy,
                "useful stored-work drain before flywheel upgrade",
            );
        }
        let unusable_tail = state
            .energy()
            .get_store(machine.drive)
            .map(|store| store.stored())
            .unwrap_or_else(|| panic!("primitive reinvestment flywheel disappeared during drain"));
        assert!(
            unusable_tail.nanojoules() < per_milligram,
            "productive drain must leave less energy than the smallest represented crusher feed"
        );
        while state
            .energy()
            .get_store(machine.drive)
            .is_some_and(|store| !store.stored().is_zero())
        {
            let outcome = advance_tick(registries, &mut state).unwrap_or_else(|error| {
                panic!("primitive reinvestment residual-loss tick failed: {error}")
            });
            assert!(
                outcome.production_availability_changes().is_empty()
                    && outcome.production_completions().is_empty()
                    && outcome.ready_mining_jobs().is_empty()
                    && outcome.manual_power().is_none()
                    && outcome.field_prospecting().is_none(),
                "primitive reinvestment residual-loss wait crossed unrelated observable work"
            );
            assert_eq!(state.player_work().active(), None);
        }
    }
    assert_eq!(
        state
            .energy()
            .get_store(machine.drive)
            .map(|store| store.stored()),
        Some(Energy::ZERO),
        "flywheel reinvestment must productively consume usable stored work and dissipate only the sub-operation tail before modification"
    );
    let drive_upgrade = registries
        .energy()
        .get_store(ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE)
        .and_then(|definition| definition.upgrade_profile())
        .unwrap_or_else(|| panic!("primitive reinvestment flywheel lost its upgrade route"));
    let native = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
    assert!(
        state
            .inventory()
            .get_stockpile(native_storage)
            .is_some_and(|stockpile| stockpile.get_mass(native) >= reinforcement_mass),
        "crusher and separator recovery must leave enough native copper for the final flywheel reinforcement"
    );
    craft_for_profile(
        registries,
        &mut state,
        raw,
        native_storage,
        shaped,
        drive_upgrade.additions(),
    );
    validate_upgrade_energy_store(
        registries,
        &state,
        machine.drive,
        ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
        shaped,
    )
    .unwrap_or_else(|error| panic!("primitive reinvestment flywheel upgrade failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| {
        panic!("primitive reinvestment flywheel upgrade commit failed: {error}")
    });
    let invested_copper_mass = multiply_mass(reinforcement_mass, 3, "mature reinvestment copper");
    assert_eq!(
        state
            .energy()
            .get_store(machine.drive)
            .map(|store| store.definition()),
        Some(ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE)
    );

    let expanded_charge_ticks =
        charge_exact_reinvestment_energy(registries, &mut state, machine, expanded_batch_energy);
    let expanded_crush_ticks = run_uninterrupted_crush(
        registries,
        &mut state,
        ore_storage,
        crushed_storage,
        machine,
        expanded_batch_mass,
        expanded_batch_energy,
        "expanded flywheel-funded crusher batch",
    );
    assert!(
        expanded_batch_mass > crush_mass_for_exact_energy(registries, base_drive_capacity),
        "flywheel reinforcement must fund a single crusher batch the base accumulator cannot hold"
    );
    assert!(
        expanded_batch_mass > base_separator_batch_capacity
            && expanded_batch_mass <= upgraded_separator_batch_capacity,
        "the mature reinvestment batch must exceed the base separator envelope while fitting the reinforced separator"
    );
    let expanded_separator = run_reinvestment_separation(
        registries,
        &mut state,
        PrimitiveSeparationPlan {
            crushed_storage,
            native_storage,
            residue_storage,
            machine,
            feed_mass: expanded_batch_mass,
            expected_target: Mass::ZERO,
        },
        "expanded reinforced-separator batch",
    );
    assert!(!expanded_separator.target_mass.is_zero());
    let survival_after = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("primitive reinvestment player disappeared after branch"));
    let survival_energy_spent_nj = survival_before.metabolic_energy().nanojoules()
        - survival_after.metabolic_energy().nanojoules();
    let survival_hydration_spent_ul =
        survival_before.hydration().microliters() - survival_after.hydration().microliters();
    assert!(survival_energy_spent_nj > 0 && survival_hydration_spent_ul > 0);
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("primitive reinvestment matter audit failed: {error}"))
            .total(),
        matter_before,
        "mature primitive reinvestment must conserve represented matter"
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("primitive reinvestment state audit failed: {error}"));
    PrimitiveReinvestmentOutcome::Completed(PrimitiveReinvestmentExperience {
        invested_copper_mass,
        base_crush_ticks,
        reinforced_crush_ticks,
        crusher_time_reduction_ppm,
        base_separator_ticks: base_separator_work.ticks,
        reinforced_separator_ticks: reinforced_separator_work.ticks,
        separator_time_reduction_ppm,
        base_separator_target_mass: base_separator_work.target_mass,
        reinforced_separator_target_mass: reinforced_separator_work.target_mass,
        base_separator_batch_capacity,
        upgraded_separator_batch_capacity,
        base_drive_capacity,
        upgraded_drive_capacity,
        expanded_batch_mass,
        expanded_batch_energy,
        expanded_charge_ticks,
        expanded_crush_ticks,
        expanded_separator_energy: expanded_separator.required_energy,
        expanded_separator_ticks: expanded_separator.ticks,
        expanded_separator_target_mass: expanded_separator.target_mass,
        survival_energy_spent_nj,
        survival_hydration_spent_ul,
    })
}
