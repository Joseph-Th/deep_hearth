//! Fungal mycelium retains finite biomass through actual seasonal fruiting.

use super::*;
use crate::content::{BLOCK_STATE_STONE, FORM_FOOD, MATERIAL_EDIBLE_FUNGI, build_registries};
use crate::core::state::{AppState, apply_clock_advance, validate_loaded_state};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;
use crate::surface::{
    SurfaceResourceId, SurfaceResourceLifecycle, player_surface_gathering_options,
    validate_start_player_surface_gathering_option,
};
use crate::world::{
    BlockStateId, ChunkCoord, WorldGenerationSeed, validate_generate_chunk,
    validate_generate_player_neighborhood,
};

const FIRST_CROP_END: u64 = FRUIT_SENESCENCE_TICKS;
const NEXT_COOL_FRONT: u64 = FIRST_CROP_END + 800;

fn fungal_world() -> (Registries, AppState, WorldGenerationSeed, SurfaceResourceId) {
    let registries = build_registries();
    let fungus = CommodityKey::new(MATERIAL_EDIBLE_FUNGI, FORM_FOOD);
    for value in 0..80 {
        let seed = WorldGenerationSeed::new(value);
        let mut state = AppState::new();
        validate_generate_chunk(
            &registries,
            &state,
            seed,
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE,
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
        let y = super::super::generation::terrain_height(seed, 0, 0) + 1;
        validate_admit_player(&registries, &state, VoxelCoord::new(0, y, 0))
            .unwrap()
            .commit(&mut state)
            .unwrap();
        validate_generate_player_neighborhood(&registries, &state)
            .unwrap()
            .unwrap()
            .commit(&mut state)
            .unwrap();
        let eligible = state
            .surface()
            .perennial_resources()
            .find(|record| {
                record.commodity() == fungus
                    && (280_150..=299_150).contains(
                        &weather_at(
                            &registries,
                            seed.value(),
                            record.position(),
                            SimulationTick::new(NEXT_COOL_FRONT),
                        )
                        .ambient_temperature()
                        .millikelvin(),
                    )
            })
            .map(|record| record.id());
        if let Some(id) = eligible {
            return (registries, state, seed, id);
        }
    }
    panic!("seeded wilderness should contain a cool-season viable fungal colony");
}

#[test]
fn shaded_moist_fungi_yield_more_without_creating_root_biomass() {
    let fruit = Mass::from_milligrams(200_000);
    let reserve = Mass::from_milligrams(150_000);
    let dry_open = ripened_fungal_crop(reserve, fruit, RootMoisture::NoBasin, 0);
    let shaded = ripened_fungal_crop(reserve, fruit, RootMoisture::NoBasin, 5);
    let watered = ripened_fungal_crop(
        reserve,
        fruit,
        RootMoisture::Basin {
            fraction_ppm: 1_000_000,
        },
        5,
    );
    assert!(dry_open > Mass::ZERO && shaded > dry_open && watered > shaded);
    assert!(watered <= reserve);
    assert_eq!(
        ripened_fungal_crop(Mass::ZERO, fruit, RootMoisture::NoBasin, 5),
        Mass::ZERO,
        "exhausted mycelium cannot yield a free mushroom crop",
    );
    assert_eq!(
        ripened_fungal_crop(Mass::from_milligrams(1), fruit, RootMoisture::NoBasin, 5),
        Mass::from_milligrams(1),
    );
}

#[test]
fn real_fungal_colony_senesces_and_refruits_from_the_same_reserve() {
    let (registries, mut state, seed, id) = fungal_world();
    let source = state.surface().get(id).unwrap();
    let origin = source.position();
    let original = source.perennial.unwrap().original_fruit;
    let total = calculate_matter_accounting(&state).unwrap().total();
    assert_eq!(
        source.initial_mass(),
        original
            .checked_add(Mass::from_milligrams(original.milligrams() / 2))
            .unwrap()
    );
    assert!(super::super::seeded_perennial_matches(
        state.world().generation_profile().unwrap(),
        origin,
        source.commodity(),
        original
    ));
    assert!(!super::super::seeded_perennial_matches(
        state.world().generation_profile().unwrap(),
        origin,
        source.commodity(),
        Mass::from_milligrams(original.milligrams() + 1)
    ));
    assert_eq!(
        ripening_site_shade(
            &registries,
            &state,
            source,
            seed,
            Season::Spring,
            SimulationTick::new(NEXT_COOL_FRONT),
        ),
        None,
        "living first fruit must senesce before a second crop can emerge",
    );

    let first_end = SimulationTick::new(FIRST_CROP_END);
    apply_clock_advance(&mut state, first_end);
    apply_natural_forage_cycle(&registries, &mut state, first_end).unwrap();
    let withered = state.surface().get(id).unwrap();
    assert_eq!(withered.lifecycle(), SurfaceResourceLifecycle::Depleted);
    assert_eq!(withered.remaining_mass(), Mass::ZERO);
    assert_eq!(withered.perennial_root_mass(), withered.initial_mass());
    assert_eq!(withered.depleted_at(), Some(first_end));
    assert!(
        ripening_site_shade(
            &registries,
            &state,
            withered,
            seed,
            Season::Summer,
            SimulationTick::new(NEXT_COOL_FRONT),
        )
        .is_none(),
        "mycelium must not fruit in summer regardless of moisture",
    );
    let shade = ripening_site_shade(
        &registries,
        &state,
        withered,
        seed,
        Season::Spring,
        SimulationTick::new(NEXT_COOL_FRONT),
    )
    .expect("sampled fungal site has real surviving soil and cool weather");
    assert!(shade <= 15);
    let next = SimulationTick::new(NEXT_COOL_FRONT);
    apply_clock_advance(&mut state, next);
    apply_natural_forage_cycle(&registries, &mut state, next).unwrap();
    let ripened = state.surface().get(id).unwrap();
    assert_eq!(ripened.lifecycle(), SurfaceResourceLifecycle::Available);
    assert!(ripened.remaining_mass() > Mass::ZERO);
    assert!(ripened.remaining_mass() < original);
    assert_eq!(
        ripened
            .perennial_root_mass()
            .checked_add(ripened.remaining_mass()),
        Some(ripened.initial_mass()),
    );
    assert_eq!(state.world().block_at(origin), Some(BlockStateId::AIR));
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let serialized = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&serialized)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
}

