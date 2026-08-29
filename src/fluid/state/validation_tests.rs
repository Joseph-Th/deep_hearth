//! Contract tests for trusted-load validation of finite fluid-store state.

use super::*;

#[test]
fn zero_store_identity_is_rejected_after_deserialization_boundary() {
    let zero = FluidStoreId(0);

    assert_eq!(
        validate_fluid_store_identity(zero, zero),
        Err(FluidValidationError::ZeroStoreId)
    );
}
