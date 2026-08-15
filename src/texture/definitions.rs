//! Immutable palette, indexed-tile, and appearance definitions consumed by sibling texture baking.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::equipment::{EquipmentDefinitionId, EquipmentRegistry};
use crate::material::{CommodityKey, MaterialRegistry};

pub const PALETTE_RAMP_COLOR_COUNT: usize = 16;
pub const TEXTURE_PALETTE_SLOT_COUNT: usize = 16;
pub const TEXTURE_SIDE: usize = 16;
pub const TEXTURE_TEXEL_COUNT: usize = TEXTURE_SIDE * TEXTURE_SIDE;
pub const BLOCK_FACE_COUNT: usize = 6;

const MAX_PALETTE_RAMP_ID: u16 = 4_095;
const MAX_TEXTURE_ID: u16 = 4_095;
const MAX_APPEARANCE_ID: u16 = 4_095;

/// Stable authored identifier for one reusable 16-color ramp.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PaletteRampId(u16);

impl PaletteRampId {
    #[must_use]
    pub const fn new(value: u16) -> Self {
        assert!(value != 0, "palette ramp id must be nonzero");
        assert!(
            value <= MAX_PALETTE_RAMP_ID,
            "palette ramp id exceeds the compact lookup-table limit"
        );
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

/// Stable authored identifier for one indexed texture tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TextureId(u16);

impl TextureId {
    #[must_use]
    pub const fn new(value: u16) -> Self {
        assert!(value != 0, "texture id must be nonzero");
        assert!(
            value <= MAX_TEXTURE_ID,
            "texture id exceeds the dense hot-lookup limit"
        );
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

/// Stable authored identifier for one six-face block appearance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BlockAppearanceId(u16);

impl BlockAppearanceId {
    #[must_use]
    pub const fn new(value: u16) -> Self {
        assert!(value != 0, "block appearance id must be nonzero");
        assert!(
            value <= MAX_APPEARANCE_ID,
            "block appearance id exceeds the dense hot-lookup limit"
        );
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

/// Stable authored identifier for one ordered object material-slot appearance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ObjectAppearanceId(u16);

impl ObjectAppearanceId {
    #[must_use]
    pub const fn new(value: u16) -> Self {
        assert!(value != 0, "object appearance id must be nonzero");
        assert!(
            value <= MAX_APPEARANCE_ID,
            "object appearance id exceeds the dense hot-lookup limit"
        );
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

/// One byte-addressable RGBA color used by a palette lookup texture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct ColorRgba8 {
    red: u8,
    green: u8,
    blue: u8,
    alpha: u8,
}

impl ColorRgba8 {
    #[must_use]
    pub const fn new(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    #[must_use]
    pub const fn opaque(red: u8, green: u8, blue: u8) -> Self {
        Self::new(red, green, blue, u8::MAX)
    }

    #[must_use]
    pub const fn red(self) -> u8 {
        self.red
    }

    #[must_use]
    pub const fn green(self) -> u8 {
        self.green
    }

    #[must_use]
    pub const fn blue(self) -> u8 {
        self.blue
    }

    #[must_use]
    pub const fn alpha(self) -> u8 {
        self.alpha
    }

    #[must_use]
    pub const fn channels(self) -> [u8; 4] {
        [self.red, self.green, self.blue, self.alpha]
    }
}

/// Immutable hue-shaped shade ramp shared by any number of texture-local palette slots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteRampDefinition {
    id: PaletteRampId,
    name: String,
    colors: [ColorRgba8; PALETTE_RAMP_COLOR_COUNT],
}

impl PaletteRampDefinition {
    #[must_use]
    pub fn new(
        id: PaletteRampId,
        name: impl Into<String>,
        colors: [ColorRgba8; PALETTE_RAMP_COLOR_COUNT],
    ) -> Self {
        let name = name.into();
        assert!(
            !name.trim().is_empty(),
            "palette ramp name must not be empty"
        );
        Self { id, name, colors }
    }

    /// Expands four authored hue/luminance anchors at shade positions 0, 5, 10, and 15.
    ///
    /// Authored intermediate anchors let shadows and highlights shift hue instead of forcing one
    /// straight RGB line. Expansion happens only during registry construction.
    #[must_use]
    pub fn from_anchors(
        id: PaletteRampId,
        name: impl Into<String>,
        anchors: [ColorRgba8; 4],
    ) -> Self {
        let colors = std::array::from_fn(|shade| {
            let segment = std::cmp::min(shade / 5, 2);
            let segment_start = segment * 5;
            let numerator = shade - segment_start;
            interpolate_color(anchors[segment], anchors[segment + 1], numerator, 5)
        });
        Self::new(id, name, colors)
    }

    #[must_use]
    pub const fn id(&self) -> PaletteRampId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn colors(&self) -> &[ColorRgba8; PALETTE_RAMP_COLOR_COUNT] {
        &self.colors
    }

    #[must_use]
    pub const fn color(&self, shade: ShadeIndex) -> ColorRgba8 {
        self.colors[shade.value() as usize]
    }
}

fn interpolate_color(
    start: ColorRgba8,
    end: ColorRgba8,
    numerator: usize,
    denominator: usize,
) -> ColorRgba8 {
    let start = start.channels();
    let end = end.channels();
    let channels: [u8; 4] = std::array::from_fn(|index| {
        let left = usize::from(start[index]) * (denominator - numerator);
        let right = usize::from(end[index]) * numerator;
        ((left + right + denominator / 2) / denominator) as u8
    });
    ColorRgba8::new(channels[0], channels[1], channels[2], channels[3])
}

/// Four-bit local palette slot encoded in the high nibble of an indexed texel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PaletteSlot(u8);

impl PaletteSlot {
    #[must_use]
    pub const fn new(value: u8) -> Self {
        assert!(
            value < TEXTURE_PALETTE_SLOT_COUNT as u8,
            "palette slot exceeds four bits"
        );
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }
}

/// Four-bit shade position encoded in the low nibble of an indexed texel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShadeIndex(u8);

impl ShadeIndex {
    #[must_use]
    pub const fn new(value: u8) -> Self {
        assert!(
            value < PALETTE_RAMP_COLOR_COUNT as u8,
            "shade index exceeds four bits"
        );
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }

    /// Applies a lighting or face-orientation delta without leaving the authored ramp.
    #[must_use]
    pub const fn saturating_offset(self, delta: i8) -> Self {
        let adjusted = self.0 as i16 + delta as i16;
        if adjusted < 0 {
            Self(0)
        } else if adjusted >= PALETTE_RAMP_COLOR_COUNT as i16 {
            Self((PALETTE_RAMP_COLOR_COUNT - 1) as u8)
        } else {
            Self(adjusted as u8)
        }
    }
}

/// One compact texture sample: four bits of local ramp selection and four bits of shade.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct PackedTexel(u8);

impl PackedTexel {
    #[must_use]
    pub const fn new(palette_slot: PaletteSlot, shade: ShadeIndex) -> Self {
        Self((palette_slot.value() << 4) | shade.value())
    }

    #[must_use]
    pub const fn from_raw(value: u8) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn palette_slot(self) -> PaletteSlot {
        PaletteSlot::new(self.0 >> 4)
    }

    #[must_use]
    pub const fn shade(self) -> ShadeIndex {
        ShadeIndex::new(self.0 & 0x0f)
    }

    #[must_use]
    pub const fn raw_value(self) -> u8 {
        self.0
    }

    /// Applies a shade-only delta while retaining the texture-local material ramp.
    #[must_use]
    pub const fn with_shade_offset(self, delta: i8) -> Self {
        Self::new(self.palette_slot(), self.shade().saturating_offset(delta))
    }
}

/// Ordered mapping from a texture's local four-bit slots to reusable global ramps.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TexturePalette {
    ramps: Vec<PaletteRampId>,
}

impl TexturePalette {
    #[must_use]
    pub fn new(ramps: Vec<PaletteRampId>) -> Self {
        assert!(!ramps.is_empty(), "texture palette must contain a ramp");
        assert!(
            ramps.len() <= TEXTURE_PALETTE_SLOT_COUNT,
            "texture palette exceeds its four-bit slot capacity"
        );
        let unique: BTreeSet<_> = ramps.iter().copied().collect();
        assert_eq!(
            unique.len(),
            ramps.len(),
            "texture palette must not repeat a ramp"
        );
        Self { ramps }
    }

    #[must_use]
    pub fn ramps(&self) -> &[PaletteRampId] {
        &self.ramps
    }

    #[must_use]
    pub fn get_ramp(&self, slot: PaletteSlot) -> Option<PaletteRampId> {
        self.ramps.get(usize::from(slot.value())).copied()
    }
}

/// GPU draw-path classification for a texture's resolved alpha values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TextureAlphaMode {
    Opaque,
    Cutout,
    Blend,
}

/// Immutable 16x16 authored indexed texture and its local palette assignment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureDefinition {
    id: TextureId,
    name: String,
    palette: TexturePalette,
    alpha_mode: TextureAlphaMode,
    texels: Box<[PackedTexel; TEXTURE_TEXEL_COUNT]>,
}

impl TextureDefinition {
    #[must_use]
    pub fn new(
        id: TextureId,
        name: impl Into<String>,
        palette: TexturePalette,
        alpha_mode: TextureAlphaMode,
        texels: [PackedTexel; TEXTURE_TEXEL_COUNT],
    ) -> Self {
        let name = name.into();
        assert!(!name.trim().is_empty(), "texture name must not be empty");
        let mut used_slots = BTreeSet::new();
        for texel in texels {
            assert!(
                palette.get_ramp(texel.palette_slot()).is_some(),
                "texture {} texel references undefined local palette slot {}",
                id.value(),
                texel.palette_slot().value()
            );
            used_slots.insert(texel.palette_slot());
        }
        assert_eq!(
            used_slots.len(),
            palette.ramps().len(),
            "texture {} defines an unused local palette slot",
            id.value()
        );
        Self {
            id,
            name,
            palette,
            alpha_mode,
            texels: Box::new(texels),
        }
    }

    #[must_use]
    pub const fn id(&self) -> TextureId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn palette(&self) -> &TexturePalette {
        &self.palette
    }

    #[must_use]
    pub const fn alpha_mode(&self) -> TextureAlphaMode {
        self.alpha_mode
    }

    #[must_use]
    pub const fn texels(&self) -> &[PackedTexel; TEXTURE_TEXEL_COUNT] {
        &self.texels
    }
}

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
        match face {
            CubeFace::Top => self.textures[0],
            CubeFace::Bottom => self.textures[1],
            CubeFace::North => self.textures[2],
            CubeFace::South => self.textures[3],
            CubeFace::East => self.textures[4],
            CubeFace::West => self.textures[5],
        }
    }

    pub(super) fn textures(&self) -> &[TextureId; BLOCK_FACE_COUNT] {
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

/// Immutable authored visual definitions and deterministic lookup bindings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextureRegistry {
    ramps: BTreeMap<PaletteRampId, PaletteRampDefinition>,
    textures: BTreeMap<TextureId, TextureDefinition>,
    blocks: BTreeMap<BlockAppearanceId, BlockAppearanceDefinition>,
    objects: BTreeMap<ObjectAppearanceId, ObjectAppearanceDefinition>,
    commodity_bindings: BTreeMap<CommodityKey, CommodityAppearanceBinding>,
    equipment_bindings: BTreeMap<EquipmentDefinitionId, EquipmentAppearanceBinding>,
}

impl TextureRegistry {
    pub(crate) fn new(
        ramps: impl IntoIterator<Item = PaletteRampDefinition>,
        textures: impl IntoIterator<Item = TextureDefinition>,
        blocks: impl IntoIterator<Item = BlockAppearanceDefinition>,
        objects: impl IntoIterator<Item = ObjectAppearanceDefinition>,
        commodity_bindings: impl IntoIterator<Item = CommodityAppearanceBinding>,
        equipment_bindings: impl IntoIterator<Item = EquipmentAppearanceBinding>,
    ) -> Self {
        let mut registry = Self::default();
        for ramp in ramps {
            let id = ramp.id();
            assert!(
                registry.ramps.insert(id, ramp).is_none(),
                "duplicate palette ramp id {}",
                id.value()
            );
        }
        for texture in textures {
            let id = texture.id();
            assert!(
                registry.textures.insert(id, texture).is_none(),
                "duplicate texture id {}",
                id.value()
            );
        }
        for block in blocks {
            let id = block.id();
            assert!(
                registry.blocks.insert(id, block).is_none(),
                "duplicate block appearance id {}",
                id.value()
            );
        }
        for object in objects {
            let id = object.id();
            assert!(
                registry.objects.insert(id, object).is_none(),
                "duplicate object appearance id {}",
                id.value()
            );
        }
        for binding in commodity_bindings {
            let commodity = binding.commodity();
            assert!(
                registry
                    .commodity_bindings
                    .insert(commodity, binding)
                    .is_none(),
                "duplicate commodity appearance binding {}",
                commodity.value()
            );
        }
        for binding in equipment_bindings {
            let equipment = binding.equipment();
            assert!(
                registry
                    .equipment_bindings
                    .insert(equipment, binding)
                    .is_none(),
                "duplicate equipment appearance binding {}",
                equipment.value()
            );
        }
        registry.validate_local_references();
        registry
    }

    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn get_ramp(&self, id: PaletteRampId) -> Option<&PaletteRampDefinition> {
        self.ramps.get(&id)
    }

    #[must_use]
    pub fn get_texture(&self, id: TextureId) -> Option<&TextureDefinition> {
        self.textures.get(&id)
    }

    #[must_use]
    pub fn get_block(&self, id: BlockAppearanceId) -> Option<&BlockAppearanceDefinition> {
        self.blocks.get(&id)
    }

    #[must_use]
    pub fn get_object(&self, id: ObjectAppearanceId) -> Option<&ObjectAppearanceDefinition> {
        self.objects.get(&id)
    }

    #[must_use]
    pub fn get_commodity_appearance(
        &self,
        commodity: CommodityKey,
    ) -> Option<CommodityAppearanceBinding> {
        self.commodity_bindings.get(&commodity).copied()
    }

    #[must_use]
    pub fn get_equipment_appearance(
        &self,
        equipment: EquipmentDefinitionId,
    ) -> Option<EquipmentAppearanceBinding> {
        self.equipment_bindings.get(&equipment).copied()
    }

    pub(crate) fn validate_references(
        &self,
        materials: &MaterialRegistry,
        equipment: &EquipmentRegistry,
    ) {
        for binding in self.commodity_bindings.values() {
            assert!(
                materials.has_commodity(binding.commodity()),
                "commodity appearance references missing commodity {}",
                binding.commodity().value()
            );
        }
        for binding in self.equipment_bindings.values() {
            assert!(
                equipment.get_equipment(binding.equipment()).is_some(),
                "equipment appearance references missing equipment {}",
                binding.equipment().value()
            );
        }
    }

    fn validate_local_references(&self) {
        for texture in self.textures.values() {
            for ramp in texture.palette().ramps() {
                assert!(
                    self.ramps.contains_key(ramp),
                    "texture {} references missing palette ramp {}",
                    texture.id().value(),
                    ramp.value()
                );
            }
            for texel in texture.texels() {
                let ramp = match texture.palette().get_ramp(texel.palette_slot()) {
                    Some(ramp) => ramp,
                    None => panic!(
                        "texture {} contains an invalid local palette slot",
                        texture.id().value()
                    ),
                };
                let alpha = match self.ramps.get(&ramp) {
                    Some(definition) => definition.color(texel.shade()).alpha(),
                    None => panic!(
                        "texture {} references missing palette ramp {}",
                        texture.id().value(),
                        ramp.value()
                    ),
                };
                match texture.alpha_mode() {
                    TextureAlphaMode::Opaque => assert_eq!(
                        alpha,
                        u8::MAX,
                        "opaque texture {} resolves a nonopaque texel",
                        texture.id().value()
                    ),
                    TextureAlphaMode::Cutout => assert!(
                        alpha == 0 || alpha == u8::MAX,
                        "cutout texture {} resolves a blended texel",
                        texture.id().value()
                    ),
                    TextureAlphaMode::Blend => {}
                }
            }
        }
        for block in self.blocks.values() {
            for texture in block.textures() {
                assert!(
                    self.textures.contains_key(texture),
                    "block appearance {} references missing texture {}",
                    block.id().value(),
                    texture.value()
                );
            }
        }
        for object in self.objects.values() {
            for texture in object.textures() {
                assert!(
                    self.textures.contains_key(texture),
                    "object appearance {} references missing texture {}",
                    object.id().value(),
                    texture.value()
                );
            }
        }
        for binding in self.commodity_bindings.values() {
            if let Some(block) = binding.block() {
                assert!(
                    self.blocks.contains_key(&block),
                    "commodity {} references missing block appearance {}",
                    binding.commodity().value(),
                    block.value()
                );
            }
            if let Some(object) = binding.object() {
                assert!(
                    self.objects.contains_key(&object),
                    "commodity {} references missing object appearance {}",
                    binding.commodity().value(),
                    object.value()
                );
            }
        }
        for binding in self.equipment_bindings.values() {
            assert!(
                self.objects.contains_key(&binding.object()),
                "equipment {} references missing object appearance {}",
                binding.equipment().value(),
                binding.object().value()
            );
        }
    }

    pub(super) fn ramps_in_id_order(
        &self,
    ) -> impl ExactSizeIterator<Item = &PaletteRampDefinition> {
        self.ramps.values()
    }

    pub(super) fn textures_in_id_order(&self) -> impl ExactSizeIterator<Item = &TextureDefinition> {
        self.textures.values()
    }

    pub(super) fn blocks_in_id_order(
        &self,
    ) -> impl ExactSizeIterator<Item = &BlockAppearanceDefinition> {
        self.blocks.values()
    }

    pub(super) fn objects_in_id_order(
        &self,
    ) -> impl ExactSizeIterator<Item = &ObjectAppearanceDefinition> {
        self.objects.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAMP: PaletteRampId = PaletteRampId::new(1);
    const TEXTURE: TextureId = TextureId::new(1);

    fn ramp(alpha: u8) -> PaletteRampDefinition {
        PaletteRampDefinition::from_anchors(
            RAMP,
            "fixture",
            [
                ColorRgba8::new(10, 20, 30, alpha),
                ColorRgba8::new(30, 40, 50, alpha),
                ColorRgba8::new(60, 70, 80, alpha),
                ColorRgba8::new(100, 110, 120, alpha),
            ],
        )
    }

    fn texture(alpha_mode: TextureAlphaMode) -> TextureDefinition {
        TextureDefinition::new(
            TEXTURE,
            "fixture",
            TexturePalette::new(vec![RAMP]),
            alpha_mode,
            [PackedTexel::new(PaletteSlot::new(0), ShadeIndex::new(8)); TEXTURE_TEXEL_COUNT],
        )
    }

    #[test]
    fn packed_texel_round_trip_uses_exactly_one_byte() {
        let texel = PackedTexel::new(PaletteSlot::new(11), ShadeIndex::new(6));

        assert_eq!(std::mem::size_of::<PackedTexel>(), 1);
        assert_eq!(std::mem::size_of::<ColorRgba8>(), 4);
        assert_eq!(texel.raw_value(), 0xb6);
        assert_eq!(texel.palette_slot(), PaletteSlot::new(11));
        assert_eq!(texel.shade(), ShadeIndex::new(6));
        assert_eq!(texel.with_shade_offset(20).raw_value(), 0xbf);
        assert_eq!(texel.with_shade_offset(-20).raw_value(), 0xb0);
    }

    #[test]
    fn authored_ramp_anchors_are_preserved_exactly() {
        let ramp = ramp(u8::MAX);

        assert_eq!(ramp.colors()[0], ColorRgba8::opaque(10, 20, 30));
        assert_eq!(ramp.colors()[5], ColorRgba8::opaque(30, 40, 50));
        assert_eq!(ramp.colors()[10], ColorRgba8::opaque(60, 70, 80));
        assert_eq!(ramp.colors()[15], ColorRgba8::opaque(100, 110, 120));
    }

    #[test]
    fn opaque_texture_rejects_transparent_palette_resolution() {
        let result = std::panic::catch_unwind(|| {
            TextureRegistry::new(
                [ramp(0)],
                [texture(TextureAlphaMode::Opaque)],
                std::iter::empty(),
                std::iter::empty(),
                std::iter::empty(),
                std::iter::empty(),
            )
        });

        assert!(result.is_err());
    }

    #[test]
    fn block_face_mapping_is_explicit_and_stable() {
        let other = TextureId::new(2);
        let block = BlockAppearanceDefinition::top_side_bottom(
            BlockAppearanceId::new(1),
            "fixture",
            TEXTURE,
            other,
            TextureId::new(3),
        );

        assert_eq!(block.texture(CubeFace::Top), TEXTURE);
        assert_eq!(block.texture(CubeFace::North), other);
        assert_eq!(block.texture(CubeFace::West), other);
        assert_eq!(block.texture(CubeFace::Bottom), TextureId::new(3));
    }
}
