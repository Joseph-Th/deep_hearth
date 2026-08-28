//! Built-in finite mechanical, electrical, and thermal energy-storage definitions.

use crate::core::quantity::{Energy, Mass, Power};
use crate::energy::{
    EnergyCarrier, EnergyRegistry, EnergyStoreDefinition, EnergyStoreDefinitionId,
};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use super::materials::{FORM_FLYWHEEL, FORM_HANDLE, MATERIAL_STONE, MATERIAL_WOOD};

pub const ENERGY_MECHANICAL_SMALL_DRIVE: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(1);
pub const ENERGY_MECHANICAL_LARGE_DRIVE: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(2);
pub const ENERGY_ELECTRICAL_BUFFER: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(3);
pub const ENERGY_THERMAL_SINK: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(4);
pub const ENERGY_STONE_FLYWHEEL_DRIVE: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(5);

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
            Energy::from_nanojoules(20_000_000_000_000_000),
            Power::from_microwatts(1_000_000_000_000),
            Power::ZERO,
        )
        .with_passive_dissipation_power(Power::from_microwatts(1_000_000_000_000)),
        EnergyStoreDefinition::new_with_transfer_limits(
            ENERGY_STONE_FLYWHEEL_DRIVE,
            "stone flywheel accumulator",
            EnergyCarrier::Mechanical,
            Energy::from_nanojoules(500_000_000_000),
            Power::from_microwatts(150_000_000),
            Power::from_microwatts(500_000_000),
        )
        .with_assembly_profile(MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                Mass::from_milligrams(900_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
        ])),
    ])
}
