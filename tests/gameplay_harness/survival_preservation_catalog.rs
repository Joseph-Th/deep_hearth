//! Registry-derived preservation storage catalog shared by generation and investment evaluation.

use deep_hearth::core::quantity::Mass;
use deep_hearth::inventory::{StockpileStorageProfile, StorageDefinitionId};
use deep_hearth::registry::Registries;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PreservationStorageCandidate {
    pub(super) definition: StorageDefinitionId,
    pub(super) preservation_multiplier_ppm: u32,
    pub(super) capacity: Mass,
}

pub(super) fn preservation_storage_candidates(
    registries: &Registries,
) -> Vec<PreservationStorageCandidate> {
    let ambient_preservation =
        StockpileStorageProfile::unbounded_solid_only().preservation_multiplier_ppm();
    let mut candidates = registries
        .storage()
        .definitions()
        .filter(|definition| {
            definition.storage_profile().preservation_multiplier_ppm() > ambient_preservation
        })
        .map(|definition| PreservationStorageCandidate {
            definition: definition.id(),
            preservation_multiplier_ppm: definition.storage_profile().preservation_multiplier_ppm(),
            capacity: definition.maximum_stockpile_capacity(),
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|candidate| candidate.definition);
    assert!(
        !candidates.is_empty(),
        "survival gameplay has no authored preservation enclosure"
    );
    candidates
}
