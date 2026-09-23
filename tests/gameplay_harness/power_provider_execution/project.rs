//! Complete human-power project execution with survival, wear, and maintenance lifecycle.

use super::*;

#[derive(Clone, Copy)]
pub(in super::super) struct ProjectExecutionResources {
    raw: StockpileId,
    shaped: StockpileId,
    service_replacement: StockpileId,
    service_spent: StockpileId,
    provisions: PowerProjectProvisions,
    matter_before: AggregateMass,
    fluid_before: AggregateVolume,
}

impl ProjectExecutionResources {
    pub(in super::super) const fn new(
        raw: StockpileId,
        shaped: StockpileId,
        service_replacement: StockpileId,
        service_spent: StockpileId,
        provisions: PowerProjectProvisions,
        matter_before: AggregateMass,
        fluid_before: AggregateVolume,
    ) -> Self {
        Self {
            raw,
            shaped,
            service_replacement,
            service_spent,
            provisions,
            matter_before,
            fluid_before,
        }
    }
}

pub(in super::super) struct ChargeOutcome {
    pub(in super::super) attention_ticks: u64,
    pub(in super::super) metabolic_nj: u128,
    pub(in super::super) hydration_ul: u128,
    pub(in super::super) condition_after_ppm: u32,
}

#[derive(Clone, Copy)]
struct ConsumerServiceOutcome {
    preparation_ticks: u64,
    service_ticks: u64,
    replacement_mass_mg: u64,
    provisioning: ProvisioningOutcome,
}

fn service_consumer_if_critical(
    registries: &Registries,
    state: &mut AppState,
    resources: ProjectExecutionResources,
    equipment: EquipmentId,
    context: &'static str,
) -> Option<ConsumerServiceOutcome> {
    let record = state
        .equipment()
        .get_equipment(equipment)
        .unwrap_or_else(|| {
            panic!("power provider {context} consumer disappeared before service check")
        });
    let definition = registries
        .equipment()
        .get_equipment(record.definition())
        .unwrap_or_else(|| panic!("power provider {context} consumer definition disappeared"));
    let condition_before = record.condition();
    if definition
        .maintenance_thresholds()
        .classify(condition_before)
        != MaintenanceBand::Critical
    {
        return None;
    }
    let profile = definition
        .maintenance_profile()
        .unwrap_or_else(|| panic!("power provider {context} consumer has no maintenance profile"));
    assert!(
        profile.is_component_replacement(),
        "power provider {context} long-project service expects an authored component replacement"
    );
    let replacement_mass = profile.required_replacement_mass(condition_before);
    let service_duration = profile.required_service_duration(condition_before);
    let (craft, batches, source) = manual_craft_plan_for_available_output(
        registries,
        state,
        &[resources.raw],
        profile.replacement(),
        replacement_mass,
        context,
    );
    let craft_batches = NonZeroU64::new(batches)
        .unwrap_or_else(|| unreachable!("positive service replacement requires craft batches"));
    let craft_projection = project_manual_craft_hand_work(registries, craft, craft_batches)
        .unwrap_or_else(|error| {
            panic!("power provider {context} replacement craft projection failed: {error}")
        });
    let mut provisioning = provision_for_project_leg(
        registries,
        state,
        resources.provisions,
        craft_projection.resource_budget().metabolic_energy(),
        craft_projection.resource_budget().hydration(),
        context,
    );
    let preparation_ticks = execute_manual_craft_batches(
        registries,
        state,
        craft.process(),
        source,
        resources.service_replacement,
        batches,
        context,
    )
    .value();
    assert_eq!(
        preparation_ticks,
        craft_projection.duration().value(),
        "power provider {context} replacement preparation diverged from its planning duration"
    );
    let service_budget = project_survival_resource_budget(
        registries.survival().physiology(),
        profile.exertion(),
        service_duration,
    )
    .unwrap_or_else(|error| {
        panic!("power provider {context} maintenance survival projection failed: {error:?}")
    });
    provisioning.add(provision_for_project_leg(
        registries,
        state,
        resources.provisions,
        service_budget.metabolic_energy(),
        service_budget.hydration(),
        context,
    ));
    let resolution = resolve_equipment_maintenance(
        registries,
        state,
        EquipmentMaintenanceRequest::new(
            equipment,
            resources.service_replacement,
            resources.service_spent,
        ),
    )
    .unwrap_or_else(|error| {
        panic!("power provider {context} maintenance resolution failed: {error}")
    });
    assert_eq!(resolution.material_mass(), replacement_mass);
    assert_eq!(resolution.duration(), service_duration);
    let start =
        validate_equipment_maintenance(registries, state, resolution).unwrap_or_else(|error| {
            panic!("power provider {context} maintenance validation failed: {error}")
        });
    let admitted = start.commit(state).unwrap_or_else(|error| {
        panic!("power provider {context} maintenance commit failed: {error}")
    });
    assert_eq!(admitted.equipment(), equipment);
    let (service_ticks, completed) =
        finish_active_equipment_maintenance(registries, state, context);
    assert_eq!(completed.equipment(), equipment);
    Some(ConsumerServiceOutcome {
        preparation_ticks,
        service_ticks,
        replacement_mass_mg: replacement_mass.milligrams(),
        provisioning,
    })
}

