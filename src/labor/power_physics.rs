//! Pure direct-labor power calculations shared by admission and persistence replay.

use crate::core::arithmetic::{checked_mul_div_ceil, scale_u128_fraction_floor};
use crate::core::quantity::{Energy, Volume};
use crate::core::time::TickSpan;
use crate::survival::SurvivalExertion;

const PARTS_PER_MILLION: u32 = 1_000_000;

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

pub(crate) fn metabolic_output_per_tick(energy_cost: Energy, efficiency_ppm: u32) -> Energy {
    Energy::from_nanojoules(scale_u128_fraction_floor(
        energy_cost.nanojoules(),
        efficiency_ppm,
        PARTS_PER_MILLION,
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
        u128::from(PARTS_PER_MILLION),
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

#[cfg(test)]
#[path = "power_physics_tests.rs"]
mod tests;
