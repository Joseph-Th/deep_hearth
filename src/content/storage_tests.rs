//! Built-in preservation-storage tradeoff, recovery, and anti-arbitrage contracts.

use std::collections::BTreeMap;

use super::*;
use crate::content::{
    FORM_BOARD, FORM_BULK_CRATE_BODY, FORM_CHEST_BODY, FORM_CHIP, FORM_DOUBLE_WALL_CHEST_BODY,
    FORM_INSULATED_PANTRY_BODY, FORM_LOG, FORM_LUMP, FORM_ROUGH_BOX_BODY, FORM_SCRAP,
    FORM_STONE_CROCK_BODY, MATERIAL_STONE, MATERIAL_WOOD, PROCESS_ASSEMBLE_BULK_TIMBER_CRATE,
    PROCESS_ASSEMBLE_DOUBLE_WALL_TIMBER_CHEST, PROCESS_ASSEMBLE_INSULATED_TIMBER_PANTRY,
    PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX, PROCESS_ASSEMBLE_TIMBER_CHEST,
    PROCESS_REKNAP_STONE_SCRAP_TOOL, PROCESS_SALVAGE_BULK_TIMBER_CRATE_BODY,
    PROCESS_SALVAGE_DOUBLE_WALL_TIMBER_CHEST_BODY, PROCESS_SALVAGE_INSULATED_TIMBER_PANTRY_BODY,
    PROCESS_SALVAGE_ROUGH_TIMBER_FIELD_BOX_BODY, PROCESS_SALVAGE_STONE_PROVISIONS_CROCK_BODY,
    PROCESS_SALVAGE_TIMBER_CHEST_BODY, PROCESS_SHAPE_STONE_PROVISIONS_CROCK,
    PROCESS_SHAPE_WOOD_BOARDS, build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::time::TickSpan;
use crate::material::{CommodityKey, MaterialId, MaterialInputSpec};
use crate::registry::Registries;

const RECOVERY_SCALE: u128 = 1_000_000_000_000;

#[test]
fn storage_dismantle_duration_is_derived_from_embodied_assembly_mass() {
    let registry = build_storage_registry();
    let mut definitions = 0_usize;

    for definition in registry.definitions() {
        definitions += 1;
        assert_eq!(
            definition.dismantle_duration(),
            dismantle_duration(definition.assembly_profile().input_mass()),
            "storage {} dismantle duration must derive from its embodied assembly mass",
            definition.id().value()
        );
    }

    assert_eq!(definitions, 6);
}

fn transitive_manual_recovery_upper_bounds(
    registries: &Registries,
    material: MaterialId,
    target: CommodityKey,
) -> BTreeMap<CommodityKey, u128> {
    let definitions = registries
        .crafting()
        .definitions()
        .filter(|definition| definition.input().material() == material)
        .collect::<Vec<_>>();
    let mut recovery = BTreeMap::from([(target, RECOVERY_SCALE)]);

    // Every candidate is a conservative upper bound on the fraction of one input commodity that
    // can become the target through same-material manual crafting. One pass extends paths by one
    // craft edge. A useful lossless cycle can be removed from a best path, so more passes than
    // there are definitions cannot reveal a new full-recovery route.
    for _ in 0..=definitions.len() {
        let previous = recovery.clone();
        let mut changed = false;
        for definition in &definitions {
            let denominator = u128::from(definition.input_mass().milligrams());
            let mut candidate = 0_u128;
            for output in definition
                .outputs()
                .iter()
                .filter(|output| output.commodity().material() == material)
            {
                let output_recovery = previous.get(&output.commodity()).copied().unwrap_or(0);
                let contribution = u128::from(output.mass().milligrams())
                    .checked_mul(output_recovery)
                    .unwrap_or_else(|| panic!("manual recovery upper-bound product overflowed"))
                    .div_ceil(denominator);
                candidate = candidate
                    .checked_add(contribution)
                    .unwrap_or_else(|| panic!("manual recovery upper-bound sum overflowed"));
            }
            candidate = candidate.min(RECOVERY_SCALE);
            let entry = recovery.entry(definition.input()).or_default();
            if candidate > *entry {
                *entry = candidate;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    recovery
}

#[test]
fn built_in_preservation_storage_has_a_complete_material_route_and_legible_component() {
    let registries = build_registries();
    let chest = registries
        .storage()
        .get(STORAGE_TIMBER_PROVISIONS_CHEST)
        .unwrap_or_else(|| panic!("built-in provisions chest definition disappeared"));
    assert_eq!(
        chest.maximum_stockpile_capacity(),
        Mass::from_milligrams(20_000_000)
    );
    assert_eq!(
        chest.storage_profile().preservation_multiplier_ppm(),
        2_000_000
    );
    assert_eq!(
        chest.storage_profile().maximum_temperature(),
        Temperature::from_millikelvin(333_150),
        "ordinary timber provisions storage must not act as high-temperature containment"
    );
    assert_eq!(
        chest.assembly_profile().input_mass(),
        Mass::from_milligrams(2_400_000)
    );
    assert_eq!(chest.assembly_profile().inputs().len(), 1);
    let body = chest.assembly_profile().inputs()[0].commodity();
    assert_eq!(body, CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY));
    assert!(
        registries
            .textures()
            .get_commodity_appearance(body)
            .and_then(|binding| binding.object())
            .is_some(),
        "preservation chest body must remain player-legible"
    );
    let chest_process = registries
        .crafting()
        .manual_producers(body)
        .next()
        .unwrap_or_else(|| panic!("provisions chest body lost its ordinary joinery route"));
    assert_eq!(chest_process.process(), PROCESS_ASSEMBLE_TIMBER_CHEST);
    assert_eq!(
        chest_process.input(),
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)
    );
    assert_eq!(chest_process.input_mass(), Mass::from_milligrams(2_400_000));
    assert_eq!(chest_process.duration(), TickSpan::new(80));
    let board = chest_process.input();
    let board_process = registries
        .crafting()
        .manual_producers(board)
        .next()
        .unwrap_or_else(|| panic!("provisions chest boards lost their ordinary shaping route"));
    assert_eq!(board_process.process(), PROCESS_SHAPE_WOOD_BOARDS);
    assert_eq!(
        board_process.input(),
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG)
    );
}

#[test]
fn double_wall_preservation_trades_more_timber_and_attention_for_slower_food_aging() {
    let registries = build_registries();
    let standard = registries
        .storage()
        .get(STORAGE_TIMBER_PROVISIONS_CHEST)
        .unwrap_or_else(|| panic!("standard timber provisions chest disappeared"));
    let insulated = registries
        .storage()
        .get(STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST)
        .unwrap_or_else(|| panic!("double-wall timber provisions chest disappeared"));

    assert_eq!(
        insulated.maximum_stockpile_capacity(),
        standard.maximum_stockpile_capacity(),
        "stronger preservation must not silently buy more usable storage capacity"
    );
    assert_eq!(
        insulated.storage_profile().maximum_temperature(),
        standard.storage_profile().maximum_temperature(),
        "double timber walls do not create high-temperature containment"
    );
    assert_eq!(
        standard.storage_profile().preservation_multiplier_ppm(),
        2_000_000
    );
    assert_eq!(
        insulated.storage_profile().preservation_multiplier_ppm(),
        3_000_000
    );
    assert!(insulated.assembly_profile().input_mass() > standard.assembly_profile().input_mass());
    assert_eq!(
        insulated.assembly_profile().inputs(),
        &[MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_WOOD, FORM_DOUBLE_WALL_CHEST_BODY),
            Mass::from_milligrams(4_000_000),
        )]
    );

    let standard_joinery = registries
        .crafting()
        .get_manual(PROCESS_ASSEMBLE_TIMBER_CHEST)
        .unwrap_or_else(|| panic!("standard chest joinery disappeared"));
    let insulated_joinery = registries
        .crafting()
        .get_manual(PROCESS_ASSEMBLE_DOUBLE_WALL_TIMBER_CHEST)
        .unwrap_or_else(|| panic!("double-wall chest joinery disappeared"));
    let boards = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("timber board shaping disappeared"));
    let board_output = boards
        .outputs()
        .iter()
        .find(|output| output.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))
        .map(|output| output.mass())
        .unwrap_or_else(|| panic!("board shaping lost board output"));
    let standard_board_batches = standard_joinery
        .input_mass()
        .milligrams()
        .div_ceil(board_output.milligrams());
    let insulated_board_batches = insulated_joinery
        .input_mass()
        .milligrams()
        .div_ceil(board_output.milligrams());
    assert_eq!(standard_board_batches, 3);
    assert_eq!(insulated_board_batches, 5);
    let standard_attention =
        boards.duration().value() * standard_board_batches + standard_joinery.duration().value();
    let insulated_attention =
        boards.duration().value() * insulated_board_batches + insulated_joinery.duration().value();
    assert_eq!(standard_attention, 230);
    assert_eq!(insulated_attention, 370);
    assert!(insulated_attention > standard_attention);
    assert_eq!(
        boards.input_mass().milligrams() * standard_board_batches,
        3_000_000
    );
    assert_eq!(
        boards.input_mass().milligrams() * insulated_board_batches,
        5_000_000
    );
    assert!(
        registries
            .textures()
            .get_commodity_appearance(CommodityKey::new(
                MATERIAL_WOOD,
                FORM_DOUBLE_WALL_CHEST_BODY,
            ))
            .and_then(|binding| binding.object())
            .is_some(),
        "double-wall chest body must remain player-legible"
    );
}

