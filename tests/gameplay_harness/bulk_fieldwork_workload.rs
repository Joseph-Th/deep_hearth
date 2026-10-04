//! Large ordinary fieldwork workload sizing shared with power-provider investment.

use deep_hearth::content::EQUIPMENT_STONE_QUARRY_PICK;
use deep_hearth::core::quantity::Mass;
use deep_hearth::registry::Registries;

use super::primitive_workload::pristine_mining_batch_mass;
use super::seed::mix64;

/// Ordinary bulk fieldwork horizon. Powered-ore planning consumes the same disclosed extraction
/// scale so better power equipment is motivated by upstream work rather than a synthetic crossover.
pub(super) const BULK_FIELDWORK_ORDER_MIN_BATCHES: u64 = 32;
pub(super) const BULK_FIELDWORK_ORDER_MAX_BATCHES: u64 = 96;

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
