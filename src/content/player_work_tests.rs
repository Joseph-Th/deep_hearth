//! Cross-content contracts for fixed-duration exclusive player work.

use crate::core::time::TickSpan;
use crate::labor::calculate_player_work_resource_budget;
use crate::maintenance::Condition;
use crate::survival::{PhysiologyDefinition, SurvivalExertion};

use super::{crafting, equipment, labor, storage, survival};

fn assert_work_fits_full_reserves(
    physiology: PhysiologyDefinition,
    exertion: SurvivalExertion,
    duration: TickSpan,
    context: &str,
) {
    let budget = calculate_player_work_resource_budget(physiology, exertion, duration)
        .unwrap_or_else(|error| panic!("{context} work budget overflowed: {error:?}"));
    assert!(
        budget.metabolic_energy() <= physiology.maximum_metabolic_energy(),
        "{context} requires {} nJ but the player can hold only {} nJ",
        budget.metabolic_energy().nanojoules(),
        physiology.maximum_metabolic_energy().nanojoules(),
    );
    assert!(
        budget.hydration() <= physiology.maximum_hydration(),
        "{context} requires {} uL hydration but the player can hold only {} uL",
        budget.hydration().microliters(),
        physiology.maximum_hydration().microliters(),
    );
}

#[test]
fn fixed_duration_player_work_fits_full_survival_reserves() {
    let physiology = survival::build_survival_registry().physiology();

    for definition in crafting::build_crafting_registry().definitions() {
        assert_work_fits_full_reserves(
            physiology,
            definition.exertion(),
            definition.duration(),
            &format!("manual craft process {}", definition.process().value()),
        );
    }

    for definition in equipment::build_equipment_registry().definitions() {
        let Some(maintenance) = definition.maintenance_profile() else {
            continue;
        };
        assert_work_fits_full_reserves(
            physiology,
            maintenance.exertion(),
            maintenance.required_service_duration(Condition::FAILED),
            &format!("equipment {} full service", definition.id().value()),
        );
    }

    for definition in labor::build_labor_registry().prospecting_definitions() {
        assert_work_fits_full_reserves(
            physiology,
            definition.exertion(),
            definition.duration(),
            &format!("prospecting method {}", definition.id().value()),
        );
    }

    for definition in storage::build_storage_registry().definitions() {
        assert_work_fits_full_reserves(
            physiology,
            definition.dismantle_exertion(),
            definition.dismantle_duration(),
            &format!("storage definition {} dismantling", definition.id().value()),
        );
    }
}
