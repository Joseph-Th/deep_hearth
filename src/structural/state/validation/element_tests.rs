//! Contract tests for trusted-load validation of structural element identity.

use super::*;

#[test]
fn zero_element_identity_is_rejected_after_deserialization_boundary() {
    let zero = StructuralElementId(0);

    assert_eq!(
        validate_structural_element_identity(zero, zero),
        Err(StructureValidationError::ZeroElementId)
    );
}
