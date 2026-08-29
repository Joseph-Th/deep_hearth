//! Deterministic seed mixing shared by gameplay harnesses.

pub(super) const MAINTAINED_VARIATION_ROOT: u64 = 0xE7A1_0A7E_5EED_2026;

pub(super) fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

/// Resolves a deterministic mixed seed that is distinct from an already reserved bounded set.
///
/// The collision search is deliberately bounded. Generated gameplay samples are tiny, so needing
/// more probes than the number of already reserved seeds indicates a broken seed-generation
/// assumption and should fail the evaluator instead of allowing an accidental infinite search.
pub(super) fn unique_mixed_seed(mut candidate: u64, reserved: &[u64]) -> u64 {
    let attempt_budget = reserved
        .len()
        .checked_add(1)
        .unwrap_or_else(|| panic!("gameplay seed collision budget overflowed"));
    for _ in 0..attempt_budget {
        if !reserved.contains(&candidate) {
            return candidate;
        }
        candidate = mix64(candidate);
    }
    panic!(
        "gameplay seed mixer failed to escape {} reserved seed(s) within its bounded collision budget",
        reserved.len()
    );
}
