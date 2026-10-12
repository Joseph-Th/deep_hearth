//! Immutable player physiology and direct-consumption timing definitions.

use crate::core::arithmetic::NORMALIZED_PARTS_PER_MILLION;
use crate::core::quantity::{Energy, Mass, Volume};
use crate::core::time::TickSpan;
use crate::survival::resource_cost::SurvivalExertion;

/// Authored unaided locomotion cost for one open-space voxel step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocomotionDefinition {
    step_duration: TickSpan,
    unloaded_exertion: SurvivalExertion,
    fully_loaded_exertion: SurvivalExertion,
}

impl LocomotionDefinition {
    #[must_use]
    pub fn new(
        step_duration: TickSpan,
        unloaded_exertion: SurvivalExertion,
        fully_loaded_exertion: SurvivalExertion,
    ) -> Self {
        assert!(
            !step_duration.is_zero(),
            "player step duration must be nonzero"
        );
        unloaded_exertion.assert_active_player_work();
        fully_loaded_exertion.assert_active_player_work();
        assert!(
            fully_loaded_exertion.energy_cost_per_tick()
                >= unloaded_exertion.energy_cost_per_tick(),
            "fully loaded locomotion cannot cost less metabolic energy than unloaded locomotion"
        );
        assert!(
            fully_loaded_exertion.hydration_loss_per_tick()
                >= unloaded_exertion.hydration_loss_per_tick(),
            "fully loaded locomotion cannot lose less hydration than unloaded locomotion"
        );
        Self {
            step_duration,
            unloaded_exertion,
            fully_loaded_exertion,
        }
    }

    #[must_use]
    pub const fn step_duration(self) -> TickSpan {
        self.step_duration
    }

    #[must_use]
    pub fn exertion_for_load(self, carried: Mass, capacity: Mass) -> SurvivalExertion {
        assert!(
            !capacity.is_zero(),
            "locomotion carrying capacity must be nonzero"
        );
        assert!(
            carried <= capacity,
            "locomotion carried mass cannot exceed body capacity"
        );
        let numerator = u128::from(carried.milligrams());
        let denominator = u128::from(capacity.milligrams());
        let scale = |low: u128, high: u128| {
            let delta = high.checked_sub(low).unwrap_or_else(|| {
                unreachable!("locomotion upper cost is validated above lower cost")
            });
            low + delta / denominator * numerator + (delta % denominator) * numerator / denominator
        };
        let energy = scale(
            self.unloaded_exertion.energy_cost_per_tick().nanojoules(),
            self.fully_loaded_exertion
                .energy_cost_per_tick()
                .nanojoules(),
        );
        let hydration = scale(
            u128::from(
                self.unloaded_exertion
                    .hydration_loss_per_tick()
                    .microliters(),
            ),
            u128::from(
                self.fully_loaded_exertion
                    .hydration_loss_per_tick()
                    .microliters(),
            ),
        );
        SurvivalExertion::new(
            Energy::from_nanojoules(energy),
            Volume::from_microliters(
                u64::try_from(hydration)
                    .unwrap_or_else(|_| unreachable!("interpolated locomotion hydration fits u64")),
            ),
        )
    }
}

/// Authored rate at which recent dietary balance fades and can support vitality recovery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NutritionDefinition {
    decay_ppm_per_tick: u32,
    vitality_recovery_ppm_per_tick: u32,
}

impl NutritionDefinition {
    #[must_use]
    pub fn new(decay_ppm_per_tick: u32, vitality_recovery_ppm_per_tick: u32) -> Self {
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
        Self {
            decay_ppm_per_tick,
            vitality_recovery_ppm_per_tick,
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

/// Immutable physiology parameters for the player survival owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysiologyDefinition {
    metabolism: MetabolismDefinition,
    hydration: HydrationDefinition,
    nutrition: NutritionDefinition,
    direct_consumption: DirectConsumptionDefinition,
    locomotion: LocomotionDefinition,
    carried_mass_capacity: Mass,
    starvation_vitality_loss_ppm_per_tick: u32,
    dehydration_vitality_loss_ppm_per_tick: u32,
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
        starvation_vitality_loss_ppm_per_tick: u32,
        dehydration_vitality_loss_ppm_per_tick: u32,
    ) -> Self {
        assert!(
            !carried_mass_capacity.is_zero(),
            "player carried mass capacity must be nonzero"
        );
        assert!(
            starvation_vitality_loss_ppm_per_tick > 0,
            "starvation vitality loss must be nonzero"
        );
        assert!(
            starvation_vitality_loss_ppm_per_tick <= NORMALIZED_PARTS_PER_MILLION,
            "starvation vitality loss cannot exceed normalized vitality"
        );
        assert!(
            dehydration_vitality_loss_ppm_per_tick > 0,
            "dehydration vitality loss must be nonzero"
        );
        assert!(
            dehydration_vitality_loss_ppm_per_tick <= NORMALIZED_PARTS_PER_MILLION,
            "dehydration vitality loss cannot exceed normalized vitality"
        );
        Self {
            metabolism,
            hydration,
            nutrition,
            direct_consumption,
            locomotion,
            carried_mass_capacity,
            starvation_vitality_loss_ppm_per_tick,
            dehydration_vitality_loss_ppm_per_tick,
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
        self.starvation_vitality_loss_ppm_per_tick
    }

    #[must_use]
    pub const fn dehydration_vitality_loss_ppm_per_tick(self) -> u32 {
        self.dehydration_vitality_loss_ppm_per_tick
    }
}
