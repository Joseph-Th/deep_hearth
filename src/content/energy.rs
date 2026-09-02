//! Built-in finite mechanical, electrical, and thermal energy-storage definitions.

use crate::core::quantity::{Energy, Mass, Power};
use crate::energy::{
    EnergyCarrier, EnergyRegistry, EnergyStoreDefinition, EnergyStoreDefinitionId,
    EnergyStoreUpgradeProfile,
};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use super::materials::{
    FORM_FLYWHEEL, FORM_HANDLE, FORM_REINFORCEMENT, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
};

pub const ENERGY_MECHANICAL_SMALL_DRIVE: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(1);
pub const ENERGY_MECHANICAL_LARGE_DRIVE: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(2);
pub const ENERGY_ELECTRICAL_BUFFER: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(3);
pub const ENERGY_THERMAL_SINK: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(4);
pub const ENERGY_STONE_FLYWHEEL_DRIVE: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(5);
pub const ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE: EnergyStoreDefinitionId =
    EnergyStoreDefinitionId::new(6);
pub const ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE: EnergyStoreDefinitionId =
    EnergyStoreDefinitionId::new(7);

const WORKSHOP_ELECTRICAL_BUFFER_CAPACITY: Energy = Energy::from_nanojoules(25_000_000_000_000_000);
const WORKSHOP_ELECTRICAL_BUFFER_TRANSFER_POWER: Power = Power::from_microwatts(1_000_000_000_000);
const WORKSHOP_THERMAL_SINK_CAPACITY: Energy = Energy::from_nanojoules(20_000_000_000_000_000);
const WORKSHOP_THERMAL_SINK_INPUT_POWER: Power = Power::from_microwatts(1_000_000_000_000);
const WORKSHOP_THERMAL_SINK_PASSIVE_DISSIPATION_POWER: Power =
    Power::from_microwatts(100_000_000_000);
/// Low but nonzero bearing/windage loss for the crude mechanical accumulator.
///
/// At the authoritative 3.6-second tick this rejects exactly 180 mJ per tick. A full 500 J charge
/// therefore cannot function as permanent stored work, while freshly charged workshop-scale
/// operations still have a useful multi-minute physical window.
const STONE_FLYWHEEL_PASSIVE_DISSIPATION_POWER: Power = Power::from_microwatts(50_000);
const PAIRED_STONE_FLYWHEEL_PASSIVE_DISSIPATION_POWER: Power = Power::from_microwatts(100_000);

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
        EnergyStoreDefinition::new_with_transfer_limits(
            ENERGY_ELECTRICAL_BUFFER,
            "workshop electrical buffer",
            EnergyCarrier::Electrical,
            WORKSHOP_ELECTRICAL_BUFFER_CAPACITY,
            WORKSHOP_ELECTRICAL_BUFFER_TRANSFER_POWER,
            WORKSHOP_ELECTRICAL_BUFFER_TRANSFER_POWER,
        ),
        EnergyStoreDefinition::new_with_transfer_limits(
            ENERGY_THERMAL_SINK,
            "workshop thermal sink",
            EnergyCarrier::Thermal,
            WORKSHOP_THERMAL_SINK_CAPACITY,
            WORKSHOP_THERMAL_SINK_INPUT_POWER,
            Power::ZERO,
        )
        .with_passive_dissipation_power(WORKSHOP_THERMAL_SINK_PASSIVE_DISSIPATION_POWER),
        EnergyStoreDefinition::new_with_transfer_limits(
            ENERGY_STONE_FLYWHEEL_DRIVE,
            "stone flywheel accumulator",
            EnergyCarrier::Mechanical,
            Energy::from_nanojoules(500_000_000_000),
            Power::from_microwatts(150_000_000),
            Power::from_microwatts(500_000_000),
        )
        .with_passive_dissipation_power(STONE_FLYWHEEL_PASSIVE_DISSIPATION_POWER)
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
        EnergyStoreDefinition::new_with_transfer_limits(
            ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
            "copper-banded stone flywheel accumulator",
            EnergyCarrier::Mechanical,
            Energy::from_nanojoules(750_000_000_000),
            Power::from_microwatts(150_000_000),
            Power::from_microwatts(500_000_000),
        )
        .with_passive_dissipation_power(STONE_FLYWHEEL_PASSIVE_DISSIPATION_POWER)
        .with_assembly_profile(MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                Mass::from_milligrams(900_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                Mass::from_milligrams(20_000),
            ),
        ]))
        .with_upgrade_profile(EnergyStoreUpgradeProfile::new(
            ENERGY_STONE_FLYWHEEL_DRIVE,
            MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                Mass::from_milligrams(20_000),
            )]),
        )),
        EnergyStoreDefinition::new_with_transfer_limits(
            ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE,
            "paired stone flywheel accumulator",
            EnergyCarrier::Mechanical,
            Energy::from_nanojoules(1_000_000_000_000),
            Power::from_microwatts(100_000_000),
            Power::from_microwatts(500_000_000),
        )
        .with_passive_dissipation_power(PAIRED_STONE_FLYWHEEL_PASSIVE_DISSIPATION_POWER)
        .with_assembly_profile(MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                Mass::from_milligrams(1_800_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(400_000),
            ),
        ])),
    ])
}
