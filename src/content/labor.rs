//! Built-in player-labor methods for primitive power generation and geological observation.

use crate::core::quantity::{Energy, Mass, Pressure, Volume};
use crate::core::time::TickSpan;
use crate::energy::EnergyCarrier;
use crate::geology::GeologicalEvidenceKind;
use crate::labor::{
    LaborRegistry, ManualPowerDefinition, ManualPowerMethodId, ProspectingDefinition,
    ProspectingEquipmentProfile, ProspectingMethodId, ProspectingSpatialResolution,
};
use crate::survival::SurvivalExertion;

use super::capabilities::{
    CAPABILITY_MANUAL_POWER_OUTPUT, CAPABILITY_TREADLE_POWER_OUTPUT,
    CAPABILITY_WALKING_WHEEL_POWER_OUTPUT,
};
use super::equipment::{
    EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER, EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
};

pub const MANUAL_POWER_HAND_CRANK: ManualPowerMethodId = ManualPowerMethodId::new(1);
pub const MANUAL_POWER_FOOT_TREADLE: ManualPowerMethodId = ManualPowerMethodId::new(2);
pub const MANUAL_POWER_WALKING_WHEEL: ManualPowerMethodId = ManualPowerMethodId::new(3);
pub const PROSPECTING_FIELD_INSPECTION: ProspectingMethodId = ProspectingMethodId::new(1);
pub const PROSPECTING_DETAILED_FIELD_SURVEY: ProspectingMethodId = ProspectingMethodId::new(2);
pub const PROSPECTING_REGIONAL_RECONNAISSANCE: ProspectingMethodId = ProspectingMethodId::new(3);
pub const PROSPECTING_LOCAL_TRANSECT: ProspectingMethodId = ProspectingMethodId::new(4);
pub const PROSPECTING_INDEXED_CHANNEL_SURVEY: ProspectingMethodId = ProspectingMethodId::new(5);

pub(crate) fn build_labor_registry() -> LaborRegistry {
    LaborRegistry::new(
        [
            ManualPowerDefinition::new(
                MANUAL_POWER_HAND_CRANK,
                CAPABILITY_MANUAL_POWER_OUTPUT,
                EnergyCarrier::Mechanical,
                200_000,
                25,
                SurvivalExertion::new(
                    Energy::from_nanojoules(3_000_000_000_000),
                    Volume::from_microliters(350),
                ),
            ),
            ManualPowerDefinition::new(
                MANUAL_POWER_FOOT_TREADLE,
                CAPABILITY_TREADLE_POWER_OUTPUT,
                EnergyCarrier::Mechanical,
                230_000,
                15,
                SurvivalExertion::new(
                    Energy::from_nanojoules(3_000_000_000_000),
                    Volume::from_microliters(400),
                ),
            ),
            ManualPowerDefinition::new(
                MANUAL_POWER_WALKING_WHEEL,
                CAPABILITY_WALKING_WHEEL_POWER_OUTPUT,
                EnergyCarrier::Mechanical,
                260_000,
                10,
                SurvivalExertion::new(
                    Energy::from_nanojoules(3_000_000_000_000),
                    Volume::from_microliters(400),
                ),
            ),
        ],
        [
            // Prospecting is active player attention. Keep the information hierarchy expensive
            // enough to matter without making repeated new-site localization dominate the physical
            // extraction it unlocks. The faster authored durations preserve the prior total body
            // cost and sampling-hammer wear by increasing per-tick exertion/wear proportionally.
            ProspectingDefinition::new(
                PROSPECTING_FIELD_INSPECTION,
                GeologicalEvidenceKind::SurfaceExposure,
                TickSpan::new(12),
                1,
                150_000,
                SurvivalExertion::new(
                    Energy::from_nanojoules(1_000_000_000_000),
                    Volume::from_microliters(250),
                ),
            ),
            ProspectingDefinition::new_with_equipment(
                PROSPECTING_DETAILED_FIELD_SURVEY,
                GeologicalEvidenceKind::ExcavationSample,
                TickSpan::new(24),
                1,
                25_000,
                SurvivalExertion::new(
                    Energy::from_nanojoules(1_300_000_000_000),
                    Volume::from_microliters(320),
                ),
                ProspectingEquipmentProfile::new(EQUIPMENT_STONE_GEOLOGICAL_HAMMER, 240)
                    .with_alternative(EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER, 120),
            )
            .with_excavation_hardness_resolution(Pressure::from_pascals(50_000_000))
            .with_resource_mass_resolution(Mass::from_milligrams(1_000_000)),
            ProspectingDefinition::new(
                PROSPECTING_REGIONAL_RECONNAISSANCE,
                GeologicalEvidenceKind::LooseIndicator,
                TickSpan::new(96),
                16,
                250_000,
                SurvivalExertion::new(
                    Energy::from_nanojoules(400_000_000_000),
                    Volume::from_microliters(100),
                ),
            ),
            ProspectingDefinition::new(
                PROSPECTING_LOCAL_TRANSECT,
                GeologicalEvidenceKind::SurfaceExposure,
                TickSpan::new(24),
                4,
                75_000,
                SurvivalExertion::new(
                    Energy::from_nanojoules(1_100_000_000_000),
                    Volume::from_microliters(300),
                ),
            ),
            ProspectingDefinition::new_with_equipment(
                PROSPECTING_INDEXED_CHANNEL_SURVEY,
                GeologicalEvidenceKind::ExcavationSample,
                // Copper reinforcement turns repeated point-by-point sampling into an indexed
                // channel pass. Keep it slower than one detailed point sample, but fast enough
                // that a multi-site campaign can repay the reinforcement in player attention.
                TickSpan::new(30),
                4,
                25_000,
                SurvivalExertion::new(
                    Energy::from_nanojoules(1_560_000_000_000),
                    Volume::from_microliters(420),
                ),
                ProspectingEquipmentProfile::new(
                    EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER,
                    360,
                ),
            )
            .with_spatial_resolution(ProspectingSpatialResolution::PerVoxel)
            .with_excavation_hardness_resolution(Pressure::from_pascals(50_000_000))
            .with_resource_mass_resolution(Mass::from_milligrams(1_000_000)),
        ],
    )
}

#[cfg(test)]
pub(super) fn empty_labor_registry() -> LaborRegistry {
    LaborRegistry::new(std::iter::empty(), std::iter::empty())
}
