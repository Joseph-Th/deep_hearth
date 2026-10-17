//! Immutable player physiology and direct-consumption timing definitions.

use crate::core::arithmetic::{NORMALIZED_PARTS_PER_MILLION, checked_mul_div_with_remainder};
use crate::core::quantity::{Acceleration, Energy, Length, Mass, Speed, Volume};
use crate::core::time::{CONTROL_TICKS_PER_SIMULATION_TICK, TickSpan};
use crate::survival::resource_cost::SurvivalExertion;

/// Authored body heights for direct-control stances.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BodyStanceDefinition {
    standing_height: Length,
    crouching_height: Length,
}

/// Authored first-person view geometry derived from the authoritative player body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BodyViewDefinition {
    standing_eye_height: Length,
    crouching_eye_height: Length,
    interaction_reach: Length,
}

impl BodyViewDefinition {
    #[must_use]
    pub fn new(
        standing_eye_height: Length,
        crouching_eye_height: Length,
        interaction_reach: Length,
    ) -> Self {
        assert!(
            !standing_eye_height.is_zero(),
            "player standing eye height must be nonzero"
        );
        assert!(
            !crouching_eye_height.is_zero(),
            "player crouching eye height must be nonzero"
        );
        assert!(
            crouching_eye_height < standing_eye_height,
            "player crouching eye height must be below standing eye height"
        );
        assert!(
            !interaction_reach.is_zero(),
            "player interaction reach must be nonzero"
        );
        assert!(
            interaction_reach.micrometers() <= (i64::MAX as u64) / 1_000,
            "player interaction reach must fit the signed-nanometer world-point range"
        );
        Self {
            standing_eye_height,
            crouching_eye_height,
            interaction_reach,
        }
    }

    #[must_use]
    pub const fn standing_eye_height(self) -> Length {
        self.standing_eye_height
    }

    #[must_use]
    pub const fn crouching_eye_height(self) -> Length {
        self.crouching_eye_height
    }

    #[must_use]
    pub const fn interaction_reach(self) -> Length {
        self.interaction_reach
    }
}

impl BodyStanceDefinition {
    #[must_use]
    pub fn new(standing_height: Length, crouching_height: Length) -> Self {
        assert!(
            !standing_height.is_zero(),
            "player standing height must be nonzero"
        );
        assert!(
            !crouching_height.is_zero(),
            "player crouching height must be nonzero"
        );
        assert!(
            crouching_height < standing_height,
            "player crouching height must be below standing height"
        );
        Self {
            standing_height,
            crouching_height,
        }
    }

    #[must_use]
    pub const fn standing_height(self) -> Length {
        self.standing_height
    }

    #[must_use]
    pub const fn crouching_height(self) -> Length {
        self.crouching_height
    }
}

/// Authored speed envelope for direct-control locomotion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocomotionSpeedDefinition {
    crouching_speed: Speed,
    walking_speed: Speed,
    sprinting_speed: Speed,
    unloaded_jump_speed: Speed,
    fully_loaded_jump_speed: Speed,
    safe_landing_speed: Speed,
    maximum_fall_speed: Speed,
}

impl LocomotionSpeedDefinition {
    #[must_use]
    pub fn new(
        crouching_speed: Speed,
        walking_speed: Speed,
        sprinting_speed: Speed,
        unloaded_jump_speed: Speed,
        fully_loaded_jump_speed: Speed,
        safe_landing_speed: Speed,
        maximum_fall_speed: Speed,
    ) -> Self {
        assert!(
            !crouching_speed.is_zero(),
            "player crouching speed must be nonzero"
        );
        assert!(
            crouching_speed < walking_speed,
            "player crouching speed must be below walking speed"
        );
        assert!(
            walking_speed < sprinting_speed,
            "player sprinting speed must exceed walking speed"
        );
        assert!(
            !unloaded_jump_speed.is_zero(),
            "player jump speed must be nonzero"
        );
        assert!(
            !fully_loaded_jump_speed.is_zero(),
            "loaded player jump speed must be nonzero"
        );
        assert!(
            fully_loaded_jump_speed <= unloaded_jump_speed,
            "carried load cannot increase jump speed"
        );
        assert!(
            !safe_landing_speed.is_zero(),
            "player safe landing speed must be nonzero"
        );
        assert!(
            !maximum_fall_speed.is_zero(),
            "player maximum fall speed must be nonzero"
        );
        assert!(
            safe_landing_speed < maximum_fall_speed,
            "player safe landing speed must remain below maximum fall speed"
        );
        Self {
            crouching_speed,
            walking_speed,
            sprinting_speed,
            unloaded_jump_speed,
            fully_loaded_jump_speed,
            safe_landing_speed,
            maximum_fall_speed,
        }
    }

