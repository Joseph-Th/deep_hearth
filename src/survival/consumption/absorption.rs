//! Exact per-tick physiological uptake for in-progress direct consumption.

use crate::core::arithmetic::checked_mul_div_with_remainder;
use crate::core::quantity::{AggregateVolume, Energy};
use crate::core::time::SimulationTick;
use crate::registry::Registries;

use super::super::{FoodCategory, PendingDirectConsumption};
use super::drinking::pending_drink_hydration_offer;
use super::eating::{NutritionGain, trace_absorption_offer};

/// Physiological contribution released by one authoritative tick of a meal or drink.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DirectConsumptionInstallment {
    energy: Energy,
    hydration: AggregateVolume,
    nutrition: NutritionGain,
    completes: bool,
}

impl DirectConsumptionInstallment {
    #[must_use]
    pub(crate) const fn energy(self) -> Energy {
        self.energy
    }

    #[must_use]
    pub(crate) const fn hydration(self) -> AggregateVolume {
        self.hydration
    }

    #[must_use]
    pub(crate) const fn nutrition(self) -> NutritionGain {
        self.nutrition
    }

    #[must_use]
    pub(crate) const fn completes(self) -> bool {
        self.completes
    }
}

fn cumulative_share(total: u128, elapsed: u64, duration: u64) -> u128 {
    debug_assert!(duration > 0);
    debug_assert!(elapsed <= duration);
    checked_mul_div_with_remainder(total, u128::from(elapsed), u128::from(duration), 0)
        .unwrap_or_else(|| panic!("bounded direct-consumption cumulative share overflowed"))
        .0
}

fn installment_share(total: u128, before: u64, after: u64, duration: u64) -> u128 {
    cumulative_share(total, after, duration)
        .checked_sub(cumulative_share(total, before, duration))
        .unwrap_or_else(|| unreachable!("direct-consumption cumulative uptake is monotonic"))
}

fn elapsed_interval(
    pending: &PendingDirectConsumption,
    current: SimulationTick,
    next: SimulationTick,
) -> (u64, u64, u64, bool) {
    let started_at = pending.started_at().value();
    let completes_at = pending.completes_at().value();
    assert!(
        started_at <= current.value() && current.value() < completes_at,
        "runtime invariant broken: pending direct consumption is not active at current tick"
    );
    assert_eq!(
        next.value(),
        current
            .value()
            .checked_add(1)
            .unwrap_or_else(|| panic!("authoritative tick overflowed before consumption uptake")),
        "direct-consumption uptake requires one authoritative tick"
    );
    assert!(
        next.value() <= completes_at,
        "runtime invariant broken: direct-consumption tick crosses its completion boundary"
    );
    let duration = completes_at - started_at;
    let before = current.value() - started_at;
    let after = next.value() - started_at;
    (before, after, duration, next.value() == completes_at)
}

/// Resolves the exact incremental physiological contribution for the next authoritative tick.
///
/// Cumulative floor allocation prevents quantity-dependent rounding drift: every intermediate tick
/// receives only its elapsed fraction while the final tick recovers all remaining whole units.
pub(crate) fn direct_consumption_installment(
    registries: &Registries,
    pending: &PendingDirectConsumption,
    current: SimulationTick,
    next: SimulationTick,
) -> DirectConsumptionInstallment {
    let (before, after, duration, completes) = elapsed_interval(pending, current, next);
    match pending {
        PendingDirectConsumption::Eating(pending) => {
            let offer = trace_absorption_offer(registries, pending.consumed());
            let energy = Energy::from_nanojoules(installment_share(
                offer.energy().nanojoules(),
                before,
                after,
                duration,
            ));
            let hydration = AggregateVolume::from_microliters(installment_share(
                offer.hydration().microliters(),
                before,
                after,
                duration,
            ));
            let nutrition = NutritionGain::from_parts_per_million(
                u32::try_from(installment_share(
                    u128::from(offer.nutrition().get(FoodCategory::Grain)),
                    before,
                    after,
                    duration,
                ))
                .unwrap_or_else(|_| unreachable!("grain nutrition installment fits u32")),
                u32::try_from(installment_share(
                    u128::from(offer.nutrition().get(FoodCategory::Fruit)),
                    before,
                    after,
                    duration,
                ))
                .unwrap_or_else(|_| unreachable!("fruit nutrition installment fits u32")),
                u32::try_from(installment_share(
                    u128::from(offer.nutrition().get(FoodCategory::Protein)),
                    before,
                    after,
                    duration,
                ))
                .unwrap_or_else(|_| unreachable!("protein nutrition installment fits u32")),
            );
            DirectConsumptionInstallment {
                energy,
                hydration,
                nutrition,
                completes,
            }
        }
        PendingDirectConsumption::Drinking(pending) => {
            let hydration =
                pending_drink_hydration_offer(registries, pending.fluid(), pending.volume());
            DirectConsumptionInstallment {
                energy: Energy::ZERO,
                hydration: AggregateVolume::from_microliters(installment_share(
                    u128::from(hydration.microliters()),
                    before,
                    after,
                    duration,
                )),
                nutrition: NutritionGain::default(),
                completes,
            }
        }
    }
}
