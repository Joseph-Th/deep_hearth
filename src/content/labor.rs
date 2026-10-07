//! Built-in player-labor methods for primitive power generation and geological observation.

use crate::core::quantity::{Energy, Mass, Pressure, Volume};
use crate::core::time::TickSpan;
use crate::energy::EnergyCarrier;
use crate::geology::GeologicalEvidenceKind;
use crate::labor::{
    LaborRegistry, ManualPowerDefinition, ManualPowerMethodId, ProspectingDefinition,
    ProspectingEquipmentProfile, ProspectingMethodId, ProspectingSpatialResolution,
    SurfaceGatheringDefinition, SurfaceGatheringMethodId,
};
use crate::material::CommodityKey;
use crate::survival::SurvivalExertion;

use super::capabilities::{
    CAPABILITY_MANUAL_POWER_OUTPUT, CAPABILITY_TREADLE_DYNAMO_OUTPUT,
    CAPABILITY_TREADLE_POWER_OUTPUT, CAPABILITY_WALKING_WHEEL_POWER_OUTPUT,
};
use super::equipment::{
    EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER, EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
    EQUIPMENT_TIMBER_CHANNEL_SAMPLING_FRAME, EQUIPMENT_TIMBER_TRIPOD_CORE_DRILL,
};
use super::{
    FORM_FOOD, FORM_LOG, FORM_LUMP, MATERIAL_BERRIES, MATERIAL_CLAY, MATERIAL_STONE, MATERIAL_WOOD,
};

pub const MANUAL_POWER_HAND_CRANK: ManualPowerMethodId = ManualPowerMethodId::new(1);
pub const SURFACE_GATHERING_HAND_COLLECT_STONE: SurfaceGatheringMethodId =
    SurfaceGatheringMethodId::new(1);
pub const SURFACE_GATHERING_HAND_COLLECT_TIMBER: SurfaceGatheringMethodId =
    SurfaceGatheringMethodId::new(2);
pub const SURFACE_GATHERING_HAND_FORAGE_BERRIES: SurfaceGatheringMethodId =
    SurfaceGatheringMethodId::new(3);
pub const SURFACE_GATHERING_HAND_COLLECT_CLAY: SurfaceGatheringMethodId =
    SurfaceGatheringMethodId::new(4);
pub const MANUAL_POWER_FOOT_TREADLE: ManualPowerMethodId = ManualPowerMethodId::new(2);
pub const MANUAL_POWER_WALKING_WHEEL: ManualPowerMethodId = ManualPowerMethodId::new(3);
pub const MANUAL_POWER_TREADLE_DYNAMO: ManualPowerMethodId = ManualPowerMethodId::new(4);
pub const PROSPECTING_FIELD_INSPECTION: ProspectingMethodId = ProspectingMethodId::new(1);
pub const PROSPECTING_DETAILED_FIELD_SURVEY: ProspectingMethodId = ProspectingMethodId::new(2);
pub const PROSPECTING_REGIONAL_RECONNAISSANCE: ProspectingMethodId = ProspectingMethodId::new(3);
pub const PROSPECTING_LOCAL_TRANSECT: ProspectingMethodId = ProspectingMethodId::new(4);
pub const PROSPECTING_INDEXED_CHANNEL_SURVEY: ProspectingMethodId = ProspectingMethodId::new(5);
pub const PROSPECTING_CHANNEL_COMPOSITE_SURVEY: ProspectingMethodId = ProspectingMethodId::new(6);
pub const PROSPECTING_SHALLOW_CORE_SURVEY: ProspectingMethodId = ProspectingMethodId::new(7);

