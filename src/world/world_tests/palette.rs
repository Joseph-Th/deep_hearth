//! Canonical section compaction across packed bit-width changes.

use super::*;

#[test]
fn compaction_preserves_exact_cells_mass_and_save_representation() {
    let state = BLOCK_STATE_STONE;
    let cell_mass = Mass::from_milligrams(1);
    let mut section = WorldSection::uniform(0, state, cell_mass);
    // Fill a palette past an eight-bit boundary, exercising packed words crossing u64 edges.
    for cell in 1..=257 {
        section.set_state_at_index(
            cell,
            BlockStateId::new(500 + cell as u16),
            cell_mass,
            cell_mass,
        );
    }
    section.canonicalize_palette();
    assert_eq!(section.palette().len(), 258);
    for cell in (1..=257).step_by(2) {
        section.set_state_at_index(cell, state, cell_mass, cell_mass);
    }
    section.canonicalize_palette();
    assert_eq!(section.palette().len(), 129);
    assert_eq!(section.non_air_count(), SECTION_CELL_COUNT as u16);
    assert_eq!(
        section.represented_mass(),
        Mass::from_milligrams(SECTION_CELL_COUNT as u64)
    );
    for cell in 0..SECTION_CELL_COUNT {
        let expected = if cell > 0 && cell <= 257 && cell % 2 == 0 {
            BlockStateId::new(500 + cell as u16)
        } else {
            state
        };
        assert_eq!(section.state_at_index(cell), Some(expected));
    }
    assert_eq!(section.validate_storage(), Ok(()));
    let encoded = serde_json::to_vec(&section)
        .unwrap_or_else(|error| panic!("compacted section save failed: {error}"));
    let decoded: WorldSection = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("compacted section decode failed: {error}"));
    assert_eq!(decoded, section);

    for cell in (2..=257).step_by(2) {
        section.set_state_at_index(cell, state, cell_mass, cell_mass);
    }
    section.canonicalize_palette();
    assert_eq!(section.palette(), &[state]);
    assert_eq!(section.packed_word_count(), 0);
    assert_eq!(section.validate_storage(), Ok(()));
}

#[test]
fn section_validation_rejects_unused_palette_entries_before_compaction() {
    let stone = BLOCK_STATE_STONE;
    let mass = Mass::from_milligrams(1);
    let mut section = WorldSection::uniform(0, stone, mass);
    section.set_state_at_index(0, BlockStateId::AIR, mass, Mass::ZERO);
    section.set_state_at_index(0, stone, Mass::ZERO, mass);

    assert_eq!(
        section.validate_storage(),
        Err(WorldValidationError::UnusedSectionPaletteState {
            section_y: 0,
            state: BlockStateId::AIR,
        })
    );
    section.canonicalize_palette();
    assert_eq!(section.palette(), &[stone]);
    assert_eq!(section.validate_storage(), Ok(()));
}

#[test]
fn homogeneous_section_validation_checks_all_implicit_cells() {
    for (state, cell_mass, count) in [
        (BlockStateId::AIR, Mass::ZERO, 0_u16),
        (
            BLOCK_STATE_STONE,
            Mass::from_milligrams(1),
            SECTION_CELL_COUNT as u16,
        ),
    ] {
        let section = WorldSection::uniform(0, state, cell_mass);
        assert_eq!(section.packed_word_count(), 0);
        assert_eq!(section.validate_storage(), Ok(()));

        let mut forged = serde_json::to_value(&section).unwrap();
        let wrong_count = if count == 0 { 1 } else { count - 1 };
        forged["non_air_count"] = serde_json::json!(wrong_count);
        let invalid: WorldSection = serde_json::from_value(forged).unwrap();
        assert_eq!(
            invalid.validate_storage(),
            Err(WorldValidationError::SectionNonAirCountMismatch {
                section_y: 0,
                stored: wrong_count,
                actual: count,
            })
        );
    }
}

#[test]
fn homogeneous_section_mass_summary_cannot_be_forged_on_load() {
    let registries = build_registries();
    let mut state = AppState::new();
    generate(&mut state, ChunkCoord::new(0, 0));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let sections = encoded["state"]["systems"]["world"]["chunks"][0]["sections"]
        .as_array_mut()
        .unwrap();
    let section = sections
        .iter_mut()
        .find(|section| section["palette"].as_array().is_some_and(|p| p.len() == 1))
        .expect("generated terrain must contain a homogeneous deep section");
    let section_y = section["y"].as_i64().unwrap();
    let actual = serde_json::from_value::<Mass>(section["represented_mass"].clone()).unwrap();
    assert!(!actual.is_zero());
    section["represented_mass"] = serde_json::json!(0);

    let forged: LoadedSaveEnvelope = serde_json::from_value(encoded).unwrap();
    assert_eq!(
        forged.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::World(
            WorldValidationError::SectionMassMismatch {
                chunk: ChunkCoord::new(0, 0),
                section_y,
                stored: Mass::ZERO,
                actual,
            }
        )))
    );
}
