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
