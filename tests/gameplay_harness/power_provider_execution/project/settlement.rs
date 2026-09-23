//! Full settlement sawmill campaign execution with service and survival lifecycle.

use super::*;

pub(in super::super::super) fn execute_selected_settlement_project(
    registries: &Registries,
    state: &AppState,
    resources: ProjectExecutionResources,
    plan: SettlementPowerPlan,
    consumer: SettlementPowerConsumer,
) -> SelectedProjectOutcome {
    let mut selected_state = state.clone();
    let started_at = selected_state.tick().value();
    let survival_before = assess_survival(registries, &selected_state)
        .unwrap_or_else(|| panic!("selected settlement power project lost initial survival state"));
    let (
        provider_definition,
        method,
        expected_attention,
        expected_metabolic_nj,
        expected_hydration_ul,
        expected_condition_ppm,
        label,
    ) = match plan.choice {
        SettlementPowerChoice::Treadle => (
            EQUIPMENT_TIMBER_TREADLE_DRIVE,
            MANUAL_POWER_FOOT_TREADLE,
            plan.treadle_lifecycle_attention,
            plan.treadle_lifecycle_metabolic_nj,
            plan.treadle_lifecycle_hydration_ul,
            plan.treadle_lifecycle_condition.parts_per_million(),
            "selected settlement treadle project",
        ),
        SettlementPowerChoice::WalkingWheel => (
            EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE,
            MANUAL_POWER_WALKING_WHEEL,
            plan.walking_lifecycle_attention,
            plan.walking_lifecycle_metabolic_nj,
            plan.walking_lifecycle_hydration_ul,
            plan.walking_lifecycle_condition.parts_per_million(),
            "selected settlement walking-wheel project",
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
        ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
        label,
    );
    let mut provider_attention_ticks = provider_build
        .attention_ticks
        .checked_add(drive_build.attention_ticks)
        .unwrap_or_else(|| panic!("selected settlement provider build attention overflowed"));
    let mut provider_metabolic_nj = provider_build
        .metabolic_nj
        .checked_add(drive_build.metabolic_nj)
        .unwrap_or_else(|| panic!("selected settlement provider build metabolism overflowed"));
    let mut provider_hydration_ul = u128::from(provider_build.hydration_ul)
        .checked_add(u128::from(drive_build.hydration_ul))
        .unwrap_or_else(|| panic!("selected settlement provider build hydration overflowed"));
    let mut remaining_nj = plan.declared_work_nj;
    let mut charge_events = 0_u64;
    let survival_limited_batches = 0_u64;
    let mut consumer_ticks = 0_u64;
    let mut consumer_services = 0_u64;
    let mut service_preparation_ticks = 0_u64;
    let mut service_ticks = 0_u64;
    let mut replacement_mass_mg = 0_u64;
    let mut provisioning = ProvisioningOutcome::default();
    while remaining_nj > 0 {
        if let Some(service) = service_consumer_if_critical(
            registries,
            &mut selected_state,
            resources,
            consumer.equipment(),
            "selected settlement sawmill service",
        ) {
            provisioning.add(service.provisioning);
            consumer_services += 1;
            service_preparation_ticks = service_preparation_ticks
                .checked_add(service.preparation_ticks)
                .unwrap_or_else(|| panic!("selected settlement service preparation overflowed"));
            service_ticks = service_ticks
                .checked_add(service.service_ticks)
                .unwrap_or_else(|| panic!("selected settlement service duration overflowed"));
            replacement_mass_mg = replacement_mass_mg
                .checked_add(service.replacement_mass_mg)
                .unwrap_or_else(|| panic!("selected settlement replacement mass overflowed"));
        }
        let requested_nj = remaining_nj.min(plan.capacity_nj);
        let saw_definition = registries
            .crafting()
            .get_powered(PROCESS_POWER_SAW_WOOD_BOARDS)
            .unwrap_or_else(|| panic!("selected settlement saw process disappeared"));
        let input_mass = deep_hearth::energy::calculate_mass_specific_energy_capacity(
            Energy::from_nanojoules(requested_nj),
            saw_definition.specific_energy(),
        );
        let craft_projection = project_powered_craft_work(
            registries,
            &selected_state,
            PROCESS_POWER_SAW_WOOD_BOARDS,
            input_mass,
            consumer.equipment(),
            drive,
        )
        .unwrap_or_else(|error| panic!("selected settlement saw work projection failed: {error}"));
        assert_eq!(
            craft_projection.required_energy(),
            Energy::from_nanojoules(requested_nj)
        );
        let passive_budget = project_survival_resource_budget(
            registries.survival().physiology(),
            SurvivalExertion::REST,
            craft_projection.duration(),
        )
        .unwrap_or_else(|error| {
            panic!("selected settlement passive survival projection failed: {error:?}")
        });
        let provider_condition = selected_state
            .equipment()
            .get_equipment(provider)
            .map(|record| record.condition())
            .unwrap_or_else(|| {
                panic!("selected settlement provider disappeared before charge planning")
            });
        let charge_projection = project_manual_power(
            registries,
            method,
            provider_definition,
            provider_condition,
            ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
            Energy::from_nanojoules(requested_nj),
        )
        .unwrap_or_else(|error| panic!("selected settlement charge projection failed: {error}"));
        let required_energy = charge_projection
            .resource_budget()
            .metabolic_energy()
            .checked_add(passive_budget.metabolic_energy())
            .unwrap_or_else(|| panic!("selected settlement leg metabolic budget overflowed"));
        let required_hydration = charge_projection
            .resource_budget()
            .hydration()
            .checked_add(passive_budget.hydration())
            .unwrap_or_else(|| panic!("selected settlement leg hydration budget overflowed"));
        provisioning.add(provision_for_project_leg(
            registries,
            &mut selected_state,
            resources.provisions,
            required_energy,
            required_hydration,
            "selected settlement charge+sawmill leg",
        ));
        let charge = charge_store(
            registries,
            &mut selected_state,
            method,
            provider,
            drive,
            requested_nj,
            label,
        );
        provider_attention_ticks = provider_attention_ticks
            .checked_add(charge.attention_ticks)
            .unwrap_or_else(|| panic!("selected settlement charge attention overflowed"));
        provider_metabolic_nj = provider_metabolic_nj
            .checked_add(charge.metabolic_nj)
            .unwrap_or_else(|| panic!("selected settlement charge metabolism overflowed"));
        provider_hydration_ul = provider_hydration_ul
            .checked_add(charge.hydration_ul)
            .unwrap_or_else(|| panic!("selected settlement charge hydration overflowed"));
        let consumer_duration = consume_settlement_charge(
            registries,
            &mut selected_state,
            consumer,
            drive,
            requested_nj,
        );
        assert_eq!(consumer_duration, craft_projection.duration().value());
        assert_eq!(
            selected_state
                .equipment()
                .get_equipment(consumer.equipment())
                .map(|record| record.condition()),
            Some(craft_projection.condition_after())
        );
        consumer_ticks = consumer_ticks
            .checked_add(consumer_duration)
            .unwrap_or_else(|| panic!("selected settlement consumer duration overflowed"));
        remaining_nj = remaining_nj.checked_sub(requested_nj).unwrap_or_else(|| {
            unreachable!("selected settlement charge is bounded by remaining work")
        });
        charge_events += 1;
    }
    assert_eq!(charge_events, plan.charge_events);
    assert_eq!(provider_attention_ticks, expected_attention);
    assert_eq!(provider_metabolic_nj, expected_metabolic_nj);
    assert_eq!(provider_hydration_ul, u128::from(expected_hydration_ul));
    assert!(stockpile_mass(&selected_state, consumer.source()).is_zero());
    let provider_condition_ppm = selected_state
        .equipment()
        .get_equipment(provider)
        .map(|record| record.condition().parts_per_million())
        .unwrap_or_else(|| panic!("selected settlement provider disappeared"));
    assert_eq!(provider_condition_ppm, expected_condition_ppm);
    let consumer_condition_ppm = selected_state
        .equipment()
        .get_equipment(consumer.equipment())
        .map(|record| record.condition().parts_per_million())
        .unwrap_or_else(|| panic!("selected settlement consumer disappeared"));
    let survival_after = assess_survival(registries, &selected_state)
        .unwrap_or_else(|| panic!("selected settlement project did not preserve player survival"));
    assert_eq!(
        calculate_matter_accounting(&selected_state)
            .unwrap_or_else(|error| panic!(
                "selected settlement project matter audit failed: {error}"
            ))
            .total(),
        resources.matter_before
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&selected_state)
            .unwrap_or_else(|error| panic!(
                "selected settlement project fluid audit failed: {error}"
            ))
            .total(),
        resources.fluid_before
    );
    validate_loaded_state(registries, &selected_state)
        .unwrap_or_else(|error| panic!("selected settlement project state invalid: {error}"));
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
            .unwrap_or_else(|| unreachable!("selected settlement project cannot run backward")),
        initial_metabolic_nj: survival_before.metabolic_energy().nanojoules(),
        initial_hydration_ul: survival_before.hydration().microliters(),
        provider_condition_ppm,
        consumer_condition_ppm,
        final_metabolic_nj: survival_after.metabolic_energy().nanojoules(),
        final_hydration_ul: survival_after.hydration().microliters(),
    }
}
