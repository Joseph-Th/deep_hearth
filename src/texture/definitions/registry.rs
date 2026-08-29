//! Immutable texture-registry assembly, lookup, and cross-reference validation.

use std::collections::BTreeMap;

use crate::equipment::{EquipmentDefinitionId, EquipmentRegistry};
use crate::material::{CommodityKey, MaterialRegistry};

use super::{
    BlockAppearanceDefinition, BlockAppearanceId, CommodityAppearanceBinding,
    EquipmentAppearanceBinding, ObjectAppearanceDefinition, ObjectAppearanceId,
    PaletteRampDefinition, PaletteRampId, TextureAlphaMode, TextureDefinition, TextureId,
};

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
            self.validate_texture_local_references(texture);
        }
        for block in self.blocks.values() {
            self.validate_block_texture_references(block);
        }
        for object in self.objects.values() {
            self.validate_object_texture_references(object);
        }
        for binding in self.commodity_bindings.values() {
            self.validate_commodity_binding_local_references(*binding);
        }
        for binding in self.equipment_bindings.values() {
            self.validate_equipment_binding_local_references(*binding);
        }
    }

    fn validate_texture_local_references(&self, texture: &TextureDefinition) {
        for ramp in texture.palette().ramps() {
            assert!(
                self.ramps.contains_key(ramp),
                "texture {} references missing palette ramp {}",
                texture.id().value(),
                ramp.value()
            );
        }
        for texel in texture.texels() {
            let ramp = texture
                .palette()
                .get_ramp(texel.palette_slot())
                .unwrap_or_else(|| {
                    panic!(
                        "texture {} contains an invalid local palette slot",
                        texture.id().value()
                    )
                });
            let alpha = self
                .ramps
                .get(&ramp)
                .unwrap_or_else(|| {
                    panic!(
                        "texture {} references missing palette ramp {}",
                        texture.id().value(),
                        ramp.value()
                    )
                })
                .color(texel.shade())
                .alpha();
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

    fn validate_block_texture_references(&self, block: &BlockAppearanceDefinition) {
        for texture in block.textures() {
            assert!(
                self.textures.contains_key(texture),
                "block appearance {} references missing texture {}",
                block.id().value(),
                texture.value()
            );
        }
    }

    fn validate_object_texture_references(&self, object: &ObjectAppearanceDefinition) {
        for texture in object.textures() {
            assert!(
                self.textures.contains_key(texture),
                "object appearance {} references missing texture {}",
                object.id().value(),
                texture.value()
            );
        }
    }

    fn validate_commodity_binding_local_references(&self, binding: CommodityAppearanceBinding) {
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

    fn validate_equipment_binding_local_references(&self, binding: EquipmentAppearanceBinding) {
        assert!(
            self.objects.contains_key(&binding.object()),
            "equipment {} references missing object appearance {}",
            binding.equipment().value(),
            binding.object().value()
        );
    }

    pub(in crate::texture) fn ramps_in_id_order(
        &self,
    ) -> impl ExactSizeIterator<Item = &PaletteRampDefinition> {
        self.ramps.values()
    }

    pub(in crate::texture) fn textures_in_id_order(
        &self,
    ) -> impl ExactSizeIterator<Item = &TextureDefinition> {
        self.textures.values()
    }

    pub(in crate::texture) fn blocks_in_id_order(
        &self,
    ) -> impl ExactSizeIterator<Item = &BlockAppearanceDefinition> {
        self.blocks.values()
    }

    pub(in crate::texture) fn objects_in_id_order(
        &self,
    ) -> impl ExactSizeIterator<Item = &ObjectAppearanceDefinition> {
        self.objects.values()
    }
}
