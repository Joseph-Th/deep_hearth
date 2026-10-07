//! Public built-in content contracts grouped for exact authoring proof on the library-test artifact.

use std::collections::{BTreeMap, BTreeSet};

use super::super::{
    ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE, ENERGY_ELECTRICAL_BUFFER,
    ENERGY_MECHANICAL_LARGE_DRIVE, ENERGY_MECHANICAL_SMALL_DRIVE,
    ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE, ENERGY_STONE_FLYWHEEL_DRIVE, ENERGY_THERMAL_SINK,
    ENERGY_TIMBER_FLYWHEEL_DRIVE, ENERGY_TIMBER_FRAME_FLYWHEEL_BANK, EQUIPMENT_CASTING_MOLD,
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_DRY_SCREEN, EQUIPMENT_ELECTRIC_FURNACE,
    EQUIPMENT_GRAVITY_SEPARATOR, EQUIPMENT_GRINDING_MILL, EQUIPMENT_JAW_CRUSHER,
    EQUIPMENT_STONE_DIGGING_SHOVEL, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_STONE_PICK,
    EQUIPMENT_STONE_QUARRY_PICK, EQUIPMENT_STONE_WOODWORKING_ADZE,
    EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH, EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
    EQUIPMENT_TIMBER_SPRING_POLE_LATHE, EQUIPMENT_TIMBER_TREADLE_DRIVE,
    EQUIPMENT_TIMBER_TREADLE_GRINDSTONE, FORM_BOARD, FORM_FOOD, FORM_HANDLE, FORM_LOG, FORM_LUMP,
    FORM_NATIVE_METAL, FORM_TOOL, MATERIAL_BERRIES, MATERIAL_CLAY, MATERIAL_COPPER,
    MATERIAL_LEGUMES, MATERIAL_MEAT, MATERIAL_STONE, MATERIAL_WOOD, MINING_METHOD_HAND_PICK,
    PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX, PROCESS_KNAP_STONE_TOOL,
    PROCESS_SHAPE_STONE_PROVISIONS_CROCK, PROCESS_SHAPE_WOOD_BOARDS, PROCESS_SHAPE_WOOD_HANDLE,
    STORAGE_CARVED_STONE_PROVISIONS_CROCK, STORAGE_ROUGH_TIMBER_FIELD_BOX,
    SURFACE_GATHERING_HAND_COLLECT_CLAY, SURFACE_GATHERING_HAND_COLLECT_STONE,
    SURFACE_GATHERING_HAND_COLLECT_TIMBER, SURFACE_GATHERING_HAND_FORAGE_BERRIES, build_registries,
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
    let stone_gathering = registries
        .labor()
        .get_surface_gathering(SURFACE_GATHERING_HAND_COLLECT_STONE)
        .copied()
        .unwrap_or_else(|| panic!("hand stone collection disappeared"));
    let timber_gathering = registries
        .labor()
        .get_surface_gathering(SURFACE_GATHERING_HAND_COLLECT_TIMBER)
        .copied()
        .unwrap_or_else(|| panic!("hand timber collection disappeared"));
    let gather_ticks = stone_gathering
        .duration_for_mass(loose_stone)
        .and_then(|stone| {
            timber_gathering
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
fn built_in_wilderness_gathering_distinguishes_foraging_from_bulk_material_collection() {
    let registries = build_registries();
    let stone = *registries
        .labor()
        .get_surface_gathering(SURFACE_GATHERING_HAND_COLLECT_STONE)
        .unwrap_or_else(|| panic!("stone collection disappeared"));
    let timber = *registries
        .labor()
        .get_surface_gathering(SURFACE_GATHERING_HAND_COLLECT_TIMBER)
        .unwrap_or_else(|| panic!("timber collection disappeared"));
    let berries = *registries
        .labor()
        .get_surface_gathering(SURFACE_GATHERING_HAND_FORAGE_BERRIES)
        .unwrap_or_else(|| panic!("berry foraging disappeared"));
    let clay = *registries
        .labor()
        .get_surface_gathering(SURFACE_GATHERING_HAND_COLLECT_CLAY)
        .unwrap_or_else(|| panic!("clay collection disappeared"));

    assert_eq!(
        stone.commodity(),
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP)
    );
    assert_eq!(
        timber.commodity(),
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG)
    );
    assert_eq!(
        berries.commodity(),
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD)
    );
    assert_eq!(
        clay.commodity(),
        CommodityKey::new(MATERIAL_CLAY, FORM_LUMP)
    );
    assert!(berries.maximum_batch_mass() < stone.maximum_batch_mass());
    assert!(berries.maximum_batch_mass() < timber.maximum_batch_mass());

    let comparison_mass = Mass::from_milligrams(500_000);
    let stone_ticks = stone
        .duration_for_mass(comparison_mass)
        .unwrap_or_else(|| panic!("stone comparison gather disappeared"));
    let timber_ticks = timber
        .duration_for_mass(comparison_mass)
        .unwrap_or_else(|| panic!("timber comparison gather disappeared"));
    let forage_ticks = berries
        .duration_for_mass(comparison_mass)
        .unwrap_or_else(|| panic!("berry comparison gather disappeared"));
    assert!(timber_ticks > stone_ticks);
    assert!(forage_ticks > timber_ticks);
}

