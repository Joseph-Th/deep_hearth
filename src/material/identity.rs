//! Stable material, form, and commodity identities.

use serde::{Deserialize, Serialize};

/// Stable authored material identifier used by registry and runtime references.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MaterialId(u32);

impl MaterialId {
    /// Builds a material identifier from its stable authored representation.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the stable authored representation.
    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Stable authored identifier for a physical material form such as log, lump, or ingot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FormId(u16);

impl FormId {
    /// Builds a form identifier from its stable authored representation.
    #[must_use]
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    /// Returns the stable authored representation.
    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

/// Runtime key for fungible matter sharing one material and physical form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CommodityKey(u64);

impl CommodityKey {
    /// Builds a material/form key. Registry validity is checked at operation boundaries.
    #[must_use]
    pub const fn new(material: MaterialId, form: FormId) -> Self {
        Self((material.value() as u64) << 16 | form.value() as u64)
    }

    /// Returns the material reference.
    #[must_use]
    pub const fn material(self) -> MaterialId {
        MaterialId::new((self.0 >> 16) as u32)
    }

    /// Returns the physical-form reference.
    #[must_use]
    pub const fn form(self) -> FormId {
        FormId::new((self.0 & u16::MAX as u64) as u16)
    }

    /// Returns the packed stable representation used for ordered storage and serialization.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}
