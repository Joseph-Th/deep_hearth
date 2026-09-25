//! Shared primitive mining workload sizing used by progression and fieldwork probes.

use deep_hearth::capability::CapabilityValue;
use deep_hearth::content::{EQUIPMENT_STONE_PICK, MINING_METHOD_HAND_PICK};
use deep_hearth::core::quantity::Mass;
use deep_hearth::registry::Registries;

use super::equipment_support::pristine_equipment_capability;
use super::seed::mix64;

/// Finite stockpiling horizon used when pricing repeated primitive processing work.
pub(super) const STOCKPILE_WORK_ORDER_CYCLES: u64 = 12;

pub(super) fn primitive_mining_cycle_mass(registries: &Registries, seed: u64) -> Mass {
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("primitive workload hand-mining method disappeared"));
    let CapabilityValue::Mass(maximum) = pristine_equipment_capability(
        registries,
        EQUIPMENT_STONE_PICK,
        method.max_batch_mass_capability(),
    ) else {
        panic!("primitive workload stone-pick batch capability changed physical kind")
    };
    let maximum = maximum.milligrams();
    assert!(maximum > 0, "primitive mining batch must be nonzero");
    let minimum = maximum
        .checked_mul(3)
        .map(|scaled| scaled.div_ceil(4))
        .unwrap_or_else(|| panic!("primitive mining-range scaling overflowed"));
    Mass::from_milligrams(minimum + mix64(seed ^ 0x5052_4F47_4D49_4E45) % (maximum - minimum + 1))
}
