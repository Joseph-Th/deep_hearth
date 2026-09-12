//! Pure direct-labor power calculations shared by admission and persistence replay.

use crate::core::arithmetic::{
    NORMALIZED_PARTS_PER_MILLION, checked_mul_div_ceil, scale_u128_fraction_floor,
};
use crate::core::quantity::{Energy, Power, Volume};
use crate::core::time::{PhysicalTickDuration, TickSpan};
use crate::energy::{PowerDurationError, calculate_power_duration_ceiling};
use crate::survival::SurvivalExertion;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ManualPowerMetabolicDurationError {
    ZeroOutput,
    DurationOverflow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ManualPowerExertionError {
    EnergyOverflow,
    ExceedsAuthoredMaximum,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ManualPowerScheduleError {
    PowerDuration(PowerDurationError),
    MetabolicDuration(ManualPowerMetabolicDurationError),
    Exertion(ManualPowerExertionError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ManualPowerSchedule {
    duration: TickSpan,
    exertion: SurvivalExertion,
}

impl ManualPowerSchedule {
    #[must_use]
    pub(crate) const fn duration(self) -> TickSpan {
        self.duration
    }

    #[must_use]
    pub(crate) const fn exertion(self) -> SurvivalExertion {
        self.exertion
    }
}

pub(crate) fn metabolic_output_per_tick(energy_cost: Energy, efficiency_ppm: u32) -> Energy {
    Energy::from_nanojoules(scale_u128_fraction_floor(
        energy_cost.nanojoules(),
        efficiency_ppm,
        NORMALIZED_PARTS_PER_MILLION,
    ))
}

pub(crate) fn calculate_metabolic_duration(
    required: Energy,
    per_tick: Energy,
) -> Result<TickSpan, ManualPowerMetabolicDurationError> {
    if per_tick.is_zero() {
        return Err(ManualPowerMetabolicDurationError::ZeroOutput);
    }
    let ticks = required.nanojoules().div_ceil(per_tick.nanojoules());
    let ticks =
        u64::try_from(ticks).map_err(|_| ManualPowerMetabolicDurationError::DurationOverflow)?;
    Ok(TickSpan::new(ticks))
}

/// Resolves actual per-tick physiological effort for a manual-power work order.
///
/// `maximum` is the authored sustainable effort ceiling, not a flat charge. Mechanical output and
/// metabolic efficiency determine the incremental metabolic work; hydration scales with the same
/// effort fraction. Equipment or sink bottlenecks therefore cannot charge maximum exertion while
/// producing only a small fraction of the corresponding mechanical work.
pub(crate) fn resolve_manual_power_exertion(
    required_output: Energy,
    duration: TickSpan,
    maximum: SurvivalExertion,
    efficiency_ppm: u32,
) -> Result<SurvivalExertion, ManualPowerExertionError> {
    let ticks = u128::from(duration.value());
    if ticks == 0 || efficiency_ppm == 0 || maximum.energy_cost_per_tick().is_zero() {
        return Err(ManualPowerExertionError::ExceedsAuthoredMaximum);
    }

    let total_metabolic = checked_mul_div_ceil(
        required_output.nanojoules(),
        u128::from(NORMALIZED_PARTS_PER_MILLION),
        u128::from(efficiency_ppm),
    )
    .ok_or(ManualPowerExertionError::EnergyOverflow)?;
    let metabolic_per_tick = total_metabolic.div_ceil(ticks);
    if metabolic_per_tick > maximum.energy_cost_per_tick().nanojoules() {
        return Err(ManualPowerExertionError::ExceedsAuthoredMaximum);
    }

    let hydration_per_tick = checked_mul_div_ceil(
        metabolic_per_tick,
        u128::from(maximum.hydration_loss_per_tick().microliters()),
        maximum.energy_cost_per_tick().nanojoules(),
    )
    .unwrap_or_else(|| {
        panic!("bounded manual-power hydration scaling exceeded its authored maximum")
    });
    let hydration_per_tick = u64::try_from(hydration_per_tick).unwrap_or_else(|_| {
        panic!("bounded manual-power hydration result exceeded the volume backing range")
    });

    Ok(SurvivalExertion::new(
        Energy::from_nanojoules(metabolic_per_tick),
        Volume::from_microliters(hydration_per_tick),
    ))
}

/// Resolves the one authoritative active duration and physiological effort for manual generation.
///
/// Mechanical transfer and sustainable metabolic conversion independently constrain throughput.
/// The slower constraint owns elapsed work time; effort is then scaled to the exact requested
/// mechanical output across that resolved interval.
pub(crate) fn resolve_manual_power_schedule(
    required_output: Energy,
    transfer_power: Power,
    physical_tick_duration: PhysicalTickDuration,
    maximum_exertion: SurvivalExertion,
    efficiency_ppm: u32,
) -> Result<ManualPowerSchedule, ManualPowerScheduleError> {
    let power_duration =
        calculate_power_duration_ceiling(transfer_power, required_output, physical_tick_duration)
            .map_err(ManualPowerScheduleError::PowerDuration)?;
    let metabolic_output =
        metabolic_output_per_tick(maximum_exertion.energy_cost_per_tick(), efficiency_ppm);
    let metabolic_duration = calculate_metabolic_duration(required_output, metabolic_output)
        .map_err(ManualPowerScheduleError::MetabolicDuration)?;
    let duration = std::cmp::max(power_duration, metabolic_duration);
    let exertion =
        resolve_manual_power_exertion(required_output, duration, maximum_exertion, efficiency_ppm)
            .map_err(ManualPowerScheduleError::Exertion)?;
    Ok(ManualPowerSchedule { duration, exertion })
}

#[cfg(test)]
#[path = "power_physics_tests.rs"]
mod tests;
