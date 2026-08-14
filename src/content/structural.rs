//! Built-in axial structural response profiles; specialized construction families may add stricter profiles later.

use crate::structural::{
    StructuralLoadMode, StructuralProfileDefinition, StructuralProfileId, StructuralRegistry,
};

pub const STRUCTURAL_PROFILE_AXIAL_COMPRESSION: StructuralProfileId = StructuralProfileId::new(1);
pub const STRUCTURAL_PROFILE_AXIAL_TENSION: StructuralProfileId = StructuralProfileId::new(2);

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
