//! Contract tests for controlled energy-store fixture allocation.

use super::*;
use crate::content::{FORM_FLYWHEEL, MATERIAL_STONE, make_test_registries_with_energy_store};
use crate::core::quantity::Power;
use crate::core::time::WorldSeed;
use crate::energy::{EnergyCarrier, EnergyStoreDefinition, EnergyStoreRecord};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

const STORE_DEFINITION: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(930_001);

fn registries() -> Registries {
    make_test_registries_with_energy_store(EnergyStoreDefinition::new_with_transfer_limits(
        STORE_DEFINITION,
        "energy allocation fixture",
        EnergyCarrier::Electrical,
        Energy::from_nanojoules(1_000),
        Power::ZERO,
        Power::from_microwatts(25),
    ))
}

fn assembly_registries() -> Registries {
    make_test_registries_with_energy_store(
        EnergyStoreDefinition::new_with_transfer_limits(
            STORE_DEFINITION,
            "assembled energy allocation fixture",
            EnergyCarrier::Mechanical,
            Energy::from_nanojoules(1_000),
            Power::ZERO,
            Power::from_microwatts(25),
        )
        .with_assembly_profile(MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                Mass::from_milligrams(1),
            ),
        ])),
    )
}

#[test]
fn fixture_allocation_rejects_store_that_requires_material_assembly() {
    let registries = assembly_registries();
    let mut state = AppState::new(WorldSeed::new(0x9300_000A));
    let before = state.clone();

    assert_eq!(
        add_energy_store_with_initial_for_fixture(
            &registries,
            &mut state,
            STORE_DEFINITION,
            Energy::from_nanojoules(500),
        ),
        Err(AddEnergyStoreError::RequiresAssembly {
            definition: STORE_DEFINITION,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn fixture_allocation_rejects_energy_above_authored_capacity_without_mutation() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x9300_0001));
    let before = state.clone();

    assert_eq!(
        add_energy_store_with_initial_for_fixture(
            &registries,
            &mut state,
            STORE_DEFINITION,
            Energy::from_nanojoules(1_001),
        ),
        Err(AddEnergyStoreError::InitialEnergyExceedsCapacity {
            initial: Energy::from_nanojoules(1_001),
            capacity: Energy::from_nanojoules(1_000),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn empty_fixture_allocation_creates_store_without_free_energy() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x9300_0005));

    let store = add_energy_store(&registries, &mut state, STORE_DEFINITION)
        .unwrap_or_else(|error| panic!("empty energy-store fixture failed: {error}"));

    assert_eq!(
        state
            .energy()
            .get_store(store)
            .map(EnergyStoreRecord::stored),
        Some(Energy::ZERO)
    );
    assert_eq!(state.energy().revision(), 1);
}
