//! Full primitive crusher campaign execution with condition-aware batching.

use super::*;

#[derive(Clone, Copy)]
struct PrimitiveLegProjection {
    batch_mass: Mass,
    required_nj: u128,
    consumer_duration: TickSpan,
    required_energy: Energy,
    required_hydration: Volume,
}

#[derive(Clone, Copy)]
struct PrimitiveProjectRoute {
    consumer: PrimitivePowerConsumer,
    provider: EquipmentId,
    method: ManualPowerMethodId,
    drive: EnergyStoreId,
    label: &'static str,
}

fn project_primitive_leg(
    registries: &Registries,
    state: &AppState,
    route: PrimitiveProjectRoute,
    batch_mass: Mass,
) -> PrimitiveLegProjection {
    let envelope = assess_powered_ore_mass_envelope(
        registries,
        state,
        PROCESS_CRUSH_ORE,
        route.consumer.equipment(),
        route.drive,
    )
    .unwrap_or_else(|error| panic!("primitive power leg planning failed: {error}"));
    let required_nj = envelope
        .required_energy_for(batch_mass)
        .unwrap_or_else(|| panic!("primitive power leg exceeds a non-energy crusher constraint"))
        .nanojoules();
    assert!(
        required_nj > 0,
        "primitive power leg must require positive productive work"
    );
    let consumer_duration = envelope
        .duration_for_mass_with_replenished_energy(batch_mass)
        .unwrap_or_else(|| panic!("primitive power leg lost its duration projection"));
    let passive_budget = project_survival_resource_budget(
        registries.survival().physiology(),
        SurvivalExertion::REST,
        consumer_duration,
    )
    .unwrap_or_else(|error| panic!("primitive passive survival projection failed: {error:?}"));
    PrimitiveLegProjection {
        batch_mass,
        required_nj,
        consumer_duration,
        required_energy: passive_budget.metabolic_energy(),
        required_hydration: passive_budget.hydration(),
    }
}

fn primitive_leg_preserves_warning_reserve(
    registries: &Registries,
    projection: PrimitiveLegProjection,
) -> bool {
    let physiology = registries.survival().physiology();
    let maximum_task_energy = physiology
        .maximum_metabolic_energy()
        .checked_sub(physiology.hungry_below())
        .unwrap_or_else(|| unreachable!("hunger warning is below maximum metabolic reserve"));
    let maximum_task_hydration = physiology
        .maximum_hydration()
        .checked_sub(physiology.thirsty_below())
        .unwrap_or_else(|| unreachable!("thirst warning is below maximum hydration reserve"));
    projection.required_energy <= maximum_task_energy
        && projection.required_hydration <= maximum_task_hydration
}

fn recharge_primitive_store(
    registries: &Registries,
    state: &mut AppState,
    resources: ProjectExecutionResources,
    plan: PrimitivePowerPlan,
    route: PrimitiveProjectRoute,
) -> (ChargeOutcome, ProvisioningOutcome) {
    let provider_record = state
        .equipment()
        .get_equipment(route.provider)
        .unwrap_or_else(|| panic!("selected primitive provider disappeared before recharge"));
    let maximum_charge = project_manual_power(
        registries,
        route.method,
        provider_record.definition(),
        provider_record.condition(),
        plan.store_definition,
        Energy::from_nanojoules(plan.capacity_nj),
    )
    .unwrap_or_else(|error| {
        panic!("selected primitive maximum recharge projection failed: {error}")
    });
    let provisioning = provision_for_project_leg(
        registries,
        state,
        resources.provisions,
        maximum_charge.resource_budget().metabolic_energy(),
        maximum_charge.resource_budget().hydration(),
        "selected primitive accumulator recharge",
    );
    let charge = super::charge_store_to_maximum_destination(
        registries,
        state,
        route.method,
        route.provider,
        route.drive,
        Energy::from_nanojoules(plan.capacity_nj),
        route.label,
    );
    (charge, provisioning)
}

