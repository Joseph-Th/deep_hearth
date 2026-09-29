//! Public built-in content contracts grouped for exact authoring proof on the library-test artifact.

use std::collections::BTreeSet;

use super::super::{
    ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE, ENERGY_ELECTRICAL_BUFFER,
    ENERGY_MECHANICAL_LARGE_DRIVE, ENERGY_MECHANICAL_SMALL_DRIVE,
    ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE, ENERGY_STONE_FLYWHEEL_DRIVE, ENERGY_THERMAL_SINK,
    ENERGY_TIMBER_FLYWHEEL_DRIVE, ENERGY_TIMBER_FRAME_FLYWHEEL_BANK, EQUIPMENT_CASTING_MOLD,
    EQUIPMENT_DRY_SCREEN, EQUIPMENT_ELECTRIC_FURNACE, EQUIPMENT_GRAVITY_SEPARATOR,
    EQUIPMENT_GRINDING_MILL, EQUIPMENT_JAW_CRUSHER, FORM_FOOD, FORM_LOG, FORM_LUMP,
    FORM_NATIVE_METAL, MATERIAL_CLAY, MATERIAL_COPPER, MATERIAL_LEGUMES, MATERIAL_MEAT,
    MATERIAL_STONE, MATERIAL_WOOD, MINING_METHOD_HAND_PICK, build_registries,
};
use crate::core::quantity::{Power, Volume};
use crate::core::time::TickSpan;
use crate::energy::{PowerRemainder, integrate_power};
use crate::material::{CommodityKey, MaterialInputSpec};
use crate::registry::Registries;
use crate::survival::FoodCategory;

#[test]
fn built_in_direct_drinking_uses_a_meaningful_serving_floor() {
    let serving = build_registries()
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume();
    assert!(
        (Volume::from_microliters(150_000)..=Volume::from_microliters(500_000)).contains(&serving),
        "ordinary drinking should use a human-scale cup serving rather than threshold-sipping"
    );
}

fn primitive_commodity_has_root_route(
    registries: &Registries,
    commodity: CommodityKey,
    roots: &BTreeSet<CommodityKey>,
    visiting: &mut BTreeSet<CommodityKey>,
) -> bool {
    if roots.contains(&commodity) {
        return true;
    }
    if !visiting.insert(commodity) {
        return false;
    }
    let manual = registries
        .crafting()
        .definitions()
        .filter(|definition| {
            definition
                .outputs()
                .iter()
                .any(|output| output.commodity() == commodity)
        })
        .any(|producer| {
            primitive_commodity_has_root_route(registries, producer.input(), roots, visiting)
        });
    let casting = !manual
        && registries
            .thermal()
            .casting_definitions()
            .filter(|definition| {
                definition.material() == commodity.material()
                    && definition.solid_form() == commodity.form()
            })
            .any(|producer| {
                primitive_commodity_has_root_route(
                    registries,
                    CommodityKey::new(commodity.material(), producer.liquid_form()),
                    roots,
                    visiting,
                )
            });
    let melting = !manual
        && !casting
        && registries
            .thermal()
            .melting_definitions()
            .filter(|definition| {
                definition.material() == commodity.material()
                    && definition.liquid_form() == commodity.form()
            })
            .any(|producer| {
                producer.solid_forms().iter().any(|form| {
                    primitive_commodity_has_root_route(
                        registries,
                        CommodityKey::new(commodity.material(), *form),
                        roots,
                        visiting,
                    )
                })
            });
    assert!(visiting.remove(&commodity));
    manual || casting || melting
}

#[test]
fn every_declared_primitive_infrastructure_component_has_a_transitive_runtime_route() {
    let registries = build_registries();
    let roots = BTreeSet::from([
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        CommodityKey::new(MATERIAL_CLAY, FORM_LUMP),
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
    ]);
    let mut required = BTreeSet::new();
    for definition in registries.equipment().definitions() {
        if !definition.has_authored_acquisition_edge() {
            continue;
        }
        if let Some(assembly) = definition.assembly_profile() {
            required.extend(assembly.inputs().iter().map(MaterialInputSpec::commodity));
        }
        if let Some(upgrade) = definition.upgrade_profile() {
            let base = registries
                .equipment()
                .get_equipment(upgrade.from())
                .unwrap_or_else(|| unreachable!("validated upgrade base exists"));
            assert!(base.has_authored_acquisition_edge());
            required.extend(
                upgrade
                    .additions()
                    .inputs()
                    .iter()
                    .map(MaterialInputSpec::commodity),
            );
        }
    }
    for definition in registries.energy().definitions() {
        if let Some(assembly) = definition.assembly_profile() {
            required.extend(assembly.inputs().iter().map(MaterialInputSpec::commodity));
        }
    }
    for definition in registries.storage().definitions() {
        required.extend(
            definition
                .assembly_profile()
                .inputs()
                .iter()
                .map(MaterialInputSpec::commodity),
        );
    }
    for commodity in required {
        assert!(
            primitive_commodity_has_root_route(
                &registries,
                commodity,
                &roots,
                &mut BTreeSet::new(),
            ),
            "primitive component commodity {} has no acyclic ordinary route from authored primitive roots",
            commodity.value()
        );
    }
}

