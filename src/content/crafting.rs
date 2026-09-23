//! Built-in pre-machine hand-processing definitions.

use crate::crafting::CraftingRegistry;

mod copper;
mod powered;
mod stone;
mod wood;

pub(crate) fn build_crafting_registry() -> CraftingRegistry {
    CraftingRegistry::new_with_powered(
        stone::definitions()
            .into_iter()
            .chain(wood::definitions())
            .chain(copper::definitions()),
        powered::definitions(),
    )
}
