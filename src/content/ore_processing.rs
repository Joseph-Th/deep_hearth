//! Built-in ore-processing semantics for canonical workshop machinery.

use crate::core::quantity::{Length, MassSpecificEnergy};
use crate::energy::EnergyCarrier;
use crate::material::{
    CommodityKey, ParticleSizeClass, ParticleSizeDistribution, ParticleSizeRange,
};

const PRIMITIVE_NATIVE_COPPER_SORTING_RECOVERY_PPM: u32 = 900_000;
use crate::ore_processing::{
    ComminutionProcessDefinition, ConstituentRecoveryProfile,
    ConstituentSeparationProcessDefinition, OreProcessingRegistry, PoweredOreProcessProfile,
    ScreeningProcessDefinition,
};

use super::capabilities::{
    CAPABILITY_CRUSHER_BATCH, CAPABILITY_CRUSHER_FLOW, CAPABILITY_GRINDER_BATCH,
    CAPABILITY_GRINDER_FLOW, CAPABILITY_SCREEN_BATCH, CAPABILITY_SCREEN_FLOW,
    CAPABILITY_SEPARATOR_BATCH, CAPABILITY_SEPARATOR_FLOW,
};
use super::materials::{
    FORM_CONCENTRATE, FORM_CRUSHED, FORM_NATIVE_METAL, FORM_ORE, FORM_TAILINGS, MATERIAL_COPPER,
};
use super::processes::{
    PROCESS_CONCENTRATE_COPPER, PROCESS_CRUSH_ORE, PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
    PROCESS_GRIND_CRUSHED_ORE, PROCESS_SCREEN_CRUSHED_ORE, PROCESS_SEPARATE_NATIVE_COPPER,
};

fn particle_size_class(minimum_micrometers: u64, maximum_micrometers: u64) -> ParticleSizeClass {
    let range = ParticleSizeRange::new(
        Length::from_micrometers(minimum_micrometers),
        Length::from_micrometers(maximum_micrometers),
    )
    .unwrap_or_else(|error| panic!("built-in particle-size class is invalid: {error}"));
    ParticleSizeClass::new(range, 1)
        .unwrap_or_else(|error| panic!("built-in particle-size weight is invalid: {error}"))
}

pub(crate) fn build_ore_processing_registry() -> OreProcessingRegistry {
    let particle_size = match ParticleSizeRange::new(
        Length::from_micrometers(500),
        Length::from_micrometers(10_000),
    ) {
        Ok(range) => range,
        Err(error) => panic!("built-in crushed particle range is invalid: {error}"),
    };
    let ground_particle_size = ParticleSizeDistribution::new(vec![
        particle_size_class(500, 2_000),
        particle_size_class(2_001, 4_000),
    ])
    .unwrap_or_else(|error| panic!("built-in ground particle distribution is invalid: {error}"));
    let screen_oversize_range = ParticleSizeRange::new(
        Length::from_micrometers(2_001),
        Length::from_micrometers(4_000),
    )
    .unwrap_or_else(|error| panic!("built-in screen oversize range is invalid: {error}"));
    let fine_particle_size = ParticleSizeDistribution::new(vec![particle_size_class(500, 2_000)])
        .unwrap_or_else(|error| panic!("built-in fine particle distribution is invalid: {error}"));
    let liberated_concentration_range = fine_particle_size.envelope();
    OreProcessingRegistry::new_with_processes(
        [
            ComminutionProcessDefinition::new(
                PROCESS_CRUSH_ORE,
                FORM_ORE,
                FORM_CRUSHED,
                particle_size,
                PoweredOreProcessProfile::new(
                    CAPABILITY_CRUSHER_FLOW,
                    CAPABILITY_CRUSHER_BATCH,
                    EnergyCarrier::Mechanical,
                    MassSpecificEnergy::from_nanojoules_per_milligram(1_000_000),
                    250,
                ),
            ),
            ComminutionProcessDefinition::new(
                PROCESS_GRIND_CRUSHED_ORE,
                FORM_CRUSHED,
                FORM_CRUSHED,
                ground_particle_size,
                PoweredOreProcessProfile::new(
                    CAPABILITY_GRINDER_FLOW,
                    CAPABILITY_GRINDER_BATCH,
                    EnergyCarrier::Mechanical,
                    MassSpecificEnergy::from_nanojoules_per_milligram(3_000_000),
                    300,
                ),
            ),
            ComminutionProcessDefinition::new_with_input_particle_size_range(
                PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
                FORM_CRUSHED,
                FORM_CRUSHED,
                screen_oversize_range,
                fine_particle_size,
                PoweredOreProcessProfile::new(
                    CAPABILITY_GRINDER_FLOW,
                    CAPABILITY_GRINDER_BATCH,
                    EnergyCarrier::Mechanical,
                    MassSpecificEnergy::from_nanojoules_per_milligram(4_000_000),
                    400,
                ),
            ),
        ],
        [ScreeningProcessDefinition::new(
            PROCESS_SCREEN_CRUSHED_ORE,
            FORM_CRUSHED,
            FORM_CRUSHED,
            Length::from_micrometers(2_000),
            PoweredOreProcessProfile::new(
                CAPABILITY_SCREEN_FLOW,
                CAPABILITY_SCREEN_BATCH,
                EnergyCarrier::Mechanical,
                MassSpecificEnergy::from_nanojoules_per_milligram(100_000),
                100,
            ),
        )],
        [
            ConstituentSeparationProcessDefinition::new_sorting(
                PROCESS_SEPARATE_NATIVE_COPPER,
                FORM_CRUSHED,
                MATERIAL_COPPER,
                FORM_NATIVE_METAL,
                FORM_CRUSHED,
                PRIMITIVE_NATIVE_COPPER_SORTING_RECOVERY_PPM,
                PoweredOreProcessProfile::new(
                    CAPABILITY_SEPARATOR_FLOW,
                    CAPABILITY_SEPARATOR_BATCH,
                    EnergyCarrier::Mechanical,
                    MassSpecificEnergy::from_nanojoules_per_milligram(250_000),
                    150,
                ),
            ),
            ConstituentSeparationProcessDefinition::new_concentration(
                PROCESS_CONCENTRATE_COPPER,
                FORM_CRUSHED,
                liberated_concentration_range,
                CommodityKey::new(MATERIAL_COPPER, FORM_CONCENTRATE),
                FORM_TAILINGS,
                ConstituentRecoveryProfile::new(900_000, 200_000),
                PoweredOreProcessProfile::new(
                    CAPABILITY_SEPARATOR_FLOW,
                    CAPABILITY_SEPARATOR_BATCH,
                    EnergyCarrier::Mechanical,
                    MassSpecificEnergy::from_nanojoules_per_milligram(250_000),
                    150,
                ),
            ),
        ],
    )
}