#[derive(Clone, Copy)]
pub(in super::super) struct SelectedProjectOutcome {
    pub(in super::super) charge_events: u64,
    pub(in super::super) survival_limited_batches: u64,
    pub(in super::super) provider_attention_ticks: u64,
    pub(in super::super) consumer_ticks: u64,
    pub(in super::super) consumer_services: u64,
    pub(in super::super) service_preparation_ticks: u64,
    pub(in super::super) service_ticks: u64,
    pub(in super::super) replacement_mass_mg: u64,
    pub(in super::super) provisioning_stops: u64,
    pub(in super::super) provisioning_attention_ticks: u64,
    pub(in super::super) drink_actions: u64,
    pub(in super::super) drink_volume_ul: u64,
    pub(in super::super) meal_actions: u64,
    pub(in super::super) meal_mass_mg: u64,
    pub(in super::super) elapsed_ticks: u64,
    pub(in super::super) metabolic_nj: u128,
    pub(in super::super) hydration_ul: u64,
    pub(in super::super) provider_condition_ppm: u32,
    pub(in super::super) consumer_condition_ppm: u32,
    pub(in super::super) final_metabolic_nj: u128,
    pub(in super::super) final_hydration_ul: u64,
}

impl SelectedProjectOutcome {
    pub(in super::super) fn active_attention_ticks(self) -> u64 {
        self.provider_attention_ticks
            .checked_add(self.service_preparation_ticks)
            .and_then(|ticks| ticks.checked_add(self.service_ticks))
            .and_then(|ticks| ticks.checked_add(self.provisioning_attention_ticks))
            .unwrap_or_else(|| panic!("selected power-project active attention overflowed"))
    }
}

pub(super) fn charge_store(
    registries: &Registries,
    state: &mut AppState,
    method: ManualPowerMethodId,
    equipment: EquipmentId,
    store: EnergyStoreId,
    requested_nj: u128,
    context: &'static str,
) -> ChargeOutcome {
    let requested = Energy::from_nanojoules(requested_nj);
    let before = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("power provider {context} lost the player before charging"));
    let charge = validate_start_manual_power(
        registries,
        state,
        ManualPowerRequest::new(method, equipment, store, requested),
    )
    .unwrap_or_else(|error| panic!("power provider {context} charge failed: {error}"));
    let work = charge.work();
    charge
        .commit(state)
        .unwrap_or_else(|error| panic!("power provider {context} charge commit failed: {error}"));
    let attention_ticks = finish_manual_power_work(registries, state, work, context);
    assert_eq!(
        state
            .energy()
            .get_store(store)
            .map(|record| record.stored().nanojoules()),
        Some(requested_nj),
        "power provider {context} must deliver the requested flywheel work"
    );
    let after = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("power provider {context} lost the player after charging"));
    let condition_after_ppm = state
        .equipment()
        .get_equipment(equipment)
        .map(|record| record.condition().parts_per_million())
        .unwrap_or_else(|| panic!("power provider {context} equipment disappeared"));
    ChargeOutcome {
        attention_ticks,
        metabolic_nj: before
            .metabolic_energy()
            .nanojoules()
            .checked_sub(after.metabolic_energy().nanojoules())
            .unwrap_or_else(|| panic!("power provider {context} metabolic audit underflowed")),
        hydration_ul: u128::from(before.hydration().microliters())
            .checked_sub(u128::from(after.hydration().microliters()))
            .unwrap_or_else(|| panic!("power provider {context} hydration audit underflowed")),
        condition_after_ppm,
    }
}

#[path = "project/primitive.rs"]
mod primitive;
#[path = "project/settlement.rs"]
mod settlement;

pub(in super::super) use primitive::execute_selected_primitive_project;
pub(in super::super) use settlement::execute_selected_settlement_project;
