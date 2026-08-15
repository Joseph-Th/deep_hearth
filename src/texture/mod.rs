//! Renderer-neutral palette, texture, and appearance definitions with a compact startup bake path.

mod definitions;
mod texture_baking;

pub use definitions::{
    BLOCK_FACE_COUNT, BlockAppearanceDefinition, BlockAppearanceId, ColorRgba8,
    CommodityAppearanceBinding, CubeFace, EquipmentAppearanceBinding, ObjectAppearanceDefinition,
    ObjectAppearanceId, ObjectTextureSlot, PALETTE_RAMP_COLOR_COUNT, PackedTexel,
    PaletteRampDefinition, PaletteRampId, PaletteSlot, ShadeIndex, TEXTURE_PALETTE_SLOT_COUNT,
    TEXTURE_SIDE, TEXTURE_TEXEL_COUNT, TextureAlphaMode, TextureDefinition, TextureId,
    TexturePalette, TextureRegistry,
};
pub use texture_baking::{
    BakedBlockAppearance, BakedObjectAppearance, BakedTextureArray, BakedTextureDescriptor,
    IndexedMipLevel, TextureLayer, TexturePaletteRow,
};
