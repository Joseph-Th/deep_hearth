//! Immutable food, drink, and direct-consumption temperature definitions.

use crate::core::quantity::{Energy, Mass, MassSpecificEnergy, Temperature, Volume};
use crate::core::time::TickSpan;
use crate::fluid::FluidDefinitionId;
use crate::material::CommodityKey;

/// Broad dietary identity used for dietary balance and planning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FoodCategory {
    Grain,
    Fruit,
    Protein,
}

/// Inclusive authored temperature envelope for direct human consumption.
///
/// This is deliberately separate from storage compatibility. A container may physically tolerate
/// material that is too hot or too cold to consume safely without another thermal operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsumptionTemperatureRange {
    minimum: Temperature,
    maximum: Temperature,
}

impl ConsumptionTemperatureRange {
    #[must_use]
    pub fn new(minimum: Temperature, maximum: Temperature) -> Self {
        assert!(
            minimum.millikelvin() != 0,
            "consumption minimum temperature must be above absolute zero"
        );
        assert!(
            minimum <= maximum,
            "consumption minimum temperature cannot exceed maximum temperature"
        );
        Self { minimum, maximum }
    }

    #[must_use]
    pub const fn minimum(self) -> Temperature {
        self.minimum
    }

    #[must_use]
    pub const fn maximum(self) -> Temperature {
        self.maximum
    }

    #[must_use]
    pub const fn contains(self, temperature: Temperature) -> bool {
        temperature.millikelvin() >= self.minimum.millikelvin()
            && temperature.millikelvin() <= self.maximum.millikelvin()
    }
}

/// Edibility and perishability of one exact material/form identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FoodDefinition {
    commodity: CommodityKey,
    category: FoodCategory,
    dietary_energy: MassSpecificEnergy,
    hydration_microliters_per_milligram: u32,
    shelf_life: TickSpan,
    consumption_temperature: ConsumptionTemperatureRange,
}

impl FoodDefinition {
    #[must_use]
    pub fn new(
        commodity: CommodityKey,
        category: FoodCategory,
        dietary_energy: MassSpecificEnergy,
        hydration_microliters_per_milligram: u32,
        shelf_life: TickSpan,
        consumption_temperature: ConsumptionTemperatureRange,
    ) -> Self {
        assert!(
            !dietary_energy.is_zero(),
            "food dietary energy must be nonzero"
        );
        assert!(!shelf_life.is_zero(), "food shelf life must be nonzero");
        Self {
            commodity,
            category,
            dietary_energy,
            hydration_microliters_per_milligram,
            shelf_life,
            consumption_temperature,
        }
    }

    #[must_use]
    pub const fn commodity(self) -> CommodityKey {
        self.commodity
    }

    #[must_use]
    pub const fn category(self) -> FoodCategory {
        self.category
    }

    #[must_use]
    pub const fn dietary_energy(self) -> MassSpecificEnergy {
        self.dietary_energy
    }

    /// Minimum whole material mass whose authored dietary energy reaches `target`.
    ///
    /// This is the owner-side inverse of the exact whole-milligram energy offer used by eating.
    /// `None` means the required represented mass exceeds the [`Mass`] range.
    #[must_use]
    pub fn minimum_mass_for_dietary_energy(self, target: Energy) -> Option<Mass> {
        if target.is_zero() {
            return Some(Mass::ZERO);
        }
        let per_milligram = u128::from(self.dietary_energy.nanojoules_per_milligram());
        let milligrams = target.nanojoules().div_ceil(per_milligram);
        u64::try_from(milligrams).ok().map(Mass::from_milligrams)
    }

    #[must_use]
    pub const fn hydration_microliters_per_milligram(self) -> u32 {
        self.hydration_microliters_per_milligram
    }

    #[must_use]
    pub const fn shelf_life(self) -> TickSpan {
        self.shelf_life
    }

    #[must_use]
    pub const fn consumption_temperature(self) -> ConsumptionTemperatureRange {
        self.consumption_temperature
    }
}

/// Hydration contribution of one exact finite fluid identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrinkDefinition {
    fluid: FluidDefinitionId,
    hydration_multiplier_ppm: u32,
    consumption_temperature: ConsumptionTemperatureRange,
}

impl DrinkDefinition {
    #[must_use]
    pub fn new(
        fluid: FluidDefinitionId,
        hydration_multiplier_ppm: u32,
        consumption_temperature: ConsumptionTemperatureRange,
    ) -> Self {
        assert!(
            (1..=1_000_000).contains(&hydration_multiplier_ppm),
            "drink hydration multiplier must be inside 1..=1,000,000 ppm"
        );
        Self {
            fluid,
            hydration_multiplier_ppm,
            consumption_temperature,
        }
    }

    #[must_use]
    pub const fn fluid(self) -> FluidDefinitionId {
        self.fluid
    }

    #[must_use]
    pub const fn hydration_multiplier_ppm(self) -> u32 {
        self.hydration_multiplier_ppm
    }

    /// Minimum consumed whole-fluid volume whose authored hydration offer reaches `target`.
    ///
    /// This owns the inverse rounding boundary of drinking so callers do not reconstruct ppm
    /// arithmetic. `None` means the required represented source volume exceeds [`Volume`].
    #[must_use]
    pub fn minimum_volume_for_hydration(self, target: Volume) -> Option<Volume> {
        if target.is_zero() {
            return Some(Volume::ZERO);
        }
        let numerator = u128::from(target.microliters()) * 1_000_000_u128;
        let microliters = numerator.div_ceil(u128::from(self.hydration_multiplier_ppm));
        u64::try_from(microliters)
            .ok()
            .map(Volume::from_microliters)
    }

    /// Projects the whole-microliter hydration represented by one consumed fluid volume.
    ///
    /// Flooring occurs once at the physiological volume boundary. The authored multiplier is at
    /// most one million ppm, so the result can never exceed the finite source volume.
    #[must_use]
    pub(crate) fn hydration_offer(self, volume: Volume) -> Volume {
        let numerator =
            u128::from(volume.microliters()) * u128::from(self.hydration_multiplier_ppm);
        let microliters = numerator / 1_000_000;
        Volume::from_microliters(
            u64::try_from(microliters)
                .unwrap_or_else(|_| unreachable!("drink hydration cannot exceed source volume")),
        )
    }

    #[must_use]
    pub const fn consumption_temperature(self) -> ConsumptionTemperatureRange {
        self.consumption_temperature
    }
}
