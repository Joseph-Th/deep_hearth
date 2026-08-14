//! Built-in thermal-process resolution semantics; registrations remain empty until concrete processes are authored.

use crate::thermal::{
    CastingProcessDefinition, MeltingProcessDefinition, SensibleHeatingProcessDefinition,
    ThermalRegistry,
};

pub(crate) fn build_thermal_registry() -> ThermalRegistry {
    ThermalRegistry::new(
        std::iter::empty::<SensibleHeatingProcessDefinition>(),
        std::iter::empty::<MeltingProcessDefinition>(),
        std::iter::empty::<CastingProcessDefinition>(),
    )
}
