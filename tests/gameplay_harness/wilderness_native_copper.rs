//! First usable metal from a finite generated surface float, without a furnished workshop.

use super::*;
use deep_hearth::content::{
    FORM_NATIVE_METAL, FORM_REINFORCEMENT, FORM_STONE_CROCK_BODY, MATERIAL_COPPER,
    STORAGE_COPPER_BANDED_STONE_PROVISIONS_CROCK,
};
use deep_hearth::logistics::validate_quick_move_player_stack;

#[derive(Clone, Copy)]
pub(super) struct CopperRoute {
    pub(super) junction: VoxelCoord,
    pub(super) forage: VoxelCoord,
    pub(super) water: VoxelCoord,
    pub(super) outcrop: VoxelCoord,
    pub(super) stone: VoxelCoord,
    pub(super) carried: StockpileId,
}

pub(super) struct CopperPayoff {
    pub(super) copper_source_before: Mass,
    pub(super) copper_source_after: Mass,
    pub(super) copper_collected: Mass,
    pub(super) project_ticks: u64,
    pub(super) preservation_ppm: u32,
    pub(super) avoided_age_ticks: u128,
    pub(super) recovered_berries: Mass,
}

/// One alternate plan chosen by the same opening actor, not a second spawned
/// inventory. Discovery, trips, timed gathering and material transformation are
/// all executed by the canonical owners. A 40 g metal commitment produces a
/// higher-preservation crock, not a first foundry or a fictitious ingot.
pub(super) fn exercise_native_copper_preservation(
    registries: &Registries,
    source: &AppState,
    route: CopperRoute,
    nuts: Mass,
    conserved: deep_hearth::core::quantity::AggregateMass,
) -> CopperPayoff {
    let mut built = source.clone();
    let started = built.tick();
    let copper = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
    let reinforcement = CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT);
    let stone = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let crock_body = CommodityKey::new(MATERIAL_STONE, FORM_STONE_CROCK_BODY);
    let berries = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let wild_nuts = CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD);
    let copper_collected = Mass::from_milligrams(40_000);
    for (step, label) in [
        (route.forage, "native-copper outcrop fork"),
        (route.water, "native-copper outcrop water branch"),
        (route.outcrop, "native-copper outcrop"),
    ] {
        walk_continuous_body_adjacent_and_settle(registries, &mut built, step, label);
    }
    let copper_source = local_surface_resource(&built, copper);
    let copper_source_before = built
        .available_surface_resources()
        .find(|record| record.id() == copper_source)
        .unwrap_or_else(|| panic!("finite native-copper outcrop disappeared"))
        .remaining_mass();
    assert!(
        copper_source_before >= copper_collected,
        "the player must see enough exposed copper before committing to the metal upgrade"
    );
    assert!(
        built
            .available_surface_resources()
            .any(|record| record.commodity() == CommodityKey::new(MATERIAL_CLAY, FORM_LUMP)),
        "clay and copper must be co-located but independently selectable natural resources"
    );
    // Two different gatherable materials occupy this same outcrop. The
    // unqualified hand action is ambiguous; the real direct-control gesture
    // selects the visible copper source and exactly 40 g, then waits for
    // its slow-boundary admission and timed completion.
    let direction = ViewDirection::new(0, VIEW_DIRECTION_PARTS_PER_MILLION, 0).unwrap();
    let chosen =
        advance_control_tick_with_direct_input(
            registries,
            &mut built,
            PlayerDirectControlInput::NEUTRAL.with_secondary(
                SecondaryControlIntent::gather_surface(copper_source, copper_collected, direction),
            ),
        )
        .unwrap();
    let wait = chosen.queued_action_wait_control_ticks().unwrap();
    assert_eq!(
        chosen.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    let mut copper_work = None;
    for _ in 0..wait {
        let update = advance_control_tick(registries, &mut built).unwrap();
        if let Some(tick) = update.simulation() {
            assert_tick_events_within(
                tick,
                TickEventAllowance::default(),
                "selected copper queue admission",
            );
        }
        if let Some(SecondaryActionOutcome::SurfaceGatheringStarted(work)) =
            update.secondary_action()
        {
            assert!(copper_work.replace(*work).is_none());
        } else {
            assert_eq!(update.secondary_action(), None);
        }
    }
    let work = copper_work.unwrap_or_else(|| panic!("direct copper choice never began work"));
    assert_eq!(work.resource(), copper_source);
    assert_eq!(work.gathered_mass(), copper_collected);
    advance_to(
        registries,
        &mut built,
        work.completes_at(),
        TickEventAllowance {
            surface_resources: &[copper_source],
            ..TickEventAllowance::default()
        },
        "explicit native copper harvest",
    );
    let copper_source_after = built
        .available_surface_resources()
        .find(|record| record.id() == copper_source)
        .unwrap_or_else(|| panic!("copper source identity changed after hand collection"))
        .remaining_mass();
    assert_eq!(
        copper_source_before.checked_sub(copper_collected),
        Some(copper_source_after)
    );
    for (step, label) in [
        (route.water, "copper return to water"),
        (route.forage, "copper return to fork"),
        (route.junction, "copper return to camp"),
        (route.stone, "stone crock raw material"),
    ] {
        walk_continuous_body_adjacent_and_settle(registries, &mut built, step, label);
    }
    let stone_source = local_surface_resource(&built, stone);
    gather_in_batches(
        registries,
        &mut built,
        stone_source,
        Mass::from_milligrams(3_000_000),
        "physical stone crock blank",
    );
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut built,
        route.junction,
        "stone crock return to worksite",
    );
    craft_in_place(
        registries,
        &mut built,
        copper,
        reinforcement,
        2,
        "cold-work native copper into bands",
    );
    craft_in_place(
        registries,
        &mut built,
        stone,
        crock_body,
        1,
        "carve stone vessel body",
    );
    let stored = validate_place_player_storage(
        registries,
        &built,
        STORAGE_COPPER_BANDED_STONE_PROVISIONS_CROCK,
    )
    .unwrap_or_else(|error| panic!("player-crafted copper-banded crock not ready: {error}"))
    .commit(&mut built)
    .unwrap_or_else(|error| panic!("copper-banded crock placement failed: {error}"));
    let preservation_ppm = built
        .inventory()
        .get_stockpile(stored)
        .unwrap_or_else(|| panic!("copper-banded crock lost physical custody"))
        .storage_profile()
        .preservation_multiplier_ppm();
    assert!(preservation_ppm > 1_750_000);
    let berries_mass = Mass::from_milligrams(2_000_000);
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut built,
        route.forage,
        "native-copper vessel fruit expedition",
    );
    let berry_source = local_surface_resource(&built, berries);
    gather_in_batches(
        registries,
        &mut built,
        berry_source,
        berries_mass,
        "native-copper vessel fresh berries",
    );
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut built,
        route.junction,
        "native-copper vessel provisions return",
    );
    for (commodity, mass) in [(wild_nuts, nuts), (berries, berries_mass)] {
        let stack = project_material_stacks(registries, &built, route.carried)
            .unwrap_or_else(|error| panic!("metal vessel carried-food catalog failed: {error}"))
            .into_iter()
            .find(|entry| entry.commodity() == commodity)
            .unwrap_or_else(|| panic!("actual gathered food missing before metal vessel storage"));
        validate_drop_stack_to_ground(registries, &built, stored, &stack, mass)
            .unwrap_or_else(|error| panic!("metal vessel food transfer rejected: {error}"))
            .commit(&mut built)
            .unwrap_or_else(|error| panic!("metal vessel food transfer failed: {error}"));
    }
    let age = |state: &AppState| {
        project_material_stacks(registries, state, stored)
            .unwrap_or_else(|error| panic!("metal vessel age projection failed: {error}"))
            .into_iter()
            .find(|stack| stack.commodity() == berries)
            .unwrap_or_else(|| panic!("stored berries disappeared"))
            .ambient_storage_age_parts()
    };
    let age_before = age(&built);
    let observation = built.tick().checked_add_span(TickSpan::new(120)).unwrap();
    advance_to(
        registries,
        &mut built,
        observation,
        TickEventAllowance::default(),
        "metal vessel preservation observation",
    );
    let age_after = age(&built);
    let avoided_age_ticks = 120_u128 - (age_after - age_before) / 1_000_000;
    assert!(avoided_age_ticks > 0);
    let fresh_stack = project_material_stacks(registries, &built, stored)
        .unwrap()
        .into_iter()
        .find(|stack| stack.commodity() == berries)
        .unwrap();
    let recovered_berries =
        validate_quick_move_player_stack(registries, &built, &fresh_stack, route.carried)
            .unwrap_or_else(|error| panic!("metal vessel recovery rejected: {error}"))
            .commit(&mut built)
            .unwrap_or_else(|error| panic!("metal vessel recovery failed: {error}"))
            .mass();
    assert_eq!(recovered_berries, berries_mass);
    assert_eq!(
        calculate_matter_accounting(&built).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(registries, &built), Ok(()));
    let replay: LoadedSaveEnvelope = serde_json::from_slice(
        &serde_json::to_vec(&SaveEnvelope::new(registries, &built)).unwrap(),
    )
    .unwrap();
    assert_eq!(replay.into_state(registries), Ok(built));
    CopperPayoff {
        copper_source_before,
        copper_source_after,
        copper_collected,
        project_ticks: observation.value() - started.value() - 120,
        preservation_ppm,
        avoided_age_ticks,
        recovered_berries,
    }
}
