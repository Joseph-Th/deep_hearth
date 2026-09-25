//! Registry-derived gameplay planning for immutable powered-craft work requirements.

use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::energy::calculate_mass_specific_energy;
use deep_hearth::production::ProcessId;
use deep_hearth::registry::Registries;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AuthoredPoweredCraftBatch {
    pub(super) input_mass: Mass,
    pub(super) work: Energy,
}

/// Returns the exact input mass and stored-work requirement for one authored transform batch.
pub(super) fn authored_batch(
    registries: &Registries,
    process: ProcessId,
    context: &'static str,
) -> AuthoredPoweredCraftBatch {
    let powered = registries
        .crafting()
        .get_powered(process)
        .unwrap_or_else(|| panic!("gameplay harness {context} powered craft disappeared"));
    let transform = registries
        .crafting()
        .get_manual(powered.transform())
        .unwrap_or_else(|| {
            panic!("gameplay harness {context} powered craft references missing manual transform")
        });
    let input_mass = transform.input_mass();
    AuthoredPoweredCraftBatch {
        input_mass,
        work: calculate_mass_specific_energy(input_mass, powered.specific_energy()),
    }
}
