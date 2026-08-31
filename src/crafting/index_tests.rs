//! Exact reverse-index coverage for immutable built-in manual craft relationships.

use crate::content::{
    FORM_BOARD, MATERIAL_WOOD, PROCESS_ASSEMBLE_DOUBLE_WALL_TIMBER_CHEST,
    PROCESS_ASSEMBLE_TIMBER_CHEST, PROCESS_SALVAGE_DOUBLE_WALL_TIMBER_CHEST_BODY,
    PROCESS_SALVAGE_TIMBER_CHEST_BODY, PROCESS_SHAPE_WOOD_BOARDS, build_registries,
};
use crate::material::CommodityKey;

#[test]
fn manual_craft_reverse_indexes_match_authored_board_edges_in_stable_process_order() {
    let registries = build_registries();
    let boards = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);

    let producers = registries
        .crafting()
        .manual_producers(boards)
        .map(super::ManualCraftDefinition::process)
        .collect::<Vec<_>>();
    assert_eq!(
        producers,
        vec![
            PROCESS_SHAPE_WOOD_BOARDS,
            PROCESS_SALVAGE_TIMBER_CHEST_BODY,
            PROCESS_SALVAGE_DOUBLE_WALL_TIMBER_CHEST_BODY,
        ]
    );

    let consumers = registries
        .crafting()
        .manual_consumers(boards)
        .map(super::ManualCraftDefinition::process)
        .collect::<Vec<_>>();
    assert_eq!(
        consumers,
        vec![
            PROCESS_ASSEMBLE_TIMBER_CHEST,
            PROCESS_ASSEMBLE_DOUBLE_WALL_TIMBER_CHEST,
        ]
    );
}
