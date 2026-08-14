//! Built-in ore-processing semantics; registrations remain empty until concrete equipment and processes are authored.

use crate::ore_processing::{ComminutionProcessDefinition, OreProcessingRegistry};

pub(crate) fn build_ore_processing_registry() -> OreProcessingRegistry {
    OreProcessingRegistry::new(std::iter::empty::<ComminutionProcessDefinition>())
}
