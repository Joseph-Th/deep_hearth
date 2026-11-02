//! Palette-compressed block-section storage and canonical packed index validation.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;

use super::{BlockStateId, SECTION_CELL_COUNT};

/// Palette-compressed 16-cubed block-state section.
///
/// A one-state section stores no index words. Multi-state sections use the minimum whole number
/// of bits needed for their local palette. The common homogeneous terrain case is therefore one
/// state ID plus a small header rather than 4,096 global IDs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldSection {
    y: i64,
    palette: Vec<BlockStateId>,
    words: Vec<u64>,
    non_air_count: u16,
    represented_mass: Mass,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    mass_overrides: BTreeMap<u16, Mass>,
}

impl WorldSection {
    pub(crate) fn uniform(y: i64, state: BlockStateId, state_mass: Mass) -> Self {
        let represented_mass = if state == BlockStateId::AIR {
            assert_eq!(state_mass, Mass::ZERO, "air section must be massless");
            Mass::ZERO
        } else {
            let total = u128::from(state_mass.milligrams()) * SECTION_CELL_COUNT as u128;
            Mass::from_milligrams(
                u64::try_from(total)
                    .unwrap_or_else(|_| panic!("uniform section represented mass overflowed")),
            )
        };
        Self {
            y,
            palette: vec![state],
            words: Vec::new(),
            non_air_count: if state == BlockStateId::AIR {
                0
            } else {
                SECTION_CELL_COUNT as u16
            },
            represented_mass,
            mass_overrides: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn y(&self) -> i64 {
        self.y
    }

    #[must_use]
    pub fn palette(&self) -> &[BlockStateId] {
        &self.palette
    }

    #[must_use]
    pub const fn non_air_count(&self) -> u16 {
        self.non_air_count
    }

    /// Exact matter represented by all occupied cells in this section.
    #[must_use]
    pub const fn represented_mass(&self) -> Mass {
        self.represented_mass
    }

    #[must_use]
    pub fn packed_word_count(&self) -> usize {
        self.words.len()
    }

    #[must_use]
    pub(crate) fn state_at_index(&self, index: usize) -> Option<BlockStateId> {
        if index >= SECTION_CELL_COUNT {
            return None;
        }
        if self.palette.len() == 1 {
            return self.palette.first().copied();
        }
        let palette_index =
            read_packed_index(&self.words, bits_for_palette(self.palette.len()), index)?;
        self.palette.get(palette_index).copied()
    }

    #[must_use]
    pub(crate) fn mass_override_at_index(&self, index: usize) -> Option<Mass> {
        self.mass_overrides
            .get(&u16::try_from(index).ok()?)
            .copied()
    }

    /// A canonical homogeneous section with no exceptional partial-cell mass can be
    /// accounted as one repeated state instead of decoding all of its cells.
    pub(crate) fn uniform_state_without_mass_overrides(&self) -> Option<BlockStateId> {
        if self.palette.len() == 1 && self.mass_overrides.is_empty() {
            self.palette.first().copied()
        } else {
            None
        }
    }

    pub(crate) fn set_mass_override_at_index(
        &mut self,
        index: usize,
        default_mass: Mass,
        remaining_mass: Mass,
    ) {
        assert!(
            index < SECTION_CELL_COUNT,
            "section cell index out of range"
        );
        assert!(
            !default_mass.is_zero(),
            "mass override requires material-backed cell"
        );
        assert!(
            !remaining_mass.is_zero() && remaining_mass <= default_mass,
            "cell remaining mass must stay within its positive default mass"
        );
        assert_ne!(
            self.state_at_index(index),
            Some(BlockStateId::AIR),
            "air cell cannot retain represented matter"
        );
        let key =
            u16::try_from(index).unwrap_or_else(|_| unreachable!("section cell index fits u16"));
        let previous = self
            .mass_overrides
            .get(&key)
            .copied()
            .unwrap_or(default_mass);
        if remaining_mass == default_mass {
            self.mass_overrides.remove(&key);
        } else {
            self.mass_overrides.insert(key, remaining_mass);
        }
        self.represented_mass = self
            .represented_mass
            .checked_sub(previous)
            .and_then(|mass| mass.checked_add(remaining_mass))
            .unwrap_or_else(|| panic!("section represented mass override overflowed"));
    }

    pub(crate) fn set_state_at_index(
        &mut self,
        index: usize,
        state: BlockStateId,
        previous_mass: Mass,
        new_mass: Mass,
    ) {
        assert!(
            index < SECTION_CELL_COUNT,
            "section cell index out of range"
        );
        let previous = self
            .state_at_index(index)
            .unwrap_or_else(|| panic!("trusted section index must resolve"));
        if previous == state {
            assert_eq!(
                previous_mass, new_mass,
                "unchanged block state changed represented mass"
            );
            return;
        }
        let key =
            u16::try_from(index).unwrap_or_else(|_| unreachable!("section cell index fits u16"));
        let represented_previous_mass = self.mass_overrides.remove(&key).unwrap_or(previous_mass);
        let palette_index = match self
            .palette
            .iter()
            .position(|candidate| *candidate == state)
        {
            Some(index) => index,
            None => {
                let old_bits = bits_for_palette(self.palette.len());
                self.palette.push(state);
                let new_bits = bits_for_palette(self.palette.len());
                if old_bits != new_bits {
                    self.words = repack_words(&self.words, old_bits, new_bits);
                }
                self.palette.len() - 1
            }
        };
        let bits = bits_for_palette(self.palette.len());
        if self.words.is_empty() {
            self.words = vec![0; packed_word_count(bits)];
        }
        write_packed_index(&mut self.words, bits, index, palette_index);
        if previous == BlockStateId::AIR {
            self.non_air_count = self
                .non_air_count
                .checked_add(1)
                .unwrap_or_else(|| panic!("section non-air count overflowed"));
        }
        if state == BlockStateId::AIR {
            self.non_air_count = self
                .non_air_count
                .checked_sub(1)
                .unwrap_or_else(|| panic!("section non-air count underflowed"));
        }
        self.represented_mass = self
            .represented_mass
            .checked_sub(represented_previous_mass)
            .and_then(|mass| mass.checked_add(new_mass))
            .unwrap_or_else(|| panic!("section represented mass mutation overflowed"));
    }

    pub(crate) fn canonicalize_palette(&mut self) {
        if self.palette.len() <= 1 {
            return;
        }
        let old_bits = bits_for_palette(self.palette.len());
        let mut used = vec![false; self.palette.len()];
        for cell in 0..SECTION_CELL_COUNT {
            let old_index = read_packed_index(&self.words, old_bits, cell)
                .unwrap_or_else(|| panic!("trusted section cell must resolve before compaction"));
            *used
                .get_mut(old_index)
                .unwrap_or_else(|| panic!("trusted section palette index must resolve")) = true;
        }
        if used.iter().all(|present| *present) {
            return;
        }
        // Keep the original palette order while assigning a dense index to each live state.
        // Scanning packed indices directly avoids a cell-by-palette quadratic search.
        let mut remap = vec![0; self.palette.len()];
        let mut palette = Vec::with_capacity(used.iter().filter(|present| **present).count());
        for (old_index, state) in self.palette.iter().copied().enumerate() {
            if used[old_index] {
                remap[old_index] = palette.len();
                palette.push(state);
            }
        }
        let bits = bits_for_palette(palette.len());
        let mut words = vec![0; packed_word_count(bits)];
        if bits > 0 {
            for cell in 0..SECTION_CELL_COUNT {
                let old_index =
                    read_packed_index(&self.words, old_bits, cell).unwrap_or_else(|| {
                        panic!("trusted section cell must resolve before compaction")
                    });
                write_packed_index(&mut words, bits, cell, remap[old_index]);
            }
        }
        self.palette = palette;
        self.words = words;
    }

    pub(crate) fn validate_storage(&self) -> Result<(), crate::world::WorldValidationError> {
        use crate::world::WorldValidationError;

        if self.palette.is_empty() {
            return Err(WorldValidationError::EmptySectionPalette { section_y: self.y });
        }
        if self.palette.len() > SECTION_CELL_COUNT {
            return Err(WorldValidationError::SectionPaletteTooLarge {
                section_y: self.y,
                entries: self.palette.len(),
            });
        }
        let mut seen = BTreeSet::new();
        for state in &self.palette {
            if !seen.insert(*state) {
                return Err(WorldValidationError::DuplicateSectionPaletteState {
                    section_y: self.y,
                    state: *state,
                });
            }
        }
        let bits = bits_for_palette(self.palette.len());
        let expected_words = packed_word_count(bits);
        if self.words.len() != expected_words {
            return Err(WorldValidationError::InvalidSectionWordCount {
                section_y: self.y,
                expected: expected_words,
                actual: self.words.len(),
            });
        }
        // Homogeneous sections need no per-cell decode. They dominate ordinary terrain,
        // and their canonical empty words already prove all 4,096 indices are zero.
        let mut used_palette_indexes = Vec::new();
        let counted_non_air = if self.palette.len() == 1 {
            if self.palette[0] == BlockStateId::AIR {
                0
            } else {
                SECTION_CELL_COUNT as u16
            }
        } else {
            // Multi-state sections require index membership, not a per-cell tree lookup.
            used_palette_indexes = vec![false; self.palette.len()];
            let mut counted = 0_u16;
            for index in 0..SECTION_CELL_COUNT {
                let palette_index =
                    read_packed_index(&self.words, bits, index).unwrap_or_else(|| {
                        unreachable!("validated word count covers every section cell")
                    });
                let Some(state) = self.palette.get(palette_index) else {
                    return Err(WorldValidationError::SectionPaletteIndexOutOfRange {
                        section_y: self.y,
                        cell: index,
                        palette_index,
                        palette_len: self.palette.len(),
                    });
                };
                used_palette_indexes[palette_index] = true;
                if *state != BlockStateId::AIR {
                    counted = counted
                        .checked_add(1)
                        .unwrap_or_else(|| unreachable!("section has at most 4096 non-air cells"));
                }
            }
            counted
        };
        if counted_non_air != self.non_air_count {
            return Err(WorldValidationError::SectionNonAirCountMismatch {
                section_y: self.y,
                stored: self.non_air_count,
                actual: counted_non_air,
            });
        }
        if let Some(unused_index) = used_palette_indexes.iter().position(|used| !*used) {
            return Err(WorldValidationError::UnusedSectionPaletteState {
                section_y: self.y,
                state: self.palette[unused_index],
            });
        }
        for (&cell, &mass) in &self.mass_overrides {
            if usize::from(cell) >= SECTION_CELL_COUNT {
                return Err(WorldValidationError::SectionMassOverrideIndexOutOfRange {
                    section_y: self.y,
                    cell,
                });
            }
            if mass.is_zero() {
                return Err(WorldValidationError::ZeroSectionMassOverride {
                    section_y: self.y,
                    cell,
                });
            }
        }
        Ok(())
    }
}

fn bits_for_palette(palette_len: usize) -> u8 {
    if palette_len <= 1 {
        0
    } else {
        usize::BITS as u8 - (palette_len - 1).leading_zeros() as u8
    }
}

fn packed_word_count(bits: u8) -> usize {
    if bits == 0 {
        0
    } else {
        (SECTION_CELL_COUNT * usize::from(bits)).div_ceil(u64::BITS as usize)
    }
}

fn read_packed_index(words: &[u64], bits: u8, index: usize) -> Option<usize> {
    if bits == 0 {
        return Some(0);
    }
    let bit_offset = index.checked_mul(usize::from(bits))?;
    let word_index = bit_offset / u64::BITS as usize;
    let shift = bit_offset % u64::BITS as usize;
    let mask = (1_u64 << bits) - 1;
    let first = *words.get(word_index)? >> shift;
    let spill = shift + usize::from(bits) > u64::BITS as usize;
    let value = if spill {
        let second = *words.get(word_index + 1)?;
        first | (second << (u64::BITS as usize - shift))
    } else {
        first
    };
    usize::try_from(value & mask).ok()
}

fn write_packed_index(words: &mut [u64], bits: u8, index: usize, value: usize) {
    assert!(bits > 0);
    let bit_offset = index * usize::from(bits);
    let word_index = bit_offset / u64::BITS as usize;
    let shift = bit_offset % u64::BITS as usize;
    let mask = (1_u64 << bits) - 1;
    let value = u64::try_from(value).unwrap_or_else(|_| panic!("palette index exceeds u64"));
    assert_eq!(value & !mask, 0, "palette index exceeds packed bit width");
    words[word_index] &= !(mask << shift);
    words[word_index] |= value << shift;
    if shift + usize::from(bits) > u64::BITS as usize {
        let spill_bits = shift + usize::from(bits) - u64::BITS as usize;
        let spill_mask = (1_u64 << spill_bits) - 1;
        words[word_index + 1] &= !spill_mask;
        words[word_index + 1] |= value >> (u64::BITS as usize - shift);
    }
}

fn repack_words(old_words: &[u64], old_bits: u8, new_bits: u8) -> Vec<u64> {
    let mut words = vec![0; packed_word_count(new_bits)];
    for index in 0..SECTION_CELL_COUNT {
        let value = if old_bits == 0 {
            0
        } else {
            read_packed_index(old_words, old_bits, index)
                .unwrap_or_else(|| panic!("trusted old packed section must cover every cell"))
        };
        write_packed_index(&mut words, new_bits, index, value);
    }
    words
}
