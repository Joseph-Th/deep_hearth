//! Regression coverage for fieldwork planning and tool-choice economics.

use deep_hearth::content::gameplay_fixture::seed_lot;
use deep_hearth::core::time::WorldSeed;
use deep_hearth::survival::initialize_player_survival;

use super::super::environment::ROOM_TEMPERATURE;
use super::super::focused_seeds::FocusedProbeRole;
use super::super::inventory_support::add_solid_stockpile;
use super::extraction::FieldworkStop;
use super::*;

/// Preserves the executed follow-up order when the selected tool's known batch cap applies.
///
/// The actor already knows the selected tool's batch capacity from its planning frame, so it must
/// size the first admitted batch directly instead of probing an oversized request for rejection.
/// The requested mass, not the per-batch tool limit, remains the player goal.
#[test]
fn batch_capped_mining_finishes_the_requested_order() {
    let registries = deep_hearth::content::build_registries();
    for seed in [1, 2, 3] {
        let FieldworkEpisode {
            projected_ticks,
            extraction:
                extraction::FieldworkExtraction {
                    ticks: actual_ticks,
                    batches,
                    stop,
                    adaptation,
                    ..
                },
            ..
        } = run_fieldwork_order(
            &registries,
            FocusedProbeCase::new(seed, None, FocusedProbeRole::ExplicitReplay),
            fieldwork_order(&registries, seed),
        );
        assert_eq!(stop, FieldworkStop::OrderComplete);
        assert!(
            batches > 1,
            "the requested order must outlive its first claim"
        );
        assert_eq!(adaptation, "preparation-plus-order+batch-limit");
        assert_eq!(
            actual_ticks, projected_ticks,
            "wear-adjusted effort must match execution"
        );
    }
}

#[test]
fn preparation_cost_selects_light_tools_for_short_orders() {
    let registries = deep_hearth::content::build_registries();
    for seed in [1, 2, 3] {
        let FieldworkEpisode {
            tool,
            preparation_ticks,
            extraction:
                extraction::FieldworkExtraction {
                    ticks: mining_ticks,
                    batches,
                    stop,
                    ..
                },
            ..
        } = run_fieldwork_order(
            &registries,
            FocusedProbeCase::new(seed, None, FocusedProbeRole::ExplicitReplay),
            short_fieldwork_order(fieldwork_mining_limits(&registries).base_quarry_batch, seed),
        );
        assert!(
            matches!(
                tool,
                EQUIPMENT_STONE_PICK | EQUIPMENT_COPPER_REINFORCED_PICK
            ),
            "short orders should select a light pick rather than paying for a quarry tool"
        );
        assert!(preparation_ticks > mining_ticks);
        assert!(batches > 1);
        assert_eq!(stop, FieldworkStop::OrderComplete);
    }
}

fn fieldwork_planning_fixture(
    registries: &Registries,
    include_copper: bool,
) -> (AppState, StockpileId) {
    let mut state = AppState::new(WorldSeed::new(71));
    let (raw_opportunity, capacity) = fieldwork_raw_opportunity(registries);
    let raw = add_solid_stockpile(&mut state, capacity);
    for (commodity, mass) in raw_opportunity {
        if include_copper || commodity.material() != MATERIAL_COPPER {
            seed_lot(
                registries,
                &mut state,
                raw,
                commodity,
                mass,
                ROOM_TEMPERATURE,
            );
        }
    }
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("fieldwork planning survival failed: {error}"));
    (state, raw)
}

#[test]
fn candidate_frame_respects_visible_hardness_and_finite_copper() {
    let registries = deep_hearth::content::build_registries();
    let limits = fieldwork_mining_limits(&registries);
    let (state, raw) = fieldwork_planning_fixture(&registries, false);
    let before = state.clone();
    let selected = choose_fieldwork_tool(
        &registries,
        &state,
        raw,
        limits.base_quarry_hardness,
        limits.base_quarry_batch,
    )
    .unwrap_or_else(|| panic!("stone route remains available"));
    assert_eq!(selected.tool.target, EQUIPMENT_STONE_PICK);
    assert!(
        matches!(estimate_fieldwork_tool(&registries, &state, raw, FIELDWORK_TOOLS[1],
        limits.reinforced_quarry_hardness, limits.base_quarry_batch),
        Err(FieldworkToolBlocker::RawInput { commodity, .. }) if commodity.material() == MATERIAL_COPPER)
    );
    assert!(
        matches!(estimate_fieldwork_tool(&registries, &state, raw, FIELDWORK_TOOLS[2],
        limits.reinforced_quarry_hardness, limits.base_quarry_batch),
        Err(FieldworkToolBlocker::AcquiredHardness { upper, maximum })
            if upper == limits.reinforced_quarry_hardness && maximum == limits.base_quarry_hardness)
    );
    assert!(
        choose_fieldwork_tool(
            &registries,
            &state,
            raw,
            limits.reinforced_quarry_hardness,
            limits.base_quarry_batch
        )
        .is_none()
    );
    assert_eq!(
        state, before,
        "pre-action comparison must not mutate or execute hypothetical worlds"
    );
}

