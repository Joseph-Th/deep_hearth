//! Real forage acquisition and persistence across generated-world ownership.

use super::*;

use crate::content::{BLOCK_STATE_STONE, FORM_FOOD, build_registries};
use crate::core::quantity::Mass;
use crate::core::state::{AppState, validate_loaded_state};
use crate::core::time::SimulationTick;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::surface::{
    SurfaceResourceKind, player_surface_gathering_options,
    validate_start_player_surface_gathering_option,
};
use crate::world::validate_generate_chunk;

#[test]
fn obstructed_seeded_forage_uses_a_valid_alternate_without_changing_its_seed_identity() {
    // This world has a seed-derived tree at the first candidate shrub site.
    // Occupancy selects a later site; trusted load must recognize that
    // alternative without admitting fabricated locations or biomass.
    let registries = build_registries();
    let seed = super::super::WorldGenerationSeed::new(1293);
    let coord = super::super::ChunkCoord::new(0, 0);
    let profile = super::super::WorldGenerationProfile::new(seed, BLOCK_STATE_STONE);
    let chunk = super::super::build_generated_chunk(registries.world(), profile, coord).unwrap();
    let tree = super::super::super::generated_tree(profile, &chunk).unwrap();
    let candidates = seeded_forage_candidates(profile, coord).collect::<Vec<_>>();
    assert_eq!(candidates.len(), 5);
    assert_eq!(candidates[0].0, tree.root());

    let mut state = AppState::new();
    let bootstrap = super::super::ChunkCoord::new(-1, 0);
    validate_generate_chunk(&registries, &state, seed, bootstrap, BLOCK_STATE_STONE)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let spawn = VoxelCoord::new(-8, super::super::terrain_height(seed, -8, 0) + 1, 0);
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let demand = super::super::validate_generate_player_neighborhood(&registries, &state)
        .unwrap()
        .expect("a neighbor of the admitted player has not yet been generated");
    assert!(demand.chunks().any(|generated| generated.coord() == coord));
    demand.commit(&mut state).unwrap();

    let actual = state
        .surface()
        .perennial_resources_in_chunk(coord)
        .next()
        .expect("the real neighborhood must choose unblocked seeded forage");
    let chosen = candidates
        .iter()
        .find(|(position, _, _)| *position == actual.position())
        .expect("generated patch must use an immutable seed-derived site");
    assert_ne!(chosen.0, tree.root());
    assert_eq!(actual.commodity(), chosen.1);
    assert_eq!(actual.remaining_mass(), chosen.2);
    assert!(seeded_perennial_matches(
        profile, chosen.0, chosen.1, chosen.2
    ));
    assert!(!seeded_perennial_matches(
        profile,
        chosen.0,
        chosen.1,
        chosen.2.checked_add(Mass::from_milligrams(1)).unwrap(),
    ));
    assert!(!seeded_perennial_matches(
        profile,
        VoxelCoord::new(chosen.0.x(), chosen.0.y() + 1, chosen.0.z()),
        chosen.1,
        chosen.2,
    ));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, state);
}

