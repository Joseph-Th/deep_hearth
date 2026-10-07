//! Built-in finite mechanical, electrical, and thermal energy-storage definitions.

use crate::core::quantity::{Energy, Mass, Power};
use crate::energy::{
    EnergyCarrier, EnergyRegistry, EnergyStoreDefinition, EnergyStoreDefinitionId,
    EnergyStoreUpgradeProfile,
};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use super::crafted_parts::{
    COPPER_ELECTRICAL_WINDING_MASS, COPPER_REINFORCEMENT_MASS, STONE_FLYWHEEL_MASS,
    TIMBER_FLYWHEEL_MASS,
};
use super::materials::{
    FORM_BOARD, FORM_ELECTRICAL_WINDING, FORM_FLYWHEEL, FORM_HANDLE, FORM_LUMP, FORM_REINFORCEMENT,
    MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
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
pub const ENERGY_TIMBER_FLYWHEEL_DRIVE: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(8);
pub const ENERGY_TIMBER_FRAME_FLYWHEEL_BANK: EnergyStoreDefinitionId =
    EnergyStoreDefinitionId::new(9);
pub const ENERGY_COPPER_PLATE_ELECTRICAL_BUFFER: EnergyStoreDefinitionId =
    EnergyStoreDefinitionId::new(10);
pub const ENERGY_STONE_THERMAL_SINK: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(11);
pub const ENERGY_COPPER_RACK_ELECTRICAL_BUFFER: EnergyStoreDefinitionId =
    EnergyStoreDefinitionId::new(12);
pub const ENERGY_COPPER_BANDED_STONE_THERMAL_SINK: EnergyStoreDefinitionId =
    EnergyStoreDefinitionId::new(13);

const WORKSHOP_ELECTRICAL_BUFFER_CAPACITY: Energy = Energy::from_nanojoules(25_000_000_000_000_000);
const WORKSHOP_ELECTRICAL_BUFFER_TRANSFER_POWER: Power = Power::from_microwatts(1_000_000_000_000);
const WORKSHOP_THERMAL_SINK_CAPACITY: Energy = Energy::from_nanojoules(20_000_000_000_000_000);
const WORKSHOP_THERMAL_SINK_INPUT_POWER: Power = Power::from_microwatts(1_000_000_000_000);
const WORKSHOP_THERMAL_SINK_PASSIVE_DISSIPATION_POWER: Power =
    Power::from_microwatts(100_000_000_000);
/// Low but nonzero bearing/windage loss keeps the crude flywheel a short-term work buffer rather
/// than a long-duration battery.
const STONE_FLYWHEEL_PASSIVE_DISSIPATION_POWER: Power = Power::from_microwatts(1_000_000);
const PAIRED_STONE_FLYWHEEL_PASSIVE_DISSIPATION_POWER: Power = Power::from_microwatts(2_000_000);
const TIMBER_FLYWHEEL_PASSIVE_DISSIPATION_POWER: Power = Power::from_microwatts(500_000);
const TIMBER_FRAME_FLYWHEEL_BANK_PASSIVE_DISSIPATION_POWER: Power =
    Power::from_microwatts(10_000_000);

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
                STONE_FLYWHEEL_MASS,
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
        ])),
        EnergyStoreDefinition::new_with_transfer_limits(
            ENERGY_TIMBER_FLYWHEEL_DRIVE,
            "timber flywheel accumulator",
            EnergyCarrier::Mechanical,
            Energy::from_nanojoules(300_000_000_000),
            Power::from_microwatts(100_000_000),
            Power::from_microwatts(250_000_000),
        )
        .with_passive_dissipation_power(TIMBER_FLYWHEEL_PASSIVE_DISSIPATION_POWER)
        .with_assembly_profile(MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_FLYWHEEL),
                TIMBER_FLYWHEEL_MASS,
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
                STONE_FLYWHEEL_MASS,
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                COPPER_REINFORCEMENT_MASS,
            ),
        ]))
        .with_upgrade_profile(EnergyStoreUpgradeProfile::new(
            ENERGY_STONE_FLYWHEEL_DRIVE,
            MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                COPPER_REINFORCEMENT_MASS,
            )]),
        )),
        EnergyStoreDefinition::new_with_transfer_limits(
            ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE,
            "paired stone flywheel accumulator",
            EnergyCarrier::Mechanical,
            Energy::from_nanojoules(1_000_000_000_000),
            Power::from_microwatts(150_000_000),
            Power::from_microwatts(500_000_000),
        )
        .with_passive_dissipation_power(PAIRED_STONE_FLYWHEEL_PASSIVE_DISSIPATION_POWER)
        .with_assembly_profile(MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                Mass::from_milligrams(
                    STONE_FLYWHEEL_MASS
                        .milligrams()
                        .checked_mul(2)
                        .unwrap_or_else(|| panic!("paired stone flywheel mass overflows")),
                ),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(400_000),
            ),
        ]))
        .with_upgrade_profile(EnergyStoreUpgradeProfile::new(
            ENERGY_STONE_FLYWHEEL_DRIVE,
            MaterialAssemblyProfile::new(vec![
                MaterialInputSpec::pure(
                    CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                    STONE_FLYWHEEL_MASS,
                ),
                MaterialInputSpec::pure(
                    CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                    Mass::from_milligrams(200_000),
                ),
            ]),
        )),
        // The bank scales primitive flywheel storage through more stone, framing, and shafting
        // without introducing a new material tier. Passive drag keeps it a workshop work buffer
        // rather than long-duration storage.
        EnergyStoreDefinition::new_with_transfer_limits(
            ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
            "timber-framed stone flywheel bank",
            EnergyCarrier::Mechanical,
            Energy::from_nanojoules(5_000_000_000_000),
            Power::from_microwatts(150_000_000),
            Power::from_microwatts(500_000_000),
        )
        .with_passive_dissipation_power(TIMBER_FRAME_FLYWHEEL_BANK_PASSIVE_DISSIPATION_POWER)
        .with_assembly_profile(MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                Mass::from_milligrams(
                    STONE_FLYWHEEL_MASS
                        .milligrams()
                        .checked_mul(10)
                        .unwrap_or_else(|| panic!("flywheel bank stone mass overflows")),
                ),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(3_200_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(800_000),
            ),
        ]))
        .with_upgrade_profile(EnergyStoreUpgradeProfile::new(
            ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE,
            MaterialAssemblyProfile::new(vec![
                MaterialInputSpec::pure(
                    CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                    Mass::from_milligrams(
                        STONE_FLYWHEEL_MASS
                            .milligrams()
                            .checked_mul(8)
                            .unwrap_or_else(|| {
                                panic!("flywheel-bank upgrade stone mass overflows")
                            }),
                    ),
                ),
                MaterialInputSpec::pure(
                    CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                    Mass::from_milligrams(3_200_000),
                ),
                MaterialInputSpec::pure(
                    CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                    Mass::from_milligrams(400_000),
                ),
            ]),
        )),
        // This is a small workshop buffer, not industrial storage. It supports a primitive melt
        // while repeated casting still requires repeated finite charging.
        EnergyStoreDefinition::new_with_transfer_limits(
            ENERGY_COPPER_PLATE_ELECTRICAL_BUFFER,
            "copper-plate electrical buffer",
            EnergyCarrier::Electrical,
            Energy::from_nanojoules(15_000_000_000_000),
            Power::from_microwatts(100_000_000),
            Power::from_microwatts(100_000_000),
        )
        .with_assembly_profile(MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                Mass::from_milligrams(80_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(1_600_000),
            ),
        ])),
        // The stone sink provides finite thermal headroom for a larger cast while its pickup rate
        // and passive rejection keep repeated casting constrained by real heat accumulation.
        EnergyStoreDefinition::new_with_transfer_limits(
            ENERGY_STONE_THERMAL_SINK,
            "stone foundry heat sink",
            EnergyCarrier::Thermal,
            Energy::from_nanojoules(50_000_000_000_000),
            Power::from_microwatts(200_000_000),
            Power::ZERO,
        )
        .with_passive_dissipation_power(Power::from_microwatts(20_000_000))
        .with_assembly_profile(MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(4_000_000),
        )])),
        // Parallel copper plates and a purpose-made winding support settlement foundry batches
        // while transfer limits keep charging and melting visible player-time costs.
        EnergyStoreDefinition::new_with_transfer_limits(
            ENERGY_COPPER_RACK_ELECTRICAL_BUFFER,
            "copper-rack electrical buffer",
            EnergyCarrier::Electrical,
            Energy::from_nanojoules(60_000_000_000_000),
            Power::from_microwatts(150_000_000),
            Power::from_microwatts(300_000_000),
        )
        .with_assembly_profile(MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                Mass::from_milligrams(320_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
                COPPER_ELECTRICAL_WINDING_MASS,
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(4_800_000),
            ),
        ]))
        .with_upgrade_profile(EnergyStoreUpgradeProfile::new(
            ENERGY_COPPER_PLATE_ELECTRICAL_BUFFER,
            MaterialAssemblyProfile::new(vec![
                MaterialInputSpec::pure(
                    CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                    Mass::from_milligrams(240_000),
                ),
                MaterialInputSpec::pure(
                    CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
                    COPPER_ELECTRICAL_WINDING_MASS,
                ),
                MaterialInputSpec::pure(
                    CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                    Mass::from_milligrams(3_200_000),
                ),
            ]),
        )),
        // Added thermal mass and copper banding increase reservoir headroom and heat pickup without
        // creating a generic heat-transfer network. Unchanged passive loss preserves a recovery
        // interval under sustained casting instead of deleting waste energy.
        EnergyStoreDefinition::new_with_transfer_limits(
            ENERGY_COPPER_BANDED_STONE_THERMAL_SINK,
            "copper-banded stone foundry heat sink",
            EnergyCarrier::Thermal,
            Energy::from_nanojoules(200_000_000_000_000),
            Power::from_microwatts(400_000_000),
            Power::ZERO,
        )
        .with_passive_dissipation_power(Power::from_microwatts(20_000_000))
        .with_assembly_profile(MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
                Mass::from_milligrams(16_000_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                Mass::from_milligrams(80_000),
            ),
        ]))
        .with_upgrade_profile(EnergyStoreUpgradeProfile::new(
            ENERGY_STONE_THERMAL_SINK,
            MaterialAssemblyProfile::new(vec![
                MaterialInputSpec::pure(
                    CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
                    Mass::from_milligrams(12_000_000),
                ),
                MaterialInputSpec::pure(
                    CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                    Mass::from_milligrams(80_000),
                ),
            ]),
        )),
    ])
}
