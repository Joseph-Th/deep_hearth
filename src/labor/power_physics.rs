//! Pure direct-labor power calculations shared by admission and persistence replay.

use crate::core::quantity::Energy;
use crate::core::time::TickSpan;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ManualPowerMetabolicDurationError {
    ZeroOutput,
    DurationOverflow,
}

pub(crate) fn metabolic_output_per_tick(energy_cost: Energy, efficiency_ppm: u32) -> Energy {
    let energy = energy_cost.nanojoules();
    let scale = 1_000_000_u128;
    let efficiency = u128::from(efficiency_ppm);
    let whole = (energy / scale) * efficiency;
    let fractional = (energy % scale) * efficiency / scale;
    Energy::from_nanojoules(whole + fractional)
}

pub(crate) fn calculate_metabolic_duration(
    required: Energy,
    per_tick: Energy,
) -> Result<TickSpan, ManualPowerMetabolicDurationError> {
    if per_tick.is_zero() {
        return Err(ManualPowerMetabolicDurationError::ZeroOutput);
    }
    let quotient = required.nanojoules() / per_tick.nanojoules();
    let remainder = required.nanojoules() % per_tick.nanojoules();
    let ticks = quotient + u128::from(remainder != 0);
    let ticks =
        u64::try_from(ticks).map_err(|_| ManualPowerMetabolicDurationError::DurationOverflow)?;
    Ok(TickSpan::new(ticks.max(1)))
}