pub(crate) fn build_labor_registry() -> LaborRegistry {
    LaborRegistry::new(
        [
            SurfaceGatheringDefinition::new(
                SURFACE_GATHERING_HAND_COLLECT_STONE,
                CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
                Mass::from_milligrams(5_000_000),
                TickSpan::new(60),
                SurvivalExertion::new(
                    Energy::from_nanojoules(1_200_000_000_000),
                    Volume::from_microliters(300),
                ),
            ),
            // Fallen timber is bulky and awkward to drag into carried custody even when it needs
            // no cutting. Keep the same practical batch ceiling as loose stone but make collection
            // somewhat slower so a wood-heavy camp project carries a visible acquisition cost.
            SurfaceGatheringDefinition::new(
                SURFACE_GATHERING_HAND_COLLECT_TIMBER,
                CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                Mass::from_milligrams(5_000_000),
                TickSpan::new(90),
                SurvivalExertion::new(
                    Energy::from_nanojoules(1_200_000_000_000),
                    Volume::from_microliters(300),
                ),
            ),
            // Foraging is an attention-limited search-and-pick action, not bulk material loading.
            // A smaller batch and lower mass rate keep food acquisition distinct from scooping up
            // construction material without inventing ecology that the world shell does not own yet.
            SurfaceGatheringDefinition::new(
                SURFACE_GATHERING_HAND_FORAGE_BERRIES,
                CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
                Mass::from_milligrams(1_000_000),
                TickSpan::new(120),
                SurvivalExertion::new(
                    Energy::from_nanojoules(900_000_000_000),
                    Volume::from_microliters(220),
                ),
            ),
            // Exposed clay-rich earth remains a low-tech hand-collected root for later pottery and
            // binder work. Keep it distinct from stone so authored loose-earth sources cannot be
            // collected accidentally through a rock-scavenging action.
            SurfaceGatheringDefinition::new(
                SURFACE_GATHERING_HAND_COLLECT_CLAY,
                CommodityKey::new(MATERIAL_CLAY, FORM_LUMP),
                Mass::from_milligrams(5_000_000),
                TickSpan::new(75),
                SurvivalExertion::new(
                    Energy::from_nanojoules(1_000_000_000_000),
                    Volume::from_microliters(260),
                ),
            ),
        ],
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
            // The first ordinary electrical route stays direct player work. The equipment's
            // capability is electrical output after generator losses; the lower metabolic
            // efficiency captures the additional conversion penalty relative to a direct treadle.
            ManualPowerDefinition::new(
                MANUAL_POWER_TREADLE_DYNAMO,
                CAPABILITY_TREADLE_DYNAMO_OUTPUT,
                EnergyCarrier::Electrical,
                180_000,
                20,
                SurvivalExertion::new(
                    Energy::from_nanojoules(3_000_000_000_000),
                    Volume::from_microliters(420),
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
            // Prospecting is active player attention. Broad reconnaissance and transects are
            // compressed so repeated localization does not dominate the extraction they unlock;
            // their higher per-tick exertion preserves the same total physical work. Hand and
            // channel sampling retain deliberate timing/wear costs, while specialist machinery
            // can trade capital and service obligations for faster, tighter local evidence.
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
            // A hand-dug sample can establish local scale, but not settlement-capital reserve
            // confidence. Keep it deliberately coarser than core drilling so better information
            // can change a tool investment instead of only changing a printed number.
            .with_resource_mass_resolution(Mass::from_milligrams(5_000_000)),
            ProspectingDefinition::new(
                PROSPECTING_REGIONAL_RECONNAISSANCE,
                GeologicalEvidenceKind::LooseIndicator,
                TickSpan::new(48),
                16,
                250_000,
                SurvivalExertion::new(
                    Energy::from_nanojoules(800_000_000_000),
                    Volume::from_microliters(200),
                ),
            ),
            ProspectingDefinition::new(
                PROSPECTING_LOCAL_TRANSECT,
                GeologicalEvidenceKind::SurfaceExposure,
                TickSpan::new(12),
                4,
                75_000,
                SurvivalExertion::new(
                    Energy::from_nanojoules(2_200_000_000_000),
                    Volume::from_microliters(600),
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
            .with_resource_mass_resolution(Mass::from_milligrams(5_000_000)),
            // A gridded sampling frame buys a different information product rather than replacing
            // the geological hammer. It screens a broader exposure with tighter abundance
            // uncertainty but produces one composite observation with no hardness or resource-scale
            // estimate. Promising regions still need localized work before extraction planning.
            ProspectingDefinition::new_with_equipment(
                PROSPECTING_CHANNEL_COMPOSITE_SURVEY,
                GeologicalEvidenceKind::ChannelComposite,
                TickSpan::new(36),
                16,
                12_500,
                SurvivalExertion::new(
                    Energy::from_nanojoules(1_600_000_000_000),
                    Volume::from_microliters(450),
                ),
                ProspectingEquipmentProfile::new(EQUIPMENT_TIMBER_CHANNEL_SAMPLING_FRAME, 160),
            ),
            // The tripod core drill is a capital-gated local confirmation tool. Its flywheel makes
            // one prepared sample faster than hand excavation, while one-voxel coverage prevents
            // it from replacing broad reconnaissance or channel screening. High per-tick bit wear
            // preserves a real service cost across repeated precision work.
            ProspectingDefinition::new_with_equipment(
                PROSPECTING_SHALLOW_CORE_SURVEY,
                GeologicalEvidenceKind::CoreSample,
                TickSpan::new(12),
                1,
                8_000,
                SurvivalExertion::new(
                    Energy::from_nanojoules(1_800_000_000_000),
                    Volume::from_microliters(520),
                ),
                ProspectingEquipmentProfile::new(EQUIPMENT_TIMBER_TRIPOD_CORE_DRILL, 1_500),
            )
            .with_excavation_hardness_resolution(Pressure::from_pascals(20_000_000))
            .with_resource_mass_resolution(Mass::from_milligrams(250_000)),
        ],
    )
}

#[cfg(test)]
pub(super) fn empty_labor_registry() -> LaborRegistry {
    LaborRegistry::new(std::iter::empty(), std::iter::empty(), std::iter::empty())
}
