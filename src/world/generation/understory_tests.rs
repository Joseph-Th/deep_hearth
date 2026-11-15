//! Habitat-specific edible understory must be physical, finite, and replayable.

use super::*;
use crate::content::{BLOCK_STATE_STONE, FORM_FOOD, build_registries};
use crate::core::state::{AppState, validate_loaded_state};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::surface::{
    player_surface_gathering_options, validate_start_player_surface_gathering_option,
};
use crate::world::{
    WorldGenerationSeed, validate_generate_chunk, validate_generate_player_neighborhood,
};
use std::collections::BTreeSet;

#[test]
fn roots_and_fungi_offer_distinct_real_survival_tradeoffs() {
    use crate::survival::FoodCategory;
    let registries = build_registries();
    let tubers = registries
        .survival()
        .get_food(CommodityKey::new(MATERIAL_WILD_TUBERS, FORM_FOOD))
        .expect("registered tubers must be edible");
    let fungi = registries
        .survival()
        .get_food(CommodityKey::new(MATERIAL_EDIBLE_FUNGI, FORM_FOOD))
        .expect("registered fungi must be edible");
    assert_eq!(tubers.category(), FoodCategory::Grain);
    assert_eq!(fungi.category(), FoodCategory::Protein);
    assert!(tubers.dietary_energy() > fungi.dietary_energy());
    assert!(fungi.hydration_multiplier_ppm() > tubers.hydration_multiplier_ppm());
    assert!(fungi.shelf_life() < tubers.shelf_life());
}

#[test]
fn seeded_understory_has_real_biome_preferences_and_distinct_species() {
    let mut observed = [BTreeSet::new(), BTreeSet::new(), BTreeSet::new()];
    let mut absent = [0_u32; 3];
    let mut biomass = [(u64::MAX, 0_u64); 2];
    let mut locations = BTreeSet::new();
    for seed_value in 0..4 {
        let seed = WorldGenerationSeed::new(seed_value);
        let profile = WorldGenerationProfile::new(seed, BLOCK_STATE_STONE);
        for district_x in -18..18_i64 {
            for district_z in -18..18_i64 {
                let district = ChunkCoord::new(district_x * 2, district_z * 2);
                let hash = district_hash(profile, district);
                let chunk = ChunkCoord::new(
                    district.x() + (hash & 1) as i64,
                    district.z() + ((hash >> 1) & 1) as i64,
                );
                let (_, x, z) = site_origin(profile, chunk).unwrap();
                assert_eq!(ChunkCoord::containing(VoxelCoord::new(x, 0, z)), chunk,);
                assert!(locations.insert((seed_value, x, z)));
                let biome = super::super::habitat_at(seed, x, z);
                let kind = match biome {
                    HabitatKind::DryScrub => 0,
                    HabitatKind::MixedWoodland => 1,
                    HabitatKind::DampGrove => 2,
                };
                match understory_crop(biome, hash) {
                    Some((commodity, mass)) => {
                        assert_eq!(commodity.form(), FORM_FOOD);
                        assert!(mass > Mass::ZERO);
                        observed[kind].insert(commodity.material());
                        let species = if commodity.material() == MATERIAL_WILD_TUBERS {
                            0
                        } else {
                            1
                        };
                        biomass[species].0 = biomass[species].0.min(mass.milligrams());
                        biomass[species].1 = biomass[species].1.max(mass.milligrams());
                    }
                    None => absent[kind] += 1,
                }
            }
        }
    }
    assert!(
        biomass
            .iter()
            .all(|(minimum, maximum)| maximum - minimum >= 100_000),
        "different districts must vary actual available plant biomass",
    );
    assert_eq!(observed[0], BTreeSet::from([MATERIAL_WILD_TUBERS]));
    assert_eq!(
        observed[1],
        BTreeSet::from([MATERIAL_WILD_TUBERS, MATERIAL_EDIBLE_FUNGI]),
        "mixed woodland must support both understory food niches",
    );
    assert_eq!(observed[2], BTreeSet::from([MATERIAL_EDIBLE_FUNGI]));
    assert!(
        absent.iter().all(|count| *count > 0),
        "each biome includes naturally bare undergrowth districts",
    );
}

#[test]
fn natural_understory_is_player_gatherable_conserved_and_survives_save_replay() {
    let registries = build_registries();
    let mut found = None;
    for seed_value in 0..32_u64 {
        let seed = WorldGenerationSeed::new(seed_value);
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
        let spawn = VoxelCoord::new(0, terrain_height(seed, 0, 0) + 1, 0);
        validate_admit_player(&registries, &state, spawn)
            .unwrap()
            .commit(&mut state)
            .unwrap();
        validate_generate_player_neighborhood(&registries, &state)
            .unwrap()
            .unwrap()
            .commit(&mut state)
            .unwrap();
        let observed = state
            .surface()
            .resources()
            .find(|record| {
                matches!(
                    record.commodity().material(),
                    MATERIAL_WILD_TUBERS | MATERIAL_EDIBLE_FUNGI
                )
            })
            .map(|record| (record.id(), record.position(), record.commodity()));
        if let Some((id, position, commodity)) = observed {
            found = Some((state, id, position, commodity));
            break;
        }
    }
    let (mut state, resource_id, position, commodity) =
        found.expect("sampled wilderness must contain a real understory food source");
    let before = state.surface().get(resource_id).unwrap().remaining_mass();
    assert_eq!(state.world().block_at(position), Some(BlockStateId::AIR));
    assert_eq!(
        state.world().block_at(VoxelCoord::new(
            position.x(),
            position.y() - 1,
            position.z()
        )),
        Some(BLOCK_STATE_STONE),
    );
    let total = calculate_matter_accounting(&state).unwrap().total();
    crate::logistics::relocate_player_for_test(&mut state, position);
    let gathering = player_surface_gathering_options(&registries, &state)
        .unwrap()
        .into_iter()
        .find(|option| option.resource() == resource_id)
        .expect("real tubers and fungi must have a production gathering action");
    assert!(gathering.food_value(&registries, &state).unwrap().is_some());
    let harvest = Mass::from_milligrams(25_000);
    validate_start_player_surface_gathering_option(&registries, &state, gathering, harvest)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    for _ in 0..400 {
        if state.player_work().active().is_none() {
            break;
        }
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert!(
        state.player_work().active().is_none(),
        "harvest must finish"
    );
    assert_eq!(
        state.surface().get(resource_id).unwrap().remaining_mass(),
        before.checked_sub(harvest).unwrap(),
    );
    let carried = state.logistics().player().unwrap().carried_stockpile();
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(commodity),
        harvest,
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    if let Some(expansion) = validate_generate_player_neighborhood(&registries, &state).unwrap() {
        expansion.commit(&mut state).unwrap();
    }
    assert_eq!(
        state.surface().get(resource_id).unwrap().remaining_mass(),
        before.checked_sub(harvest).unwrap(),
        "exploration cannot refill the consumed fruiting body or tuber",
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
}