#[test]
fn built_in_wilderness_storage_offers_early_capacity_or_preservation_investments() {
    let registries = build_registries();
    let timber = registries
        .storage()
        .get(STORAGE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap_or_else(|| panic!("rough timber field box disappeared"));
    let stone = registries
        .storage()
        .get(STORAGE_CARVED_STONE_PROVISIONS_CROCK)
        .unwrap_or_else(|| panic!("carved stone provisions crock disappeared"));
    assert!(
        timber.maximum_stockpile_capacity() > stone.maximum_stockpile_capacity(),
        "rough timber storage should remain the higher-capacity early option"
    );
    assert!(
        stone.storage_profile().preservation_multiplier_ppm()
            > timber.storage_profile().preservation_multiplier_ppm(),
        "carved stone storage should repay its smaller capacity with stronger preservation"
    );

    let boards = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("wilderness board shaping disappeared"));
    let field_box = registries
        .crafting()
        .get_manual(PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap_or_else(|| panic!("rough field-box joinery disappeared"));
    let board_output = boards
        .outputs()
        .iter()
        .find(|output| output.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))
        .map(|output| output.mass())
        .unwrap_or_else(|| panic!("board shaping lost its board output"));
    let timber_batches = field_box
        .input_mass()
        .milligrams()
        .div_ceil(board_output.milligrams());
    let raw_timber = Mass::from_milligrams(
        boards
            .input_mass()
            .milligrams()
            .checked_mul(timber_batches)
            .unwrap_or_else(|| panic!("field-box raw timber requirement overflowed")),
    );
    let timber_gathering = registries
        .labor()
        .get_surface_gathering(SURFACE_GATHERING_HAND_COLLECT_TIMBER)
        .copied()
        .unwrap_or_else(|| panic!("hand timber collection disappeared"));
    let timber_ticks = timber_gathering
        .duration_for_mass(raw_timber)
        .map(|gathering| gathering.value())
        .and_then(|gathering| {
            boards
                .duration()
                .value()
                .checked_mul(timber_batches)
                .and_then(|shaping| gathering.checked_add(shaping))
        })
        .and_then(|subtotal| subtotal.checked_add(field_box.duration().value()))
        .unwrap_or_else(|| panic!("field-box opening attention overflowed"));

    let crock = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_STONE_PROVISIONS_CROCK)
        .unwrap_or_else(|| panic!("stone crock shaping disappeared"));
    let stone_gathering = registries
        .labor()
        .get_surface_gathering(SURFACE_GATHERING_HAND_COLLECT_STONE)
        .copied()
        .unwrap_or_else(|| panic!("hand stone collection disappeared"));
    let stone_ticks = stone_gathering
        .duration_for_mass(crock.input_mass())
        .and_then(|gathering| gathering.value().checked_add(crock.duration().value()))
        .unwrap_or_else(|| panic!("stone-crock opening attention overflowed"));

    let minute = 60_u128 * 1_000_000;
    for (name, ticks) in [
        ("rough timber field box", timber_ticks),
        ("stone crock", stone_ticks),
    ] {
        let physical_microseconds = u128::from(ticks)
            .checked_mul(u128::from(
                registries.core().physical_tick_duration().microseconds(),
            ))
            .unwrap_or_else(|| panic!("{name} physical duration overflowed"));
        assert!(
            (5 * minute..=20 * minute).contains(&physical_microseconds),
            "{name} should remain a meaningful but attainable wilderness storage project"
        );
    }
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
fn maintained_pre_copper_infrastructure_stays_reachable_from_stone_clay_and_timber() {
    let registries = build_registries();
    let roots = BTreeSet::from([
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        CommodityKey::new(MATERIAL_CLAY, FORM_LUMP),
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
    ]);

    for equipment in [
        EQUIPMENT_STONE_PICK,
        EQUIPMENT_STONE_DIGGING_SHOVEL,
        EQUIPMENT_STONE_QUARRY_PICK,
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        EQUIPMENT_STONE_HAND_CRANK,
        EQUIPMENT_TIMBER_SPRING_POLE_LATHE,
        EQUIPMENT_TIMBER_TREADLE_GRINDSTONE,
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
    ] {
        let profile = registries
            .equipment()
            .get_equipment(equipment)
            .and_then(|definition| definition.assembly_profile())
            .unwrap_or_else(|| panic!("maintained pre-copper equipment lost assembly profile"));
        for input in profile.inputs() {
            assert_ne!(
                input.commodity().material(),
                MATERIAL_COPPER,
                "maintained pre-copper equipment {} gained a copper assembly dependency",
                equipment.value()
            );
            assert!(
                primitive_commodity_has_root_route(
                    &registries,
                    input.commodity(),
                    &roots,
                    &mut BTreeSet::new(),
                ),
                "maintained pre-copper equipment {} input {} lost its route from stone/clay/timber roots",
                equipment.value(),
                input.commodity().value()
            );
        }
    }

    let stone_drive = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("pre-copper stone flywheel drive lost its assembly profile"));
    for input in stone_drive.inputs() {
        assert_ne!(input.commodity().material(), MATERIAL_COPPER);
        assert!(primitive_commodity_has_root_route(
            &registries,
            input.commodity(),
            &roots,
            &mut BTreeSet::new(),
        ));
    }
}

