//! Contract tests for trusted-load validation of mining job ownership and identity.

use super::*;

#[test]
fn zero_job_identity_is_rejected_after_deserialization_boundary() {
    let zero = MiningJobId(0);

    assert_eq!(
        validate_mining_job_id(1, zero, zero),
        Err(MiningValidationError::ZeroJobId)
    );
}
