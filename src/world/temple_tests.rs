//! Generated deep sanctuaries are real excavatable geometry and hold visible heavy castes.

use super::*;

use crate::content::{
    BLOCK_STATE_STONE, BLOCK_STATE_TEMPLE_ARCHIVE, BLOCK_STATE_TEMPLE_DRIVE,
    BLOCK_STATE_TEMPLE_IRON_BEARING_HOUSING, BLOCK_STATE_TEMPLE_REFRACTORY_FIXTURE,
    BLOCK_STATE_TEMPLE_SEAL, BLOCK_STATE_TEMPLE_TIN_DATA_PLATE,
    BLOCK_STATE_TEMPLE_ZINC_CORROSION_PANEL, FORM_CHIP, FORM_SCRAP, MATERIAL_COPPER, MATERIAL_IRON,
    MATERIAL_STONE, MATERIAL_TIN, MATERIAL_ZINC, build_registries,
};
use crate::core::state::{AppState, validate_loaded_state};
use crate::kobolds::{KoboldRole, KoboldStature, available_nearby_kobold_scouts};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;

fn site_for_seed(seed: WorldGenerationSeed) -> (ChunkCoord, generation::temple::TempleSite) {
    (0..8)
        .flat_map(|x| (0..8).map(move |z| ChunkCoord::new(x, z)))
        .find_map(|chunk| {
            generation::temple::TempleSite::for_chunk(seed, chunk).map(|site| (chunk, site))
        })
        .expect("each district contains one seeded temple")
}

#[test]
fn four_service_bays_have_accessible_rooms_and_distinct_finite_components() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x0054_454D_504C_4553);
    let (chunk, site) = site_for_seed(seed);
    let mut state = AppState::new();
    validate_generate_chunk(&registries, &state, seed, chunk, BLOCK_STATE_STONE)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let service_bay_components = site.service_bay_components();
    let mut seen = service_bay_components.map(|(_, block)| block);
    seen.sort_unstable();
    let mut expected = [
        BLOCK_STATE_TEMPLE_TIN_DATA_PLATE,
        BLOCK_STATE_TEMPLE_ZINC_CORROSION_PANEL,
        BLOCK_STATE_TEMPLE_IRON_BEARING_HOUSING,
        BLOCK_STATE_TEMPLE_REFRACTORY_FIXTURE,
    ];
    expected.sort_unstable();
    assert_eq!(
        seen, expected,
        "each of the four engineered assemblies must be generated"
    );
    let appearances = service_bay_components
        .iter()
        .map(|(_, id)| {
            registries
                .world()
                .block_for_state(*id)
                .unwrap()
                .appearance()
                .unwrap()
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        appearances.len(),
        4,
        "each material-specific assembly must have a distinct block appearance"
    );
    let original_mass = calculate_matter_accounting(&state).unwrap().total();
    for (location, relic) in service_bay_components {
        assert_eq!(state.world().block_at(location), Some(relic));
        assert_eq!(
            state.world().block_at(VoxelCoord::new(
                location.x(),
                location.y() + 1,
                location.z()
            )),
            Some(BlockStateId::AIR),
            "the installed part must have overhead clearance for inspection"
        );
        assert_eq!(
            state.world().block_at(VoxelCoord::new(
                location.x(),
                location.y() - 1,
                location.z()
            )),
            Some(BLOCK_STATE_STONE),
            "an artifact rests on an actual excavatable native floor"
        );
        let closer_z = if location.z() < site.warden().z() - 2 {
            location.z() + 1
        } else {
            location.z() - 1
        };
        let walkway_x = if location.x() < site.warden().x() {
            location.x() + 1
        } else {
            location.x() - 1
        };
        for z in [
            location.z(),
            closer_z,
            closer_z
                + if location.z() < site.warden().z() - 2 {
                    1
                } else {
                    -1
                },
        ] {
            let foot = VoxelCoord::new(walkway_x, location.y(), z);
            assert_eq!(state.world().block_at(foot), Some(BlockStateId::AIR));
            assert_eq!(
                state
                    .world()
                    .block_at(VoxelCoord::new(foot.x(), foot.y() + 1, foot.z())),
                Some(BlockStateId::AIR),
                "service bays need a two-high connected approach"
            );
        }
    }
    let tin = CommodityKey::new(MATERIAL_TIN, FORM_SCRAP);
    let iron = CommodityKey::new(MATERIAL_IRON, FORM_SCRAP);
    let zinc = CommodityKey::new(MATERIAL_ZINC, FORM_SCRAP);
    let stone = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    for (block, mass, fill, fragments) in [
        (BLOCK_STATE_TEMPLE_TIN_DATA_PLATE, 3_655_000, 500, tin),
        (
            BLOCK_STATE_TEMPLE_ZINC_CORROSION_PANEL,
            7_140_000,
            1_000,
            zinc,
        ),
        (
            BLOCK_STATE_TEMPLE_IRON_BEARING_HOUSING,
            7_870_000,
            1_000,
            iron,
        ),
        (
            BLOCK_STATE_TEMPLE_REFRACTORY_FIXTURE,
            530_000_000,
            200_000,
            stone,
        ),
    ] {
        let definition = registries.world().block_for_state(block).unwrap();
        assert_eq!(definition.material_fill_fraction_ppm(), fill);
        assert_eq!(
            definition.commodity().unwrap().material(),
            fragments.material()
        );
        let density = registries
            .materials()
            .get_material(fragments.material())
            .unwrap()
            .properties()
            .density_kg_per_m3();
        assert_eq!(
            mass,
            u64::from(density) * u64::from(fill),
            "component mass must be density times occupied full-cube fraction"
        );
        assert_eq!(
            definition.excavation().unwrap().fragmented_commodity(),
            fragments
        );
        assert_eq!(
            registries
                .world()
                .represented_mass_for_state(block)
                .unwrap()
                .milligrams(),
            mass
        );
    }
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        original_mass
    );
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
    assert_eq!(validate_loaded_state(&registries, &loaded), Ok(()));
}