#[test]
fn timber_enclosure_salvage_returns_boards_with_explicit_chip_loss() {
    let registries = build_registries();
    for (process, input_form, input_mass, board_mass, duration) in [
        (
            PROCESS_SALVAGE_TIMBER_CHEST_BODY,
            FORM_CHEST_BODY,
            2_400_000,
            1_600_000,
            70,
        ),
        (
            PROCESS_SALVAGE_ROUGH_TIMBER_FIELD_BOX_BODY,
            FORM_ROUGH_BOX_BODY,
            1_600_000,
            800_000,
            50,
        ),
        (
            PROCESS_SALVAGE_DOUBLE_WALL_TIMBER_CHEST_BODY,
            FORM_DOUBLE_WALL_CHEST_BODY,
            4_000_000,
            3_200_000,
            100,
        ),
        (
            PROCESS_SALVAGE_BULK_TIMBER_CRATE_BODY,
            FORM_BULK_CRATE_BODY,
            3_200_000,
            2_400_000,
            80,
        ),
        (
            PROCESS_SALVAGE_INSULATED_TIMBER_PANTRY_BODY,
            FORM_INSULATED_PANTRY_BODY,
            4_800_000,
            4_000_000,
            120,
        ),
    ] {
        let salvage = registries
            .crafting()
            .get_manual(process)
            .unwrap_or_else(|| panic!("timber enclosure salvage process disappeared"));
        assert_eq!(
            salvage.input(),
            CommodityKey::new(MATERIAL_WOOD, input_form)
        );
        assert_eq!(salvage.input_mass(), Mass::from_milligrams(input_mass));
        assert_eq!(salvage.duration(), TickSpan::new(duration));
        assert_eq!(
            salvage
                .outputs()
                .iter()
                .find(|output| output.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))
                .map(|output| output.mass()),
            Some(Mass::from_milligrams(board_mass))
        );
        assert_eq!(
            salvage
                .outputs()
                .iter()
                .find(|output| output.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_CHIP))
                .map(|output| output.mass()),
            Some(Mass::from_milligrams(800_000))
        );
        assert_eq!(
            salvage
                .outputs()
                .iter()
                .map(|output| output.mass().milligrams())
                .sum::<u64>(),
            input_mass,
            "manual enclosure salvage must conserve every milligram"
        );
    }
}

