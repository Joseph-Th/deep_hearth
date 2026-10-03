//! Content-relative variation contracts for ordinary power-provider workloads.

use std::collections::BTreeSet;

use deep_hearth::capability::CapabilityValue;
use deep_hearth::content::{
    ENERGY_TIMBER_FRAME_FLYWHEEL_BANK, FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL, MATERIAL_COPPER,
    MATERIAL_STONE, MATERIAL_WOOD, PROCESS_CRUSH_ORE, PROCESS_POWER_SAW_WOOD_BOARDS,
};
use deep_hearth::energy::EnergyCarrier;
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::{CommoditySource, ProcessEquipmentRole, Registries};

use super::planning::{PrimitivePowerChoice, SettlementPowerChoice};
use super::*;

fn commodity_reachable_from_roots(
    registries: &Registries,
    commodity: CommodityKey,
    roots: &BTreeSet<CommodityKey>,
) -> bool {
    if roots.contains(&commodity) {
        return true;
    }
    registries
        .commodity_handbook_entry(commodity)
        .unwrap_or_else(|| panic!("power-provider availability requested unknown commodity"))
        .sources()
        .iter()
        .filter_map(|source| {
            let CommoditySource::ManualCraft { process, .. } = *source else {
                return None;
            };
            registries.crafting().get_manual(process)
        })
        .filter(|producer| {
            registries
                .process_topology(producer.process())
                .unwrap_or_else(|| panic!("manual producer lost process topology"))
                .equipment_role()
                != ProcessEquipmentRole::Required
        })
        .any(|producer| roots.contains(&producer.input()))
}

fn reachable_mechanical_power_providers(
    registries: &Registries,
    roots: impl IntoIterator<Item = CommodityKey>,
) -> BTreeSet<(
    deep_hearth::labor::ManualPowerMethodId,
    deep_hearth::equipment::EquipmentDefinitionId,
)> {
    let roots = roots.into_iter().collect::<BTreeSet<_>>();
    let methods = registries
        .labor()
        .manual_power_definitions()
        .filter(|method| method.carrier() == EnergyCarrier::Mechanical)
        .collect::<Vec<_>>();
    let mut providers = BTreeSet::new();
    for method in methods {
        for equipment in registries.equipment().definitions() {
            if equipment.requires_structural_support() {
                continue;
            }
            if !matches!(
                equipment
                    .capabilities()
                    .get_capability(method.power_capability()),
                Some(CapabilityValue::Power(power)) if !power.is_zero()
            ) {
                continue;
            }
            let Some(assembly) = equipment.assembly_profile() else {
                continue;
            };
            if assembly
                .inputs()
                .iter()
                .all(|input| commodity_reachable_from_roots(registries, input.commodity(), &roots))
            {
                providers.insert((method.id(), equipment.id()));
            }
        }
    }
    providers
}

#[test]
fn played_power_provider_sets_match_current_buildable_mechanical_content() {
    let registries = deep_hearth::content::build_registries();
    let primitive_roots = [
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
    ];
    let settlement_roots = [
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
    ];
    let primitive_expected = PrimitivePowerChoice::ALL
        .into_iter()
        .map(|choice| (choice.method(), choice.equipment()))
        .collect::<BTreeSet<_>>();
    let settlement_expected = SettlementPowerChoice::ALL
        .into_iter()
        .map(|choice| (choice.method(), choice.equipment()))
        .collect::<BTreeSet<_>>();

    assert_eq!(
        reachable_mechanical_power_providers(&registries, primitive_roots),
        primitive_expected,
        "primitive played provider set diverged from mechanical equipment buildable from disclosed stone/wood roots"
    );
    assert_eq!(
        reachable_mechanical_power_providers(&registries, settlement_roots),
        settlement_expected,
        "settlement played provider set diverged from mechanical equipment buildable from disclosed stone/wood/native-copper roots"
    );
}

#[test]
fn organic_power_workload_sampling_straddles_the_current_crossover_scale() {
    let registries = deep_hearth::content::build_registries();
    let crusher = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("primitive crusher process disappeared"));
    let saw = registries
        .crafting()
        .get_powered(PROCESS_POWER_SAW_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("settlement saw process disappeared"));
    let bank = registries
        .energy()
        .get_store(ENERGY_TIMBER_FRAME_FLYWHEEL_BANK)
        .unwrap_or_else(|| panic!("settlement flywheel bank disappeared"));
    let saw_mass_per_bank = deep_hearth::energy::calculate_mass_specific_energy_capacity(
        bank.capacity(),
        saw.specific_energy(),
    );

    let store_definition = primitive_accumulator_for_current_crusher(&registries);
    let primitive_store = registries
        .energy()
        .get_store(store_definition)
        .unwrap_or_else(|| panic!("primitive accumulator disappeared"));
    let primitive_mass_per_charge = deep_hearth::energy::calculate_mass_specific_energy_capacity(
        primitive_store.capacity(),
        crusher.specific_energy(),
    );
    let primitive = (1_u64..=64)
        .map(|seed| {
            declared_primitive_crushing_project(&registries, seed, store_definition, Some(8)).0
        })
        .collect::<Vec<_>>();
    let settlement = (1_u64..=64)
        .map(|seed| declared_settlement_lumber_project(&registries, seed, Some(8)).0)
        .collect::<Vec<_>>();

    assert!(
        primitive.iter().all(|mass| mass
            .milligrams()
            .is_multiple_of(primitive_mass_per_charge.milligrams())),
        "primitive organic projects must remain whole current accumulator workloads"
    );
    assert!(
        settlement.iter().all(|mass| mass
            .milligrams()
            .is_multiple_of(saw_mass_per_bank.milligrams())),
        "settlement organic projects must remain whole current flywheel-backed saw workloads"
    );
    assert!(
        primitive
            .iter()
            .map(|mass| mass.milligrams())
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "fresh primitive roots must vary declared productive work"
    );
    assert!(
        settlement
            .iter()
            .map(|mass| mass.milligrams())
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "fresh settlement roots must vary declared productive work"
    );
    let primitive_units = primitive
        .iter()
        .map(|mass| mass.milligrams() / primitive_mass_per_charge.milligrams())
        .collect::<BTreeSet<_>>();
    let settlement_units = settlement
        .iter()
        .map(|mass| mass.milligrams() / saw_mass_per_bank.milligrams())
        .collect::<BTreeSet<_>>();
    assert!(primitive_units.iter().any(|units| *units < 8));
    assert!(primitive_units.iter().any(|units| *units >= 8));
    assert!(settlement_units.iter().any(|units| *units < 8));
    assert!(settlement_units.iter().any(|units| *units >= 8));

    let no_primitive_crossover = (1_u64..=64)
        .map(|seed| {
            declared_primitive_crushing_project(&registries, seed, store_definition, None).0
        })
        .map(|mass| mass.milligrams() / primitive_mass_per_charge.milligrams())
        .collect::<BTreeSet<_>>();
    let no_settlement_crossover = (1_u64..=64)
        .map(|seed| declared_settlement_lumber_project(&registries, seed, None).0)
        .map(|mass| mass.milligrams() / saw_mass_per_bank.milligrams())
        .collect::<BTreeSet<_>>();
    assert!(no_primitive_crossover.len() > 1);
    assert!(no_settlement_crossover.len() > 1);
}
