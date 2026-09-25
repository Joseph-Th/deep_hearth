//! Read-only metabolic planning for direct meals.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass};
use crate::core::time::TickSpan;
use crate::survival::{
    FoodDefinition, PhysiologyDefinition, SurvivalExertion, SurvivalResourceProjectionError,
    project_survival_resource_budget,
};

/// Smallest represented single-food meal that leaves the player at or above one metabolic target.
///
/// The projection includes the survival energy spent during the eating action itself. It does not
/// prove inventory availability, freshness, temperature, player attention, or current revisions;
/// runtime eating still goes through the canonical eating admission path.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MinimumMealMetabolicProjection {
    mass: Mass,
    duration: TickSpan,
    energy_offered: Energy,
    metabolic_energy_after: Energy,
}

impl MinimumMealMetabolicProjection {
    #[must_use]
    pub const fn mass(self) -> Mass {
        self.mass
    }

    #[must_use]
    pub const fn duration(self) -> TickSpan {
        self.duration
    }

    #[must_use]
    pub const fn energy_offered(self) -> Energy {
        self.energy_offered
    }

    #[must_use]
    pub const fn metabolic_energy_after(self) -> Energy {
        self.metabolic_energy_after
    }
}

/// Failure while projecting a minimum direct meal for a desired metabolic reserve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MealMetabolicProjectionError {
    CurrentEnergyExceedsMaximum { current: Energy, maximum: Energy },
    TargetEnergyExceedsMaximum { target: Energy, maximum: Energy },
    TargetUnreachableWithinIntakeLimit { maximum_meal_mass: Mass },
    ResourceBudgetOverflow,
}

impl Display for MealMetabolicProjectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CurrentEnergyExceedsMaximum { current, maximum } => write!(
                formatter,
                "current metabolic energy {} nJ exceeds the authored maximum of {} nJ",
                current.nanojoules(),
                maximum.nanojoules()
            ),
            Self::TargetEnergyExceedsMaximum { target, maximum } => write!(
                formatter,
                "target metabolic energy {} nJ exceeds the authored maximum of {} nJ",
                target.nanojoules(),
                maximum.nanojoules()
            ),
            Self::TargetUnreachableWithinIntakeLimit { maximum_meal_mass } => write!(
                formatter,
                "metabolic target cannot be reached within the direct-consumption limit of {} mg",
                maximum_meal_mass.milligrams()
            ),
            Self::ResourceBudgetOverflow => write!(
                formatter,
                "meal-time survival resource projection overflowed"
            ),
        }
    }
}

impl Error for MealMetabolicProjectionError {}

/// Projects the smallest represented meal that reaches target after eating-time metabolism.
///
/// `Ok(None)` means the current reserve already satisfies the target. The fixed-point calculation
/// starts from the food-only deficit and increases meal mass whenever the meal's own duration adds
/// survival cost. This prevents tiny-snack loops where a meal replaces only the reserve deficit but
/// not the energy spent while eating it.
pub fn project_minimum_meal_to_metabolic_target(
    physiology: PhysiologyDefinition,
    food: FoodDefinition,
    current: Energy,
    target: Energy,
) -> Result<Option<MinimumMealMetabolicProjection>, MealMetabolicProjectionError> {
    let maximum = physiology.maximum_metabolic_energy();
    if current > maximum {
        return Err(MealMetabolicProjectionError::CurrentEnergyExceedsMaximum { current, maximum });
    }
    if target > maximum {
        return Err(MealMetabolicProjectionError::TargetEnergyExceedsMaximum { target, maximum });
    }
    if current >= target {
        return Ok(None);
    }

    let direct = physiology.direct_consumption();
    let minimum_meal_mass = direct.minimum_meal_mass();
    let maximum_meal_mass = direct.maximum_meal_mass();
    let reserve_gap = target
        .checked_sub(current)
        .unwrap_or_else(|| unreachable!("target above current energy has a positive gap"));
    let mut mass = std::cmp::max(
        food.minimum_mass_for_dietary_energy(reserve_gap).ok_or(
            MealMetabolicProjectionError::TargetUnreachableWithinIntakeLimit { maximum_meal_mass },
        )?,
        minimum_meal_mass,
    );

    loop {
        if mass > maximum_meal_mass {
            return Err(
                MealMetabolicProjectionError::TargetUnreachableWithinIntakeLimit {
                    maximum_meal_mass,
                },
            );
        }
        let duration = direct
            .meal_duration(mass)
            .unwrap_or_else(|| unreachable!("bounded nonzero meal has an authored duration"));
        let meal_cost =
            project_survival_resource_budget(physiology, SurvivalExertion::REST, duration)
                .map_err(|error| match error {
                    SurvivalResourceProjectionError::EnergyOverflow
                    | SurvivalResourceProjectionError::HydrationOverflow => {
                        MealMetabolicProjectionError::ResourceBudgetOverflow
                    }
                })?
                .metabolic_energy();
        let required_offer = reserve_gap.checked_add(meal_cost).ok_or(
            MealMetabolicProjectionError::TargetUnreachableWithinIntakeLimit { maximum_meal_mass },
        )?;
        let next = std::cmp::max(
            food.minimum_mass_for_dietary_energy(required_offer).ok_or(
                MealMetabolicProjectionError::TargetUnreachableWithinIntakeLimit {
                    maximum_meal_mass,
                },
            )?,
            minimum_meal_mass,
        );
        if next > maximum_meal_mass {
            return Err(
                MealMetabolicProjectionError::TargetUnreachableWithinIntakeLimit {
                    maximum_meal_mass,
                },
            );
        }
        if next != mass {
            debug_assert!(next > mass);
            mass = next;
            continue;
        }

        let energy_offered = food.dietary_energy_for_mass(mass);
        let metabolic_energy_after = current
            .checked_add(energy_offered)
            .and_then(|value| value.checked_sub(meal_cost))
            .map(|value| value.min(maximum))
            .unwrap_or_else(|| {
                unreachable!("feasible meal projection has represented final metabolic energy")
            });
        debug_assert!(metabolic_energy_after >= target);
        return Ok(Some(MinimumMealMetabolicProjection {
            mass,
            duration,
            energy_offered,
            metabolic_energy_after,
        }));
    }
}
