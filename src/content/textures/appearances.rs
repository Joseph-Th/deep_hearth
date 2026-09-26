//! Renderer-neutral object appearance definition assembly.

use crate::texture::{ObjectAppearanceDefinition, ObjectAppearanceId, TextureId};

mod first_foundry;
mod foundations;
mod primitive_workshop;
mod settlement_workshop;

pub(super) fn build_object_appearances() -> Vec<ObjectAppearanceDefinition> {
    foundations::definitions()
        .chain(first_foundry::definitions())
        .chain(primitive_workshop::definitions())
        .chain(settlement_workshop::definitions())
        .collect()
}

fn object(
    id: ObjectAppearanceId,
    name: &'static str,
    textures: &[TextureId],
) -> ObjectAppearanceDefinition {
    ObjectAppearanceDefinition::new(id, name, textures.to_vec())
}
