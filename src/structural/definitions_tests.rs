//! Tests for the sibling definitions module; isolated so test-only edits do not invalidate production builds.

use super::*;

const TEST_PROFILE: StructuralProfileId = StructuralProfileId::new(950_001);

fn profile(cracking_at_ppm: u32, cracked_capacity_ppm: u32) -> StructuralProfileDefinition {
    StructuralProfileDefinition::new(
        TEST_PROFILE,
        "structural profile fixture",
        StructuralLoadMode::Compression,
        500_000,
        cracking_at_ppm,
        cracked_capacity_ppm,
    )
}

#[test]
fn cracked_capacity_requires_a_real_post_crack_operating_range() {
    let valid = profile(800_000, 900_000);
    assert_eq!(valid.cracked_capacity_ppm(), 900_000);

    assert!(std::panic::catch_unwind(|| profile(800_000, 800_000)).is_err());
    assert!(std::panic::catch_unwind(|| profile(800_000, 1_000_000)).is_err());
}
