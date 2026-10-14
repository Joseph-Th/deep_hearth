//! Immutable block and block-state definitions used by authoritative world cells.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::material::{CommodityKey, MaterialRegistry};
use crate::texture::{BlockAppearanceId, TextureRegistry};

/// Stable authored identifier for one block behavior/material definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BlockDefinitionId(u16);

impl BlockDefinitionId {
    pub const AIR: Self = Self(1);

    #[must_use]
    pub const fn new(value: u16) -> Self {
        assert!(value != 0, "block definition id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

/// Stable authored identifier for one complete block-state variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BlockStateId(u16);

impl BlockStateId {
    pub const AIR: Self = Self(1);

    #[must_use]
    pub const fn new(value: u16) -> Self {
        assert!(value != 0, "block state id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

/// Coarse authoritative collision shape. Rich partial shapes can extend this enum without changing
/// the compact state identity stored in chunks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BlockCollisionShape {
    Empty,
    FullCube,
}

/// Immutable default physical facts shared by every state of one block definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockPhysicalDefinition {
    commodity: Option<CommodityKey>,
    represented_mass: Mass,
    collision: BlockCollisionShape,
    opaque: bool,
}

impl BlockPhysicalDefinition {
    #[must_use]
    pub fn new(
        commodity: Option<CommodityKey>,
        represented_mass: Mass,
        collision: BlockCollisionShape,
        opaque: bool,
    ) -> Self {
        assert_eq!(
            commodity.is_some(),
            !represented_mass.is_zero(),
            "material-backed blocks must represent positive mass and massless blocks must not claim a commodity"
        );
        if opaque {
            assert_ne!(
                collision,
                BlockCollisionShape::Empty,
                "opaque block physics must have physical collision"
            );
        }
        Self {
            commodity,
            represented_mass,
            collision,
            opaque,
        }
    }

    #[must_use]
    pub const fn commodity(self) -> Option<CommodityKey> {
        self.commodity
    }

    #[must_use]
    pub const fn represented_mass(self) -> Mass {
        self.represented_mass
    }

    #[must_use]
    pub const fn collision(self) -> BlockCollisionShape {
        self.collision
    }

    #[must_use]
    pub const fn is_opaque(self) -> bool {
        self.opaque
    }
}

/// Immutable behavior/material presentation shared by one or more block states.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockDefinition {
    id: BlockDefinitionId,
    name: String,
    default_state: BlockStateId,
    physical: BlockPhysicalDefinition,
    appearance: Option<BlockAppearanceId>,
}

impl BlockDefinition {
    #[must_use]
    pub fn new(
        id: BlockDefinitionId,
        name: impl Into<String>,
        default_state: BlockStateId,
        physical: BlockPhysicalDefinition,
        appearance: Option<BlockAppearanceId>,
    ) -> Self {
        let name = name.into();
        assert!(
            !name.trim().is_empty(),
            "block definition name must not be empty"
        );
        if physical.is_opaque() {
            assert!(
                appearance.is_some(),
                "opaque block definition requires appearance"
            );
        }
        Self {
            id,
            name,
            default_state,
            physical,
            appearance,
        }
    }

    #[must_use]
    pub const fn id(&self) -> BlockDefinitionId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn default_state(&self) -> BlockStateId {
        self.default_state
    }

    #[must_use]
    pub const fn commodity(&self) -> Option<CommodityKey> {
        self.physical.commodity()
    }

    /// Exact whole-milligram non-fluid matter represented by one occupied world cell.
    #[must_use]
    pub const fn represented_mass(&self) -> Mass {
        self.physical.represented_mass()
    }

    #[must_use]
    pub const fn appearance(&self) -> Option<BlockAppearanceId> {
        self.appearance
    }

    #[must_use]
    pub const fn collision(&self) -> BlockCollisionShape {
        self.physical.collision()
    }

    #[must_use]
    pub const fn is_opaque(&self) -> bool {
        self.physical.is_opaque()
    }
}

/// Immutable complete state variant. State-specific properties belong here as they are introduced;
/// runtime chunks continue to store only the compact state identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockStateDefinition {
    id: BlockStateId,
    block: BlockDefinitionId,
}

impl BlockStateDefinition {
    #[must_use]
    pub const fn new(id: BlockStateId, block: BlockDefinitionId) -> Self {
        Self { id, block }
    }

    #[must_use]
    pub const fn id(self) -> BlockStateId {
        self.id
    }

    #[must_use]
    pub const fn block(self) -> BlockDefinitionId {
        self.block
    }
}

/// Immutable authored world-cell vocabulary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldRegistry {
    blocks: BTreeMap<BlockDefinitionId, BlockDefinition>,
    states: BTreeMap<BlockStateId, BlockStateDefinition>,
}

impl WorldRegistry {
    pub(crate) fn new(
        blocks: impl IntoIterator<Item = BlockDefinition>,
        states: impl IntoIterator<Item = BlockStateDefinition>,
    ) -> Self {
        let mut blocks_by_id = BTreeMap::new();
        for block in blocks {
            let id = block.id();
            assert!(
                blocks_by_id.insert(id, block).is_none(),
                "duplicate block definition id {}",
                id.value()
            );
        }
        let mut states_by_id = BTreeMap::new();
        for state in states {
            let id = state.id();
            assert!(
                states_by_id.insert(id, state).is_none(),
                "duplicate block state id {}",
                id.value()
            );
        }
        let registry = Self {
            blocks: blocks_by_id,
            states: states_by_id,
        };
        registry.validate_local_references();
        registry
    }

