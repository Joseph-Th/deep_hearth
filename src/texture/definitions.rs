//! Defines immutable palettes, indexed textures, block appearances, and object appearances.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub const PALETTE_RAMP_COLOR_COUNT: usize = 16;
pub const TEXTURE_PALETTE_SLOT_COUNT: usize = 16;
pub const TEXTURE_SIDE: usize = 32;
pub const TEXTURE_TEXEL_COUNT: usize = TEXTURE_SIDE * TEXTURE_SIDE;
pub const TEXTURE_MIP_LEVEL_COUNT: usize = TEXTURE_SIDE.ilog2() as usize + 1;
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

/// Immutable 32x32 authored indexed texture and its local palette assignment.
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
        for texel in &texels {
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

mod appearance;
pub use appearance::{
    BlockAppearanceDefinition, CommodityAppearanceBinding, CubeFace, EquipmentAppearanceBinding,
    ObjectAppearanceDefinition, ObjectTextureSlot,
};

mod registry;
pub use registry::TextureRegistry;

#[cfg(test)]
#[path = "definitions_tests.rs"]
mod tests;
