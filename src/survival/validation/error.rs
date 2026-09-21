//! Typed failures for trusted-load validation of persistent survival state.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::fluid::FluidDefinitionId;
use crate::inventory::StockpileId;
use crate::material::MaterialId;
use crate::survival::{FoodCategory, NUTRITION_PARTS_PER_MILLION};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurvivalValidationError {
    EnergyExceedsMaximum,
    HydrationExceedsMaximum,
    VitalityExceedsMaximum,
    VitalityRecoveryRemainderOutOfRange { value: u32 },
    VitalityRecoveryRemainderAtMaximum { value: u32 },
    NutritionExceedsMaximum { category: FoodCategory, value: u32 },
    ConsumedMatterWithoutPlayer,
    UnknownConsumedMaterial { material: MaterialId },
    ZeroConsumedMass { material: MaterialId },
    UnknownConsumedFluid { fluid: FluidDefinitionId },
    ZeroConsumedFluidVolume { fluid: FluidDefinitionId },
    PendingConsumptionWithoutPlayer,
    PendingConsumptionForDeadPlayer,
    PendingConsumptionScheduleInvalid,
    PendingEatingEmpty,
    PendingEatingMassOverflow,
    PendingEatingMassExceedsIntakeLimit,
    PendingEatingTraceInvalid,
    PendingEatingSourceMissing { stockpile: StockpileId },
    PendingEatingFreshnessInvalid,
    PendingEatingAccountingMismatch { material: MaterialId },
    PendingDrinkingVolumeInvalid,
    PendingDrinkingUnknownFluid { fluid: FluidDefinitionId },
    PendingDrinkingNotDrinkable { fluid: FluidDefinitionId },
    PendingDrinkingTemperatureInvalid,
    PendingDrinkingAccountingMismatch { fluid: FluidDefinitionId },
}

impl Display for SurvivalValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EnergyExceedsMaximum => {
                formatter.write_str("player metabolic energy exceeds authored maximum")
            }
            Self::HydrationExceedsMaximum => {
                formatter.write_str("player hydration exceeds authored maximum")
            }
            Self::VitalityExceedsMaximum => {
                formatter.write_str("player vitality exceeds normalized maximum")
            }
            Self::VitalityRecoveryRemainderOutOfRange { value } => write!(
                formatter,
                "player vitality recovery remainder {value} must be below {NUTRITION_PARTS_PER_MILLION}"
            ),
            Self::VitalityRecoveryRemainderAtMaximum { value } => write!(
                formatter,
                "player at maximum vitality cannot retain fractional recovery remainder {value}"
            ),
            Self::NutritionExceedsMaximum { category, value } => write!(
                formatter,
                "player {category:?} nutrition reserve {value} ppm exceeds normalized maximum"
            ),
            Self::ConsumedMatterWithoutPlayer => {
                formatter.write_str("survival owner contains consumed matter without a player")
            }
            Self::UnknownConsumedMaterial { material } => write!(
                formatter,
                "survival consumption references unknown material {}",
                material.value()
            ),
            Self::ZeroConsumedMass { material } => write!(
                formatter,
                "survival consumption stores zero mass for material {}",
                material.value()
            ),
            Self::UnknownConsumedFluid { fluid } => write!(
                formatter,
                "survival consumption references unknown fluid {}",
                fluid.value()
            ),
            Self::ZeroConsumedFluidVolume { fluid } => write!(
                formatter,
                "survival consumption stores zero volume for fluid {}",
                fluid.value()
            ),
            Self::PendingConsumptionWithoutPlayer => {
                formatter.write_str("pending direct consumption exists without a player")
            }
            Self::PendingConsumptionForDeadPlayer => {
                formatter.write_str("dead player cannot retain pending direct consumption")
            }
            Self::PendingConsumptionScheduleInvalid => {
                formatter.write_str("pending direct consumption has an invalid active schedule")
            }
            Self::PendingEatingEmpty => {
                formatter.write_str("pending eating contains no consumed food traces")
            }
            Self::PendingEatingMassOverflow => {
                formatter.write_str("pending eating consumed mass overflowed")
            }
            Self::PendingEatingMassExceedsIntakeLimit => {
                formatter.write_str("pending eating mass exceeds the authored direct meal limit")
            }
            Self::PendingEatingTraceInvalid => {
                formatter.write_str("pending eating contains an invalid consumed food trace")
            }
            Self::PendingEatingSourceMissing { stockpile } => write!(
                formatter,
                "pending eating references missing source stockpile {}",
                stockpile.value()
            ),
            Self::PendingEatingFreshnessInvalid => formatter.write_str(
                "pending eating cannot prove that every consumed food lot was fresh at admission",
            ),
            Self::PendingEatingAccountingMismatch { material } => write!(
                formatter,
                "pending eating material {} does not reconcile its pre-intake baseline with survival consumed-matter accounting",
                material.value()
            ),
            Self::PendingDrinkingVolumeInvalid => {
                formatter.write_str("pending drinking volume or authored duration is invalid")
            }
            Self::PendingDrinkingUnknownFluid { fluid } => write!(
                formatter,
                "pending drinking references unknown fluid {}",
                fluid.value()
            ),
            Self::PendingDrinkingNotDrinkable { fluid } => write!(
                formatter,
                "pending drinking references unauthored drink fluid {}",
                fluid.value()
            ),
            Self::PendingDrinkingTemperatureInvalid => formatter
                .write_str("pending drinking temperature is outside the authored intake range"),
            Self::PendingDrinkingAccountingMismatch { fluid } => write!(
                formatter,
                "pending drinking fluid {} does not reconcile its pre-intake baseline with survival consumed-volume accounting",
                fluid.value()
            ),
        }
    }
}

impl Error for SurvivalValidationError {}