#[test]
fn first_fungal_crop_already_withered_before_late_discovery() {
    let (registries, early, seed, id) = fungal_world();
    let wanted = early.surface().get(id).unwrap().position();
    let original = early.surface().get(id).unwrap().initial_mass();
    let mut continuous = early.clone();
    let at = SimulationTick::new(FIRST_CROP_END);
    apply_clock_advance(&mut continuous, at);
    apply_natural_forage_cycle(&registries, &mut continuous, at).unwrap();

    let mut late = AppState::new();
    validate_generate_chunk(
        &registries,
        &late,
        seed,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut late)
    .unwrap();
    let y = super::super::generation::terrain_height(seed, 0, 0) + 1;
    validate_admit_player(&registries, &late, VoxelCoord::new(0, y, 0))
        .unwrap()
        .commit(&mut late)
        .unwrap();
    apply_clock_advance(&mut late, at);
    validate_generate_player_neighborhood(&registries, &late)
        .unwrap()
        .unwrap()
        .commit(&mut late)
        .unwrap();
    let delayed = late
        .surface()
        .resources()
        .find(|record| record.position() == wanted)
        .expect("late discovery retains the seeded fungal mycelium identity");
    let current = continuous.surface().get(id).unwrap();
    assert_eq!(delayed.commodity(), current.commodity());
    assert_eq!(delayed.initial_mass(), original);
    assert_eq!(delayed.remaining_mass(), current.remaining_mass());
    assert_eq!(delayed.perennial_root_mass(), current.perennial_root_mass());
    assert_eq!(delayed.lifecycle(), SurfaceResourceLifecycle::Depleted);
    assert_eq!(delayed.depleted_at(), current.depleted_at());
    assert_eq!(delayed.created_at(), SimulationTick::ZERO);
    assert_eq!(validate_loaded_state(&registries, &late), Ok(()));
}

