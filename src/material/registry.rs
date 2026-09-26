//! Immutable authored material registry storage and reference validation.

use std::collections::BTreeMap;

use super::definitions::{FormDefinition, MaterialDefinition, MaterialPhase};
use super::identity::{CommodityKey, FormId, MaterialId};

/// Immutable deterministic lookup tables for materials and their physical forms.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MaterialRegistry {
    materials: BTreeMap<MaterialId, MaterialDefinition>,
    forms: BTreeMap<FormId, FormDefinition>,
    commodity_names: BTreeMap<CommodityKey, String>,
}

impl MaterialRegistry {
    /// Builds an empty registry for code-owned startup assembly.
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            materials: BTreeMap::new(),
            forms: BTreeMap::new(),
            commodity_names: BTreeMap::new(),
        }
    }

    /// Registers one authored material, panicking immediately on an ID collision.
    pub(crate) fn register_material(&mut self, definition: MaterialDefinition) {
        let id = definition.id();
        assert!(
            self.materials.insert(id, definition).is_none(),
            "duplicate material id {}",
            id.value()
        );
    }

    /// Registers one exact authored material/form combination.
    pub(crate) fn register_commodity(&mut self, commodity: CommodityKey, name: impl Into<String>) {
        let material = self
            .materials
            .get(&commodity.material())
            .unwrap_or_else(|| {
                panic!(
                    "commodity references missing material {}",
                    commodity.material().value()
                )
            });
        let form = self.forms.get(&commodity.form()).unwrap_or_else(|| {
            panic!(
                "commodity references missing form {}",
                commodity.form().value()
            )
        });
        assert!(
            form.phase() != MaterialPhase::Liquid
                || material.properties().thermal().fusion().is_some(),
            "liquid commodity material {} form {} requires authored fusion properties",
            commodity.material().value(),
            commodity.form().value()
        );
        let name = name.into();
        assert!(
            !name.trim().is_empty(),
            "commodity material {} form {} name must not be empty",
            commodity.material().value(),
            commodity.form().value()
        );
        assert!(
            self.commodity_names.insert(commodity, name).is_none(),
            "duplicate commodity material {} form {}",
            commodity.material().value(),
            commodity.form().value()
        );
    }

    /// Registers one authored form, panicking immediately on an ID collision.
    pub(crate) fn register_form(&mut self, definition: FormDefinition) {
        let id = definition.id();
        assert!(
            self.forms.insert(id, definition).is_none(),
            "duplicate material form id {}",
            id.value()
        );
    }

    /// Returns one material definition by stable ID.
    #[must_use]
    pub fn get_material(&self, id: MaterialId) -> Option<&MaterialDefinition> {
        self.materials.get(&id)
    }

    /// Iterates authored materials deterministically by stable material ID.
    ///
    /// This is a read-only catalog surface for presentation, handbook, and planning code. Runtime
    /// ownership still belongs to material lots and their containing systems.
    pub fn definitions(&self) -> impl Iterator<Item = &MaterialDefinition> {
        self.materials.values()
    }

    /// Iterates authored physical forms deterministically by stable form ID.
    pub fn forms(&self) -> impl Iterator<Item = &FormDefinition> {
        self.forms.values()
    }

    /// Iterates exact authored material/form commodities in stable key order.
    pub fn commodities(&self) -> impl Iterator<Item = CommodityKey> + '_ {
        self.commodity_names.keys().copied()
    }

    /// Returns the authored player-facing name for one exact material/form commodity.
    #[must_use]
    pub fn commodity_name(&self, commodity: CommodityKey) -> Option<&str> {
        self.commodity_names.get(&commodity).map(String::as_str)
    }

    /// Returns one physical-form definition by stable ID.
    #[must_use]
    pub fn get_form(&self, id: FormId) -> Option<&FormDefinition> {
        self.forms.get(&id)
    }

    /// Reports whether the exact material/form combination is authored for runtime ownership.
    #[must_use]
    pub fn has_commodity(&self, commodity: CommodityKey) -> bool {
        self.commodity_names.contains_key(&commodity)
    }
}
