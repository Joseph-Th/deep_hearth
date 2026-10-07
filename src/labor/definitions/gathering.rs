//! Authored direct player gathering of loose finite surface matter.

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::time::TickSpan;
use crate::material::CommodityKey;
use crate::survival::SurvivalExertion;

/// Stable authored identity for one direct surface-gathering method.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SurfaceGatheringMethodId(u32);

impl SurfaceGatheringMethodId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "surface gathering method id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Authored attention and physiological cost for collecting already-loose surface matter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceGatheringDefinition {
    id: SurfaceGatheringMethodId,
    commodity: CommodityKey,
    maximum_batch_mass: Mass,
    maximum_batch_duration: TickSpan,
    exertion: SurvivalExertion,
}

impl SurfaceGatheringDefinition {
    #[must_use]
    pub fn new(
        id: SurfaceGatheringMethodId,
        commodity: CommodityKey,
        maximum_batch_mass: Mass,
        maximum_batch_duration: TickSpan,
        exertion: SurvivalExertion,
    ) -> Self {
        assert!(!maximum_batch_mass.is_zero());
        assert!(!maximum_batch_duration.is_zero());
        exertion.assert_active_player_work();
        Self {
            id,
            commodity,
            maximum_batch_mass,
            maximum_batch_duration,
            exertion,
        }
    }

    #[must_use]
    pub const fn id(self) -> SurfaceGatheringMethodId {
        self.id
    }

    #[must_use]
    pub const fn commodity(self) -> CommodityKey {
        self.commodity
    }

    #[must_use]
    pub const fn maximum_batch_mass(self) -> Mass {
        self.maximum_batch_mass
    }

    #[must_use]
    pub const fn exertion(self) -> SurvivalExertion {
        self.exertion
    }

    #[must_use]
    pub fn duration_for_mass(self, mass: Mass) -> Option<TickSpan> {
        if mass.is_zero() || mass > self.maximum_batch_mass {
            return None;
        }
        let ticks = u128::from(mass.milligrams())
            .checked_mul(u128::from(self.maximum_batch_duration.value()))?
            .div_ceil(u128::from(self.maximum_batch_mass.milligrams()));
        Some(TickSpan::new(u64::try_from(ticks).ok()?.max(1)))
    }
}