    #[must_use]
    pub fn get_block(&self, id: BlockDefinitionId) -> Option<&BlockDefinition> {
        self.blocks.get(&id)
    }

    #[must_use]
    pub fn get_state(&self, id: BlockStateId) -> Option<BlockStateDefinition> {
        self.states.get(&id).copied()
    }

    #[must_use]
    pub fn block_for_state(&self, id: BlockStateId) -> Option<&BlockDefinition> {
        let state = self.get_state(id)?;
        self.get_block(state.block())
    }

    #[must_use]
    pub fn represented_mass_for_state(&self, id: BlockStateId) -> Option<Mass> {
        self.block_for_state(id)
            .map(BlockDefinition::represented_mass)
    }

    pub fn blocks(&self) -> impl ExactSizeIterator<Item = &BlockDefinition> {
        self.blocks.values()
    }

    pub fn states(&self) -> impl ExactSizeIterator<Item = BlockStateDefinition> + '_ {
        self.states.values().copied()
    }

    pub(crate) fn validate_references(
        &self,
        materials: &MaterialRegistry,
        textures: &TextureRegistry,
    ) {
        for block in self.blocks.values() {
            if let Some(commodity) = block.commodity() {
                assert!(
                    materials.has_commodity(commodity),
                    "block {} references missing commodity {}",
                    block.id().value(),
                    commodity.value()
                );
                if block.collision() == BlockCollisionShape::FullCube {
                    let material =
                        materials
                            .get_material(commodity.material())
                            .unwrap_or_else(|| {
                                unreachable!("registered commodity has a material definition")
                            });
                    let expected_mass = Mass::from_milligrams(
                        u64::from(material.properties().density_kg_per_m3())
                            .checked_mul(1_000_000)
                            .unwrap_or_else(|| {
                                panic!("full-cube material mass exceeds supported range")
                            }),
                    );
                    assert_eq!(
                        block.represented_mass(),
                        expected_mass,
                        "full-cube block {} represented mass must follow material density",
                        block.id().value()
                    );
                }
            }
            if let Some(appearance) = block.appearance() {
                assert!(
                    textures.get_block(appearance).is_some(),
                    "block {} references missing appearance {}",
                    block.id().value(),
                    appearance.value()
                );
            }
        }
    }

    fn validate_local_references(&self) {
        let air = self
            .states
            .get(&BlockStateId::AIR)
            .unwrap_or_else(|| panic!("world registry must define the canonical air state"));
        assert_eq!(
            air.block(),
            BlockDefinitionId::AIR,
            "canonical air state must reference canonical air block"
        );
        let air_block = self
            .blocks
            .get(&BlockDefinitionId::AIR)
            .unwrap_or_else(|| panic!("world registry must define the canonical air block"));
        assert_eq!(
            air_block.default_state(),
            BlockStateId::AIR,
            "canonical air block must default to canonical air state"
        );
        assert_eq!(air_block.collision(), BlockCollisionShape::Empty);
        assert!(!air_block.is_opaque());
        assert!(air_block.commodity().is_none());
        assert_eq!(air_block.represented_mass(), Mass::ZERO);
        assert!(air_block.appearance().is_none());

        for block in self.blocks.values() {
            let state = self.states.get(&block.default_state()).unwrap_or_else(|| {
                panic!(
                    "block {} references missing default state {}",
                    block.id().value(),
                    block.default_state().value()
                )
            });
            assert_eq!(
                state.block(),
                block.id(),
                "block {} default state belongs to another block",
                block.id().value()
            );
        }
        for state in self.states.values() {
            assert!(
                self.blocks.contains_key(&state.block()),
                "block state {} references missing block {}",
                state.id().value(),
                state.block().value()
            );
        }
    }
}