    #[must_use]
    pub const fn crouching_speed(self) -> Speed {
        self.crouching_speed
    }

    #[must_use]
    pub const fn walking_speed(self) -> Speed {
        self.walking_speed
    }

    #[must_use]
    pub const fn sprinting_speed(self) -> Speed {
        self.sprinting_speed
    }

    #[must_use]
    pub const fn unloaded_jump_speed(self) -> Speed {
        self.unloaded_jump_speed
    }

    #[must_use]
    pub const fn fully_loaded_jump_speed(self) -> Speed {
        self.fully_loaded_jump_speed
    }

    #[must_use]
    pub const fn safe_landing_speed(self) -> Speed {
        self.safe_landing_speed
    }

    #[must_use]
    pub const fn maximum_fall_speed(self) -> Speed {
        self.maximum_fall_speed
    }
}

/// Authored continuous-body facts used by direct control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BodyKinematicsDefinition {
    body_width: Length,
    stances: BodyStanceDefinition,
    view: BodyViewDefinition,
    speeds: LocomotionSpeedDefinition,
    horizontal_acceleration: Acceleration,
}

impl BodyKinematicsDefinition {
    #[must_use]
    pub fn new(
        body_width: Length,
        stances: BodyStanceDefinition,
        view: BodyViewDefinition,
        speeds: LocomotionSpeedDefinition,
        horizontal_acceleration: Acceleration,
    ) -> Self {
        assert!(!body_width.is_zero(), "player body width must be nonzero");
        assert!(
            view.standing_eye_height() < stances.standing_height(),
            "player standing eye height must be below standing body height"
        );
        assert!(
            view.crouching_eye_height() < stances.crouching_height(),
            "player crouching eye height must be below crouching body height"
        );
        assert!(
            !horizontal_acceleration.is_zero(),
            "player horizontal acceleration must be nonzero"
        );
        Self {
            body_width,
            stances,
            view,
            speeds,
            horizontal_acceleration,
        }
    }

    #[must_use]
    pub const fn body_width(self) -> Length {
        self.body_width
    }

    #[must_use]
    pub const fn standing_height(self) -> Length {
        self.stances.standing_height()
    }

    #[must_use]
    pub const fn crouching_height(self) -> Length {
        self.stances.crouching_height()
    }

    #[must_use]
    pub const fn standing_eye_height(self) -> Length {
        self.view.standing_eye_height()
    }

    #[must_use]
    pub const fn crouching_eye_height(self) -> Length {
        self.view.crouching_eye_height()
    }

    #[must_use]
    pub const fn interaction_reach(self) -> Length {
        self.view.interaction_reach()
    }

    #[must_use]
    pub const fn crouching_speed(self) -> Speed {
        self.speeds.crouching_speed()
    }

    #[must_use]
    pub const fn walking_speed(self) -> Speed {
        self.speeds.walking_speed()
    }

    #[must_use]
    pub const fn sprinting_speed(self) -> Speed {
        self.speeds.sprinting_speed()
    }

    #[must_use]
    pub const fn unloaded_jump_speed(self) -> Speed {
        self.speeds.unloaded_jump_speed()
    }

    #[must_use]
    pub const fn fully_loaded_jump_speed(self) -> Speed {
        self.speeds.fully_loaded_jump_speed()
    }

    #[must_use]
    pub const fn safe_landing_speed(self) -> Speed {
        self.speeds.safe_landing_speed()
    }

    #[must_use]
    pub const fn horizontal_acceleration(self) -> Acceleration {
        self.horizontal_acceleration
    }

    #[must_use]
    pub const fn maximum_fall_speed(self) -> Speed {
        self.speeds.maximum_fall_speed()
    }
}

/// Authored unaided locomotion/body facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocomotionDefinition {
    unloaded_exertion: SurvivalExertion,
    fully_loaded_exertion: SurvivalExertion,
    body_kinematics: BodyKinematicsDefinition,
}

impl LocomotionDefinition {
    #[must_use]
    pub fn new(
        unloaded_exertion: SurvivalExertion,
        fully_loaded_exertion: SurvivalExertion,
        body_kinematics: BodyKinematicsDefinition,
    ) -> Self {
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
        let definition = Self {
            unloaded_exertion,
            fully_loaded_exertion,
            body_kinematics,
        };
        assert!(
            definition.pending_exertion_envelope().is_some(),
            "locomotion exertion must fit the persisted control-interval accumulation envelope"
        );
        definition
    }

