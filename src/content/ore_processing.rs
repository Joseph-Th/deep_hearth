//! Built-in ore-processing semantics for canonical workshop machinery.

use crate::core::quantity::{Length, MassSpecificEnergy};
use crate::energy::EnergyCarrier;
use crate::material::ParticleSizeRange;
use crate::ore_processing::{
    ComminutionOperatingProfile, ComminutionProcessDefinition, OreProcessingRegistry,
};

use super::capabilities::{CAPABILITY_CRUSHER_BATCH, CAPABILITY_CRUSHER_FLOW};
use super::materials::{FORM_CRUSHED, FORM_ORE};
use super::processes::PROCESS_CRUSH_ORE;

pub(crate) fn build_ore_processing_registry() -> OreProcessingRegistry {
    let particle_size = match ParticleSizeRange::new(
        Length::from_micrometers(500),
        Length::from_micrometers(10_000),
    ) {
        Ok(range) => range,
        Err(error) => panic!("built-in crushed particle range is invalid: {error}"),
    };
    OreProcessingRegistry::new([ComminutionProcessDefinition::new(
        PROCESS_CRUSH_ORE,
        FORM_ORE,
        FORM_CRUSHED,
        particle_size,
        ComminutionOperatingProfile::new(
            CAPABILITY_CRUSHER_FLOW,
            CAPABILITY_CRUSHER_BATCH,
            EnergyCarrier::Mechanical,
            MassSpecificEnergy::from_nanojoules_per_milligram(1_000),
            5_000,
        ),
    )])
}