#[test]
fn generated_forage_is_one_local_physical_opportunity_not_an_infinite_refill() {
    let registries = build_registries();
    let seed = super::super::WorldGenerationSeed::new(0x46_4F_52_41_47_45_57);
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        super::super::ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert!(
        state.surface().resources().next().is_none(),
        "terrain-only bootstrap must not preempt opening wilderness"
    );
    let spawn = VoxelCoord::new(0, super::super::terrain_height(seed, 0, 0) + 1, 0);
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let demand = super::super::validate_generate_player_neighborhood(&registries, &state)
        .unwrap()
        .expect("admitted player needs the surrounding wilderness");
    assert_eq!(demand.chunks().count(), 8);
    let _ = demand.commit(&mut state).unwrap();
    let sources = state
        .surface()
        .resources()
        .filter(|record| record.kind() == SurfaceResourceKind::Opportunity)
        .collect::<Vec<_>>();
    assert!(
        !sources.is_empty(),
        "player-neighborhood expansion must reveal finite wild food"
    );
    let patch = sources[0];
    assert_eq!(patch.commodity().form(), FORM_FOOD);
    assert!(patch.remaining_mass() >= Mass::from_milligrams(250_000));
    let position = patch.position();
    let source_id = patch.id();
    let starting = patch.remaining_mass();
    assert_eq!(state.world().block_at(position), Some(BlockStateId::AIR));
    assert_eq!(
        state.world().block_at(VoxelCoord::new(
            position.x(),
            position.y() - 1,
            position.z()
        )),
        Some(BLOCK_STATE_STONE)
    );
    let total_before = calculate_matter_accounting(&state).unwrap().total();

    crate::logistics::relocate_player_for_test(&mut state, position);
    assert!(
        crate::surface::AdjacentSurfaceResource::visible_in_generated_world(&registries, &state)
            .any(|sighting| sighting.resource() == source_id),
        "a natural patch must be locally observable through the production read surface"
    );
    // Look down the generated row from two cells away. Rock placed in
    // the intervening cell must conceal the patch, not advertise its ID.
    let mut visibility = state.clone();
    let middle = VoxelCoord::new(position.x() + 1, position.y(), position.z());
    let observer = VoxelCoord::new(position.x() + 2, position.y(), position.z());
    for cell in [
        middle,
        observer,
        VoxelCoord::new(observer.x(), observer.y() + 1, observer.z()),
    ] {
        crate::world::set_generated_block_for_test(
            registries.world(),
            &mut visibility,
            cell,
            BlockStateId::AIR,
        );
    }
    crate::logistics::relocate_player_for_test(&mut visibility, observer);
    assert!(
        crate::surface::AdjacentSurfaceResource::visible_in_generated_world(
            &registries,
            &visibility
        )
        .any(|sighting| sighting.resource() == source_id),
        "two-cell unobstructed local evidence should be visible"
    );
    crate::world::set_generated_block_for_test(
        registries.world(),
        &mut visibility,
        middle,
        BLOCK_STATE_STONE,
    );
    assert!(
        crate::surface::AdjacentSurfaceResource::visible_in_generated_world(
            &registries,
            &visibility
        )
        .all(|sighting| sighting.resource() != source_id),
        "an intervening opaque block must hide the finite source"
    );
    let option = player_surface_gathering_options(&registries, &state)
        .unwrap()
        .into_iter()
        .find(|option| option.resource() == source_id)
        .expect("the generated patch must admit an ordinary player gathering method");
    let food = option.food_value(&registries, &state).unwrap().unwrap();
    assert!(food.dietary_energy().nanojoules() > 0);
    let batch = Mass::from_milligrams(25_000);
    validate_start_player_surface_gathering_option(&registries, &state, option, batch)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    for _ in 0..300 {
        if state.player_work().active().is_none() {
            break;
        }
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert!(
        state.player_work().active().is_none(),
        "gathering must finish"
    );
    let source = state.surface().get(source_id).unwrap();
    assert_eq!(
        source.remaining_mass(),
        starting.checked_sub(batch).unwrap()
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        total_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, state);
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &restored,
            seed,
            super::super::ChunkCoord::containing(position),
            BLOCK_STATE_STONE
        )
        .err(),
        Some(super::super::ChunkGenerationError::AlreadyGenerated {
            chunk: super::super::ChunkCoord::containing(position)
        }),
        "revisiting the generated chunk cannot restore harvested food"
    );
}

