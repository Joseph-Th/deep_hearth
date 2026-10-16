//! Ordinary generated-trail acquisition for the primitive liberation extension.

use deep_hearth::content::{
    FORM_FOOD, FORM_LOG, FORM_LUMP, MATERIAL_BERRIES, MATERIAL_STONE, MATERIAL_WOOD,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::inventory::StockpileId;
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::Registries;
use deep_hearth::spatial::VoxelCoord;
use deep_hearth::surface::available_adjacent_surface_resources;
use deep_hearth::survival::available_adjacent_drink_sources;

use super::super::super::exact_local_runtime::walk_continuous_body_adjacent_and_settle;
use super::gather_surface_resource;

#[derive(Clone, Copy)]
pub(super) struct GeneratedRawAcquisition {
    pub(super) travel_ticks: u64,
    pub(super) gathering_ticks: u64,
}

fn move_player(
    registries: &Registries,
    state: &mut AppState,
    destination: VoxelCoord,
    context: &'static str,
) -> u64 {
    let started_at = state.tick().value();
    let control_ticks =
        walk_continuous_body_adjacent_and_settle(registries, state, destination, context);
    assert_eq!(
        state.logistics().player().map(|player| player.position()),
        Some(destination),
        "liberation {context} movement did not update authoritative player locality"
    );
    assert!(
        control_ticks > 0,
        "liberation {context} continuous-body travel must occupy control time"
    );
    let elapsed = state
        .tick()
        .value()
        .checked_sub(started_at)
        .unwrap_or_else(|| panic!("liberation {context} movement completion precedes start"));
    assert_eq!(
        elapsed, 1,
        "one bounded opening edge should settle within one slow-tick attention interval"
    );
    elapsed
}

pub(super) fn acquire_generated_opening_raw(
    registries: &Registries,
    state: &mut AppState,
    destination: StockpileId,
    stone_required: Mass,
    wood_required: Mass,
) -> GeneratedRawAcquisition {
    let spawn = state
        .logistics()
        .player()
        .map(|player| player.position())
        .unwrap_or_else(|| panic!("liberation generated raw acquisition requires admitted player"));
    let mut travel_ticks = 0_u64;
    let mut gathering_ticks = 0_u64;

    let forage = available_adjacent_surface_resources(state)
        .filter(|resource| resource.commodity() == CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD))
        .collect::<Vec<_>>();
    assert_eq!(
        forage.len(),
        1,
        "liberation generated opening must expose exactly one first-step forage landmark"
    );
    travel_ticks = travel_ticks
        .checked_add(move_player(
            registries,
            state,
            forage[0].position(),
            "forage-landmark approach",
        ))
        .unwrap_or_else(|| panic!("liberation outward travel attention overflowed"));
    let forage_position = forage[0].position();

    let water = available_adjacent_drink_sources(registries, state).collect::<Vec<_>>();
    assert_eq!(
        water.len(),
        1,
        "liberation generated opening must expose exactly one optional water branch from forage"
    );
    let water_position = water[0].position();
    let junction = state
        .available_player_steps()
        .filter(|position| *position != spawn && *position != water_position)
        .collect::<Vec<_>>();
    assert_eq!(
        junction.len(),
        1,
        "liberation actor should have one non-water continuation from the forage landmark"
    );
    travel_ticks = travel_ticks
        .checked_add(move_player(
            registries,
            state,
            junction[0],
            "material-junction approach",
        ))
        .unwrap_or_else(|| panic!("liberation outward travel attention overflowed"));
    let junction = junction[0];

    let stone = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let wood = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let mut gathered_stone = false;
    let mut gathered_wood = false;
    let mut candidates = available_adjacent_surface_resources(state)
        .filter(|resource| resource.commodity() == stone || resource.commodity() == wood)
        .collect::<Vec<_>>();
    candidates.sort_by_key(|resource| resource.commodity());
    assert_eq!(
        candidates.len(),
        2,
        "liberation material junction must expose stone and timber as simultaneous actor choices"
    );
    for target in candidates {
        let required = if target.commodity() == stone {
            assert!(
                !gathered_stone,
                "liberation generated trail repeated stone site"
            );
            gathered_stone = true;
            stone_required
        } else {
            assert!(
                !gathered_wood,
                "liberation generated trail repeated timber site"
            );
            gathered_wood = true;
            wood_required
        };
        travel_ticks = travel_ticks
            .checked_add(move_player(
                registries,
                state,
                target.position(),
                "raw-material approach",
            ))
            .unwrap_or_else(|| panic!("liberation outward travel attention overflowed"));
        let local = state
            .available_surface_resources()
            .find(|resource| resource.id() == target.resource())
            .unwrap_or_else(|| {
                panic!("liberation adjacent raw source disappeared after reaching its voxel")
            });
        assert!(
            local.remaining_mass() >= required,
            "ordinary generated opening must contain the full disclosed liberation raw requirement"
        );
        gathering_ticks = gathering_ticks
            .checked_add(gather_surface_resource(
                registries,
                state,
                local.id(),
                destination,
                required,
                if target.commodity() == stone {
                    "stone"
                } else {
                    "fallen timber"
                },
            ))
            .unwrap_or_else(|| panic!("liberation raw gathering attention overflowed"));
        travel_ticks = travel_ticks
            .checked_add(move_player(
                registries,
                state,
                junction,
                "return to material junction",
            ))
            .unwrap_or_else(|| panic!("liberation branch-return travel attention overflowed"));
    }
    assert!(
        gathered_stone && gathered_wood,
        "liberation opening graph must expose both required raw commodities"
    );

    travel_ticks = travel_ticks
        .checked_add(move_player(
            registries,
            state,
            forage_position,
            "loaded return through forage landmark",
        ))
        .and_then(|ticks| {
            ticks.checked_add(move_player(
                registries,
                state,
                spawn,
                "loaded return to workshop",
            ))
        })
        .unwrap_or_else(|| panic!("liberation return travel attention overflowed"));
    assert_eq!(
        state.logistics().player().map(|player| player.position()),
        Some(spawn),
        "liberation raw acquisition must return to the inherited workshop before fabrication"
    );
    GeneratedRawAcquisition {
        travel_ticks,
        gathering_ticks,
    }
}