#[test]
fn heavy_stone_quarry_pick_has_a_pre_copper_bulk_extraction_niche() {
    let registries = deep_hearth::content::build_registries();
    let limits = fieldwork_mining_limits(&registries);
    let (state, raw) = fieldwork_planning_fixture(&registries, false);
    let mut quarry_order = None;

    for batches in [1_u64, 2, 4, 8, 16, 32, 64, 128, 256] {
        let order = multiplied_mass(
            limits.base_quarry_batch,
            batches,
            "stone-quarry niche order",
        );
        let Some(selected) =
            choose_fieldwork_tool(&registries, &state, raw, limits.base_quarry_hardness, order)
        else {
            continue;
        };
        if selected.tool.target == EQUIPMENT_STONE_QUARRY_PICK {
            quarry_order = Some(order);
            break;
        }
    }

    assert!(
        quarry_order.is_some(),
        "authored heavy stone quarry pick is dominated across the bounded pre-copper soft-rock order range"
    );
}

#[test]
fn reinforced_quarry_pick_has_a_bulk_medium_hardness_niche() {
    let registries = deep_hearth::content::build_registries();
    let limits = fieldwork_mining_limits(&registries);
    let (state, raw) = fieldwork_planning_fixture(&registries, true);
    let mut selected_batches = None;

    for batches in [8_u64, 16, 24, 32, 40, 48, 64, 96, 128, 192, 256] {
        let order = multiplied_mass(
            limits.base_quarry_batch,
            batches,
            "reinforced-quarry niche order",
        );
        let Some(selected) = choose_fieldwork_tool(
            &registries,
            &state,
            raw,
            limits.reinforced_quarry_hardness,
            order,
        ) else {
            continue;
        };
        if selected.tool.target == EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK {
            selected_batches = Some(batches);
            break;
        }
    }

    assert!(
        selected_batches.is_some(),
        "reinforced quarry pick is dominated across the bounded medium-hardness bulk order range"
    );
}

#[test]
fn long_order_band_crosses_the_reinforced_quarry_investment_boundary() {
    let registries = deep_hearth::content::build_registries();
    let limits = fieldwork_mining_limits(&registries);
    let (state, raw) = fieldwork_planning_fixture(&registries, true);
    let lower_bulk = multiplied_mass(limits.base_quarry_batch, 32, "lower long-order boundary");
    let upper_bulk = multiplied_mass(limits.base_quarry_batch, 96, "upper long-order boundary");
    let lower = choose_fieldwork_tool(
        &registries,
        &state,
        raw,
        limits.reinforced_quarry_hardness,
        lower_bulk,
    )
    .unwrap_or_else(|| panic!("reinforcement-tier lower bulk order must have a feasible tool"));
    let upper = choose_fieldwork_tool(
        &registries,
        &state,
        raw,
        limits.reinforced_quarry_hardness,
        upper_bulk,
    )
    .unwrap_or_else(|| panic!("reinforcement-tier upper bulk order must have a feasible tool"));

    assert_eq!(lower.tool.target, EQUIPMENT_COPPER_REINFORCED_PICK);
    assert_eq!(
        upper.tool.target, EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
        "the disclosed long-order range must reach a workload where the heavy quarry investment clearly earns its setup cost"
    );
}

#[test]
fn wear_adjusted_order_can_favor_the_lighter_reinforced_tool() {
    let registries = deep_hearth::content::build_registries();
    // An explicit visible work order, not an inference from hidden deposit reserves.
    let order = multiplied_mass(
        fieldwork_mining_limits(&registries).base_quarry_batch,
        40,
        "large-order regression",
    );
    let FieldworkEpisode {
        tool,
        preparation_ticks,
        projected_ticks: projected_order_ticks,
        extraction:
            extraction::FieldworkExtraction {
                ticks: mining_ticks,
                batches,
                condition_after,
                stop,
                ..
            },
        ..
    } = run_fieldwork_with_supply(
        &registries,
        FocusedProbeCase::new(1, None, FocusedProbeRole::ExplicitReplay),
        order,
        order,
    );
    assert_eq!(tool, EQUIPMENT_COPPER_REINFORCED_PICK);
    assert_eq!(stop, FieldworkStop::OrderComplete);
    assert!(mining_ticks > preparation_ticks);
    assert_eq!(mining_ticks, projected_order_ticks);
    let (state, raw) = fieldwork_planning_fixture(&registries, true);
    let quarry = estimate_fieldwork_tool(
        &registries,
        &state,
        raw,
        FIELDWORK_TOOLS[2],
        fieldwork_mining_limits(&registries).base_quarry_hardness,
        order,
    )
    .unwrap_or_else(|error| panic!("quarry comparison failed: {error:?}"));
    assert!(
        preparation_ticks + mining_ticks < quarry.total_ticks(),
        "the selected lighter tool must finish sooner than the old pristine-policy quarry choice"
    );
    assert!(condition_after < deep_hearth::maintenance::Condition::PRISTINE);
    assert!(batches > 1);
}