#[test]
fn forage_neighborhood_batch_is_deterministic_and_does_not_reset_age() {
    let registries = build_registries();
    let seed = super::super::WorldGenerationSeed::new(0x464F_5241_4745_434F);
    let generate = |state: &mut AppState| {
        validate_generate_chunk(
            &registries,
            state,
            seed,
            super::super::ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE,
        )
        .unwrap()
        .commit(state)
        .unwrap();
        let spawn = VoxelCoord::new(0, super::super::terrain_height(seed, 0, 0) + 1, 0);
        validate_admit_player(&registries, state, spawn)
            .unwrap()
            .commit(state)
            .unwrap();
        let demand = super::super::validate_generate_player_neighborhood(&registries, state)
            .unwrap()
            .expect("seeded neighborhood must have ungenerated chunks");
        assert_eq!(demand.chunks().count(), 8);
        let _ = demand.commit(state).unwrap();
        assert!(
            super::super::validate_generate_player_neighborhood(&registries, state)
                .unwrap()
                .is_none(),
            "revisiting the same neighborhood must not regenerate the food"
        );
    };
    let mut immediate = AppState::new();
    generate(&mut immediate);
    let observed = |state: &AppState| {
        let mut patches = state
            .surface()
            .resources()
            .filter(|resource| resource.kind() == SurfaceResourceKind::Opportunity)
            .map(|resource| {
                (
                    resource.position(),
                    resource.commodity(),
                    resource.remaining_mass(),
                )
            })
            .collect::<Vec<_>>();
        patches.sort_unstable();
        patches
    };
    assert!(!observed(&immediate).is_empty());
    assert!(calculate_matter_accounting(&immediate).is_ok());
    assert_eq!(validate_loaded_state(&registries, &immediate), Ok(()));

    let mut delayed = AppState::new();
    for _ in 0..100 {
        let _ = advance_tick(&registries, &mut delayed).unwrap();
    }
    assert_eq!(delayed.tick(), SimulationTick::new(100));
    generate(&mut delayed);
    assert_eq!(observed(&immediate), observed(&delayed));
    assert!(
        delayed
            .surface()
            .resources()
            .all(|resource| resource.created_at() == SimulationTick::ZERO)
    );
    assert_eq!(validate_loaded_state(&registries, &delayed), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &delayed)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, delayed);
}
#[test]
fn competing_surface_custody_invalidates_entire_neighborhood_generation() {
    let registries = build_registries();
    let seed = super::super::WorldGenerationSeed::new(0x464F_5241_4745_434F);
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        super::super::ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let spawn = VoxelCoord::new(0, super::super::terrain_height(seed, 0, 0) + 1, 0);
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let demand = super::super::validate_generate_player_neighborhood(&registries, &state)
        .unwrap()
        .expect("the new wilderness needs a generated neighborhood");
    let spec = crate::surface::SurfaceResourceSpec::new(
        SurfaceResourceKind::Opportunity,
        spawn,
        crate::material::CommodityKey::new(crate::content::MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(100_000),
        crate::core::quantity::Temperature::from_millikelvin(293_150),
        crate::material::MaterialComposition::pure(crate::content::MATERIAL_BERRIES),
    )
    .unwrap();
    crate::surface::validate_create_surface_resources(&registries, &state, [spec])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let before = state.clone();
    assert_eq!(
        demand.commit(&mut state),
        Err(
            super::super::ChunkGenerationCommitError::StaleSurfaceRevision {
                expected: 0,
                actual: 1,
            }
        )
    );
    assert_eq!(
        state, before,
        "a stale multi-owner generation must mutate neither owner"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn a_depleted_seeded_patch_with_live_roots_cannot_spawn_duplicate_forage() {
    use crate::surface::{SurfaceResourceLifecycle, validate_create_surface_resources};

    let registries = build_registries();
    let seed = super::super::WorldGenerationSeed::new(0x464F_5241_4745_434F);
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        super::super::ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let profile = state.world().generation_profile().unwrap();
    let (chunk, spec) = (-1..=1)
        .flat_map(|x| (-1..=1).map(move |z| super::super::ChunkCoord::new(x, z)))
        .filter(|coord| *coord != super::super::ChunkCoord::new(0, 0))
        .find_map(|coord| {
            let chunk =
                super::super::build_generated_chunk(registries.world(), profile, coord).unwrap();
            seeded_forage_spec(&registries, &state, profile, &chunk).map(|spec| (chunk, spec))
        })
        .expect("seeded neighborhood contains a candidate perennial site");
    let root = spec.position();
    validate_create_surface_resources(&registries, &state, [spec])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let (id, mut plant, visible) = state
        .surface()
        .resources()
        .map(|record| {
            (
                record.id(),
                record.perennial.unwrap(),
                record.remaining_mass(),
            )
        })
        .next()
        .unwrap();
    plant.root_reserve = plant.root_reserve.checked_add(visible).unwrap();
    let next = state.surface().revision() + 1;
    state.surface_state_mut().apply_perennial_cycle(
        vec![(
            id,
            plant,
            Mass::ZERO,
            SurfaceResourceLifecycle::Depleted,
            Some(SimulationTick::ZERO),
        )],
        next,
    );
    assert!(
        state
            .surface()
            .get(id)
            .unwrap()
            .rooted_biomass_occupies_site()
    );
    // This controlled pre-generation fixture is not a trusted save: the
    // seed-derived root becomes persistent world truth only when its chunk
    // materializes. Retain the root in memory to test duplicate prevention.
    assert_eq!(
        validate_loaded_state(&registries, &state),
        Err(crate::core::state::StateValidationError::Surface(
            crate::surface::SurfaceResourceValidationError::PerennialSiteNotGenerated {
                resource: id,
                position: root,
            },
        )),
    );
    let next_forage = seeded_forage_spec(&registries, &state, profile, &chunk);
    assert!(
        next_forage.is_none(),
        "a prior seeded shrub consumes the district's allocation"
    );

    // Reuse the actual neighborhood generation path to prove the saved root
    // and the newly admitted seed-derived crop never share one voxel.
    let spawn = VoxelCoord::new(0, super::super::terrain_height(seed, 0, 0) + 1, 0);
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let demand = super::super::validate_generate_player_neighborhood(&registries, &state)
        .unwrap()
        .unwrap();
    let _ = demand.commit(&mut state).unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(
        state.surface().get(id).unwrap().position(),
        root,
        "the original rooted plant retains its identity"
    );
    assert_eq!(
        state
            .surface()
            .perennial_resources_in_chunk(chunk.coord())
            .filter(|patch| patch.position() == root)
            .count(),
        1,
        "expansion must not double-create a rooted plant at a depleted site"
    );
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, state, "load must rebuild the same root occupancy");

    // Each alternate candidate is individually consistent with the seed,
    // but the generator may choose only one for the entire district. Forging
    // another persisted root cannot duplicate that finite initial biomass.
    let (other, commodity, original_fruit, duplicate) =
        seeded_forage_candidates(profile, chunk.coord())
            .find_map(|(position, commodity, mass)| {
                if position == root || state.world().block_at(position) != Some(BlockStateId::AIR) {
                    return None;
                }
                let spec = SurfaceResourceSpec::new(
                    SurfaceResourceKind::Opportunity,
                    position,
                    commodity,
                    mass,
                    crate::core::quantity::Temperature::from_millikelvin(293_150),
                    crate::material::MaterialComposition::pure(commodity.material()),
                )
                .unwrap()
                .with_perennial_reserve(Mass::from_milligrams(mass.milligrams() / 2));
                crate::surface::validate_create_surface_resources(&registries, &state, [spec])
                    .ok()
                    .map(|plan| (position, commodity, mass, plan))
            })
            .expect("seeded shrub has a second physically clear candidate");
    assert!(seeded_perennial_matches(
        profile,
        other,
        commodity,
        original_fruit
    ));
    let duplicate_id = crate::surface::SurfaceResourceId::new(state.surface().next_resource_id());
    let mut forged = state.clone();
    duplicate.commit(&mut forged).unwrap();
    let expected = crate::core::state::StateValidationError::Surface(
        crate::surface::SurfaceResourceValidationError::DuplicateSeededPerennial {
            resource: duplicate_id,
            previous: id,
            chunk: chunk.coord(),
        },
    );
    assert_eq!(
        validate_loaded_state(&registries, &forged),
        Err(expected.clone())
    );
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &forged)).unwrap();
    let decoded = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes).unwrap();
    assert_eq!(
        decoded.into_state(&registries),
        Err(crate::persistence::LoadError::InvalidState(expected))
    );
}

