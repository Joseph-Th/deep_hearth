//! Actor-side material-choice regressions for primitive steady-state progression.

use super::*;
use deep_hearth::content::gameplay_fixture::seed_composed_lot;

#[test]
fn actor_feed_policy_does_not_merge_same_grade_physical_cohorts() {
    let registries = deep_hearth::content::build_registries();
    let mut state = AppState::new();
    let stockpile = add_solid_stockpile(&mut state, Mass::from_milligrams(2_000));
    let ore = CommodityKey::new(MATERIAL_COPPER, FORM_ORE);
    for clay_share_ppm in [250_000, 750_000] {
        let _ = seed_composed_lot(
            &registries,
            &mut state,
            stockpile,
            ore,
            Mass::from_milligrams(400),
            ROOM_TEMPERATURE,
            copper_ore_composition(700_000, clay_share_ppm),
        );
    }
    let _ = seed_composed_lot(
        &registries,
        &mut state,
        stockpile,
        ore,
        Mass::from_milligrams(700),
        ROOM_TEMPERATURE,
        copper_ore_composition(600_000, 500_000),
    );

    assert_eq!(
        steady_state::observed_copper_grade_mass(&state, stockpile, 700_000),
        Mass::from_milligrams(400),
        "same assay does not make physically different gangue cohorts one selectable batch"
    );
    assert_eq!(
        steady_state::best_owned_copper_grade_for_mass(
            &state,
            stockpile,
            Mass::from_milligrams(600),
        ),
        Some(600_000),
        "actor must prefer a feasible homogeneous cohort over an infeasible richer aggregate"
    );
    let selected = steady_state::select_observed_copper_grade(
        &state,
        stockpile,
        Mass::from_milligrams(600),
        600_000,
        "same-grade cohort regression",
    );
    assert_eq!(
        selected.iter().fold(Mass::ZERO, |total, selection| {
            total
                .checked_add(selection.mass())
                .unwrap_or_else(|| panic!("regression selection mass overflowed"))
        }),
        Mass::from_milligrams(600)
    );
}
