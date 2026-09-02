//! Read-only workshop outcome auditing and report finalization after scenario execution ends.

use super::*;

pub(super) struct ScenarioAuditInput<'a> {
    pub(super) ids: WorkshopIds,
    pub(super) current_support: StructuralElementId,
    pub(super) initial_matter: deep_hearth::core::quantity::AggregateMass,
    pub(super) initial_survival: deep_hearth::survival::SurvivalAssessment,
    pub(super) initial_ore_composition: &'a deep_hearth::material::MaterialComposition,
}

pub(super) fn finalize_scenario(
    registries: &Registries,
    state: &AppState,
    input: ScenarioAuditInput<'_>,
    mut report: ScenarioReport,
) -> ScenarioReport {
    let ScenarioAuditInput {
        ids,
        current_support,
        initial_matter,
        initial_survival,
        initial_ore_composition,
    } = input;
    let crusher_definition = registries
        .equipment()
        .get_equipment(EQUIPMENT_JAW_CRUSHER)
        .unwrap_or_else(|| panic!("canonical crusher definition disappeared"));
    let thresholds = crusher_definition.maintenance_thresholds();
    let final_condition = state
        .equipment()
        .get_equipment(ids.crusher)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("crusher disappeared after gameplay harness crushing"));
    if thresholds.classify(final_condition) != MaintenanceBand::Normal {
        report.limits.maintenance_warning = true;
    }
    let final_hand_crank_condition = state
        .equipment()
        .get_equipment(ids.hand_crank)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("workshop hand crank disappeared"));

    let crushed_mass = state
        .inventory()
        .get_stockpile(ids.crushed_storage)
        .map(|stockpile| stockpile.stored_mass())
        .unwrap_or_else(|| panic!("crushed storage disappeared"));
    if crushed_mass.is_zero() {
        println!(
            "  material: no crushed output was produced; selected ore remains in source storage or conserved work-in-process"
        );
        println!(
            "  process frontier: mixed-ore melt rejection is not probed because this scenario produced no crushed lot"
        );
    } else {
        let crushed_lots = state
            .inventory()
            .lot_ids(ids.crushed_storage)
            .collect::<Vec<_>>();
        assert!(
            !crushed_lots.is_empty(),
            "positive crushed storage mass must have at least one owned output lot"
        );
        let crusher_process = registries
            .ore_processing()
            .get_comminution(PROCESS_CRUSH_ORE)
            .unwrap_or_else(|| panic!("canonical crusher process definition disappeared"));
        let particle_distribution = crusher_process.output_particle_size_distribution();
        let particle_envelope = particle_distribution.envelope();
        let input_copper_ppm = initial_ore_composition.parts_per_million(MATERIAL_COPPER);
        let contained_copper_floor = crushed_lots
            .iter()
            .map(|lot| {
                let record = state
                    .inventory()
                    .get_lot(*lot)
                    .unwrap_or_else(|| panic!("crushed output lot disappeared"));
                assert_eq!(
                    record.composition(),
                    initial_ore_composition,
                    "every crushed batch must preserve the work-order ore composition"
                );
                assert_eq!(
                    record.particle_size_distribution(),
                    Some(particle_distribution),
                    "every crushed batch from the same authored process must carry the same particle-size state"
                );
                record
                    .composition()
                    .constituent_mass_floor(record.mass(), MATERIAL_COPPER)
            })
            .try_fold(Mass::ZERO, |total, mass| total.checked_add(mass))
            .unwrap_or_else(|| panic!("workshop contained-copper accounting overflowed"));
        println!(
            "  material: crushed={}mg lots={} composition={}ppm Cu / {}ppm gangue contained_copper_floor={}mg particle_classes={} envelope={}..={}um",
            crushed_mass.milligrams(),
            crushed_lots.len(),
            input_copper_ppm,
            1_000_000 - input_copper_ppm,
            contained_copper_floor.milligrams(),
            particle_distribution.classes().len(),
            particle_envelope.minimum_diameter().micrometers(),
            particle_envelope.maximum_diameter().micrometers(),
        );
        println!(
            "  value state: ore grade changes conserved contained copper, but this workshop scenario does not route its crushed feed through the separately verified concentration stage or later smelting"
        );
        if particle_distribution.classes().len() == 1 {
            println!(
                "  preparation state: crusher output is one unresolved size class; a screen cut through that class cannot claim a fabricated yield"
            );
        }
        report.progress.ore_frontier_visible = crushed_lots.iter().all(|lot| {
            let mixed_selection = [MaterialLotSelection::new(*lot, Mass::from_milligrams(1))];
            matches!(
                resolve_melting_process(
                    registries,
                    state,
                    MeltingRequest::new(
                        PROCESS_MELT_PURE_COPPER,
                        ids.crushed_storage,
                        &mixed_selection,
                        ids.furnace,
                        ids.electrical_buffer,
                    ),
                ),
                Err(MeltingResolutionError::Batch(
                    MeltingBatchError::InputFormNotAccepted {
                        found: FORM_CRUSHED
                    }
                ))
            )
        });
        println!(
            "  process frontier: crushed mixed ore cannot enter pure-copper melting={} (the separate ore-preparation probe provides concentration; concentrate reduction/smelting remains outside this workshop)",
            report.progress.ore_frontier_visible
        );
    }

    assert_eq!(
        calculate_matter_accounting(state).map(|accounting| accounting.total()),
        Ok(initial_matter),
        "gameplay workshop must conserve matter across production, relocation, and maintenance"
    );
    assert_eq!(validate_loaded_state(registries, state), Ok(()));
    let small_remaining = state
        .energy()
        .get_store(ids.small_drive)
        .map(|record| record.stored())
        .unwrap_or_else(|| panic!("small mechanical drive disappeared"));
    let large_remaining = state
        .energy()
        .get_store(ids.large_drive)
        .map(|record| record.stored())
        .unwrap_or_else(|| panic!("large mechanical drive disappeared"));
    let active_support = state
        .structures()
        .get_element(current_support)
        .unwrap_or_else(|| panic!("active workshop support disappeared"));
    let maintenance_remaining = state
        .inventory()
        .get_stockpile(ids.maintenance_source)
        .map(|stockpile| stockpile.stored_mass())
        .unwrap_or_else(|| panic!("maintenance replacement stockpile disappeared"));
    let survival = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("workshop player survival state disappeared"));
    let metabolic_energy_spent = initial_survival
        .metabolic_energy()
        .checked_sub(survival.metabolic_energy())
        .unwrap_or_else(|| panic!("workshop metabolic reserve exceeded its scenario start"));
    let hydration_spent = initial_survival
        .hydration()
        .checked_sub(survival.hydration())
        .unwrap_or_else(|| panic!("workshop hydration reserve exceeded its scenario start"));
    report.resources.final_condition_ppm = final_condition.parts_per_million();
    report.resources.small_drive_remaining = small_remaining;
    report.resources.large_drive_remaining = large_remaining;
    report.resources.maintenance_stock_remaining = maintenance_remaining;
    report.resources.elapsed_ticks = state.tick().value();
    report.resources.metabolic_energy_spent = metabolic_energy_spent;
    report.resources.hydration_spent = hydration_spent;
    report.resources.final_vitality_ppm = survival.vitality().parts_per_million();
    report.resources.final_hand_crank_condition_ppm =
        final_hand_crank_condition.parts_per_million();
    println!(
        "  outcome: ore={}/{}mg operations={} adaptive=[total:{} condition:{} stored-work:{}] before_event={} choices=[policy:{} single-source:{} manual-recharges:{}] suspended={} stranded_wip={} equipment=[crusher:{}ppm/{:?} crank:{}ppm] maintenance=[services:{} spent:{}mg remaining:{}mg] mechanical_reserve=[small:{}nJ high-power:{}nJ] manual_generation=[energy:{}nJ ticks:{} body:{}nJ/{}uL] survival=[total-energy:-{}nJ total-hydration:-{}uL vitality:{}ppm] active_support={:?}/cracked:{} ticks={}",
        report.progress.processed_mass.milligrams(),
        report.progress.target_mass.milligrams(),
        report.progress.operations_completed,
        report.progress.adaptive_batch_operations,
        report.progress.condition_adaptive_batch_operations,
        report.progress.energy_adaptive_batch_operations,
        report.progress.operations_before_delivery,
        report.choices.policy_power_choices,
        report.choices.single_source_power_choices,
        report.choices.manual_recharges,
        report.structure.production_suspension,
        report.structure.stranded_work_in_process,
        final_condition.parts_per_million(),
        thresholds.classify(final_condition),
        final_hand_crank_condition.parts_per_million(),
        report.maintenance.services,
        report.maintenance.replacement_spent.milligrams(),
        maintenance_remaining.milligrams(),
        small_remaining.nanojoules(),
        large_remaining.nanojoules(),
        report.resources.manually_generated_energy.nanojoules(),
        report.resources.manual_power_ticks,
        report.resources.manual_power_metabolic_energy.nanojoules(),
        report.resources.manual_power_hydration.microliters(),
        metabolic_energy_spent.nanojoules(),
        hydration_spent.microliters(),
        survival.vitality().parts_per_million(),
        active_support.lifecycle(),
        active_support.is_cracked(),
        state.tick().value(),
    );
    println!(
        "  report: structural_change={} damage_debt={} support_block={} relocation={} structural_stop={} production_suspension={} stranded_wip={} machine_ops=[small:{} large:{}] manual_recharges={} power_choices=[policy:{} single-source:{}] bottlenecks=[energy:{} throughput:{} balanced:{}] maintenance_warning={} maintenance_services={} maintenance_ticks={} maintenance_supply_exhausted={} maintenance_labor_unavailable={} stops=[maintenance:{} energy:{} recovery_declined:{} recovery_survival_limited:{}] ore_frontier={}",
        report.structure.structural_consequence,
        report.structure.structural_damage_debt,
        report.structure.support_failure_blocked_production,
        report.structure.support_relocation,
        report.structure.structural_stop,
        report.structure.production_suspension,
        report.structure.stranded_work_in_process,
        report.choices.small_drive_batches,
        report.choices.large_drive_batches,
        report.choices.manual_recharges,
        report.choices.policy_power_choices,
        report.choices.single_source_power_choices,
        report.limits.energy_bottleneck_batches,
        report.limits.throughput_bottleneck_batches,
        report.limits.balanced_bottleneck_batches,
        report.limits.maintenance_warning,
        report.maintenance.services,
        report.maintenance.service_ticks,
        report.maintenance.supply_exhausted,
        report.maintenance.labor_unavailable,
        report.limits.maintenance_stop,
        report.limits.energy_stop,
        report.limits.manual_recovery_declined,
        report.limits.manual_recovery_survival_limited,
        report.progress.ore_frontier_visible,
    );
    report
}