#[test]
fn real_fungal_mycelium_does_not_fruit_in_summer_but_can_in_autumn() {
    let (registries, mut state, seed, id) = fungal_world();
    let initial = calculate_matter_accounting(&state).unwrap().total();
    let first_end = SimulationTick::new(FIRST_CROP_END);
    apply_clock_advance(&mut state, first_end);
    apply_natural_forage_cycle(&registries, &mut state, first_end).unwrap();
    assert_eq!(
        state.surface().get(id).unwrap().lifecycle(),
        SurfaceResourceLifecycle::Depleted
    );
    let calendar = registries.core().calendar();
    let season_span = calendar.ticks_per_day()
        * u64::from(calendar.days_per_month())
        * u64::from(calendar.months_per_year() / 4);
    let summer = SimulationTick::new(season_span);
    assert_eq!(calendar.season_at(summer), Season::Summer);
    apply_clock_advance(&mut state, summer);
    apply_natural_forage_cycle(&registries, &mut state, summer).unwrap();
    assert_eq!(
        state.surface().get(id).unwrap().lifecycle(),
        SurfaceResourceLifecycle::Depleted,
        "summer must not consume fungal mycelium even if warm",
    );
    let source = state.surface().get(id).unwrap();
    let autumn = (0..96_u64)
        .map(|step| SimulationTick::new(2 * season_span + step * FIELD_CHECK_TICKS))
        .find(|&at| {
            (280_150..=299_150).contains(
                &weather_at(&registries, seed.value(), source.position(), at)
                    .ambient_temperature()
                    .millikelvin(),
            )
        })
        .expect("at least one cool autumn front in a sampled natural wet woodland");
    assert_eq!(calendar.season_at(autumn), Season::Autumn);
    apply_clock_advance(&mut state, autumn);
    apply_natural_forage_cycle(&registries, &mut state, autumn).unwrap();
    let source = state.surface().get(id).unwrap();
    assert_eq!(source.lifecycle(), SurfaceResourceLifecycle::Available);
    assert!(source.remaining_mass() > Mass::ZERO);
    assert_eq!(
        source
            .perennial_root_mass()
            .checked_add(source.remaining_mass()),
        Some(source.initial_mass()),
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn forged_fungal_species_or_original_mycelium_yield_fails_trusted_load() {
    let (registries, state, _seed, id) = fungal_world();
    let original = state
        .surface()
        .get(id)
        .unwrap()
        .perennial
        .unwrap()
        .original_fruit;
    let path = id.value().to_string();
    let baseline = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();

    let mut inflated = baseline.clone();
    inflated["state"]["systems"]["surface"]["resources"][&path]["perennial"]["original_fruit"] =
        serde_json::to_value(Mass::from_milligrams(original.milligrams() + 1)).unwrap();
    let forged: LoadedSaveEnvelope = serde_json::from_value(inflated).unwrap();
    assert!(forged.into_state(&registries).is_err());

    let mut converted = baseline;
    converted["state"]["systems"]["surface"]["resources"][&path]["material"]["profile"]["commodity"] =
        serde_json::to_value(CommodityKey::new(
            crate::content::MATERIAL_WILD_TUBERS,
            FORM_FOOD,
        ))
        .unwrap();
    let forged: LoadedSaveEnvelope = serde_json::from_value(converted).unwrap();
    assert!(forged.into_state(&registries).is_err());
}

#[test]
fn independent_fungal_and_shrub_colonies_can_share_a_generated_chunk() {
    let registries = build_registries();
    for value in 0..48 {
        let seed = WorldGenerationSeed::new(value);
        let mut state = AppState::new();
        validate_generate_chunk(
            &registries,
            &state,
            seed,
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE,
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
        let y = super::super::generation::terrain_height(seed, 0, 0) + 1;
        validate_admit_player(&registries, &state, VoxelCoord::new(0, y, 0))
            .unwrap()
            .commit(&mut state)
            .unwrap();
        validate_generate_player_neighborhood(&registries, &state)
            .unwrap()
            .unwrap()
            .commit(&mut state)
            .unwrap();
        let found = state.surface().perennial_resources().any(|fungus| {
            fungus.commodity().material() == MATERIAL_EDIBLE_FUNGI
                && state
                    .surface()
                    .perennial_resources_in_chunk(ChunkCoord::containing(fungus.position()))
                    .any(|shrub| {
                        matches!(
                            shrub.commodity().material(),
                            crate::content::MATERIAL_BERRIES | crate::content::MATERIAL_WILD_NUTS
                        )
                    })
        });
        if found {
            assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
            let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
            let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
                .unwrap()
                .into_state(&registries)
                .unwrap();
            assert_eq!(loaded, state);
            return;
        }
    }
    panic!("independent shrubs and fungal colonies should share some real seeded chunks");
}

#[test]
fn player_harvest_reduces_future_mushroom_fruiting_reserve_without_refilling_food() {
    let (registries, mut state, _seed, id) = fungal_world();
    let source = state.surface().get(id).unwrap();
    let position = source.position();
    let commodity = source.commodity();
    let initial = source.initial_mass();
    let existing = calculate_matter_accounting(&state).unwrap().total();
    crate::logistics::relocate_player_for_test(&mut state, position);
    let option = player_surface_gathering_options(&registries, &state)
        .unwrap()
        .into_iter()
        .find(|candidate| candidate.resource() == id)
        .expect("the real edible colony should be hand-gatherable");
    let harvest = Mass::from_milligrams(25_000);
    validate_start_player_surface_gathering_option(&registries, &state, option, harvest)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    for _ in 0..400 {
        if state.player_work().active().is_none() {
            break;
        }
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert!(state.player_work().active().is_none());
    let carried = state.logistics().player().unwrap().carried_stockpile();
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(commodity),
        harvest,
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        existing
    );
    let senescence = SimulationTick::new(FIRST_CROP_END);
    apply_clock_advance(&mut state, senescence);
    apply_natural_forage_cycle(&registries, &mut state, senescence).unwrap();
    let withered = state.surface().get(id).unwrap();
    assert_eq!(withered.remaining_mass(), Mass::ZERO);
    assert_eq!(
        withered.perennial_root_mass(),
        initial.checked_sub(harvest).unwrap(),
        "harvested mushrooms cannot be returned to the mycelium at senescence",
    );
    let regrowth = SimulationTick::new(NEXT_COOL_FRONT);
    apply_clock_advance(&mut state, regrowth);
    apply_natural_forage_cycle(&registries, &mut state, regrowth).unwrap();
    let crop = state.surface().get(id).unwrap();
    assert_eq!(crop.lifecycle(), SurfaceResourceLifecycle::Available);
    assert_eq!(
        crop.perennial_root_mass()
            .checked_add(crop.remaining_mass()),
        initial.checked_sub(harvest),
        "future fruiting cannot recover consumed or carried food",
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        existing
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
