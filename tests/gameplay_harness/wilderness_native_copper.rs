//! First usable metal from a finite, surveyed and hand-dressed weathered copper seam.

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
    pub(super) timber: VoxelCoord,
    pub(super) carried: StockpileId,
}

pub(super) struct CopperPayoff {
    pub(super) ore_mined: Mass,
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
    let acquired = first_copper::acquire(
        registries,
        &mut built,
        first_copper::Route {
            junction: route.junction,
            forage: route.forage,
            water: route.water,
            outcrop: route.outcrop,
            stone: route.stone,
            timber: route.timber,
            carried: route.carried,
        },
        Mass::from_milligrams(40_000),
    );
    let copper_collected = acquired.native_recovered;
    assert!(copper_collected >= Mass::from_milligrams(40_000));
    assert!(
        acquired.survey_ticks > 0
            && acquired.tooling_ticks > 0
            && acquired.mining_ticks > 0
            && acquired.dressing_ticks > 0,
        "first metal must incur real stone-tool, survey, mining, and ore-dressing work"
    );
    assert!(
        acquired.hardness_upper_pa <= 500_000_000,
        "the stone-era entry deposit must not deadlock behind the copper pick"
    );
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut built,
        route.stone,
        "stone crock raw material",
    );
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
        ore_mined: acquired.ore_mined,
        copper_collected,
        project_ticks: observation.value() - started.value() - 120,
        preservation_ppm,
        avoided_age_ticks,
        recovered_berries,
    }
}
