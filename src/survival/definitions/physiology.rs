//! Immutable survival, nutrition, intake, and thermal physiology definitions.

mod locomotion;

pub use locomotion::{
    BodyKinematicsDefinition, BodyStanceDefinition, BodyViewDefinition, LocomotionDefinition,
    LocomotionSpeedDefinition, WalkingSpeedDefinition,
};

use crate::core::arithmetic::{NORMALIZED_PARTS_PER_MILLION, checked_mul_div_with_remainder};
use crate::core::quantity::{Energy, Mass, Volume};
use crate::core::time::TickSpan;

/// Authored rate at which recent dietary balance fades and can support vitality recovery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NutritionDefinition {
    decay_ppm_per_tick: u32,
    vitality_recovery_ppm_per_tick: u32,
    full_reserve_food_mass: Mass,
}

impl NutritionDefinition {
    #[must_use]
    pub fn new(
        decay_ppm_per_tick: u32,
        vitality_recovery_ppm_per_tick: u32,
        full_reserve_food_mass: Mass,
    ) -> Self {
        assert!(decay_ppm_per_tick > 0, "nutrition decay must be nonzero");
        assert!(
            decay_ppm_per_tick <= NORMALIZED_PARTS_PER_MILLION,
            "nutrition decay cannot exceed the normalized reserve range"
        );
        assert!(
            vitality_recovery_ppm_per_tick > 0,
            "nutrition-supported vitality recovery must be nonzero"
        );
        assert!(
            vitality_recovery_ppm_per_tick <= NORMALIZED_PARTS_PER_MILLION,
            "nutrition-supported vitality recovery cannot exceed normalized vitality"
        );
        assert!(
            !full_reserve_food_mass.is_zero(),
            "full dietary reserve food mass must be nonzero"
        );
        Self {
            decay_ppm_per_tick,
            vitality_recovery_ppm_per_tick,
            full_reserve_food_mass,
        }
    }

    #[must_use]
    pub const fn decay_ppm_per_tick(self) -> u32 {
        self.decay_ppm_per_tick
    }

    #[must_use]
    pub const fn vitality_recovery_ppm_per_tick(self) -> u32 {
        self.vitality_recovery_ppm_per_tick
    }

    /// Mass of a given food category that replenishes its full recent dietary reserve.
    /// Metabolic calories and nutrition quality are intentionally distinct benefits.
    #[must_use]
    pub const fn full_reserve_food_mass(self) -> Mass {
        self.full_reserve_food_mass
    }
}

/// Immutable physiology parameters for the player survival owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MetabolismDefinition {
    maximum: Energy,
    hungry_below: Energy,
    basal_cost_per_tick: Energy,
}

impl MetabolismDefinition {
    #[must_use]
    pub fn new(maximum: Energy, hungry_below: Energy, basal_cost_per_tick: Energy) -> Self {
        assert!(
            !maximum.is_zero(),
            "maximum metabolic energy must be nonzero"
        );
        assert!(
            hungry_below < maximum,
            "hunger warning threshold must be below maximum metabolic energy"
        );
        assert!(
            !basal_cost_per_tick.is_zero(),
            "basal energy cost per tick must be nonzero"
        );
        assert!(
            basal_cost_per_tick <= maximum,
            "basal energy cost per tick cannot exceed maximum metabolic reserve"
        );
        Self {
            maximum,
            hungry_below,
            basal_cost_per_tick,
        }
    }
}

/// Immutable hydration capacity, warning threshold, and passive loss rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HydrationDefinition {
    maximum: Volume,
    thirsty_below: Volume,
    loss_per_tick: Volume,
}

impl HydrationDefinition {
    #[must_use]
    pub fn new(maximum: Volume, thirsty_below: Volume, loss_per_tick: Volume) -> Self {
        assert!(!maximum.is_zero(), "maximum hydration must be nonzero");
        assert!(
            thirsty_below < maximum,
            "thirst warning threshold must be below maximum hydration"
        );
        assert!(
            !loss_per_tick.is_zero(),
            "hydration loss per tick must be nonzero"
        );
        assert!(
            loss_per_tick <= maximum,
            "hydration loss per tick cannot exceed maximum hydration reserve"
        );
        Self {
            maximum,
            thirsty_below,
            loss_per_tick,
        }
    }
}

/// Authored quantity and attention-time envelope for direct consumption.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectConsumptionDefinition {
    minimum_meal_mass: Mass,
    maximum_meal_mass: Mass,
    maximum_meal_duration: TickSpan,
    minimum_drink_volume: Volume,
    maximum_drink_volume: Volume,
    maximum_drink_duration: TickSpan,
}

