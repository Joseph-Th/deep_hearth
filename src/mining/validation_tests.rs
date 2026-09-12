//! Cross-owner trusted-load regressions for mining jobs.

use super::*;

use crate::content::{EQUIPMENT_JAW_CRUSHER, build_registries};

#[test]
fn trusted_replay_rejects_extraction_equipment_that_requires_installation() {
    let registries = build_registries();
    let definition = registries
        .equipment()
        .get_equipment(EQUIPMENT_JAW_CRUSHER)
        .unwrap_or_else(|| panic!("fixed mining replay fixture definition disappeared"));
    let job = MiningJobId::new(71);

    assert_eq!(
        validate_mining_equipment_portability(job, definition),
        Err(MiningJobValidationError::WorkingEquipmentRequiresStructuralSupport { job })
    );
}
