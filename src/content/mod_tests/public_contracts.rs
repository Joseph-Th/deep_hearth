//! Public built-in content contracts grouped for exact authoring proof on the library-test artifact.

use std::collections::{BTreeMap, BTreeSet};

use super::super::{
    ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE, ENERGY_ELECTRICAL_BUFFER,
    ENERGY_MECHANICAL_LARGE_DRIVE, ENERGY_MECHANICAL_SMALL_DRIVE,
    ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE, ENERGY_STONE_FLYWHEEL_DRIVE, ENERGY_THERMAL_SINK,
    ENERGY_TIMBER_FLYWHEEL_DRIVE, ENERGY_TIMBER_FRAME_FLYWHEEL_BANK, EQUIPMENT_CASTING_MOLD,
    EQUIPMENT_DRY_SCREEN, EQUIPMENT_ELECTRIC_FURNACE, EQUIPMENT_GRAVITY_SEPARATOR,
    EQUIPMENT_GRINDING_MILL, EQUIPMENT_JAW_CRUSHER, EQUIPMENT_STONE_DIGGING_SHOVEL,
    EQUIPMENT_STONE_PICK, EQUIPMENT_STONE_WOODWORKING_ADZE, FORM_FOOD, FORM_HANDLE, FORM_LOG,
    FORM_LUMP, FORM_NATIVE_METAL, FORM_TOOL, MATERIAL_CLAY, MATERIAL_COPPER, MATERIAL_LEGUMES,
    MATERIAL_MEAT, MATERIAL_STONE, MATERIAL_WOOD, MINING_METHOD_HAND_PICK, PROCESS_KNAP_STONE_TOOL,
    PROCESS_SHAPE_WOOD_HANDLE, SURFACE_GATHERING_HAND_SCAVENGE, build_registries,
};
use crate::core::quantity::{Mass, Power, Volume};
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

#[test]
fn built_in_wilderness_stone_toolkit_has_a_real_attention_horizon_before_copper() {
    let registries = build_registries();
    let stone_tool = CommodityKey::new(MATERIAL_STONE, FORM_TOOL);
    let wood_handle = CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE);
    let mut required = BTreeMap::<CommodityKey, Mass>::new();

    // These three tools cover the opening physical jobs that already have authored equipment
    // owners: rock extraction, woodworking, and earthwork. Their construction should remain a
    // meaningful stone-age project rather than a near-instant prelude to copper.
    for equipment in [
        EQUIPMENT_STONE_PICK,
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        EQUIPMENT_STONE_DIGGING_SHOVEL,
    ] {
        let definition = registries
            .equipment()
            .get_equipment(equipment)
            .unwrap_or_else(|| panic!("wilderness stone-tool definition disappeared"));
        let assembly = definition
            .assembly_profile()
            .unwrap_or_else(|| panic!("wilderness stone tool lost its assembly route"));
        for input in assembly.inputs() {
            assert!(
                input.commodity() == stone_tool || input.commodity() == wood_handle,
                "opening stone tool {} gained a non-stone/timber assembly input {}",
                equipment.value(),
                input.commodity().value(),
            );
            let total = required.entry(input.commodity()).or_insert(Mass::ZERO);
            *total = total
                .checked_add(input.mass())
                .unwrap_or_else(|| panic!("wilderness toolkit material requirement overflowed"));
        }
    }

    let knapping = registries
        .crafting()
        .get_manual(PROCESS_KNAP_STONE_TOOL)
        .unwrap_or_else(|| panic!("stone-tool knapping disappeared"));
    let handle_shaping = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_WOOD_HANDLE)
        .unwrap_or_else(|| panic!("wood-handle shaping disappeared"));
    let stone_output = knapping
        .outputs()
        .iter()
        .find(|output| output.commodity() == stone_tool)
        .map(|output| output.mass())
        .unwrap_or_else(|| panic!("stone-tool knapping lost its useful output"));
    let handle_output = handle_shaping
        .outputs()
        .iter()
        .find(|output| output.commodity() == wood_handle)
        .map(|output| output.mass())
        .unwrap_or_else(|| panic!("wood-handle shaping lost its useful output"));
    let stone_batches = required
        .get(&stone_tool)
        .copied()
        .unwrap_or(Mass::ZERO)
        .milligrams()
        .div_ceil(stone_output.milligrams());
    let handle_batches = required
        .get(&wood_handle)
        .copied()
        .unwrap_or(Mass::ZERO)
        .milligrams()
        .div_ceil(handle_output.milligrams());
    let loose_stone = Mass::from_milligrams(
        knapping
            .input_mass()
            .milligrams()
            .checked_mul(stone_batches)
            .unwrap_or_else(|| panic!("wilderness stone gathering requirement overflowed")),
    );
    let loose_wood = Mass::from_milligrams(
        handle_shaping
            .input_mass()
            .milligrams()
            .checked_mul(handle_batches)
            .unwrap_or_else(|| panic!("wilderness wood gathering requirement overflowed")),
    );
    let gathering = registries
        .labor()
        .get_surface_gathering(SURFACE_GATHERING_HAND_SCAVENGE)
        .copied()
        .unwrap_or_else(|| panic!("hand surface gathering disappeared"));
    let gather_ticks = gathering
        .duration_for_mass(loose_stone)
        .and_then(|stone| {
            gathering
                .duration_for_mass(loose_wood)
                .and_then(|wood| stone.value().checked_add(wood.value()))
        })
        .unwrap_or_else(|| panic!("wilderness toolkit no longer fits ordinary hand gathering"));
    let craft_ticks = knapping
        .duration()
        .value()
        .checked_mul(stone_batches)
        .and_then(|stone| {
            handle_shaping
                .duration()
                .value()
                .checked_mul(handle_batches)
                .and_then(|wood| stone.checked_add(wood))
        })
        .unwrap_or_else(|| panic!("wilderness toolkit hand-work duration overflowed"));
    let total_ticks = gather_ticks
        .checked_add(craft_ticks)
        .unwrap_or_else(|| panic!("wilderness toolkit total duration overflowed"));
    let physical_microseconds = u128::from(total_ticks)
        .checked_mul(u128::from(
            registries.core().physical_tick_duration().microseconds(),
        ))
        .unwrap_or_else(|| panic!("wilderness toolkit physical duration overflowed"));
    let minute = 60_u128 * 1_000_000;
    assert!(
        (15 * minute..=45 * minute).contains(&physical_microseconds),
        "gathering and hand-fabricating the basic pick/adze/shovel toolkit should consume a meaningful opening-time horizon before food, fire, shelter, travel, or geology"
    );
}

#[test]
fn built_in_direct_consumption_keeps_one_use_action_at_human_scale() {
    let direct = build_registries()
        .survival()
        .physiology()
        .direct_consumption();
    assert!(
        (Mass::from_milligrams(250_000)..=Mass::from_milligrams(600_000))
            .contains(&direct.maximum_meal_mass()),
        "ordinary eating should use a meal-sized portion rather than a bulk daily refill"
    );
    assert!(
        (Volume::from_microliters(400_000)..=Volume::from_microliters(750_000))
            .contains(&direct.maximum_drink_volume()),
        "ordinary drinking should use a bottle-sized serving rather than a litre-scale refill"
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