impl DirectConsumptionDefinition {
    #[must_use]
    pub fn new(
        minimum_meal_mass: Mass,
        maximum_meal_mass: Mass,
        maximum_meal_duration: TickSpan,
        minimum_drink_volume: Volume,
        maximum_drink_volume: Volume,
        maximum_drink_duration: TickSpan,
    ) -> Self {
        assert!(
            !minimum_meal_mass.is_zero(),
            "minimum direct meal mass must be nonzero"
        );
        assert!(
            !maximum_meal_mass.is_zero(),
            "maximum direct meal mass must be nonzero"
        );
        assert!(
            minimum_meal_mass <= maximum_meal_mass,
            "minimum direct meal mass cannot exceed maximum direct meal mass"
        );
        assert!(
            !maximum_meal_duration.is_zero(),
            "maximum direct meal duration must be nonzero"
        );
        assert!(
            !minimum_drink_volume.is_zero(),
            "minimum direct drink volume must be nonzero"
        );
        assert!(
            !maximum_drink_volume.is_zero(),
            "maximum direct drink volume must be nonzero"
        );
        assert!(
            minimum_drink_volume <= maximum_drink_volume,
            "minimum direct drink volume cannot exceed maximum direct drink volume"
        );
        assert!(
            !maximum_drink_duration.is_zero(),
            "maximum direct drink duration must be nonzero"
        );
        Self {
            minimum_meal_mass,
            maximum_meal_mass,
            maximum_meal_duration,
            minimum_drink_volume,
            maximum_drink_volume,
            maximum_drink_duration,
        }
    }

    #[must_use]
    pub const fn minimum_meal_mass(self) -> Mass {
        self.minimum_meal_mass
    }

    #[must_use]
    pub const fn maximum_meal_mass(self) -> Mass {
        self.maximum_meal_mass
    }

    #[must_use]
    pub const fn minimum_drink_volume(self) -> Volume {
        self.minimum_drink_volume
    }

    #[must_use]
    pub const fn maximum_drink_volume(self) -> Volume {
        self.maximum_drink_volume
    }

    fn scaled_duration(amount: u64, maximum: u64, maximum_duration: TickSpan) -> TickSpan {
        let ticks = u128::from(amount)
            .checked_mul(u128::from(maximum_duration.value()))
            .unwrap_or_else(|| unreachable!("u64 direct-consumption scaling fits u128"))
            .div_ceil(u128::from(maximum));
        let ticks = u64::try_from(ticks)
            .unwrap_or_else(|_| unreachable!("bounded direct-consumption duration fits u64"));
        TickSpan::new(ticks.max(1))
    }

    #[must_use]
    pub fn meal_duration(self, mass: Mass) -> Option<TickSpan> {
        if mass < self.minimum_meal_mass || mass > self.maximum_meal_mass {
            return None;
        }
        Some(Self::scaled_duration(
            mass.milligrams(),
            self.maximum_meal_mass.milligrams(),
            self.maximum_meal_duration,
        ))
    }

    #[must_use]
    pub fn drink_duration(self, volume: Volume) -> Option<TickSpan> {
        if volume < self.minimum_drink_volume || volume > self.maximum_drink_volume {
            return None;
        }
        Some(Self::scaled_duration(
            volume.microliters(),
            self.maximum_drink_volume.microliters(),
            self.maximum_drink_duration,
        ))
    }
}

/// Authored survival stresses that can deplete vitality.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurvivalStressDefinition {
    starvation_ppm_per_tick: u32,
    dehydration_ppm_per_tick: u32,
    thermal_comfort: ThermalComfortDefinition,
}

impl SurvivalStressDefinition {
    #[must_use]
    pub fn new(
        starvation_ppm_per_tick: u32,
        dehydration_ppm_per_tick: u32,
        thermal_comfort: ThermalComfortDefinition,
    ) -> Self {
        assert!(
            starvation_ppm_per_tick > 0,
            "starvation vitality loss must be nonzero"
        );
        assert!(
            starvation_ppm_per_tick <= NORMALIZED_PARTS_PER_MILLION,
            "starvation vitality loss cannot exceed normalized vitality"
        );
        assert!(
            dehydration_ppm_per_tick > 0,
            "dehydration vitality loss must be nonzero"
        );
        assert!(
            dehydration_ppm_per_tick <= NORMALIZED_PARTS_PER_MILLION,
            "dehydration vitality loss cannot exceed normalized vitality"
        );
        Self {
            starvation_ppm_per_tick,
            dehydration_ppm_per_tick,
            thermal_comfort,
        }
    }

