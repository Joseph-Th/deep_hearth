//! Built-in thermal-process resolution semantics; registrations remain empty until concrete processes are authored.

use crate::thermal::{SensibleHeatingProcessDefinition, ThermalRegistry};

pub(crate) fn build_thermal_registry() -> ThermalRegistry {
    ThermalRegistry::new(std::iter::empty::<SensibleHeatingProcessDefinition>())
}
