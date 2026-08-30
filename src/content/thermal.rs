//! Built-in thermal-process resolution semantics for canonical workshop equipment.

use crate::core::quantity::Temperature;
use crate::energy::EnergyCarrier;
use crate::thermal::{
    CastingPhaseChange, CastingProcessDefinition, MeltingProcessDefinition, PhaseChangeForms,
    PhaseChangeProcessProfile, SensibleHeatingProcessDefinition, ThermalRegistry,
};

use super::capabilities::{
    CAPABILITY_COOLING_POWER, CAPABILITY_HEATING_POWER, CAPABILITY_THERMAL_BATCH,
    CAPABILITY_THERMAL_MAX_TEMPERATURE,
};
use super::materials::{
    FORM_INGOT, FORM_MOLTEN, FORM_NATIVE_METAL, FORM_REINFORCEMENT, FORM_SCRAP, MATERIAL_COPPER,
};
use super::processes::{
    PROCESS_CAST_PURE_COPPER, PROCESS_HEAT_MATERIAL_BATCH, PROCESS_MELT_PURE_COPPER,
};

pub(crate) fn build_thermal_registry() -> ThermalRegistry {
    ThermalRegistry::new(
        [SensibleHeatingProcessDefinition::new(
            PROCESS_HEAT_MATERIAL_BATCH,
            CAPABILITY_HEATING_POWER,
            CAPABILITY_THERMAL_MAX_TEMPERATURE,
            CAPABILITY_THERMAL_BATCH,
            EnergyCarrier::Electrical,
            10,
        )],
        [MeltingProcessDefinition::new(
            PROCESS_MELT_PURE_COPPER,
            PhaseChangeProcessProfile::new(
                CAPABILITY_HEATING_POWER,
                CAPABILITY_THERMAL_MAX_TEMPERATURE,
                CAPABILITY_THERMAL_BATCH,
                EnergyCarrier::Electrical,
                10,
            ),
            MATERIAL_COPPER,
            vec![
                FORM_INGOT,
                FORM_REINFORCEMENT,
                FORM_NATIVE_METAL,
                FORM_SCRAP,
            ],
            FORM_MOLTEN,
        )],
        [CastingProcessDefinition::new(
            PROCESS_CAST_PURE_COPPER,
            PhaseChangeProcessProfile::new(
                CAPABILITY_COOLING_POWER,
                CAPABILITY_THERMAL_MAX_TEMPERATURE,
                CAPABILITY_THERMAL_BATCH,
                EnergyCarrier::Thermal,
                10,
            ),
            MATERIAL_COPPER,
            CastingPhaseChange::new(
                PhaseChangeForms::new(FORM_MOLTEN, FORM_INGOT),
                Temperature::from_millikelvin(293_150),
            ),
        )],
    )
}
