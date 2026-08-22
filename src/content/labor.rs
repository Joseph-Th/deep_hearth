//! Built-in player-labor methods for primitive power generation and geological observation.

use crate::core::quantity::{Energy, Volume};
use crate::core::time::TickSpan;
use crate::energy::EnergyCarrier;
use crate::geology::GeologicalEvidenceKind;
use crate::labor::{
    LaborRegistry, ManualPowerDefinition, ManualPowerMethodId, ProspectingDefinition,
    ProspectingMethodId,
};
use crate::survival::SurvivalExertion;

use super::capabilities::CAPABILITY_MANUAL_POWER_OUTPUT;

pub const MANUAL_POWER_HAND_CRANK: ManualPowerMethodId = ManualPowerMethodId::new(1);
pub const PROSPECTING_FIELD_INSPECTION: ProspectingMethodId = ProspectingMethodId::new(1);

pub(crate) fn build_labor_registry() -> LaborRegistry {
    LaborRegistry::new(
        [ManualPowerDefinition::new(
            MANUAL_POWER_HAND_CRANK,
            CAPABILITY_MANUAL_POWER_OUTPUT,
            EnergyCarrier::Mechanical,
            200_000,
            25,
            SurvivalExertion::new(
                Energy::from_nanojoules(1_500_000_000_000),
                Volume::from_microliters(350),
            ),
        )],
        [ProspectingDefinition::new(
            PROSPECTING_FIELD_INSPECTION,
            GeologicalEvidenceKind::SurfaceExposure,
            TickSpan::new(24),
            1,
            100_000,
            SurvivalExertion::new(
                Energy::from_nanojoules(500_000_000_000),
                Volume::from_microliters(125),
            ),
        )],
    )
}

#[cfg(test)]
pub(super) fn empty_labor_registry() -> LaborRegistry {
    LaborRegistry::new(std::iter::empty(), std::iter::empty())
}
