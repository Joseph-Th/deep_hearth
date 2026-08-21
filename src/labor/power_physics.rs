//! Pure direct-labor power calculations shared by admission and persistence replay.

use crate::core::quantity::{Energy, Volume};
use crate::core::time::TickSpan;
use crate::survival::SurvivalExertion;

const PARTS_PER_MILLION: u128 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ManualPowerMetabolicDurationError {
    ZeroOutput,
    DurationOverflow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ManualPowerExertionError {
    EnergyOverflow,
    HydrationOverflow,
    ExceedsAuthoredMaximum,
}

pub(crate) fn metabolic_output_per_tick(energy_cost: Energy, efficiency_ppm: u32) -> Energy {
    let energy = energy_cost.nanojoules();
    let efficiency = u128::from(efficiency_ppm);
    let whole = (energy / PARTS_PER_MILLION) * efficiency;
    let fractional = (energy % PARTS_PER_MILLION) * efficiency / PARTS_PER_MILLION;
    Energy::from_nanojoules(whole + fractional)
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

    let scaled_output = required_output
        .nanojoules()
        .checked_mul(PARTS_PER_MILLION)
        .ok_or(ManualPowerExertionError::EnergyOverflow)?;
    let total_metabolic = scaled_output.div_ceil(u128::from(efficiency_ppm));
    let metabolic_per_tick = total_metabolic.div_ceil(ticks);
    if metabolic_per_tick > maximum.energy_cost_per_tick().nanojoules() {
        return Err(ManualPowerExertionError::ExceedsAuthoredMaximum);
    }

    let scaled_hydration = metabolic_per_tick
        .checked_mul(u128::from(maximum.hydration_loss_per_tick().microliters()))
        .ok_or(ManualPowerExertionError::HydrationOverflow)?;
    let hydration_per_tick = scaled_hydration.div_ceil(maximum.energy_cost_per_tick().nanojoules());
    let hydration_per_tick = u64::try_from(hydration_per_tick)
        .map_err(|_| ManualPowerExertionError::HydrationOverflow)?;

    Ok(SurvivalExertion::new(
        Energy::from_nanojoules(metabolic_per_tick),
        Volume::from_microliters(hydration_per_tick),
    ))
}

#[cfg(all(
    test,
    any(not(feature = "test-unit-sharded"), feature = "test-unit-player")
))]
mod tests {
    use super::*;

    #[test]
    fn zero_required_output_has_zero_metabolic_duration() {
        assert_eq!(
            calculate_metabolic_duration(Energy::ZERO, Energy::from_nanojoules(1)),
            Ok(TickSpan::ZERO)
        );
    }

    #[test]
    fn bottlenecked_manual_power_scales_effort_to_actual_output() {
        let maximum = SurvivalExertion::new(
            Energy::from_nanojoules(1_500_000_000_000),
            Volume::from_microliters(350),
        );
        let output = Energy::from_nanojoules(25_000_000_000);

        let slow = resolve_manual_power_exertion(output, TickSpan::new(10), maximum, 200_000)
            .unwrap_or_else(|error| panic!("slow manual-power effort failed: {error:?}"));
        let fast = resolve_manual_power_exertion(output, TickSpan::new(5), maximum, 200_000)
            .unwrap_or_else(|error| panic!("fast manual-power effort failed: {error:?}"));

        assert_eq!(
            slow.energy_cost_per_tick(),
            Energy::from_nanojoules(12_500_000_000)
        );
        assert_eq!(
            fast.energy_cost_per_tick(),
            Energy::from_nanojoules(25_000_000_000)
        );
        assert_eq!(slow.hydration_loss_per_tick(), Volume::from_microliters(3));
        assert_eq!(fast.hydration_loss_per_tick(), Volume::from_microliters(6));
        assert_eq!(
            slow.energy_cost_per_tick().nanojoules() * 10,
            fast.energy_cost_per_tick().nanojoules() * 5
        );
    }
}
