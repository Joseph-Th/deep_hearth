//! Built-in capability definitions; registrations remain empty until real equipment/tool/worker providers exist.

use crate::capability::CapabilityRegistry;

pub(crate) fn build_capability_registry() -> CapabilityRegistry {
    CapabilityRegistry::new()
}
