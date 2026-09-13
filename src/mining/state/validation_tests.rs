//! Contract tests for trusted-load validation of mining job ownership and identity.

use super::*;

#[test]
fn zero_job_identity_is_rejected_after_deserialization_boundary() {
    let zero: MiningJobId =
        serde_json::from_value(serde_json::json!(0_u64)).unwrap_or_else(|error| {
            panic!("zero mining job ID fixture failed to deserialize: {error}")
        });

    assert_eq!(
        validate_mining_job_id(1, zero, zero),
        Err(MiningValidationError::ZeroJobId)
    );
}