#[test]
fn seeded_districts_have_different_caste_chapel_orders_without_changing_resident_posts() {
    let mut orders = Vec::new();
    for seed in 1..=20 {
        let seed = WorldGenerationSeed::new(seed);
        let (_, site) = site_for_seed(seed);
        let blocks = site.service_bay_components().map(|(_, block)| block);
        assert_eq!(site.warden().z() - site.archivist().z(), 4);
        if !orders.contains(&blocks) {
            orders.push(blocks);
        }
    }
    assert!(
        orders.len() >= 3,
        "district seeds must vary physical component locations"
    );
}

#[test]
fn old_registry_versions_cannot_reinterpret_temple_metal_assemblies() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x0054_454D_504C_4553);
    let (chunk, _) = site_for_seed(seed);
    let mut state = AppState::new();
    validate_generate_chunk(&registries, &state, seed, chunk, BLOCK_STATE_STONE)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let mut saved = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    saved["registry_schema_version"] = serde_json::json!(140);
    let legacy = serde_json::from_value::<LoadedSaveEnvelope>(saved).unwrap();
    assert!(matches!(
        legacy.into_state(&registries),
        Err(crate::persistence::LoadError::RegistrySchemaMismatch { .. })
    ));
}

#[test]
fn seeded_temple_is_buried_carved_and_matter_backed_with_two_ancient_residents() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x0054_454D_504C_4553);
    let (chunk, site) = site_for_seed(seed);
    let mut state = AppState::new();
    let guardian = site.warden();
    let archivist = site.archivist();
    let center = VoxelCoord::new(guardian.x(), guardian.y(), guardian.z() - 2);

    validate_generate_chunk(&registries, &state, seed, chunk, BLOCK_STATE_STONE)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(state.world().block_at(guardian), Some(BlockStateId::AIR));
    assert_eq!(state.world().block_at(archivist), Some(BlockStateId::AIR));
    assert_eq!(
        state
            .world()
            .block_at(VoxelCoord::new(center.x() - 5, center.y(), center.z())),
        Some(BLOCK_STATE_TEMPLE_ARCHIVE)
    );
    assert_eq!(
        state
            .world()
            .block_at(VoxelCoord::new(center.x() + 5, center.y(), center.z())),
        Some(BLOCK_STATE_TEMPLE_DRIVE)
    );
    assert_eq!(
        state
            .world()
            .block_at(VoxelCoord::new(center.x(), center.y() - 1, center.z())),
        Some(BLOCK_STATE_STONE),
        "the temple retains a supporting floor"
    );
    assert_eq!(
        state
            .world()
            .block_at(VoxelCoord::new(center.x(), center.y() + 3, center.z())),
        Some(BLOCK_STATE_STONE),
        "the temple starts sealed under native rock"
    );
    let scrap = CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP);
    for (block, fill) in [
        (BLOCK_STATE_TEMPLE_ARCHIVE, 1_000_u32),
        (BLOCK_STATE_TEMPLE_DRIVE, 2_000_u32),
        (BLOCK_STATE_TEMPLE_SEAL, 500_u32),
    ] {
        let definition = registries.world().block_for_state(block).unwrap();
        assert_eq!(
            definition.excavation().unwrap().fragmented_commodity(),
            scrap
        );
        assert_eq!(definition.material_fill_fraction_ppm(), fill);
        assert_eq!(
            registries
                .world()
                .represented_mass_for_state(block)
                .unwrap()
                .milligrams(),
            u64::from(fill) * 8_960
        );
    }
    assert_eq!(
        temple_residents_at(&registries, &state, VoxelCoord::new(200, center.y(), 200)),
        None,
        "unvisited regions do not disclose a temple or its inhabitants"
    );
    validate_admit_player(&registries, &state, center)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let sightings = available_nearby_kobold_scouts(&registries, &state).collect::<Vec<_>>();
    assert_eq!(sightings.len(), 2);
    assert!(sightings.iter().any(|seen| {
        seen.role() == KoboldRole::TempleWarden
            && seen.position() == guardian
            && seen.stature() == KoboldStature::Heavy
    }));
    assert!(sightings.iter().any(|seen| {
        seen.role() == KoboldRole::TempleArchivist
            && seen.position() == archivist
            && seen.stature() == KoboldStature::Massive
    }));

    let mut obstructed = state.clone();
    set_generated_block_for_test(
        registries.world(),
        &mut obstructed,
        guardian,
        BLOCK_STATE_STONE,
    );
    let surviving = available_nearby_kobold_scouts(&registries, &obstructed).collect::<Vec<_>>();
    assert_eq!(
        surviving.len(),
        1,
        "one blocked caste must not hide every resident"
    );
    assert_eq!(surviving[0].role(), KoboldRole::TempleArchivist);

    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let save = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&save)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
    assert_eq!(validate_loaded_state(&registries, &loaded), Ok(()));
    assert_eq!(
        calculate_matter_accounting(&loaded).unwrap().total(),
        conserved
    );
}