#[test]
fn copper_extends_existing_stone_age_specializations_instead_of_creating_the_first_workshop() {
    let registries = build_registries();
    for (base, upgraded) in [
        (EQUIPMENT_STONE_PICK, EQUIPMENT_COPPER_REINFORCED_PICK),
        (
            EQUIPMENT_TIMBER_SPRING_POLE_LATHE,
            EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
        ),
        (
            EQUIPMENT_TIMBER_TREADLE_GRINDSTONE,
            EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH,
        ),
    ] {
        let base_definition = registries
            .equipment()
            .get_equipment(base)
            .unwrap_or_else(|| panic!("pre-copper specialization base disappeared"));
        assert!(
            base_definition
                .assembly_profile()
                .is_some_and(|profile| profile
                    .inputs()
                    .iter()
                    .all(|input| input.commodity().material() != MATERIAL_COPPER)),
            "specialization base {} must remain directly buildable without copper",
            base.value()
        );

        let upgrade = registries
            .equipment()
            .get_equipment(upgraded)
            .and_then(|definition| definition.upgrade_profile())
            .unwrap_or_else(|| panic!("copper-era specialization upgrade disappeared"));
        assert_eq!(
            upgrade.from(),
            base,
            "copper-era specialization {} must extend the already-useful stone-age base {}",
            upgraded.value(),
            base.value()
        );
        assert!(
            upgrade
                .additions()
                .inputs()
                .iter()
                .any(|input| input.commodity().material() == MATERIAL_COPPER),
            "copper-era specialization {} must spend copper for the capability extension",
            upgraded.value()
        );
    }
}

#[test]
fn every_declared_post_wilderness_primitive_infrastructure_component_has_a_transitive_runtime_route()
 {
    let registries = build_registries();
    // This broader primitive-workshop closure deliberately includes already-acquired native copper.
    // It proves the post-wilderness copper graph, not the fresh wilderness starting boundary above.
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
            "post-wilderness primitive component commodity {} has no acyclic ordinary route from authored primitive roots",
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