    #[must_use]
    pub const fn body_width(self) -> Length {
        self.body_kinematics.body_width()
    }

    #[must_use]
    pub const fn standing_height(self) -> Length {
        self.body_kinematics.standing_height()
    }

    #[must_use]
    pub const fn crouching_height(self) -> Length {
        self.body_kinematics.crouching_height()
    }

    #[must_use]
    pub const fn standing_eye_height(self) -> Length {
        self.body_kinematics.standing_eye_height()
    }

    #[must_use]
    pub const fn crouching_eye_height(self) -> Length {
        self.body_kinematics.crouching_eye_height()
    }

    #[must_use]
    pub const fn interaction_reach(self) -> Length {
        self.body_kinematics.interaction_reach()
    }

    #[must_use]
    pub const fn crouching_speed(self) -> Speed {
        self.body_kinematics.crouching_speed()
    }

    #[must_use]
    pub const fn walking_speed(self) -> Speed {
        self.body_kinematics.walking_speed()
    }

    #[must_use]
    pub const fn sprinting_speed(self) -> Speed {
        self.body_kinematics.sprinting_speed()
    }

    #[must_use]
    pub const fn horizontal_acceleration(self) -> Acceleration {
        self.body_kinematics.horizontal_acceleration()
    }

    #[must_use]
    pub const fn maximum_fall_speed(self) -> Speed {
        self.body_kinematics.maximum_fall_speed()
    }

    #[must_use]
    pub const fn safe_landing_speed(self) -> Speed {
        self.body_kinematics.safe_landing_speed()
    }

    /// Returns deterministic vitality loss for one downward landing impact.
    ///
    /// Impacts at or below the authored safe landing speed cause no injury. Above that threshold,
    /// loss scales linearly to a lethal normalized loss at the authored terminal fall speed.
    #[must_use]
    pub fn fall_impact_vitality_loss_ppm(self, impact_speed: Speed) -> u32 {
        let safe = self.safe_landing_speed().nanometers_per_second();
        let maximum = self.maximum_fall_speed().nanometers_per_second();
        let impact = impact_speed.nanometers_per_second().min(maximum);
        if impact <= safe {
            return 0;
        }
        let excess = u128::from(impact - safe);
        let span = u128::from(maximum - safe);
        let loss = excess
            .checked_mul(u128::from(
                crate::core::arithmetic::NORMALIZED_PARTS_PER_MILLION,
            ))
            .and_then(|value| value.checked_div(span))
            .unwrap_or_else(|| unreachable!("bounded fall-impact injury arithmetic fits u128"));
        u32::try_from(loss).unwrap_or_else(|_| unreachable!("normalized fall injury fits u32"))
    }

    #[must_use]
    pub fn sprinting_speed_for_load(self, carried: Mass, capacity: Mass) -> Speed {
        assert!(
            !capacity.is_zero(),
            "locomotion carrying capacity must be nonzero"
        );
        assert!(
            carried <= capacity,
            "locomotion carried mass cannot exceed body capacity"
        );
        let walking = u128::from(self.walking_speed().nanometers_per_second());
        let sprinting = u128::from(self.sprinting_speed().nanometers_per_second());
        let bonus = sprinting - walking;
        let remaining = u128::from(capacity.milligrams() - carried.milligrams());
        let denominator = u128::from(capacity.milligrams());
        let scaled_bonus =
            bonus / denominator * remaining + (bonus % denominator) * remaining / denominator;
        Speed::from_nanometers_per_second(
            u64::try_from(walking + scaled_bonus)
                .unwrap_or_else(|_| unreachable!("authored sprint speed fits u64")),
        )
    }

    #[must_use]
    pub fn jump_speed_for_load(self, carried: Mass, capacity: Mass) -> Speed {
        assert!(
            !capacity.is_zero(),
            "locomotion carrying capacity must be nonzero"
        );
        assert!(
            carried <= capacity,
            "locomotion carried mass cannot exceed body capacity"
        );
        let unloaded = u128::from(
            self.body_kinematics
                .unloaded_jump_speed()
                .nanometers_per_second(),
        );
        let loaded = u128::from(
            self.body_kinematics
                .fully_loaded_jump_speed()
                .nanometers_per_second(),
        );
        let loss = unloaded - loaded;
        let numerator = u128::from(carried.milligrams());
        let denominator = u128::from(capacity.milligrams());
        let scaled_loss =
            loss / denominator * numerator + (loss % denominator) * numerator / denominator;
        Speed::from_nanometers_per_second(
            u64::try_from(unloaded - scaled_loss)
                .unwrap_or_else(|_| unreachable!("authored jump speed fits u64")),
        )
    }

