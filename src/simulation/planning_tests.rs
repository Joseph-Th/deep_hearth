//! Revision-budget checks for same-tick cross-owner planning.

use super::*;

#[test]
fn shared_owner_revision_capacity_accounts_for_all_same_tick_mutations() {
    assert_eq!(
        require_revision_capacity(u64::MAX - 2, [1, 1], TickError::EquipmentRevisionExhausted,),
        Ok(())
    );
    assert_eq!(
        require_revision_capacity(
            u64::MAX - 2,
            [1, 1, 1],
            TickError::EquipmentRevisionExhausted,
        ),
        Err(TickError::EquipmentRevisionExhausted)
    );
    assert_eq!(
        require_revision_capacity(u64::MAX - 1, [1, 1], TickError::EquipmentRevisionExhausted,),
        Err(TickError::EquipmentRevisionExhausted)
    );
}
