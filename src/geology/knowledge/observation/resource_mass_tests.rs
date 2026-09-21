//! Resource-mass estimate invariant contracts.

use super::*;

#[test]
fn estimate_rejects_exact_hidden_reserve_bounds() {
    let exact = Mass::from_milligrams(5_000_000);

    assert_eq!(
        ResourceMassEstimate::new(exact, exact),
        Err(ResourceMassEstimateError::ZeroWidth { bound: exact })
    );
}

#[test]
fn persisted_zero_width_estimate_is_rejected_during_decode() {
    let decoded = serde_json::from_value::<ResourceMassEstimate>(serde_json::json!({
        "lower": 5_000_000,
        "upper": 5_000_000,
    }));

    assert!(decoded.is_err());
}