#[test]
fn juvenile_transplant_cannot_replace_harvested_seeded_roots() {
    use crate::core::state::StateValidationError;
    use crate::surface::{
        SurfaceResourceLifecycle, SurfaceResourceSpec, SurfaceResourceValidationError,
        validate_create_surface_resources,
    };
    use crate::world::{
        SaplingTransplantCommitError, SaplingTransplantError, validate_transplant_juvenile,
    };

    let registries = build_registries();
    let chunk = super::super::ChunkCoord::new(1, 0);
    // Find a real juvenile and a seed-plausible shrub two steps away on
    // supported level ground; no fabricated tree or terrain is required.
    let (seed, from, to, player_at) = (1..100_000_u64)
        .find_map(|number| {
            let seed = super::super::WorldGenerationSeed::new(number);
            let profile = super::super::WorldGenerationProfile::new(seed, BLOCK_STATE_STONE);
            let tree = crate::world::trees::seeded_tree_identity(profile, chunk)?;
            if tree.initial_wood().milligrams() != 8_000_000 {
                return None;
            }
            let from = tree.root();
            seeded_forage_candidates(profile, chunk).find_map(|(to, _, _)| {
                let two_steps = (from.x().abs_diff(to.x()) == 2 && from.z() == to.z())
                    || (from.x() == to.x() && from.z().abs_diff(to.z()) == 2);
                if !two_steps || to.y() != from.y() {
                    return None;
                }
                let halfway =
                    VoxelCoord::new((from.x() + to.x()) / 2, from.y(), (from.z() + to.z()) / 2);
                (terrain_height(seed, halfway.x(), halfway.z()) + 1 == from.y())
                    .then_some((seed, from, to, halfway))
            })
        })
        .expect("seeded grove has a juvenile beside a rooted site");

    let mut state = AppState::new();
    validate_generate_chunk(&registries, &state, seed, chunk, BLOCK_STATE_STONE)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_admit_player(&registries, &state, player_at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let pending = validate_transplant_juvenile(&registries, &state, from, to).unwrap();
    let profile = state.world().generation_profile().unwrap();
    let (commodity, mass) = seeded_forage_candidates(profile, chunk)
        .find_map(|(site, commodity, mass)| (site == to).then_some((commodity, mass)))
        .expect("the existing plant was selected from this seed's real sites");
    let source = SurfaceResourceSpec::new(
        SurfaceResourceKind::Opportunity,
        to,
        commodity,
        mass,
        crate::core::quantity::Temperature::from_millikelvin(293_150),
        crate::material::MaterialComposition::pure(commodity.material()),
    )
    .unwrap()
    .with_perennial_reserve(Mass::from_milligrams(mass.milligrams() / 2));
    assert!(seeded_perennial_matches(profile, to, commodity, mass));
    let id = crate::surface::SurfaceResourceId::new(state.surface().next_resource_id());
    validate_create_surface_resources(&registries, &state, [source])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let revision = state.surface().revision() + 1;
    state
        .surface_state_mut()
        .apply_gathering(id, mass, SimulationTick::ZERO, revision);
    assert_eq!(
        state.surface().get(id).unwrap().lifecycle(),
        SurfaceResourceLifecycle::Depleted
    );
    assert!(state.surface().rooted_biomass_occupies_voxel(to));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let before = state.clone();
    assert_eq!(
        validate_transplant_juvenile(&registries, &state, from, to).err(),
        Some(SaplingTransplantError::DestinationBlocked { root: to })
    );
    assert_eq!(state, before);
    let mut stale = state.clone();
    assert_eq!(
        pending.commit(&registries, &mut stale),
        Err(SaplingTransplantCommitError::SurfaceChanged),
        "a newly claimed root must invalidate an older transplant token"
    );
    assert_eq!(stale, state);

    // Surface/world trusted-load clearance already detects a solid tree
    // planted through these living roots. Runtime admission must agree.
    let mut forged = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let world = &mut forged["state"]["systems"]["world"];
    world["trees"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|record| record["root"] == serde_json::to_value(from).unwrap())
        .unwrap()["root"] = serde_json::to_value(to).unwrap();
    world["transplants"] = serde_json::json!([{
        "origin": from, "from": from, "to": to,
        "transplanted_at": state.tick(),
    }]);
    assert_eq!(
        serde_json::from_value::<LoadedSaveEnvelope>(forged)
            .unwrap()
            .into_state(&registries),
        Err(crate::persistence::LoadError::InvalidState(
            StateValidationError::Surface(SurfaceResourceValidationError::EmbeddedInSolidWorld {
                resource: id,
                voxel: to
            })
        ))
    );
}
