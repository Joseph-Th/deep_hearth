//! Built-in axial structural response profiles.

use crate::core::quantity::{Area, Length};
use crate::structural::{
    StructuralLoadMode, StructuralProfileDefinition, StructuralProfileId, StructuralRegistry,
};

pub const STRUCTURAL_PROFILE_AXIAL_COMPRESSION: StructuralProfileId = StructuralProfileId::new(1);
pub const STRUCTURAL_PROFILE_AXIAL_TENSION: StructuralProfileId = StructuralProfileId::new(2);
/// Familiar one-voxel timber post used by the first direct pointed-face build gesture.
pub const BASIC_TIMBER_POST_LENGTH: Length = Length::from_micrometers(1_000_000);
pub const BASIC_TIMBER_POST_CROSS_SECTION: Area = Area::from_square_millimeters(5_000);
/// One square metre of 10 mm solid timber boarding used by the first overhead-cover gesture.
/// At the authored 650 kg/m3 wood density this embodies exactly 6.5 kg of material.
pub const BASIC_TIMBER_COVER_PANEL_THICKNESS: Length = Length::from_micrometers(10_000);
pub const BASIC_TIMBER_COVER_PANEL_AREA: Area = Area::from_square_millimeters(1_000_000);

pub(crate) fn build_structural_registry() -> StructuralRegistry {
    StructuralRegistry::new([
        StructuralProfileDefinition::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            "axial compression",
            StructuralLoadMode::Compression,
            600_000,
            850_000,
            900_000,
        ),
        StructuralProfileDefinition::new(
            STRUCTURAL_PROFILE_AXIAL_TENSION,
            "axial tension",
            StructuralLoadMode::Tension,
            550_000,
            800_000,
            850_000,
        ),
    ])
}
