//! Crusher maintenance resolution and exact service transaction for workshop gameplay evaluation.

use super::super::report::ScenarioMaintenanceReport;
use super::WorkshopIds;
use deep_hearth::content::EQUIPMENT_JAW_CRUSHER;
use deep_hearth::core::state::AppState;
use deep_hearth::equipment::{
    EquipmentMaintenanceError, EquipmentMaintenanceRequest, EquipmentMaintenanceResolutionError,
    resolve_equipment_maintenance, validate_equipment_maintenance,
};
use deep_hearth::labor::PlayerWorkStartError;
use deep_hearth::maintenance::MaintenanceBand;
use deep_hearth::registry::Registries;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MaintenanceAttempt {
    Serviced,
    SupplyExhausted,
    LaborUnavailable,
}

pub(super) fn service_crusher(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    maintenance: &mut ScenarioMaintenanceReport,
) -> MaintenanceAttempt {
    let resolution = match resolve_equipment_maintenance(
        registries,
        state,
        EquipmentMaintenanceRequest::new(
            ids.crusher,
            ids.maintenance_source,
            ids.maintenance_spent,
        ),
    ) {
        Ok(resolution) => resolution,
        Err(EquipmentMaintenanceResolutionError::InsufficientReplacementMaterial {
            stockpile: _stockpile,
            commodity: _commodity,
            available,
            required,
        }) => {
            maintenance.supply_exhausted = true;
            println!(
                "  maintenance supply: service needs {}mg replacement stock but only {}mg remains",
                required.milligrams(),
                available.milligrams(),
            );
            return MaintenanceAttempt::SupplyExhausted;
        }
        Err(error) => panic!("gameplay harness maintenance resolution failed: {error}"),
    };
    let before = resolution.condition_before();
    let after = resolution.condition_after();
    let material_mass = resolution.material_mass();
    let spent_commodity = resolution.spent_commodity();
    let spent_form = registries
        .materials()
        .get_form(spent_commodity.form())
        .map(|form| form.name())
        .unwrap_or_else(|| panic!("gameplay harness maintenance spent form disappeared"));
    let maintenance_start = match validate_equipment_maintenance(registries, state, resolution) {
        Ok(start) => start,
        Err(EquipmentMaintenanceError::PlayerWork(
            PlayerWorkStartError::InsufficientMetabolicEnergy {
                available,
                required,
            },
        )) => {
            maintenance.labor_unavailable = true;
            println!(
                "  maintenance labor: service needs {}nJ metabolic reserve but only {}nJ remains",
                required.nanojoules(),
                available.nanojoules(),
            );
            return MaintenanceAttempt::LaborUnavailable;
        }
        Err(EquipmentMaintenanceError::PlayerWork(
            PlayerWorkStartError::InsufficientHydration {
                available,
                required,
            },
        )) => {
            maintenance.labor_unavailable = true;
            println!(
                "  maintenance labor: service needs {}uL hydration reserve but only {}uL remains",
                required.microliters(),
                available.microliters(),
            );
            return MaintenanceAttempt::LaborUnavailable;
        }
        Err(error) => panic!("gameplay harness maintenance validation failed: {error}"),
    };
    let outcome = maintenance_start
        .commit(state)
        .unwrap_or_else(|error| panic!("gameplay harness maintenance commit failed: {error}"));
    assert_eq!(outcome.condition_before(), before);
    assert_eq!(outcome.target_condition(), after);
    assert_eq!(outcome.material_mass(), material_mass);
    let thresholds = registries
        .equipment()
        .get_equipment(EQUIPMENT_JAW_CRUSHER)
        .unwrap_or_else(|| panic!("workshop crusher definition disappeared"))
        .maintenance_thresholds();
    if thresholds.classify(before) == MaintenanceBand::Critical {
        maintenance.critical_services += 1;
    }
    maintenance.services = maintenance
        .services
        .checked_add(1)
        .unwrap_or_else(|| panic!("gameplay harness maintenance service count overflowed"));
    maintenance.replacement_spent = maintenance
        .replacement_spent
        .checked_add(material_mass)
        .unwrap_or_else(|| panic!("gameplay harness maintenance material accounting overflowed"));
    assert!(
        state
            .inventory()
            .get_stockpile(ids.maintenance_spent)
            .is_some_and(|stockpile| stockpile.get_mass(spent_commodity) >= material_mass),
        "gameplay maintenance must preserve spent matter in its authored non-reusable form"
    );
    println!(
        "  maintenance service: spend={}mg replacement stock condition={}ppm->{}ppm by tick {}; output becomes {spent_form} and is no longer replacement stock",
        material_mass.milligrams(),
        before.parts_per_million(),
        after.parts_per_million(),
        outcome.completes_at().value(),
    );
    MaintenanceAttempt::Serviced
}