#[test]
fn preservation_storage_spans_bulk_capacity_and_compact_protection_specialists() {
    let registries = build_registries();
    let rough = registries
        .storage()
        .get(STORAGE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap_or_else(|| panic!("rough timber field box disappeared"));
    let bulk = registries
        .storage()
        .get(STORAGE_BULK_TIMBER_PROVISIONS_CRATE)
        .unwrap_or_else(|| panic!("bulk timber provisions crate disappeared"));
    let standard = registries
        .storage()
        .get(STORAGE_TIMBER_PROVISIONS_CHEST)
        .unwrap_or_else(|| panic!("standard timber provisions chest disappeared"));
    let protected = registries
        .storage()
        .get(STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST)
        .unwrap_or_else(|| panic!("double-wall timber provisions chest disappeared"));
    let pantry = registries
        .storage()
        .get(STORAGE_INSULATED_TIMBER_PANTRY)
        .unwrap_or_else(|| panic!("insulated timber pantry disappeared"));
    let crock = registries
        .storage()
        .get(STORAGE_CARVED_STONE_PROVISIONS_CROCK)
        .unwrap_or_else(|| panic!("carved stone provisions crock disappeared"));

    assert_eq!(
        rough.maximum_stockpile_capacity(),
        Mass::from_milligrams(10_000_000)
    );
    assert_eq!(
        bulk.maximum_stockpile_capacity(),
        Mass::from_milligrams(50_000_000)
    );
    assert_eq!(
        standard.maximum_stockpile_capacity(),
        Mass::from_milligrams(20_000_000)
    );
    assert_eq!(
        protected.maximum_stockpile_capacity(),
        Mass::from_milligrams(20_000_000)
    );
    assert_eq!(
        pantry.maximum_stockpile_capacity(),
        Mass::from_milligrams(8_000_000)
    );
    assert_eq!(
        crock.maximum_stockpile_capacity(),
        Mass::from_milligrams(6_000_000)
    );
    assert_eq!(
        rough.storage_profile().preservation_multiplier_ppm(),
        1_750_000
    );
    assert_eq!(
        bulk.storage_profile().preservation_multiplier_ppm(),
        1_500_000
    );
    assert_eq!(
        standard.storage_profile().preservation_multiplier_ppm(),
        2_000_000
    );
    assert_eq!(
        protected.storage_profile().preservation_multiplier_ppm(),
        3_000_000
    );
    assert_eq!(
        pantry.storage_profile().preservation_multiplier_ppm(),
        4_000_000
    );
    assert_eq!(
        crock.storage_profile().preservation_multiplier_ppm(),
        2_500_000
    );

    let rough_joinery = registries
        .crafting()
        .get_manual(PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap_or_else(|| panic!("rough timber field box joinery disappeared"));
    let bulk_joinery = registries
        .crafting()
        .get_manual(PROCESS_ASSEMBLE_BULK_TIMBER_CRATE)
        .unwrap_or_else(|| panic!("bulk timber crate joinery disappeared"));
    let pantry_joinery = registries
        .crafting()
        .get_manual(PROCESS_ASSEMBLE_INSULATED_TIMBER_PANTRY)
        .unwrap_or_else(|| panic!("insulated timber pantry joinery disappeared"));
    let crock_shaping = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_STONE_PROVISIONS_CROCK)
        .unwrap_or_else(|| panic!("stone provisions crock shaping disappeared"));
    let boards = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("timber board shaping disappeared"));
    let board_output = boards
        .outputs()
        .iter()
        .find(|output| output.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))
        .map(|output| output.mass())
        .unwrap_or_else(|| panic!("board shaping lost board output"));
    let rough_batches = rough_joinery
        .input_mass()
        .milligrams()
        .div_ceil(board_output.milligrams());
    let bulk_batches = bulk_joinery
        .input_mass()
        .milligrams()
        .div_ceil(board_output.milligrams());
    let pantry_batches = pantry_joinery
        .input_mass()
        .milligrams()
        .div_ceil(board_output.milligrams());
    assert_eq!(rough_batches, 2);
    assert_eq!(bulk_batches, 4);
    assert_eq!(pantry_batches, 6);
    assert_eq!(
        boards.duration().value() * rough_batches + rough_joinery.duration().value(),
        125
    );
    assert_eq!(
        boards.duration().value() * bulk_batches + bulk_joinery.duration().value(),
        290
    );
    assert_eq!(
        boards.duration().value() * pantry_batches + pantry_joinery.duration().value(),
        440
    );
    assert_eq!(boards.input_mass().milligrams() * rough_batches, 2_000_000);
    assert_eq!(boards.input_mass().milligrams() * bulk_batches, 4_000_000);
    assert_eq!(boards.input_mass().milligrams() * pantry_batches, 6_000_000);
    assert_eq!(
        crock_shaping.input(),
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP)
    );
    assert_eq!(crock_shaping.input_mass(), Mass::from_milligrams(3_000_000));
    assert_eq!(crock_shaping.duration(), TickSpan::new(180));
    assert_eq!(
        crock_shaping
            .outputs()
            .iter()
            .find(|output| {
                output.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_STONE_CROCK_BODY)
            })
            .map(|output| output.mass()),
        Some(Mass::from_milligrams(2_400_000))
    );
    assert_eq!(
        crock_shaping
            .outputs()
            .iter()
            .find(|output| output.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_CHIP))
            .map(|output| output.mass()),
        Some(Mass::from_milligrams(600_000))
    );
    assert!(
        crock
            .assembly_profile()
            .inputs()
            .iter()
            .all(|input| input.commodity().material() == MATERIAL_STONE)
    );
    assert!(crock.maximum_stockpile_capacity() < standard.maximum_stockpile_capacity());
    assert!(
        crock.storage_profile().preservation_multiplier_ppm()
            > standard.storage_profile().preservation_multiplier_ppm()
    );
    assert!(
        crock.storage_profile().preservation_multiplier_ppm()
            < protected.storage_profile().preservation_multiplier_ppm()
    );
    assert!(rough.maximum_stockpile_capacity() < standard.maximum_stockpile_capacity());
    assert!(
        rough.storage_profile().preservation_multiplier_ppm()
            < standard.storage_profile().preservation_multiplier_ppm()
    );
    assert!(
        rough.storage_profile().preservation_multiplier_ppm()
            > bulk.storage_profile().preservation_multiplier_ppm(),
        "rough field box should trade capacity for better short-horizon protection than the slatted bulk crate"
    );
    assert!(rough_joinery.input_mass() < standard.assembly_profile().input_mass());
    assert!(bulk.maximum_stockpile_capacity() > standard.maximum_stockpile_capacity());
    assert!(
        bulk.storage_profile().preservation_multiplier_ppm()
            < standard.storage_profile().preservation_multiplier_ppm()
    );
    assert!(pantry.maximum_stockpile_capacity() < standard.maximum_stockpile_capacity());
    assert!(
        pantry.storage_profile().preservation_multiplier_ppm()
            > protected.storage_profile().preservation_multiplier_ppm()
    );
}

