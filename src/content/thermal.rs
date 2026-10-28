//! Built-in thermal-process resolution semantics for canonical workshop equipment.

use crate::core::quantity::Temperature;
use crate::energy::EnergyCarrier;
use crate::material::CommodityKey;
use crate::thermal::{
    CastingPhaseChange, CastingProcessDefinition, FiringConversion, MeltingProcessDefinition,
    PhaseChangeForms, PhaseChangeProcessProfile, SensibleCoolingProcessDefinition,
    SensibleHeatingProcessDefinition, ThermalRegistry,
};

use super::capabilities::{
    CAPABILITY_COOLING_POWER, CAPABILITY_HEATING_POWER, CAPABILITY_KILN_COOLING_POWER,
    CAPABILITY_KILN_HEATING_POWER, CAPABILITY_THERMAL_BATCH, CAPABILITY_THERMAL_MAX_TEMPERATURE,
};
use super::materials::{
    FORM_BLOCK, FORM_CHIP, FORM_FIRED_CROCK, FORM_FIRED_INGOT_MOLD, FORM_GREENWARE_CROCK,
    FORM_GREENWARE_INGOT_MOLD, FORM_INGOT, FORM_MOLTEN, FORM_NATIVE_METAL, FORM_REINFORCEMENT,
    FORM_SCRAP, MATERIAL_CLAY, MATERIAL_COPPER, MATERIAL_IRON, MATERIAL_LEAD, MATERIAL_TIN,
    MATERIAL_ZINC,
};
use super::processes::{
    PROCESS_CAST_IRON_INGOT, PROCESS_CAST_LEAD_INGOT, PROCESS_CAST_PURE_COPPER,
    PROCESS_CAST_TIN_INGOT, PROCESS_CAST_ZINC_INGOT, PROCESS_COOL_FIRED_CLAY_CROCK,
    PROCESS_COOL_FIRED_CLAY_INGOT_MOLD, PROCESS_FIRE_CLAY_CROCK, PROCESS_FIRE_CLAY_INGOT_MOLD,
    PROCESS_HEAT_MATERIAL_BATCH, PROCESS_MELT_PURE_COPPER, PROCESS_MELT_RECYCLED_IRON,
    PROCESS_MELT_RECYCLED_LEAD, PROCESS_MELT_RECYCLED_TIN, PROCESS_MELT_RECYCLED_ZINC,
};

