//! Provider-eligibility contracts for powered crafting.

use super::*;
use crate::capability::{
    CapabilityComparison, CapabilityEvaluationError, CapabilityRequirement, CapabilityValue,
};
use crate::content::{
    ENERGY_MECHANICAL_SMALL_DRIVE, EQUIPMENT_TIMBER_SASH_SAWMILL, FORM_BOARD, FORM_HANDLE,
    FORM_LOG, FORM_REINFORCEMENT, FORM_SAW_BLADE, MATERIAL_COPPER, MATERIAL_WOOD,
    PROCESS_POWER_SAW_WOOD_BOARDS, PROCESS_SAW_WOOD_BOARDS,
};
use crate::core::quantity::{Energy, Mass, MassFlow, Temperature};
use crate::core::state::AppState;
use crate::core::time::WorldSeed;
use crate::energy::add_energy_store_with_initial_for_fixture;
use crate::equipment::{degrade_equipment_condition_for_test, validate_assemble_equipment};
use crate::inventory::{
    MaterialLotSelection, StockpileId, add_solid_stockpile_for_test, deposit_lot_for_test,
};
use crate::maintenance::Condition;
use crate::material::CommodityKey;
use crate::registry::{Registries, RegistryDomains, RegistryPresentation};

const TEST_POWERED_PROCESS: ProcessId = ProcessId::new(990_071);
const ROOM_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

fn augmented_powered_registries(
    throughput_comparison: CapabilityComparison,
) -> (Registries, crate::capability::CapabilityId) {
    let base = build_registries();
    let powered = base
        .crafting()
        .get_powered(PROCESS_POWER_SAW_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("built-in powered saw definition disappeared"));
    let manual_saw_capability = base
        .crafting()
        .get_manual(PROCESS_SAW_WOOD_BOARDS)
        .and_then(ManualCraftDefinition::equipment_profile)
        .map(ManualCraftEquipmentProfile::mass_flow_capability)
        .unwrap_or_else(|| panic!("built-in frame-saw equipment profile disappeared"));
    let custom_powered = PoweredCraftDefinition::new(
        TEST_POWERED_PROCESS,
        powered.transform(),
        powered.mass_flow_capability(),
        powered.energy_carrier(),
        powered.specific_energy(),
        powered.condition_wear_ppm_per_active_tick(),
    );
    let crafting = CraftingRegistry::new_with_powered(
        base.crafting().definitions().cloned(),
        base.crafting()
            .powered_definitions()
            .chain(std::iter::once(custom_powered)),
    );
    let mut production = base.production().clone();
    production.register_process(ProcessDefinition::new(
        TEST_POWERED_PROCESS,
        "condition-sensitive powered saw fixture",
        vec![
            CapabilityRequirement::new(
                powered.mass_flow_capability(),
                throughput_comparison,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1)),
            ),
            CapabilityRequirement::new(
                manual_saw_capability,
                CapabilityComparison::AtLeast,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(30_000)),
            ),
        ],
    ));
    let registries = Registries::new(
        base.schema_version(),
        base.core().clone(),
        RegistryDomains {
            energy: base.energy().clone(),
            fluid: base.fluid().clone(),
            capabilities: base.capabilities().clone(),
            crafting,
            labor: base.labor().clone(),
            equipment: base.equipment().clone(),
            storage: base.storage().clone(),
            structural: base.structural().clone(),
            materials: base.materials().clone(),
            mining: base.mining().clone(),
            ore_processing: base.ore_processing().clone(),
            thermal: base.thermal().clone(),
            production,
            survival: base.survival().clone(),
            presentation: RegistryPresentation {
                textures: base.textures().clone(),
                shaders: base.shaders().clone(),
            },
        },
    );
    (registries, manual_saw_capability)
}

fn stockpile(state: &mut AppState, capacity_mg: u64) -> StockpileId {
    add_solid_stockpile_for_test(state, Mass::from_milligrams(capacity_mg))
        .unwrap_or_else(|error| panic!("powered capability stockpile fixture failed: {error}"))
}

fn deposit(
    registries: &Registries,
    state: &mut AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass_mg: u64,
) -> crate::inventory::MaterialLotId {
    deposit_lot_for_test(
        registries,
        state,
        stockpile,
        commodity,
        Mass::from_milligrams(mass_mg),
        ROOM_TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("powered capability material fixture failed: {error}"))
}

#[test]
fn powered_craft_registry_requires_minimum_throughput_semantics() {
    let result = std::panic::catch_unwind(|| {
        let _ = augmented_powered_registries(CapabilityComparison::AtMost);
    });

    assert!(result.is_err());
}

#[test]
fn powered_craft_enforces_all_condition_adjusted_provider_requirements() {
    let (registries, extra_capability) =
        augmented_powered_registries(CapabilityComparison::AtLeast);
    let mut state = AppState::new(WorldSeed::new(0xC4AF_71C0));
    let assembly = stockpile(&mut state, 6_000_000);
    for (commodity, mass) in [
        (CommodityKey::new(MATERIAL_WOOD, FORM_BOARD), 4_000_000),
        (CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE), 800_000),
        (CommodityKey::new(MATERIAL_COPPER, FORM_SAW_BLADE), 54_000),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            20_000,
        ),
    ] {
        deposit(&registries, &mut state, assembly, commodity, mass);
    }
    let sawmill =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_TIMBER_SASH_SAWMILL, assembly)
            .unwrap_or_else(|error| panic!("sawmill assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("sawmill assembly commit failed: {error}"));
    degrade_equipment_condition_for_test(&mut state, sawmill, 500_000);
    assert_eq!(
        state
            .equipment()
            .get_equipment(sawmill)
            .map(|record| record.condition()),
        Some(
            Condition::new(500_000)
                .unwrap_or_else(|error| panic!("condition fixture failed: {error}"))
        )
    );

    let source = stockpile(&mut state, 1_000_000);
    let log = deposit(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        1_000_000,
    );
    let drive = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_MECHANICAL_SMALL_DRIVE,
        Energy::from_nanojoules(1_000_000_000_000),
    )
    .unwrap_or_else(|error| panic!("powered capability drive fixture failed: {error}"));

    let result = resolve_powered_craft(
        &registries,
        &state,
        &PoweredCraftRequest::single(
            TEST_POWERED_PROCESS,
            source,
            MaterialLotSelection::new(log, Mass::from_milligrams(1_000_000)),
            sawmill,
            drive,
        ),
    );

    assert_eq!(
        result,
        Err(PoweredCraftError::Capability(
            CapabilityEvaluationError::ThresholdNotMet {
                capability: extra_capability,
                comparison: CapabilityComparison::AtLeast,
                required: CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(30_000)),
                provided: CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(20_000)),
            }
        ))
    );
}
