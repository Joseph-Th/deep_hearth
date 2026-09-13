//! Defines block/object appearance mappings and material/equipment appearance bindings.

use crate::equipment::EquipmentDefinitionId;
use crate::material::CommodityKey;

use super::{BLOCK_FACE_COUNT, BlockAppearanceId, ObjectAppearanceId, TextureId};

/// Canonical cube-face vocabulary used by block mesh adapters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CubeFace {
    Top,
    Bottom,
    North,
    South,
    East,
    West,
}

impl CubeFace {
    /// Deterministic face-to-slot index shared by definition and baking views.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Top => 0,
            Self::Bottom => 1,
            Self::North => 2,
            Self::South => 3,
            Self::East => 4,
            Self::West => 5,
        }
    }
}

/// Immutable face-to-texture mapping for one block appearance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockAppearanceDefinition {
    id: BlockAppearanceId,
    name: String,
    textures: [TextureId; BLOCK_FACE_COUNT],
}

impl BlockAppearanceDefinition {
    #[must_use]
    pub fn uniform(id: BlockAppearanceId, name: impl Into<String>, texture: TextureId) -> Self {
        Self::new(id, name, [texture; BLOCK_FACE_COUNT])
    }

    /// Builds a block with one top, one bottom, and four identical sides.
    ///
    /// Parameter order is top/side/bottom for caller readability; storage follows
    /// [`CubeFace::index`] order (top, bottom, then four sides).
    #[must_use]
    pub fn top_side_bottom(
        id: BlockAppearanceId,
        name: impl Into<String>,
        top: TextureId,
        side: TextureId,
        bottom: TextureId,
    ) -> Self {
        Self::new(id, name, [top, bottom, side, side, side, side])
    }

    #[must_use]
    pub fn new(
        id: BlockAppearanceId,
        name: impl Into<String>,
        textures: [TextureId; BLOCK_FACE_COUNT],
    ) -> Self {
        let name = name.into();
        assert!(
            !name.trim().is_empty(),
            "block appearance name must not be empty"
        );
        Self { id, name, textures }
    }

    #[must_use]
    pub const fn id(&self) -> BlockAppearanceId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn texture(&self, face: CubeFace) -> TextureId {
        self.textures[face.index()]
    }

    pub(in crate::texture) fn textures(&self) -> &[TextureId; BLOCK_FACE_COUNT] {
        &self.textures
    }
}

/// Zero-based material slot authored by an object mesh adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectTextureSlot(u8);

impl ObjectTextureSlot {
    #[must_use]
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }
}

/// Immutable ordered texture assignment for an object's mesh material slots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectAppearanceDefinition {
    id: ObjectAppearanceId,
    name: String,
    textures: Vec<TextureId>,
}

impl ObjectAppearanceDefinition {
    #[must_use]
    pub fn new(id: ObjectAppearanceId, name: impl Into<String>, textures: Vec<TextureId>) -> Self {
        let name = name.into();
        assert!(
            !name.trim().is_empty(),
            "object appearance name must not be empty"
        );
        assert!(
            !textures.is_empty(),
            "object appearance must contain a texture slot"
        );
        assert!(
            textures.len() <= usize::from(u8::MAX) + 1,
            "object appearance exceeds its slot-addressing limit"
        );
        Self { id, name, textures }
    }

    #[must_use]
    pub const fn id(&self) -> ObjectAppearanceId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn get_texture(&self, slot: ObjectTextureSlot) -> Option<TextureId> {
        self.textures.get(usize::from(slot.value())).copied()
    }

    #[must_use]
    pub fn textures(&self) -> &[TextureId] {
        &self.textures
    }
}

/// Optional visual bindings for one authored material/form commodity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommodityAppearanceBinding {
    commodity: CommodityKey,
    block: Option<BlockAppearanceId>,
    object: Option<ObjectAppearanceId>,
}

impl CommodityAppearanceBinding {
    #[must_use]
    pub fn new(
        commodity: CommodityKey,
        block: Option<BlockAppearanceId>,
        object: Option<ObjectAppearanceId>,
    ) -> Self {
        assert!(
            block.is_some() || object.is_some(),
            "commodity appearance binding must expose a block or object appearance"
        );
        Self {
            commodity,
            block,
            object,
        }
    }

    #[must_use]
    pub const fn commodity(self) -> CommodityKey {
        self.commodity
    }

    #[must_use]
    pub const fn block(self) -> Option<BlockAppearanceId> {
        self.block
    }

    #[must_use]
    pub const fn object(self) -> Option<ObjectAppearanceId> {
        self.object
    }
}

/// Visual binding from one equipment definition to an object material-slot appearance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EquipmentAppearanceBinding {
    equipment: EquipmentDefinitionId,
    object: ObjectAppearanceId,
}

impl EquipmentAppearanceBinding {
    #[must_use]
    pub const fn new(equipment: EquipmentDefinitionId, object: ObjectAppearanceId) -> Self {
        Self { equipment, object }
    }

    #[must_use]
    pub const fn equipment(self) -> EquipmentDefinitionId {
        self.equipment
    }

    #[must_use]
    pub const fn object(self) -> ObjectAppearanceId {
        self.object
    }
}
