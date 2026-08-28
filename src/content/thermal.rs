//! Built-in thermal-process resolution semantics for canonical workshop equipment.

use crate::core::quantity::Temperature;
use crate::energy::EnergyCarrier;
use crate::thermal::{
    CastingPhaseChange, CastingProcessDefinition, MeltingProcessDefinition, PhaseChangeForms,
    SensibleHeatingProcessDefinition, ThermalRegistry,
};

use super::capabilities::{
    CAPABILITY_COOLING_POWER, CAPABILITY_HEATING_POWER, CAPABILITY_THERMAL_BATCH,
    CAPABILITY_THERMAL_MAX_TEMPERATURE,
};
use super::materials::{FORM_INGOT, FORM_MOLTEN};
use super::processes::{PROCESS_CAST_PURE_COPPER, PROCESS_MELT_PURE_COPPER};

pub(crate) fn build_thermal_registry() -> ThermalRegistry {
    ThermalRegistry::new(
        std::iter::empty::<SensibleHeatingProcessDefinition>(),
        [MeltingProcessDefinition::new(
            PROCESS_MELT_PURE_COPPER,
            CAPABILITY_HEATING_POWER,
            CAPABILITY_THERMAL_MAX_TEMPERATURE,
            CAPABILITY_THERMAL_BATCH,
            EnergyCarrier::Electrical,
            PhaseChangeForms::new(FORM_INGOT, FORM_MOLTEN),
            10,
        )],
        [CastingProcessDefinition::new(
            PROCESS_CAST_PURE_COPPER,
            CAPABILITY_COOLING_POWER,
            CAPABILITY_THERMAL_MAX_TEMPERATURE,
            CAPABILITY_THERMAL_BATCH,
            EnergyCarrier::Thermal,
            CastingPhaseChange::new(
                PhaseChangeForms::new(FORM_MOLTEN, FORM_INGOT),
                Temperature::from_millikelvin(293_150),
            ),
            10,
        )],
    )
}
