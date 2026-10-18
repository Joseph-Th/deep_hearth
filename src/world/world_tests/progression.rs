//! Material progression from actual generated terrain to a second usable primitive tool head.

use super::*;
use crate::content::PROCESS_SORT_AND_KNAP_STONE_CHIP_TOOL;
use crate::crafting::{
    ManualCraftInputMode, player_manual_craft_options, validate_start_player_manual_craft_option,
};
use crate::simulation::advance_tick;
use std::num::NonZeroU64;

#[test]
fn excavated_stone_debris_can_become_a_new_craftable_tool_head() {
    let (registries, mut state, _pick, target) = generated_excavation_fixture();
    let initial_mass = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("debris-tool initial matter audit failed: {error}"))
        .total();
    let carried = state
        .logistics()
        .player()
        .unwrap_or_else(|| panic!("debris-tool player not admitted"))
        .carried_stockpile();
    let chips = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    let heads = CommodityKey::new(MATERIAL_STONE, FORM_TOOL);
    let needed = registries
        .crafting()
        .get_manual(PROCESS_SORT_AND_KNAP_STONE_CHIP_TOOL)
        .unwrap_or_else(|| panic!("excavation debris recipe not registered"))
        .input_mass();
    let mut work_batches = 0_u64;
    while state
        .inventory()
        .get_stockpile(carried)
        .map_or(Mass::ZERO, |stock| stock.get_mass(chips))
        < needed
    {
        let started =
            validate_start_player_block_excavation_active(&registries, &state, positive_x_view())
                .unwrap_or_else(|error| panic!("debris-tool live excavation rejected: {error}"))
                .commit(&mut state)
                .unwrap_or_else(|error| panic!("debris-tool excavation commit failed: {error}"));
        assert_eq!(started.target(), target);
        while state.control_tick() < started.completes_at() {
            let _ = advance_control_tick(&registries, &mut state)
                .unwrap_or_else(|error| panic!("debris-tool excavation tick failed: {error}"));
        }
        work_batches += 1;
        assert!(
            work_batches <= 16,
            "stone extraction cannot supply a basic tool head"
        );
    }
    let chips_before = state
        .inventory()
        .get_stockpile(carried)
        .unwrap_or_else(|| panic!("debris-tool carried stockpile disappeared"))
        .get_mass(chips);
    let option = player_manual_craft_options(&registries, &state)
        .unwrap_or_else(|error| panic!("debris-tool player catalog failed: {error}"))
        .into_iter()
        .find(|option| option.process() == PROCESS_SORT_AND_KNAP_STONE_CHIP_TOOL)
        .unwrap_or_else(|| panic!("mined chips did not unlock the basic tool-head craft"));
    assert!(matches!(
        option.input_mode(),
        ManualCraftInputMode::Automatic(availability)
            if availability.maximum_batches() >= 1
    ));
    let job = validate_start_player_manual_craft_option(
        &registries,
        &state,
        option,
        NonZeroU64::new(1).unwrap_or_else(|| unreachable!()),
        None,
    )
    .unwrap_or_else(|error| panic!("debris-tool craft start rejected: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("debris-tool craft commit rejected: {error}"));
    let finishes = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("debris-tool job disappeared"))
        .completes_at();
    while state.tick() < finishes {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("debris-tool crafting tick failed: {error}"));
    }
    let stock = state
        .inventory()
        .get_stockpile(carried)
        .unwrap_or_else(|| panic!("debris-tool finished stockpile disappeared"));
    assert_eq!(stock.get_mass(heads), Mass::from_milligrams(800_000));
    assert_eq!(
        stock.get_mass(chips),
        chips_before
            .checked_sub(Mass::from_milligrams(800_000))
            .unwrap_or_else(|| panic!("debris-tool chips accounting underflowed")),
        "stone fragments not retained as useful head stock must remain conserved as chips"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("debris-tool final matter audit failed: {error}"))
            .total(),
        initial_mass
    );
    crate::core::state::validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("debris-tool final state invalid: {error}"));
}