#[test]
fn stone_crock_salvage_returns_exact_reworkable_stone_scrap() {
    let registries = build_registries();
    let salvage = registries
        .crafting()
        .get_manual(PROCESS_SALVAGE_STONE_PROVISIONS_CROCK_BODY)
        .unwrap_or_else(|| panic!("stone crock salvage process disappeared"));
    assert_eq!(
        salvage.input(),
        CommodityKey::new(MATERIAL_STONE, FORM_STONE_CROCK_BODY)
    );
    assert_eq!(salvage.input_mass(), Mass::from_milligrams(2_400_000));
    assert_eq!(salvage.duration(), TickSpan::new(70));
    assert_eq!(salvage.outputs().len(), 1);
    assert_eq!(
        salvage.outputs()[0].commodity(),
        CommodityKey::new(MATERIAL_STONE, FORM_SCRAP)
    );
    assert_eq!(
        salvage.outputs()[0].mass(),
        Mass::from_milligrams(2_400_000)
    );
    assert!(
        registries
            .crafting()
            .manual_consumers(CommodityKey::new(MATERIAL_STONE, FORM_SCRAP))
            .any(|process| process.process() == PROCESS_REKNAP_STONE_SCRAP_TOOL),
        "crock salvage must return stone to the existing rework economy"
    );
}