#[test]
fn built_in_missing_acquisition_edges_are_exactly_capability_only_infrastructure() {
    let registries = build_registries();
    assert_eq!(
        registries
            .equipment()
            .definitions()
            .filter(|definition| !definition.has_authored_acquisition_edge())
            .map(|definition| definition.id())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            EQUIPMENT_JAW_CRUSHER,
            EQUIPMENT_ELECTRIC_FURNACE,
            EQUIPMENT_CASTING_MOLD,
            EQUIPMENT_DRY_SCREEN,
            EQUIPMENT_GRINDING_MILL,
            EQUIPMENT_GRAVITY_SEPARATOR,
        ])
    );
    assert_eq!(
        registries
            .energy()
            .definitions()
            .filter(|definition| !definition.has_authored_assembly_edge())
            .map(|definition| definition.id())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            ENERGY_MECHANICAL_SMALL_DRIVE,
            ENERGY_MECHANICAL_LARGE_DRIVE,
            ENERGY_ELECTRICAL_BUFFER,
            ENERGY_THERMAL_SINK,
        ])
    );
}

#[test]
fn primitive_flywheel_loses_stored_rotation_without_erasing_short_work_windows() {
    let registries = build_registries();
    for store in [
        ENERGY_TIMBER_FLYWHEEL_DRIVE,
        ENERGY_STONE_FLYWHEEL_DRIVE,
        ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
        ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE,
        ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
    ] {
        let flywheel = registries
            .energy()
            .get_store(store)
            .unwrap_or_else(|| panic!("primitive flywheel definition disappeared"));
        let loss = flywheel.passive_dissipation_power();
        assert!(!loss.is_zero());
        assert!(loss < flywheel.max_input_power() && loss < flywheel.max_output_power());
        let integrated = integrate_power(
            loss,
            TickSpan::new(1),
            registries.core().physical_tick_duration(),
            PowerRemainder::ZERO,
        )
        .unwrap_or_else(|error| panic!("primitive flywheel loss integration failed: {error}"));
        assert_eq!(integrated.remainder(), PowerRemainder::ZERO);
        assert!(!integrated.energy().is_zero());
        let coast_ticks = flywheel.capacity().nanojoules() / integrated.energy().nanojoules();
        let coast_microseconds =
            coast_ticks * u128::from(registries.core().physical_tick_duration().microseconds());
        assert!(
            (7_u128 * 60 * 1_000_000..=13_u128 * 60 * 1_000_000).contains(&coast_microseconds),
            "primitive flywheel must remain a multi-minute work buffer, not long-term storage"
        );
    }
}

#[test]
fn built_in_workshop_energy_buffers_have_coherent_transfer_and_recovery_rates() {
    let registries = build_registries();
    let electrical = registries
        .energy()
        .get_store(ENERGY_ELECTRICAL_BUFFER)
        .unwrap_or_else(|| panic!("electrical buffer disappeared"));
    let thermal = registries
        .energy()
        .get_store(ENERGY_THERMAL_SINK)
        .unwrap_or_else(|| panic!("thermal sink disappeared"));
    assert!(!electrical.max_input_power().is_zero());
    assert_eq!(electrical.max_output_power(), electrical.max_input_power());
    assert!(!thermal.max_input_power().is_zero());
    assert!(!thermal.passive_dissipation_power().is_zero());
    assert!(thermal.passive_dissipation_power() < thermal.max_input_power());
}

#[test]
fn built_in_protein_options_trade_immediate_density_for_storage_resilience() {
    let registries = build_registries();
    let meat = *registries
        .survival()
        .get_food(CommodityKey::new(MATERIAL_MEAT, FORM_FOOD))
        .unwrap_or_else(|| panic!("meat food definition disappeared"));
    let legumes = *registries
        .survival()
        .get_food(CommodityKey::new(MATERIAL_LEGUMES, FORM_FOOD))
        .unwrap_or_else(|| panic!("legume food definition disappeared"));
    assert_eq!(meat.category(), FoodCategory::Protein);
    assert_eq!(legumes.category(), FoodCategory::Protein);
    assert!(meat.dietary_energy() > legumes.dietary_energy());
    assert!(meat.hydration_multiplier_ppm() > 0);
    assert!(meat.hydration_multiplier_ppm() < 1_000_000);
    assert_eq!(legumes.hydration_multiplier_ppm(), 0);
    assert!(legumes.shelf_life() > meat.shelf_life());
}

#[test]
fn hand_mining_exertion_remains_a_sustained_human_workload() {
    let registries = build_registries();
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("hand-mining method disappeared"));
    let total_energy_per_tick = registries
        .survival()
        .physiology()
        .basal_energy_cost_per_tick()
        .checked_add(method.exertion().energy_cost_per_tick())
        .unwrap_or_else(|| panic!("hand-mining metabolic cost overflowed"));
    let sustainable_upper_bound = integrate_power(
        Power::from_microwatts(600_000_000),
        TickSpan::new(1),
        registries.core().physical_tick_duration(),
        PowerRemainder::ZERO,
    )
    .unwrap_or_else(|error| panic!("human workload power integration failed: {error}"));
    assert_eq!(sustainable_upper_bound.remainder(), PowerRemainder::ZERO);
    assert!(
        total_energy_per_tick <= sustainable_upper_bound.energy(),
        "sustained hand mining must not require implausible kilowatt-scale human metabolism"
    );
}
