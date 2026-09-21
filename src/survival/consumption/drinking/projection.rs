//! Read-only hydration planning for direct drinking.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Volume;
use crate::core::time::TickSpan;
use crate::survival::{DrinkDefinition, PhysiologyDefinition};

/// Minimum represented drink that leaves the player at or above one hydration target.
///
/// The projection includes basal hydration loss during the drinking action itself. It deliberately
/// does not prove source availability, temperature, player attention, or current-state revisions;
/// runtime drinking still goes through the canonical drinking admission path.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MinimumDrinkHydrationProjection {
    volume: Volume,
    duration: TickSpan,
    hydration_offered: Volume,
    hydration_after: Volume,
}

impl MinimumDrinkHydrationProjection {
    #[must_use]
    pub const fn volume(self) -> Volume {
        self.volume
    }

    #[must_use]
    pub const fn duration(self) -> TickSpan {
        self.duration
    }

    #[must_use]
    pub const fn hydration_offered(self) -> Volume {
        self.hydration_offered
    }

    #[must_use]
    pub const fn hydration_after(self) -> Volume {
        self.hydration_after
    }
}

/// Failure while projecting a minimum direct drink for a desired hydration reserve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrinkHydrationProjectionError {
    CurrentHydrationExceedsMaximum { current: Volume, maximum: Volume },
    TargetHydrationExceedsMaximum { target: Volume, maximum: Volume },
    TargetUnreachableWithinIntakeLimit { maximum_drink_volume: Volume },
}

impl Display for DrinkHydrationProjectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CurrentHydrationExceedsMaximum { current, maximum } => write!(
                formatter,
                "current hydration {} uL exceeds the authored maximum of {} uL",
                current.microliters(),
                maximum.microliters()
            ),
            Self::TargetHydrationExceedsMaximum { target, maximum } => write!(
                formatter,
                "target hydration {} uL exceeds the authored maximum of {} uL",
                target.microliters(),
                maximum.microliters()
            ),
            Self::TargetUnreachableWithinIntakeLimit {
                maximum_drink_volume,
            } => write!(
                formatter,
                "hydration target cannot be reached within the direct-consumption limit of {} uL",
                maximum_drink_volume.microliters()
            ),
        }
    }
}

impl Error for DrinkHydrationProjectionError {}

/// Projects the smallest represented drink that reaches target after drinking-time hydration loss.
///
/// Ok(None) means the current reserve already satisfies the target. The fixed-point calculation
/// starts from the drink-only requirement and increases the candidate only when its own authored
/// duration introduces additional basal hydration loss. If a candidate crosses a duration boundary,
/// the next iteration prices that longer duration. A candidate that remains inside its priced
/// duration bucket is therefore the minimum feasible represented volume.
pub fn project_minimum_drink_to_hydration_target(
    physiology: PhysiologyDefinition,
    drink: DrinkDefinition,
    current: Volume,
    target: Volume,
) -> Result<Option<MinimumDrinkHydrationProjection>, DrinkHydrationProjectionError> {
    let maximum_hydration = physiology.maximum_hydration();
    if current > maximum_hydration {
        return Err(
            DrinkHydrationProjectionError::CurrentHydrationExceedsMaximum {
                current,
                maximum: maximum_hydration,
            },
        );
    }
    if target > maximum_hydration {
        return Err(
            DrinkHydrationProjectionError::TargetHydrationExceedsMaximum {
                target,
                maximum: maximum_hydration,
            },
        );
    }
    if current >= target {
        return Ok(None);
    }

    let direct = physiology.direct_consumption();
    let maximum_drink_volume = direct.maximum_drink_volume();
    let reserve_gap = target
        .checked_sub(current)
        .unwrap_or_else(|| unreachable!("target above current hydration has a positive gap"));
    let mut volume = drink.minimum_volume_for_hydration(reserve_gap).ok_or(
        DrinkHydrationProjectionError::TargetUnreachableWithinIntakeLimit {
            maximum_drink_volume,
        },
    )?;

    loop {
        if volume.is_zero() || volume > maximum_drink_volume {
            return Err(
                DrinkHydrationProjectionError::TargetUnreachableWithinIntakeLimit {
                    maximum_drink_volume,
                },
            );
        }
        let duration = direct
            .drink_duration(volume)
            .unwrap_or_else(|| unreachable!("bounded nonzero drink has an authored duration"));
        let drinking_loss = u128::from(physiology.hydration_loss_per_tick().microliters())
            .checked_mul(u128::from(duration.value()))
            .unwrap_or_else(|| unreachable!("u64 hydration loss and duration product fits u128"));
        let required_offer = u128::from(reserve_gap.microliters())
            .checked_add(drinking_loss)
            .ok_or(
                DrinkHydrationProjectionError::TargetUnreachableWithinIntakeLimit {
                    maximum_drink_volume,
                },
            )?;
        let required_offer = u64::try_from(required_offer)
            .ok()
            .map(Volume::from_microliters)
            .ok_or(
                DrinkHydrationProjectionError::TargetUnreachableWithinIntakeLimit {
                    maximum_drink_volume,
                },
            )?;
        let next = drink.minimum_volume_for_hydration(required_offer).ok_or(
            DrinkHydrationProjectionError::TargetUnreachableWithinIntakeLimit {
                maximum_drink_volume,
            },
        )?;
        if next > maximum_drink_volume {
            return Err(
                DrinkHydrationProjectionError::TargetUnreachableWithinIntakeLimit {
                    maximum_drink_volume,
                },
            );
        }
        if next != volume {
            debug_assert!(next > volume);
            volume = next;
            continue;
        }

        let hydration_offered = drink.hydration_offer(volume);
        let hydration_after = u128::from(current.microliters())
            .checked_add(u128::from(hydration_offered.microliters()))
            .and_then(|hydration| hydration.checked_sub(drinking_loss))
            .map(|hydration| hydration.min(u128::from(maximum_hydration.microliters())))
            .and_then(|hydration| u64::try_from(hydration).ok())
            .map(Volume::from_microliters)
            .unwrap_or_else(|| {
                unreachable!("feasible drink projection has represented final hydration")
            });
        debug_assert!(hydration_after >= target);
        return Ok(Some(MinimumDrinkHydrationProjection {
            volume,
            duration,
            hydration_offered,
            hydration_after,
        }));
    }
}