#[test]
fn preservation_salvage_requires_fresh_primary_input_before_rebuilding_the_same_body() {
    let registries = build_registries();

    for storage in registries.storage().definitions() {
        for body in storage.assembly_profile().inputs() {
            let producers = registries
                .crafting()
                .manual_producers(body.commodity())
                .filter(|producer| {
                    producer.outputs().iter().any(|output| {
                        output.commodity() == body.commodity() && output.mass() == body.mass()
                    })
                })
                .collect::<Vec<_>>();
            assert!(
                !producers.is_empty(),
                "storage body {} has no exact manual construction recipe",
                body.commodity().value()
            );
            let salvage_routes = registries
                .crafting()
                .manual_consumers(body.commodity())
                .filter(|salvage| {
                    salvage.input_mass() == body.mass()
                        && salvage.outputs().iter().all(|output| {
                            output.commodity().material() == body.commodity().material()
                                && output.commodity() != body.commodity()
                        })
                        && salvage
                            .outputs()
                            .iter()
                            .try_fold(Mass::ZERO, |total, output| total.checked_add(output.mass()))
                            == Some(body.mass())
                })
                .collect::<Vec<_>>();
            assert!(
                !salvage_routes.is_empty(),
                "storage body {} has no exact same-material salvage recipe",
                body.commodity().value(),
            );
            for salvage in salvage_routes {
                for producer in &producers {
                    let recovery = transitive_manual_recovery_upper_bounds(
                        &registries,
                        body.commodity().material(),
                        producer.input(),
                    );
                    let recovered_primary_input_mg = salvage
                        .outputs()
                        .iter()
                        .filter(|output| {
                            output.commodity().material() == body.commodity().material()
                        })
                        .map(|output| {
                            let fraction = recovery.get(&output.commodity()).copied().unwrap_or(0);
                            u128::from(output.mass().milligrams())
                                .checked_mul(fraction)
                                .unwrap_or_else(|| {
                                    panic!(
                                        "storage body {} transitive recovery product overflowed",
                                        body.commodity().value()
                                    )
                                })
                                .div_ceil(RECOVERY_SCALE)
                        })
                        .try_fold(0_u128, u128::checked_add)
                        .unwrap_or_else(|| {
                            panic!(
                                "storage body {} transitive recovery sum overflowed",
                                body.commodity().value()
                            )
                        });
                    assert!(
                        recovered_primary_input_mg < u128::from(producer.input_mass().milligrams()),
                        "storage body {} salvage can transitively recover up to {} mg of its {} mg primary construction input; rebuilding must require fresh material",
                        body.commodity().value(),
                        recovered_primary_input_mg,
                        producer.input_mass().milligrams()
                    );
                }
            }
        }
    }
}
