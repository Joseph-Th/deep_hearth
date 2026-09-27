//! Portable settlement field instruments that deepen information play without hidden-state access.

use crate::capability::CapabilityProfile;
use crate::content::crafted_parts::{STONE_DRILL_BIT_MASS, STONE_FLYWHEEL_MASS};
use crate::content::materials::{
    FORM_BOARD, FORM_DRILL_BIT, FORM_FLYWHEEL, FORM_HANDLE, FORM_REINFORCEMENT, MATERIAL_COPPER,
    MATERIAL_STONE, MATERIAL_WOOD,
};
use crate::core::quantity::Mass;
use crate::equipment::EquipmentDefinition;
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use super::super::authoring::{EquipmentDefinitionAuthoringExt, assembled_definition, thresholds};
use super::super::{EQUIPMENT_TIMBER_CHANNEL_SAMPLING_FRAME, EQUIPMENT_TIMBER_TRIPOD_CORE_DRILL};

/// Portable gridded frame for systematic channel-chip collection across a bounded exposure.
///
/// The instrument does not reveal hidden deposit identity and carries no generic "prospecting
/// level" capability. Its value exists only because an authored prospecting method accepts this
/// exact physical instrument and converts its repeatable spacing into better aggregate evidence.
pub(super) fn timber_channel_sampling_frame() -> EquipmentDefinition {
    assembled_definition(
        EQUIPMENT_TIMBER_CHANNEL_SAMPLING_FRAME,
        "timber channel-sampling frame",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(1_600_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(400_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                Mass::from_milligrams(40_000),
            ),
        ]),
        CapabilityProfile::default(),
        thresholds(),
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE))
}

/// Portable hand-driven tripod for a shallow, localized core sample.
///
/// The drill is intentionally not a mining machine and has no generic drilling capability. An
/// authored prospecting method accepts this exact instrument, so its flywheel and replaceable bit
/// buy a better information product without granting autonomous excavation or a hidden power
/// network. Compared with the channel frame it is slower, heavier, and local, but can measure
/// hardness and resource scale at the sampled voxel.
pub(super) fn timber_tripod_core_drill() -> EquipmentDefinition {
    assembled_definition(
        EQUIPMENT_TIMBER_TRIPOD_CORE_DRILL,
        "timber tripod shallow-core drill",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(3_200_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(400_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                STONE_FLYWHEEL_MASS,
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT),
                STONE_DRILL_BIT_MASS,
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                Mass::from_milligrams(20_000),
            ),
        ]),
        CapabilityProfile::default(),
        thresholds(),
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT))
}
