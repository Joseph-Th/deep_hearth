//! Material-backed primitive self-defense weapons with distinct reach and handling.

use crate::core::quantity::Mass;
use crate::equipment::{EquipmentDefinition, MeleeWeaponProfile};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use super::super::authoring::{
    EquipmentDefinitionAuthoringExt, assembled_definition, profile, thresholds,
};
use super::super::{EQUIPMENT_STONE_SPEAR, EQUIPMENT_WOODEN_CLUB};
use crate::content::materials::{FORM_HANDLE, FORM_LOG, FORM_TOOL, MATERIAL_STONE, MATERIAL_WOOD};

/// One solid portable branch/log, carved only enough for an improvised grip.
/// It can be replaced from the same conserved wood commodity after wear.
pub(super) fn wooden_club() -> EquipmentDefinition {
    assembled_definition(
        EQUIPMENT_WOODEN_CLUB,
        "rough wooden club",
        MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(1_000_000),
        )]),
        profile([]),
        thresholds(),
    )
    .with_melee_profile(MeleeWeaponProfile::new(1_500, 22, 3_000))
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_WOOD, FORM_LOG))
}

/// A prepared long handle and chipped stone point. The material profile
/// is exact, so a weapon never materializes a free stone head or shaft.
pub(super) fn stone_spear() -> EquipmentDefinition {
    assembled_definition(
        EQUIPMENT_STONE_SPEAR,
        "hafted stone thrusting spear",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(600_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(200_000),
            ),
        ]),
        profile([]),
        thresholds(),
    )
    .with_melee_profile(MeleeWeaponProfile::new(2_750, 28, 4_500))
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_TOOL))
}
