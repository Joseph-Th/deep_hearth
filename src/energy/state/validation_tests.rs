//! Contract tests for trusted-load validation of finite energy-store state.

use super::*;

#[test]
fn zero_store_identity_is_rejected_after_deserialization_boundary() {
    let zero = EnergyStoreId(0);

    assert_eq!(
        validate_energy_store_identity(zero, zero),
        Err(EnergyValidationError::ZeroStoreId)
    );
}