    #[must_use]
    pub const fn starvation_ppm_per_tick(self) -> u32 {
        self.starvation_ppm_per_tick
    }

    #[must_use]
    pub const fn dehydration_ppm_per_tick(self) -> u32 {
        self.dehydration_ppm_per_tick
    }

    #[must_use]
    pub const fn thermal_comfort(self) -> ThermalComfortDefinition {
        self.thermal_comfort
    }
}

/// Authored unsheltered thermal-comfort reserve and local-heat response.
///
/// This models only the player's short-horizon warmth reserve. Ambient weather, clothing
/// insulation, enclosure quality, and broader shelter remain separate owners. `OverheadCover` is
/// deliberately narrower: it consumes a coarse fully opaque roof assessment over the current body
/// footprint and reduces exposure without implying walls, insulation, weather sealing, or heating.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThermalExposure {
    Open,
    OverheadCover,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThermalComfortDefinition {
    warning_below_ppm: u32,
    unsheltered_loss_ppm_per_tick: u32,
    overhead_cover_loss_ppm_per_tick: u32,
    maximum_local_heat_gain_ppm_per_tick: u32,
    local_heat_for_maximum_gain: Energy,
    cold_vitality_loss_ppm_per_tick: u32,
}

impl ThermalComfortDefinition {
    #[must_use]
    pub fn new(
        warning_below_ppm: u32,
        unsheltered_loss_ppm_per_tick: u32,
        overhead_cover_loss_ppm_per_tick: u32,
        maximum_local_heat_gain_ppm_per_tick: u32,
        local_heat_for_maximum_gain: Energy,
        cold_vitality_loss_ppm_per_tick: u32,
    ) -> Self {
        assert!(warning_below_ppm < NORMALIZED_PARTS_PER_MILLION);
        assert!(unsheltered_loss_ppm_per_tick > 0);
        assert!(unsheltered_loss_ppm_per_tick <= NORMALIZED_PARTS_PER_MILLION);
        assert!(overhead_cover_loss_ppm_per_tick > 0);
        assert!(overhead_cover_loss_ppm_per_tick <= unsheltered_loss_ppm_per_tick);
        assert!(maximum_local_heat_gain_ppm_per_tick > 0);
        assert!(maximum_local_heat_gain_ppm_per_tick <= NORMALIZED_PARTS_PER_MILLION);
        assert!(!local_heat_for_maximum_gain.is_zero());
        assert!(cold_vitality_loss_ppm_per_tick > 0);
        assert!(cold_vitality_loss_ppm_per_tick <= NORMALIZED_PARTS_PER_MILLION);
        Self {
            warning_below_ppm,
            unsheltered_loss_ppm_per_tick,
            overhead_cover_loss_ppm_per_tick,
            maximum_local_heat_gain_ppm_per_tick,
            local_heat_for_maximum_gain,
            cold_vitality_loss_ppm_per_tick,
        }
    }

    #[must_use]
    pub const fn warning_below_ppm(self) -> u32 {
        self.warning_below_ppm
    }

    #[must_use]
    pub const fn unsheltered_loss_ppm_per_tick(self) -> u32 {
        self.unsheltered_loss_ppm_per_tick
    }

    #[must_use]
    pub const fn overhead_cover_loss_ppm_per_tick(self) -> u32 {
        self.overhead_cover_loss_ppm_per_tick
    }

    #[must_use]
    pub const fn exposure_loss_ppm_per_tick(self, exposure: ThermalExposure) -> u32 {
        match exposure {
            ThermalExposure::Open => self.unsheltered_loss_ppm_per_tick,
            ThermalExposure::OverheadCover => self.overhead_cover_loss_ppm_per_tick,
        }
    }

    /// Earliest slow tick when the player ceases to be comfortable if the specified cover
    /// stays unchanged and there is no local heat. This is a conditional planning estimate,
    /// not a promise about the future environment or fuel supply.
    ///
    /// The threshold is strict: comfort exactly at the warning is still comfortable. When the
    /// authored warning is zero, the zero-comfort Cold band is the first adverse state.
    #[must_use]
    pub fn ticks_until_unheated_discomfort(
        self,
        current_comfort_ppm: u32,
        exposure: ThermalExposure,
    ) -> TickSpan {
        assert!(
            current_comfort_ppm <= NORMALIZED_PARTS_PER_MILLION,
            "thermal-comfort projection requires normalized player comfort"
        );
        let comfortable_floor = self.warning_below_ppm.max(1);
        if current_comfort_ppm < comfortable_floor {
            return TickSpan::ZERO;
        }
        let reserve_to_spend = current_comfort_ppm - comfortable_floor + 1;
        TickSpan::new(u64::from(
            reserve_to_spend.div_ceil(self.exposure_loss_ppm_per_tick(exposure)),
        ))
    }

