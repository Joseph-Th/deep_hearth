//! Shared primitive mining workload sizing used by progression and fieldwork probes.

use deep_hearth::capability::CapabilityValue;
use deep_hearth::content::{
    EQUIPMENT_STONE_PICK, EQUIPMENT_STONE_QUARRY_PICK, MINING_METHOD_HAND_PICK,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::registry::Registries;

use super::equipment_support::pristine_equipment_capability;
use super::seed::mix64;

/// Finite stockpiling horizon used when pricing repeated primitive processing work.
pub(super) const STOCKPILE_WORK_ORDER_CYCLES: u64 = 12;
/// Ordinary bulk fieldwork horizon. This is shared with powered-ore planning so a large disclosed
/// extraction project can carry forward into the workload that motivates better power equipment.
pub(super) const BULK_FIELDWORK_ORDER_MIN_BATCHES: u64 = 32;
pub(super) const BULK_FIELDWORK_ORDER_MAX_BATCHES: u64 = 96;

fn pristine_mining_batch_mass(
    registries: &Registries,
    equipment: deep_hearth::equipment::EquipmentDefinitionId,
    context: &'static str,
) -> Mass {
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("primitive workload hand-mining method disappeared"));
    let CapabilityValue::Mass(maximum) =
        pristine_equipment_capability(registries, equipment, method.max_batch_mass_capability())
    else {
        panic!("primitive workload {context} batch capability changed physical kind")
    };
    assert!(
        !maximum.is_zero(),
        "primitive workload {context} batch must be nonzero"
    );
    maximum
}

pub(super) fn primitive_mining_cycle_mass(registries: &Registries, seed: u64) -> Mass {
    let maximum = pristine_mining_batch_mass(registries, EQUIPMENT_STONE_PICK, "stone-pick");
    let maximum = maximum.milligrams();
    let minimum = maximum
        .checked_mul(3)
        .map(|scaled| scaled.div_ceil(4))
        .unwrap_or_else(|| panic!("primitive mining-range scaling overflowed"));
    Mass::from_milligrams(minimum + mix64(seed ^ 0x5052_4F47_4D49_4E45) % (maximum - minimum + 1))
}

pub(super) fn primitive_quarry_batch_mass(registries: &Registries) -> Mass {
    pristine_mining_batch_mass(registries, EQUIPMENT_STONE_QUARRY_PICK, "stone-quarry")
}

pub(super) fn bulk_fieldwork_order_mass(registries: &Registries, seed: u64) -> Mass {
    let batch = primitive_quarry_batch_mass(registries);
    let batch_span = BULK_FIELDWORK_ORDER_MAX_BATCHES - BULK_FIELDWORK_ORDER_MIN_BATCHES + 1;
    let batches =
        BULK_FIELDWORK_ORDER_MIN_BATCHES + mix64(seed ^ 0x4649_454C_4442_4C4B) % batch_span;
    Mass::from_milligrams(
        batch
            .milligrams()
            .checked_mul(batches)
            .unwrap_or_else(|| panic!("bulk fieldwork order mass overflowed")),
    )
}
