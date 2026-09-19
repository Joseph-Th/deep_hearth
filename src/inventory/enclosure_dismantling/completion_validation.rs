//! Completion and trusted-load compatibility replay for enclosure removal.

use crate::core::time::SimulationTick;
use crate::registry::Registries;

use super::super::{InventoryState, StockpileId, StockpileStorageProfile};
use super::StorageEnclosureDismantlingError;
use crate::inventory::storage_validation::validate_stockpile_storage_profile;

pub(crate) fn validate_storage_dismantling_target_for_completion(
    registries: &Registries,
    inventory: &InventoryState,
    target: StockpileId,
    at: SimulationTick,
) -> Result<(), StorageEnclosureDismantlingError> {
    let target_record = inventory
        .get_stockpile(target)
        .ok_or(StorageEnclosureDismantlingError::UnknownTarget { stockpile: target })?;
    let next_profile = StockpileStorageProfile::unbounded_solid_only();
    let source_preservation = target_record
        .storage_profile()
        .preservation_multiplier_ppm();
    let destination_preservation = next_profile.preservation_multiplier_ppm();
    for lot in inventory.lot_ids(target) {
        let record = inventory
            .get_lot(lot)
            .unwrap_or_else(|| unreachable!("stockpile lot index references a live lot"));
        validate_stockpile_storage_profile(
            registries,
            next_profile,
            target,
            record.commodity(),
            record.composition(),
            record.temperature(),
            record.particle_size_distribution(),
        )
        .map_err(
            |error| StorageEnclosureDismantlingError::TargetContentsIncompatible { lot, error },
        )?;
        assert!(
            record
                .storage_history()
                .transition_preservation(at, source_preservation, destination_preservation)
                .is_some(),
            "runtime invariant broken: physically reachable storage history must checkpoint during enclosure dismantling"
        );
    }
    Ok(())
}