    #[must_use]
    pub const fn cold_vitality_loss_ppm_per_tick(self) -> u32 {
        self.cold_vitality_loss_ppm_per_tick
    }

    #[must_use]
    pub const fn local_heat_for_maximum_gain(self) -> Energy {
        self.local_heat_for_maximum_gain
    }

    #[must_use]
    pub fn local_heat_gain_ppm(self, local_heat: Energy) -> u32 {
        if local_heat.is_zero() {
            return 0;
        }
        if local_heat >= self.local_heat_for_maximum_gain {
            return self.maximum_local_heat_gain_ppm_per_tick;
        }
        let (gain, _remainder) = checked_mul_div_with_remainder(
            local_heat.nanojoules(),
            u128::from(self.maximum_local_heat_gain_ppm_per_tick),
            self.local_heat_for_maximum_gain.nanojoules(),
            0,
        )
        .unwrap_or_else(|| {
            unreachable!("sub-threshold thermal-comfort gain is bounded by authored maximum")
        });
        u32::try_from(gain)
            .unwrap_or_else(|_| unreachable!("bounded thermal-comfort gain fits u32"))
    }
}

/// Immutable physiology parameters for the player survival owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysiologyDefinition {
    metabolism: MetabolismDefinition,
    hydration: HydrationDefinition,
    nutrition: NutritionDefinition,
    direct_consumption: DirectConsumptionDefinition,
    locomotion: LocomotionDefinition,
    carried_mass_capacity: Mass,
    stress: SurvivalStressDefinition,
}

impl PhysiologyDefinition {
    #[must_use]
    pub fn new(
        metabolism: MetabolismDefinition,
        hydration: HydrationDefinition,
        nutrition: NutritionDefinition,
        direct_consumption: DirectConsumptionDefinition,
        locomotion: LocomotionDefinition,
        carried_mass_capacity: Mass,
        stress: SurvivalStressDefinition,
    ) -> Self {
        assert!(
            !carried_mass_capacity.is_zero(),
            "player carried mass capacity must be nonzero"
        );
        Self {
            metabolism,
            hydration,
            nutrition,
            direct_consumption,
            locomotion,
            carried_mass_capacity,
            stress,
        }
    }

    #[must_use]
    pub const fn maximum_metabolic_energy(self) -> Energy {
        self.metabolism.maximum
    }

    #[must_use]
    pub const fn maximum_hydration(self) -> Volume {
        self.hydration.maximum
    }

    #[must_use]
    pub const fn hungry_below(self) -> Energy {
        self.metabolism.hungry_below
    }

    #[must_use]
    pub const fn thirsty_below(self) -> Volume {
        self.hydration.thirsty_below
    }

    #[must_use]
    pub const fn basal_energy_cost_per_tick(self) -> Energy {
        self.metabolism.basal_cost_per_tick
    }

    #[must_use]
    pub const fn hydration_loss_per_tick(self) -> Volume {
        self.hydration.loss_per_tick
    }

    #[must_use]
    pub const fn nutrition(self) -> NutritionDefinition {
        self.nutrition
    }

    #[must_use]
    pub const fn direct_consumption(self) -> DirectConsumptionDefinition {
        self.direct_consumption
    }

    #[must_use]
    pub const fn locomotion(self) -> LocomotionDefinition {
        self.locomotion
    }

    /// Maximum solid-matter custody the unaided player can carry at once.
    ///
    /// This is a body capability, not a caller-selected inventory size. Logistics admission uses
    /// it to allocate the player's carried stockpile; later transport technology should extend
    /// haulage through distinct physical custody rather than silently enlarging the body limit.
    #[must_use]
    pub const fn carried_mass_capacity(self) -> Mass {
        self.carried_mass_capacity
    }

    #[must_use]
    pub const fn starvation_vitality_loss_ppm_per_tick(self) -> u32 {
        self.stress.starvation_ppm_per_tick()
    }

    #[must_use]
    pub const fn dehydration_vitality_loss_ppm_per_tick(self) -> u32 {
        self.stress.dehydration_ppm_per_tick()
    }

    #[must_use]
    pub const fn thermal_comfort(self) -> ThermalComfortDefinition {
        self.stress.thermal_comfort()
    }
}