#[test]
fn complex_has_connected_workshops_a_stepped_approach_and_a_surface_clue() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x0054_454D_504C_4553);
    let (chunk, site) = site_for_seed(seed);
    let center = VoxelCoord::new(site.warden().x(), -19, site.warden().z() - 2);
    let mut world = AppState::new();
    validate_generate_chunk(&registries, &world, seed, chunk, BLOCK_STATE_STONE)
        .unwrap()
        .commit(&mut world)
        .unwrap();
    for offset in -4..=4 {
        let cell = VoxelCoord::new(center.x() + offset, center.y(), center.z());
        assert_eq!(
            world.world().block_at(cell),
            Some(BlockStateId::AIR),
            "both workshop alcoves must connect through physical open corridors"
        );
    }
    let mut previous = VoxelCoord::new(center.x() + 3, center.y(), center.z() + 3);
    for (step, (dx, dz)) in generation::temple::STAIR_PATH.iter().copied().enumerate() {
        let foot = center.y() + 1 + step as i64;
        let cell = VoxelCoord::new(center.x() + dx, foot, center.z() + dz);
        assert_eq!(
            previous.x().abs_diff(cell.x()) + previous.z().abs_diff(cell.z()),
            1,
            "successive steps must be true horizontal neighbors"
        );
        assert_eq!(
            world.world().block_at(cell),
            Some(BlockStateId::AIR),
            "the inner stair must have actual standing clearance"
        );
        assert_eq!(
            world
                .world()
                .block_at(VoxelCoord::new(cell.x(), cell.y() + 1, cell.z())),
            Some(BlockStateId::AIR)
        );
        assert_eq!(
            world
                .world()
                .block_at(VoxelCoord::new(cell.x(), cell.y() - 1, cell.z())),
            Some(BLOCK_STATE_STONE),
            "steps must have a physical stone floor"
        );
        previous = cell;
    }
    let markers = (-5..=5)
        .filter_map(|y| {
            let pos = VoxelCoord::new(previous.x(), y, previous.z());
            (world.world().block_at(pos) == Some(BLOCK_STATE_TEMPLE_SEAL)).then_some(pos)
        })
        .collect::<Vec<_>>();
    assert_eq!(markers.len(), 1, "one legible seal reveals the buried site");
    assert_eq!(
        world.world().block_at(VoxelCoord::new(
            previous.x(),
            markers[0].y() + 1,
            previous.z(),
        )),
        Some(BlockStateId::AIR),
        "the threshold must be exposed, not buried beneath an invisible roof"
    );
    assert_eq!(validate_loaded_state(&registries, &world), Ok(()));
}

#[test]
fn temple_sites_are_district_bounded_and_seed_dependent() {
    let a = site_for_seed(WorldGenerationSeed::new(1));
    let b = site_for_seed(WorldGenerationSeed::new(2));
    assert_ne!(a.1.warden(), b.1.warden());
    for (x_min, z_min) in [(0, 0), (-8, 0), (0, -8), (-8, -8)] {
        for seed in [WorldGenerationSeed::new(1), WorldGenerationSeed::new(2)] {
            let sites = (x_min..x_min + 8)
                .flat_map(|x| (z_min..z_min + 8).map(move |z| ChunkCoord::new(x, z)))
                .filter(|chunk| generation::temple::TempleSite::for_chunk(seed, *chunk).is_some())
                .count();
            assert_eq!(
                sites, 1,
                "every positive or negative district has exactly one site"
            );
        }
    }
}