    #[must_use]
    pub fn maximum_exertion(self) -> SurvivalExertion {
        scale_exertion(
            self.fully_loaded_exertion,
            u128::from(self.sprinting_speed().nanometers_per_second()),
            u128::from(self.walking_speed().nanometers_per_second()),
        )
        .unwrap_or_else(|| unreachable!("authored maximum locomotion exertion must fit"))
    }

    /// Maximum pending locomotion exertion that canonical control-rate execution can accumulate
    /// before a slow-simulation settlement.
    ///
    /// Continuous motion can contribute at most one fully loaded sprint-rate interval. Discrete
    /// jump launch is a whole-event impulse, so persistence must additionally admit one fully
    /// loaded jump impulse for every control sample in the interval. The latter deliberately
    /// over-approximates physically reachable jump cadence; it keeps trusted-load validation
    /// independent of jump-arc timing while still rejecting arbitrary accumulator magnitudes.
    pub(crate) fn maximum_pending_exertion(self) -> SurvivalExertion {
        self.pending_exertion_envelope().unwrap_or_else(|| {
            unreachable!("locomotion construction validates the pending exertion envelope")
        })
    }

    fn pending_exertion_envelope(self) -> Option<SurvivalExertion> {
        let continuous = scale_exertion(
            self.fully_loaded_exertion,
            u128::from(self.sprinting_speed().nanometers_per_second()),
            u128::from(self.walking_speed().nanometers_per_second()),
        )?;
        let impulse_count = u128::from(CONTROL_TICKS_PER_SIMULATION_TICK);
        let energy = self
            .fully_loaded_exertion
            .energy_cost_per_tick()
            .nanojoules()
            .checked_mul(impulse_count)?
            .checked_add(continuous.energy_cost_per_tick().nanojoules())?;
        let hydration = u128::from(
            self.fully_loaded_exertion
                .hydration_loss_per_tick()
                .microliters(),
        )
        .checked_mul(impulse_count)?
        .checked_add(u128::from(
            continuous.hydration_loss_per_tick().microliters(),
        ))?;
        Some(SurvivalExertion::new(
            Energy::from_nanojoules(energy),
            Volume::from_microliters(u64::try_from(hydration).ok()?),
        ))
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

    /// Returns the one-shot body effort charged when a grounded jump is launched.
    ///
    /// Jumping is an impulse rather than a continuous locomotion rate. The current body model
    /// deliberately calibrates one launch to one load-scaled walking quantum so the cost remains
    /// legible beside ordinary travel without inventing an unmodeled player body mass. Callers
    /// must settle this as a discrete event rather than dividing it across control ticks.
    #[must_use]
    pub fn jump_exertion_for_load(self, carried: Mass, capacity: Mass) -> SurvivalExertion {
        self.exertion_for_load(carried, capacity)
    }

    pub fn exertion_for_horizontal_speed(
        self,
        carried: Mass,
        capacity: Mass,
        speed_nanometers_per_second: u128,
    ) -> Option<SurvivalExertion> {
        if speed_nanometers_per_second > u128::from(self.sprinting_speed().nanometers_per_second())
        {
            return None;
        }
        scale_exertion(
            self.exertion_for_load(carried, capacity),
            speed_nanometers_per_second,
            u128::from(self.walking_speed().nanometers_per_second()),
        )
    }
}

fn scale_exertion(
    exertion: SurvivalExertion,
    numerator: u128,
    denominator: u128,
) -> Option<SurvivalExertion> {
    if denominator == 0 {
        return None;
    }
    let scale = |value: u128| {
        let quotient = value / denominator;
        let remainder = value % denominator;
        quotient
            .checked_mul(numerator)?
            .checked_add(remainder.checked_mul(numerator)? / denominator)
    };
    Some(SurvivalExertion::new(
        Energy::from_nanojoules(scale(exertion.energy_cost_per_tick().nanojoules())?),
        Volume::from_microliters(
            u64::try_from(scale(u128::from(
                exertion.hydration_loss_per_tick().microliters(),
            ))?)
            .ok()?,
        ),
    ))
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
