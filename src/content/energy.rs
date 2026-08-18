//! Built-in finite workshop energy-source and sink definitions.

use crate::core::quantity::{Energy, Power};
use crate::energy::{
    EnergyCarrier, EnergyRegistry, EnergyStoreDefinition, EnergyStoreDefinitionId,
};

pub const ENERGY_MECHANICAL_SMALL_DRIVE: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(1);
pub const ENERGY_MECHANICAL_LARGE_DRIVE: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(2);
pub const ENERGY_ELECTRICAL_BUFFER: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(3);
pub const ENERGY_THERMAL_SINK: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(4);

pub(crate) fn build_energy_registry() -> EnergyRegistry {
    EnergyRegistry::new([
        EnergyStoreDefinition::new_with_transfer_limits(
            ENERGY_MECHANICAL_SMALL_DRIVE,
            "small mechanical drive",
            EnergyCarrier::Mechanical,
            Energy::from_nanojoules(200_000_000_000_000),
            Power::from_microwatts(50_000_000),
            Power::from_microwatts(1_000_000_000),
        ),
        EnergyStoreDefinition::new_with_transfer_limits(
            ENERGY_MECHANICAL_LARGE_DRIVE,
            "upgraded mechanical drive",
            EnergyCarrier::Mechanical,
            Energy::from_nanojoules(400_000_000_000_000),
            Power::from_microwatts(500_000_000),
            Power::from_microwatts(20_000_000_000),
        ),
        EnergyStoreDefinition::new(
            ENERGY_ELECTRICAL_BUFFER,
            "workshop electrical buffer",
            EnergyCarrier::Electrical,
            Energy::from_nanojoules(25_000_000_000_000_000),
            Power::from_microwatts(1_000_000_000_000),
        ),
        EnergyStoreDefinition::new_with_transfer_limits(
            ENERGY_THERMAL_SINK,
            "workshop thermal sink",
            EnergyCarrier::Thermal,
            Energy::from_nanojoules(10_000_000_000_000_000),
            Power::from_microwatts(1_000_000_000_000),
            Power::ZERO,
        ),
    ])
}
