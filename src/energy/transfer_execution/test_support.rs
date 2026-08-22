//! Test-only construction of opaque energy-transfer resolutions.

use crate::core::quantity::Energy;

use super::{EnergyStoreId, EnergyTransferResolution};

pub(crate) const fn make_test_energy_transfer_resolution(
    source: EnergyStoreId,
    destination: EnergyStoreId,
    energy: Energy,
) -> EnergyTransferResolution {
    EnergyTransferResolution {
        source,
        destination,
        energy,
    }
}
