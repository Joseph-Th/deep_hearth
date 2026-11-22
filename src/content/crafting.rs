//! Built-in pre-machine hand-processing definitions.

use crate::crafting::CraftingRegistry;

mod clay;
mod copper;
mod food;
mod iron;
mod powered;
mod stone;
mod wood;

pub(crate) fn build_crafting_registry() -> CraftingRegistry {
    CraftingRegistry::new(
        stone::definitions()
            .into_iter()
            .chain(wood::definitions())
            .chain(clay::definitions())
            .chain(copper::definitions())
            .chain(food::definitions())
            .chain(iron::definitions()),
        powered::definitions(),
    )
}
