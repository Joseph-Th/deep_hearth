//! Shared constructors for deterministic built-in equipment authoring.

use crate::capability::{CapabilityId, CapabilityProfile, CapabilityValue};
use crate::core::quantity::{Mass, MassFlow, Power};
use crate::equipment::{
    CapabilityConditionCurve, CapabilityConditionPoint, EquipmentMaintenanceProfile,
};
use crate::maintenance::{Condition, MaintenanceThresholds};
use crate::material::CommodityKey;

use super::super::materials::{FORM_INGOT, FORM_SCRAP, MATERIAL_COPPER};

pub(super) const INDUSTRIAL_MAINTENANCE_MASS_DIVISOR: u64 = 1_000;

pub(super) fn condition(parts_per_million: u32) -> Condition {
    match Condition::new(parts_per_million) {
        Ok(condition) => condition,
        Err(error) => panic!("built-in equipment condition is invalid: {error}"),
    }
}

pub(super) fn mass_condition_curve(
    capability: CapabilityId,
    degraded_condition_ppm: u32,
    degraded_mass: Mass,
) -> CapabilityConditionCurve {
    CapabilityConditionCurve::new(
        capability,
        vec![
            CapabilityConditionPoint::new(Condition::FAILED, CapabilityValue::Mass(Mass::ZERO)),
            CapabilityConditionPoint::new(
                condition(degraded_condition_ppm),
                CapabilityValue::Mass(degraded_mass),
            ),
        ],
    )
}

pub(super) fn component_maintenance(
    replacement: CommodityKey,
    component_mass: Mass,
) -> EquipmentMaintenanceProfile {
    EquipmentMaintenanceProfile::new_component_replacement(
        replacement,
        component_mass,
        CommodityKey::new(replacement.material(), FORM_SCRAP),
        Condition::PRISTINE,
    )
}

pub(super) fn industrial_maintenance(equipment_mass: Mass) -> EquipmentMaintenanceProfile {
    // A failed-to-target overhaul represents replacement of one tenth of one percent of machine
    // mass in wear components. Runtime maintenance scales this full-service stock by the actual
    // condition restored, so preventive service consumes less material than deep repair.
    let replacement_mass = Mass::from_milligrams(
        equipment_mass
            .milligrams()
            .div_ceil(INDUSTRIAL_MAINTENANCE_MASS_DIVISOR),
    );
    EquipmentMaintenanceProfile::new(
        CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
        replacement_mass,
        CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
        condition(900_000),
    )
}

pub(super) fn thresholds() -> MaintenanceThresholds {
    match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
        Ok(thresholds) => thresholds,
        Err(error) => panic!("built-in equipment maintenance thresholds are invalid: {error}"),
    }
}

pub(super) fn profile(
    entries: impl IntoIterator<Item = (CapabilityId, CapabilityValue)>,
) -> CapabilityProfile {
    match CapabilityProfile::new(entries) {
        Ok(profile) => profile,
        Err(error) => panic!("built-in equipment capability profile is invalid: {error}"),
    }
}

pub(super) fn mass_flow_condition_curve(
    capability: CapabilityId,
    degraded_condition_ppm: u32,
    degraded_flow: MassFlow,
) -> CapabilityConditionCurve {
    CapabilityConditionCurve::new(
        capability,
        vec![
            CapabilityConditionPoint::new(
                Condition::FAILED,
                CapabilityValue::MassFlow(MassFlow::ZERO),
            ),
            CapabilityConditionPoint::new(
                condition(degraded_condition_ppm),
                CapabilityValue::MassFlow(degraded_flow),
            ),
        ],
    )
}

pub(super) fn power_condition_curve(
    capability: CapabilityId,
    degraded_condition_ppm: u32,
    degraded_power: Power,
) -> CapabilityConditionCurve {
    CapabilityConditionCurve::new(
        capability,
        vec![
            CapabilityConditionPoint::new(Condition::FAILED, CapabilityValue::Power(Power::ZERO)),
            CapabilityConditionPoint::new(
                condition(degraded_condition_ppm),
                CapabilityValue::Power(degraded_power),
            ),
        ],
    )
}
