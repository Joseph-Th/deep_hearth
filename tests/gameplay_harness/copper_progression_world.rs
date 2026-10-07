//! Shared physical copper-ore generation for progression-continuity gameplay probes.
//!
//! Primitive progression and first-foundry inheritance use the same assay vocabulary so the later
//! episode cannot silently invent a different class of already-owned ore. This module generates
//! physical setup only; it never reads actor policy, investment thresholds, or route outcomes.

use super::seed::mix64;

const SOFT_ORE_MIN_PPM: u32 = 450_000;
const SOFT_ORE_MAX_PPM: u32 = 750_000;
const HARD_ORE_MIN_PPM: u32 = 500_000;
const HARD_ORE_MAX_PPM: u32 = 900_000;

pub(super) fn progression_ore_grades(seed: u64) -> (u32, u32) {
    let soft_roll = mix64(seed ^ 0x5052_4F47_4752_4144);
    let hard_roll = mix64(seed ^ 0x4841_5244_5F47_5244);
    (
        SOFT_ORE_MIN_PPM + (soft_roll % u64::from(SOFT_ORE_MAX_PPM - SOFT_ORE_MIN_PPM + 1)) as u32,
        HARD_ORE_MIN_PPM + (hard_roll % u64::from(HARD_ORE_MAX_PPM - HARD_ORE_MIN_PPM + 1)) as u32,
    )
}
