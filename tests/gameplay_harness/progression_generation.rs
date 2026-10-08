//! Primitive progression opportunity and clue-order generation without episode execution.

use super::seed::mix64;

pub(super) const SHALLOW_OPPORTUNITY_MIN_BATCHES: u64 = 6;
pub(super) const SHALLOW_OPPORTUNITY_MAX_BATCHES: u64 = 40;
pub(super) const MARGINAL_OPPORTUNITY_MIN_BATCHES: u64 = 48;
pub(super) const MARGINAL_OPPORTUNITY_MAX_BATCHES: u64 = 192;
pub(super) const DEEP_OPPORTUNITY_MIN_BATCHES: u64 = 384;
pub(super) const DEEP_OPPORTUNITY_MAX_BATCHES: u64 = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OreOpportunity {
    batch_budget: u64,
}

impl OreOpportunity {
    pub(super) const fn batch_budget(self) -> u64 {
        self.batch_budget
    }
}

pub(super) fn ore_opportunity(seed: u64, maintained_reinvestment_required: bool) -> OreOpportunity {
    if maintained_reinvestment_required {
        return OreOpportunity {
            batch_budget: DEEP_OPPORTUNITY_MAX_BATCHES,
        };
    }
    let opportunity_roll = mix64(seed ^ 0x4F50_504F_5254_554E);
    let magnitude_roll = opportunity_roll / 3;
    match opportunity_roll % 3 {
        0 => OreOpportunity {
            batch_budget: SHALLOW_OPPORTUNITY_MIN_BATCHES
                + magnitude_roll
                    % (SHALLOW_OPPORTUNITY_MAX_BATCHES - SHALLOW_OPPORTUNITY_MIN_BATCHES + 1),
        },
        1 => OreOpportunity {
            batch_budget: MARGINAL_OPPORTUNITY_MIN_BATCHES
                + magnitude_roll
                    % (MARGINAL_OPPORTUNITY_MAX_BATCHES - MARGINAL_OPPORTUNITY_MIN_BATCHES + 1),
        },
        2 => OreOpportunity {
            batch_budget: DEEP_OPPORTUNITY_MIN_BATCHES
                + magnitude_roll
                    % (DEEP_OPPORTUNITY_MAX_BATCHES - DEEP_OPPORTUNITY_MIN_BATCHES + 1),
        },
        _ => unreachable!("modulo-three opportunity class is bounded"),
    }
}

pub(super) fn varied_four_way_order(seed: u64) -> [usize; 4] {
    let mut order = [0, 1, 2, 3];
    let mut random = seed;
    for upper in (1..order.len()).rev() {
        random = mix64(random ^ upper as u64);
        let selected = usize::try_from(random % (upper as u64 + 1))
            .unwrap_or_else(|_| unreachable!("four-way shuffle index fits usize"));
        order.swap(upper, selected);
    }
    order
}