fn largest_survivable_primitive_leg(
    registries: &Registries,
    state: &AppState,
    route: PrimitiveProjectRoute,
    upper_mass: Mass,
) -> PrimitiveLegProjection {
    let upper = project_primitive_leg(registries, state, route, upper_mass);
    if primitive_leg_preserves_warning_reserve(registries, upper) {
        return upper;
    }

    let mut low = 1_u64;
    let mut high = upper_mass.milligrams().saturating_sub(1);
    let mut best = None;
    while low <= high {
        let midpoint = low + (high - low) / 2;
        let candidate =
            project_primitive_leg(registries, state, route, Mass::from_milligrams(midpoint));
        if primitive_leg_preserves_warning_reserve(registries, candidate) {
            best = Some(candidate);
            low = midpoint
                .checked_add(1)
                .unwrap_or_else(|| unreachable!("bounded batch midpoint cannot overflow"));
        } else {
            high = midpoint.saturating_sub(1);
        }
    }
    best.unwrap_or_else(|| {
        panic!(
            "no positive crusher batch can preserve the player's authored survival warning reserves"
        )
    })
}

pub(in super::super::super) fn execute_selected_primitive_project(
    registries: &Registries,
    state: &AppState,
    resources: ProjectExecutionResources,
    plan: PrimitivePowerPlan,
    consumer: PrimitivePowerConsumer,
) -> SelectedProjectOutcome {
    let mut selected_state = state.clone();
    let started_at = selected_state.tick().value();
    let survival_before = assess_survival(registries, &selected_state)
        .unwrap_or_else(|| panic!("selected primitive power project lost initial survival state"));
    let (provider_definition, method, label) = match plan.choice {
        PrimitivePowerChoice::Crank => (
            EQUIPMENT_STONE_HAND_CRANK,
            MANUAL_POWER_HAND_CRANK,
            "selected primitive crank project",
        ),
        PrimitivePowerChoice::Treadle => (
            EQUIPMENT_TIMBER_TREADLE_DRIVE,
            MANUAL_POWER_FOOT_TREADLE,
            "selected primitive treadle project",
        ),
        PrimitivePowerChoice::WalkingWheel => (
            EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE,
            MANUAL_POWER_WALKING_WHEEL,
            "selected primitive walking-wheel project",
        ),
    };
    let (provider, provider_build) = build_provider(
        registries,
        &mut selected_state,
        resources.raw,
        resources.shaped,
        provider_definition,
        label,
    );
    let (drive, drive_build) = build_flywheel(
        registries,
        &mut selected_state,
        resources.raw,
        resources.shaped,
        plan.store_definition,
        label,
    );
    let mut provider_attention_ticks = provider_build
        .attention_ticks
        .checked_add(drive_build.attention_ticks)
        .unwrap_or_else(|| panic!("selected primitive provider build attention overflowed"));
    let mut charge_events = 0_u64;
    let mut survival_limited_batches = 0_u64;
    let mut consumer_ticks = 0_u64;
    let mut consumer_services = 0_u64;
    let mut service_preparation_ticks = 0_u64;
    let mut service_ticks = 0_u64;
    let mut replacement_mass_mg = 0_u64;
    let mut provisioning = ProvisioningOutcome::default();
    let initial_source_mass = stockpile_mass(&selected_state, consumer.source());
    assert!(
        plan.declared_mass <= initial_source_mass,
        "declared primitive project exceeds actor-visible feed opportunity"
    );
    let mut remaining_project_mass = plan.declared_mass;
    let route = PrimitiveProjectRoute {
        consumer,
        provider,
        method,
        drive,
        label,
    };
    while !remaining_project_mass.is_zero() {
        if let Some(service) = service_consumer_if_critical(
            registries,
            &mut selected_state,
            resources,
            consumer.equipment(),
            "selected primitive crusher service",
        ) {
            provisioning.add(service.provisioning);
            consumer_services += 1;
            service_preparation_ticks = service_preparation_ticks
                .checked_add(service.preparation_ticks)
                .unwrap_or_else(|| panic!("selected primitive service preparation overflowed"));
            service_ticks = service_ticks
                .checked_add(service.service_ticks)
                .unwrap_or_else(|| panic!("selected primitive service duration overflowed"));
            replacement_mass_mg = replacement_mass_mg
                .checked_add(service.replacement_mass_mg)
                .unwrap_or_else(|| panic!("selected primitive replacement mass overflowed"));
        }
        let mut envelope = assess_powered_ore_mass_envelope(
            registries,
            &selected_state,
            PROCESS_CRUSH_ORE,
            consumer.equipment(),
            drive,
        )
        .unwrap_or_else(|error| panic!("selected primitive batch planning failed: {error}"));
        let remaining_mass = stockpile_mass(&selected_state, consumer.source());
        let mut upper_batch_mass = remaining_project_mass
            .min(remaining_mass)
            .min(envelope.maximum_mass());
        if upper_batch_mass.is_zero() {
            let (charge, charge_provisioning) =
                recharge_primitive_store(registries, &mut selected_state, resources, plan, route);
            provisioning.add(charge_provisioning);
            provider_attention_ticks = provider_attention_ticks
                .checked_add(charge.attention_ticks)
                .unwrap_or_else(|| panic!("selected primitive charge attention overflowed"));
            charge_events = charge_events
                .checked_add(1)
                .unwrap_or_else(|| panic!("selected primitive charge count overflowed"));
            envelope = assess_powered_ore_mass_envelope(
                registries,
                &selected_state,
                PROCESS_CRUSH_ORE,
                consumer.equipment(),
                drive,
            )
            .unwrap_or_else(|error| {
                panic!("selected primitive post-recharge batch planning failed: {error}")
            });
            upper_batch_mass = remaining_project_mass
                .min(remaining_mass)
                .min(envelope.maximum_mass());
        }
        assert!(
            !upper_batch_mass.is_zero(),
            "selected primitive crusher has remaining feed but no positive batch after canonical accumulator recharge"
        );
        let preliminary_leg =
            largest_survivable_primitive_leg(registries, &selected_state, route, upper_batch_mass);
        let leg_provisioning = provision_for_project_leg(
            registries,
            &mut selected_state,
            resources.provisions,
            preliminary_leg.required_energy,
            preliminary_leg.required_hydration,
            "selected primitive crusher leg",
        );
        provisioning.add(leg_provisioning);

        // Eating or drinking advances canonical time, so a dissipative flywheel can lose work
        // between the pre-break projection and the actual crusher start. Re-read the current
        // production envelope after any break rather than treating the old stored-energy snapshot
        // as an authorization token.
        if leg_provisioning.stops > 0 {
            envelope = assess_powered_ore_mass_envelope(
                registries,
                &selected_state,
                PROCESS_CRUSH_ORE,
                consumer.equipment(),
                drive,
            )
            .unwrap_or_else(|error| {
                panic!("selected primitive post-provisioning batch planning failed: {error}")
            });
            if envelope.maximum_mass().is_zero() {
                let (charge, charge_provisioning) = recharge_primitive_store(
                    registries,
                    &mut selected_state,
                    resources,
                    plan,
                    route,
                );
                provisioning.add(charge_provisioning);
                provider_attention_ticks = provider_attention_ticks
                    .checked_add(charge.attention_ticks)
                    .unwrap_or_else(|| panic!("selected primitive charge attention overflowed"));
                charge_events = charge_events
                    .checked_add(1)
                    .unwrap_or_else(|| panic!("selected primitive charge count overflowed"));
                envelope = assess_powered_ore_mass_envelope(
                    registries,
                    &selected_state,
                    PROCESS_CRUSH_ORE,
                    consumer.equipment(),
                    drive,
                )
                .unwrap_or_else(|error| {
                    panic!("selected primitive post-break recharge planning failed: {error}")
                });
            }
            upper_batch_mass = remaining_project_mass
                .min(remaining_mass)
                .min(envelope.maximum_mass());
        }
        let leg = largest_survivable_primitive_leg(
            registries,
            &selected_state,
            route,
            upper_batch_mass.min(preliminary_leg.batch_mass),
        );
        if leg.batch_mass < upper_batch_mass {
            survival_limited_batches = survival_limited_batches
                .checked_add(1)
                .unwrap_or_else(|| panic!("primitive survival-limited batch count overflowed"));
        }
        let executed_consumer_ticks = consume_primitive_work(
            registries,
            &mut selected_state,
            consumer,
            drive,
            leg.required_nj,
        );
        assert_eq!(
            executed_consumer_ticks,
            leg.consumer_duration.value(),
            "selected primitive crusher duration projection diverged from canonical execution"
        );
        consumer_ticks = consumer_ticks
            .checked_add(executed_consumer_ticks)
            .unwrap_or_else(|| panic!("selected primitive consumer duration overflowed"));
        remaining_project_mass = remaining_project_mass
            .checked_sub(leg.batch_mass)
            .unwrap_or_else(|| {
                unreachable!("primitive leg is bounded by remaining declared project mass")
            });
    }
    assert!(
        charge_events >= plan.charge_events,
        "condition-aware batching cannot require fewer charges than the pristine buffer-only lower bound"
    );
    assert!(
        consumer_services >= plan.consumer_projected_services,
        "lived primitive execution cannot require fewer services than its consumer-aware projection"
    );
    if survival_limited_batches == 0 {
        assert_eq!(
            consumer_services, plan.consumer_projected_services,
            "without survival batch splitting, lived service count must match the consumer-aware projection"
        );
    }
    assert!(remaining_project_mass.is_zero());
    assert_eq!(
        stockpile_mass(&selected_state, consumer.source()),
        initial_source_mass
            .checked_sub(plan.declared_mass)
            .unwrap_or_else(|| unreachable!("declared primitive project was bounded by feed")),
        "primitive project must consume only the actor's declared workload"
    );
    let provider_condition_ppm = selected_state
        .equipment()
        .get_equipment(provider)
        .map(|record| record.condition().parts_per_million())
        .unwrap_or_else(|| panic!("selected primitive provider disappeared"));
    let consumer_condition_ppm = selected_state
        .equipment()
        .get_equipment(consumer.equipment())
        .map(|record| record.condition().parts_per_million())
        .unwrap_or_else(|| panic!("selected primitive consumer disappeared"));
    let survival_after = assess_survival(registries, &selected_state)
        .unwrap_or_else(|| panic!("selected primitive project did not preserve player survival"));
    assert_eq!(
        calculate_matter_accounting(&selected_state)
            .unwrap_or_else(|error| panic!(
                "selected primitive project matter audit failed: {error}"
            ))
            .total(),
        resources.matter_before
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&selected_state)
            .unwrap_or_else(|error| panic!(
                "selected primitive project fluid audit failed: {error}"
            ))
            .total(),
        resources.fluid_before
    );
    validate_loaded_state(registries, &selected_state)
        .unwrap_or_else(|error| panic!("selected primitive project state invalid: {error}"));
    SelectedProjectOutcome {
        charge_events,
        survival_limited_batches,
        provider_attention_ticks,
        consumer_ticks,
        consumer_services,
        service_preparation_ticks,
        service_ticks,
        replacement_mass_mg,
        provisioning_stops: provisioning.stops,
        provisioning_attention_ticks: provisioning.attention_ticks,
        drink_actions: provisioning.drink_actions,
        drink_volume_ul: provisioning.drink_volume_ul,
        meal_actions: provisioning.meal_actions,
        meal_mass_mg: provisioning.meal_mass_mg,
        elapsed_ticks: selected_state
            .tick()
            .value()
            .checked_sub(started_at)
            .unwrap_or_else(|| unreachable!("selected primitive project cannot run backward")),
        initial_metabolic_nj: survival_before.metabolic_energy().nanojoules(),
        initial_hydration_ul: survival_before.hydration().microliters(),
        provider_condition_ppm,
        consumer_condition_ppm,
        final_metabolic_nj: survival_after.metabolic_energy().nanojoules(),
        final_hydration_ul: survival_after.hydration().microliters(),
    }
}
