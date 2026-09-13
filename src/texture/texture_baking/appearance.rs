//! Dense renderer-facing appearance tables derived from validated texture definitions.

use super::{BakedBlockAppearance, BakedObjectAppearance, BakedTextureDescriptor};
use crate::texture::{TextureId, TextureRegistry};

pub(super) fn bake_block_appearances(
    registry: &TextureRegistry,
    descriptors: &[Option<BakedTextureDescriptor>],
) -> Vec<Option<BakedBlockAppearance>> {
    let maximum_id = registry
        .blocks_in_id_order()
        .map(|definition| usize::from(definition.id().value()))
        .max()
        .unwrap_or(0);
    let mut baked = vec![None; maximum_id + 1];
    for definition in registry.blocks_in_id_order() {
        let textures = definition
            .textures()
            .map(|texture| required_descriptor(descriptors, texture));
        baked[usize::from(definition.id().value())] = Some(BakedBlockAppearance { textures });
    }
    baked
}

pub(super) fn bake_object_appearances(
    registry: &TextureRegistry,
    descriptors: &[Option<BakedTextureDescriptor>],
) -> Vec<Option<BakedObjectAppearance>> {
    let maximum_id = registry
        .objects_in_id_order()
        .map(|definition| usize::from(definition.id().value()))
        .max()
        .unwrap_or(0);
    let mut baked = vec![None; maximum_id + 1];
    for definition in registry.objects_in_id_order() {
        let textures = definition
            .textures()
            .iter()
            .map(|texture| required_descriptor(descriptors, *texture))
            .collect();
        baked[usize::from(definition.id().value())] = Some(BakedObjectAppearance { textures });
    }
    baked
}

fn required_descriptor(
    descriptors: &[Option<BakedTextureDescriptor>],
    texture: TextureId,
) -> BakedTextureDescriptor {
    match descriptors
        .get(usize::from(texture.value()))
        .copied()
        .flatten()
    {
        Some(descriptor) => descriptor,
        None => panic!(
            "validated texture appearance references missing texture {} during bake",
            texture.value()
        ),
    }
}
