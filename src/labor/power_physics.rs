//! Pure direct-labor power calculations shared by admission and persistence replay.

use crate::core::quantity::{Energy, Volume};
use crate::core::time::TickSpan;
use crate::survival::{PhysiologyDefinition, SurvivalExertion};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ManualPowerResourceBudgetError {
    EnergyOverflow,
    HydrationOverflow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ManualPowerResourceBudget {
    metabolic_energy: Energy,
    hydration: Volume,
}

impl ManualPowerResourceBudget {
    #[must_use]
    pub(crate) const fn metabolic_energy(self) -> Energy {
        self.metabolic_energy
    }

    #[must_use]
    pub(crate) const fn hydration(self) -> Volume {
        self.hydration
    }
}

pub(crate) fn calculate_manual_power_resource_budget(
    physiology: PhysiologyDefinition,
    exertion: SurvivalExertion,
    duration: TickSpan,
) -> Result<ManualPowerResourceBudget, ManualPowerResourceBudgetError> {
    let energy_per_tick = physiology
        .basal_energy_cost_per_tick()
        .checked_add(exertion.energy_cost_per_tick())
        .ok_or(ManualPowerResourceBudgetError::EnergyOverflow)?;
    let metabolic_energy = energy_per_tick
        .nanojoules()
        .checked_mul(u128::from(duration.value()))
        .map(Energy::from_nanojoules)
        .ok_or(ManualPowerResourceBudgetError::EnergyOverflow)?;
    let hydration_per_tick = physiology
        .hydration_loss_per_tick()
        .checked_add(exertion.hydration_loss_per_tick())
        .ok_or(ManualPowerResourceBudgetError::HydrationOverflow)?;
    let hydration = hydration_per_tick
        .microliters()
        .checked_mul(duration.value())
        .map(Volume::from_microliters)
        .ok_or(ManualPowerResourceBudgetError::HydrationOverflow)?;
    Ok(ManualPowerResourceBudget {
        metabolic_energy,
        hydration,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::survival::{HydrationDefinition, MetabolismDefinition, NutritionDefinition};

    #[test]
    fn resource_budget_includes_basal_and_incremental_work_costs() {
        let physiology = PhysiologyDefinition::new(
            MetabolismDefinition::new(
                Energy::from_nanojoules(1_000),
                Energy::from_nanojoules(100),
                Energy::from_nanojoules(10),
            ),
            HydrationDefinition::new(
                Volume::from_microliters(1_000),
                Volume::from_microliters(100),
                Volume::from_microliters(2),
            ),
            NutritionDefinition::new(1, 1),
            1,
            1,
        );
        let exertion =
            SurvivalExertion::new(Energy::from_nanojoules(30), Volume::from_microliters(3));

        let budget = calculate_manual_power_resource_budget(physiology, exertion, TickSpan::new(4))
            .unwrap_or_else(|error| panic!("manual power budget fixture failed: {error:?}"));

        assert_eq!(budget.metabolic_energy(), Energy::from_nanojoules(160));
        assert_eq!(budget.hydration(), Volume::from_microliters(20));
    }
}
