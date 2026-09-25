//! Deterministic authored lookup and cross-definition validation for finite energy stores.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::quantity::Energy;
use crate::core::time::{PhysicalTickDuration, TickSpan};
use crate::material::MaterialRegistry;

use super::{EnergyStoreDefinition, EnergyStoreDefinitionId};
use crate::energy::integration::{PowerRemainder, integrate_power};

/// Immutable deterministic authored lookup table for finite energy stores.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnergyRegistry {
    definitions: BTreeMap<EnergyStoreDefinitionId, EnergyStoreDefinition>,
    passive_dissipation_per_tick: BTreeMap<EnergyStoreDefinitionId, Energy>,
}

impl EnergyRegistry {
    pub(crate) fn new(definitions: impl IntoIterator<Item = EnergyStoreDefinition>) -> Self {
        let mut by_id = BTreeMap::new();
        for definition in definitions {
            let id = definition.id();
            assert!(
                by_id.insert(id, definition).is_none(),
                "duplicate energy store definition id {}",
                id.value()
            );
        }
        Self {
            definitions: by_id,
            passive_dissipation_per_tick: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn get_store(&self, id: EnergyStoreDefinitionId) -> Option<&EnergyStoreDefinition> {
        self.definitions.get(&id)
    }

    /// Iterates authored storage definitions in stable ID order.
    pub fn definitions(&self) -> impl Iterator<Item = &EnergyStoreDefinition> {
        self.definitions.values()
    }

    /// Resolves immutable tick-duration-dependent values once during root registry assembly.
    pub(crate) fn prepare_tick_dependent_values(
        &mut self,
        physical_tick_duration: PhysicalTickDuration,
    ) {
        self.passive_dissipation_per_tick.clear();
        for definition in self.definitions.values() {
            let dissipation_power = definition.passive_dissipation_power();
            if dissipation_power.is_zero() {
                continue;
            }
            let per_tick = resolve_passive_dissipation_per_tick(definition, physical_tick_duration);
            assert!(
                self.passive_dissipation_per_tick
                    .insert(definition.id(), per_tick)
                    .is_none(),
                "energy store definition {} passive-loss cache was populated twice",
                definition.id().value()
            );
        }
    }

    pub(crate) fn passive_dissipation_per_tick(
        &self,
        definition: EnergyStoreDefinitionId,
    ) -> Energy {
        self.passive_dissipation_per_tick
            .get(&definition)
            .copied()
            .unwrap_or(Energy::ZERO)
    }

    pub(crate) fn validate_references(
        &self,
        materials: &MaterialRegistry,
        physical_tick_duration: PhysicalTickDuration,
    ) {
        for definition in self.definitions.values() {
            if let Some(assembly) = definition.assembly_profile() {
                assert!(
                    assembly
                        .validate_infrastructure_references(materials)
                        .is_ok(),
                    "energy store definition {} assembly profile must use existing consolidated solid commodities",
                    definition.id().value()
                );
            }
            if !definition.passive_dissipation_power().is_zero()
                && !self
                    .passive_dissipation_per_tick
                    .contains_key(&definition.id())
            {
                let _ = resolve_passive_dissipation_per_tick(definition, physical_tick_duration);
            }
        }
        self.validate_upgrade_ancestry();
        for target in self.definitions.values() {
            let Some(upgrade) = target.upgrade_profile() else {
                continue;
            };
            let base = self.definitions.get(&upgrade.from()).unwrap_or_else(|| {
                panic!(
                    "energy store definition {} upgrade references missing base definition {}",
                    target.id().value(),
                    upgrade.from().value()
                )
            });
            assert!(
                upgrade
                    .additions()
                    .validate_infrastructure_references(materials)
                    .is_ok(),
                "energy store definition {} upgrade additions must use existing consolidated solid commodities",
                target.id().value()
            );
            assert_eq!(
                target.carrier(),
                base.carrier(),
                "energy store definition {} additive upgrade cannot change energy carrier",
                target.id().value()
            );
            assert!(
                target.capacity() >= base.capacity(),
                "energy store definition {} additive upgrade cannot reduce capacity",
                target.id().value()
            );
            assert!(
                target.max_input_power() >= base.max_input_power(),
                "energy store definition {} additive upgrade cannot reduce input power",
                target.id().value()
            );
            assert!(
                target.max_output_power() >= base.max_output_power(),
                "energy store definition {} additive upgrade cannot reduce output power",
                target.id().value()
            );
            assert!(
                target.passive_dissipation_power() <= base.passive_dissipation_power(),
                "energy store definition {} additive upgrade cannot increase passive loss",
                target.id().value()
            );
            let base_assembly = base.assembly_profile().unwrap_or_else(|| {
                panic!(
                    "energy store definition {} upgrade base {} has no material assembly profile",
                    target.id().value(),
                    base.id().value()
                )
            });
            let target_assembly = target.assembly_profile().unwrap_or_else(|| {
                panic!(
                    "energy store definition {} has an upgrade profile but no material assembly profile",
                    target.id().value()
                )
            });
            assert!(
                target_assembly.is_exact_additive_extension_of(base_assembly, upgrade.additions()),
                "energy store definition {} upgrade target assembly must equal base plus additive material",
                target.id().value()
            );
        }
    }

    fn validate_upgrade_ancestry(&self) {
        for definition in self.definitions.values() {
            let mut visited = BTreeSet::new();
            let mut current = definition;
            loop {
                assert!(
                    visited.insert(current.id()),
                    "energy store upgrade ancestry contains a cycle at definition {}",
                    current.id().value()
                );
                let Some(upgrade) = current.upgrade_profile() else {
                    break;
                };
                current = self.definitions.get(&upgrade.from()).unwrap_or_else(|| {
                    panic!(
                        "energy store definition {} upgrade references missing base definition {}",
                        current.id().value(),
                        upgrade.from().value()
                    )
                });
            }
        }
    }
}

fn resolve_passive_dissipation_per_tick(
    definition: &EnergyStoreDefinition,
    physical_tick_duration: PhysicalTickDuration,
) -> Energy {
    let integration = integrate_power(
        definition.passive_dissipation_power(),
        TickSpan::new(1),
        physical_tick_duration,
        PowerRemainder::ZERO,
    )
    .unwrap_or_else(|error| {
        panic!(
            "energy store definition {} passive dissipation cannot be integrated for one authoritative tick: {error}",
            definition.id().value()
        )
    });
    assert_eq!(
        integration.remainder(),
        PowerRemainder::ZERO,
        "energy store definition {} passive dissipation must resolve to exact whole nanojoules per authoritative tick",
        definition.id().value()
    );
    integration.energy()
}
