//! Built-in direct player-power methods that bridge primitive labor into finite mechanical work.

use crate::core::quantity::{Energy, Volume};
use crate::energy::EnergyCarrier;
use crate::labor::{LaborRegistry, ManualPowerDefinition, ManualPowerMethodId};
use crate::survival::SurvivalExertion;

use super::capabilities::CAPABILITY_MANUAL_POWER_OUTPUT;

pub const MANUAL_POWER_HAND_CRANK: ManualPowerMethodId = ManualPowerMethodId::new(1);

pub(crate) fn build_labor_registry() -> LaborRegistry {
    LaborRegistry::new([ManualPowerDefinition::new(
        MANUAL_POWER_HAND_CRANK,
        CAPABILITY_MANUAL_POWER_OUTPUT,
        EnergyCarrier::Mechanical,
        200_000,
        25,
        SurvivalExertion::new(
            Energy::from_nanojoules(1_500_000_000_000),
            Volume::from_microliters(350),
        ),
    )])
}

#[cfg(test)]
pub(super) fn empty_labor_registry() -> LaborRegistry {
    LaborRegistry::new(std::iter::empty())
}
