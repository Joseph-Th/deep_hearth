//! Exact reverse-index coverage for immutable built-in manual craft relationships.

use crate::content::{FORM_BOARD, MATERIAL_WOOD, build_registries};
use crate::material::CommodityKey;

#[test]
fn manual_craft_reverse_indexes_match_authored_board_edges_in_stable_process_order() {
    let registries = build_registries();
    let boards = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);

    let expected_producers = registries
        .crafting()
        .definitions()
        .filter(|definition| {
            definition
                .outputs()
                .iter()
                .any(|output| output.commodity() == boards)
        })
        .map(super::ManualCraftDefinition::process)
        .collect::<Vec<_>>();
    let producers = registries
        .crafting()
        .manual_producers(boards)
        .map(super::ManualCraftDefinition::process)
        .collect::<Vec<_>>();
    assert_eq!(producers, expected_producers);

    let expected_consumers = registries
        .crafting()
        .definitions()
        .filter(|definition| definition.input() == boards)
        .map(super::ManualCraftDefinition::process)
        .collect::<Vec<_>>();
    let consumers = registries
        .crafting()
        .manual_consumers(boards)
        .map(super::ManualCraftDefinition::process)
        .collect::<Vec<_>>();
    assert_eq!(consumers, expected_consumers);
}