pub(crate) fn build_thermal_registry() -> ThermalRegistry {
    let melting_profile = || {
        PhaseChangeProcessProfile::new(
            CAPABILITY_HEATING_POWER,
            CAPABILITY_THERMAL_MAX_TEMPERATURE,
            CAPABILITY_THERMAL_BATCH,
            EnergyCarrier::Electrical,
            10,
        )
    };
    let casting_profile = || {
        PhaseChangeProcessProfile::new(
            CAPABILITY_COOLING_POWER,
            CAPABILITY_THERMAL_MAX_TEMPERATURE,
            CAPABILITY_THERMAL_BATCH,
            EnergyCarrier::Thermal,
            10,
        )
    };
    ThermalRegistry::new(
        [
            SensibleHeatingProcessDefinition::new(
                PROCESS_FIRE_CLAY_INGOT_MOLD,
                CAPABILITY_KILN_HEATING_POWER,
                CAPABILITY_THERMAL_MAX_TEMPERATURE,
                CAPABILITY_THERMAL_BATCH,
                EnergyCarrier::Thermal,
                40,
            )
            .with_firing_conversion(FiringConversion::new(
                CommodityKey::new(MATERIAL_CLAY, FORM_GREENWARE_INGOT_MOLD),
                CommodityKey::new(MATERIAL_CLAY, FORM_FIRED_INGOT_MOLD),
                Temperature::from_millikelvin(1_100_000),
            )),
            SensibleHeatingProcessDefinition::new(
                PROCESS_HEAT_MATERIAL_BATCH,
                CAPABILITY_HEATING_POWER,
                CAPABILITY_THERMAL_MAX_TEMPERATURE,
                CAPABILITY_THERMAL_BATCH,
                EnergyCarrier::Electrical,
                10,
            ),
            SensibleHeatingProcessDefinition::new(
                PROCESS_FIRE_CLAY_CROCK,
                CAPABILITY_KILN_HEATING_POWER,
                CAPABILITY_THERMAL_MAX_TEMPERATURE,
                CAPABILITY_THERMAL_BATCH,
                EnergyCarrier::Thermal,
                40,
            )
            .with_firing_conversion(FiringConversion::new(
                CommodityKey::new(MATERIAL_CLAY, FORM_GREENWARE_CROCK),
                CommodityKey::new(MATERIAL_CLAY, FORM_FIRED_CROCK),
                Temperature::from_millikelvin(1_100_000),
            )),
        ],
        [
            MeltingProcessDefinition::new(
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
                    FORM_CHIP,
                    FORM_REINFORCEMENT,
                    FORM_NATIVE_METAL,
                    FORM_SCRAP,
                ],
                FORM_MOLTEN,
            ),
            MeltingProcessDefinition::new(
                PROCESS_MELT_RECYCLED_IRON,
                melting_profile(),
                MATERIAL_IRON,
                vec![FORM_INGOT, FORM_CHIP, FORM_SCRAP, FORM_BLOCK],
                FORM_MOLTEN,
            ),
            MeltingProcessDefinition::new(
                PROCESS_MELT_RECYCLED_TIN,
                melting_profile(),
                MATERIAL_TIN,
                vec![FORM_INGOT, FORM_CHIP, FORM_SCRAP, FORM_BLOCK],
                FORM_MOLTEN,
            ),
            MeltingProcessDefinition::new(
                PROCESS_MELT_RECYCLED_ZINC,
                melting_profile(),
                MATERIAL_ZINC,
                vec![FORM_INGOT, FORM_CHIP, FORM_SCRAP, FORM_BLOCK],
                FORM_MOLTEN,
            ),
            MeltingProcessDefinition::new(
                PROCESS_MELT_RECYCLED_LEAD,
                melting_profile(),
                MATERIAL_LEAD,
                vec![FORM_INGOT, FORM_SCRAP],
                FORM_MOLTEN,
            ),
        ],
        [
            CastingProcessDefinition::new(
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
            ),
            CastingProcessDefinition::new(
                PROCESS_CAST_IRON_INGOT,
                casting_profile(),
                MATERIAL_IRON,
                CastingPhaseChange::new(
                    PhaseChangeForms::new(FORM_MOLTEN, FORM_INGOT),
                    Temperature::from_millikelvin(293_150),
                ),
            ),
            CastingProcessDefinition::new(
                PROCESS_CAST_TIN_INGOT,
                casting_profile(),
                MATERIAL_TIN,
                CastingPhaseChange::new(
                    PhaseChangeForms::new(FORM_MOLTEN, FORM_INGOT),
                    Temperature::from_millikelvin(293_150),
                ),
            ),
            CastingProcessDefinition::new(
                PROCESS_CAST_ZINC_INGOT,
                casting_profile(),
                MATERIAL_ZINC,
                CastingPhaseChange::new(
                    PhaseChangeForms::new(FORM_MOLTEN, FORM_INGOT),
                    Temperature::from_millikelvin(293_150),
                ),
            ),
            CastingProcessDefinition::new(
                PROCESS_CAST_LEAD_INGOT,
                casting_profile(),
                MATERIAL_LEAD,
                CastingPhaseChange::new(
                    PhaseChangeForms::new(FORM_MOLTEN, FORM_INGOT),
                    Temperature::from_millikelvin(293_150),
                ),
            ),
        ],
    )
    .with_sensible_cooling([
        SensibleCoolingProcessDefinition::new(
            PROCESS_COOL_FIRED_CLAY_CROCK,
            CommodityKey::new(MATERIAL_CLAY, FORM_FIRED_CROCK),
            Temperature::from_millikelvin(293_150),
            PhaseChangeProcessProfile::new(
                CAPABILITY_KILN_COOLING_POWER,
                CAPABILITY_THERMAL_MAX_TEMPERATURE,
                CAPABILITY_THERMAL_BATCH,
                EnergyCarrier::Thermal,
                20,
            ),
        ),
        SensibleCoolingProcessDefinition::new(
            PROCESS_COOL_FIRED_CLAY_INGOT_MOLD,
            CommodityKey::new(MATERIAL_CLAY, FORM_FIRED_INGOT_MOLD),
            Temperature::from_millikelvin(293_150),
            PhaseChangeProcessProfile::new(
                CAPABILITY_KILN_COOLING_POWER,
                CAPABILITY_THERMAL_MAX_TEMPERATURE,
                CAPABILITY_THERMAL_BATCH,
                EnergyCarrier::Thermal,
                20,
            ),
        ),
    ])
}
