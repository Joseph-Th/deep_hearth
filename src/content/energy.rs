//! Built-in finite-energy store definitions; registrations remain empty until concrete world content is authored.

use crate::energy::{EnergyRegistry, EnergyStoreDefinition};

pub(crate) fn build_energy_registry() -> EnergyRegistry {
    EnergyRegistry::new(std::iter::empty::<EnergyStoreDefinition>())
}
