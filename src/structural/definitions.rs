//! Immutable structural response profiles; sibling state stores only authored references and runtime member condition.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Normalization scale for structural utilization and retained-capacity fractions.
pub const STRUCTURAL_PARTS_PER_MILLION: u32 = 1_000_000;

/// Stable authored identifier for one structural response profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StructuralProfileId(u32);

impl StructuralProfileId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "structural profile id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Material-strength axis used by one member under its modeled load path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StructuralLoadMode {
    Compression,
    Tension,
}

/// Immutable authored mapping from physical strength to readable warning and damage behavior.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuralProfileDefinition {
    id: StructuralProfileId,
    name: String,
    load_mode: StructuralLoadMode,
    strained_at_ppm: u32,
    cracking_at_ppm: u32,
    cracked_capacity_ppm: u32,
}

impl StructuralProfileDefinition {
    #[must_use]
    pub fn new(
        id: StructuralProfileId,
        name: impl Into<String>,
        load_mode: StructuralLoadMode,
        strained_at_ppm: u32,
        cracking_at_ppm: u32,
        cracked_capacity_ppm: u32,
    ) -> Self {
        let name = name.into();
        assert!(
            !name.trim().is_empty(),
            "structural profile name must not be empty"
        );
        assert!(
            strained_at_ppm > 0 && strained_at_ppm < cracking_at_ppm,
            "structural profile strained threshold must be positive and below cracking threshold"
        );
        assert!(
            cracking_at_ppm <= STRUCTURAL_PARTS_PER_MILLION,
            "structural profile cracking threshold exceeds normalized strength"
        );
        assert!(
            cracked_capacity_ppm > 0 && cracked_capacity_ppm <= STRUCTURAL_PARTS_PER_MILLION,
            "structural profile cracked capacity must be within normalized bounds"
        );
        Self {
            id,
            name,
            load_mode,
            strained_at_ppm,
            cracking_at_ppm,
            cracked_capacity_ppm,
        }
    }

    #[must_use]
    pub const fn id(&self) -> StructuralProfileId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn load_mode(&self) -> StructuralLoadMode {
        self.load_mode
    }

    #[must_use]
    pub const fn strained_at_ppm(&self) -> u32 {
        self.strained_at_ppm
    }

    #[must_use]
    pub const fn cracking_at_ppm(&self) -> u32 {
        self.cracking_at_ppm
    }

    #[must_use]
    pub const fn cracked_capacity_ppm(&self) -> u32 {
        self.cracked_capacity_ppm
    }
}

/// Immutable deterministic structural-profile lookup table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StructuralRegistry {
    definitions: BTreeMap<StructuralProfileId, StructuralProfileDefinition>,
}

impl StructuralRegistry {
    pub(crate) fn new(definitions: impl IntoIterator<Item = StructuralProfileDefinition>) -> Self {
        let mut by_id = BTreeMap::new();
        for definition in definitions {
            let id = definition.id();
            assert!(
                by_id.insert(id, definition).is_none(),
                "duplicate structural profile id {}",
                id.value()
            );
        }
        Self { definitions: by_id }
    }

    #[must_use]
    pub fn get_profile(&self, id: StructuralProfileId) -> Option<&StructuralProfileDefinition> {
        self.definitions.get(&id)
    }
}
