//! Deterministic manual-crafting topology indexes and authored reference validation.

use std::collections::BTreeMap;

use crate::material::{CommodityKey, MaterialRegistry, ParticleSizeStatePolicy};
use crate::production::{ProcessId, ProcessInputPolicy, ProductionRegistry};

use super::ManualCraftDefinition;

/// Deterministic immutable lookup for manual shaping semantics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CraftingRegistry {
    manual: BTreeMap<ProcessId, ManualCraftDefinition>,
    producers_by_output: BTreeMap<CommodityKey, Vec<ProcessId>>,
    consumers_by_input: BTreeMap<CommodityKey, Vec<ProcessId>>,
}

impl CraftingRegistry {
    pub(crate) fn new(definitions: impl IntoIterator<Item = ManualCraftDefinition>) -> Self {
        let mut manual = BTreeMap::new();
        for definition in definitions {
            let process = definition.process();
            assert!(
                manual.insert(process, definition).is_none(),
                "duplicate manual craft process {}",
                process.value()
            );
        }
        let mut producers_by_output: BTreeMap<CommodityKey, Vec<ProcessId>> = BTreeMap::new();
        let mut consumers_by_input: BTreeMap<CommodityKey, Vec<ProcessId>> = BTreeMap::new();
        for definition in manual.values() {
            consumers_by_input
                .entry(definition.input())
                .or_default()
                .push(definition.process());
            for output in definition.outputs() {
                producers_by_output
                    .entry(output.commodity())
                    .or_default()
                    .push(definition.process());
            }
        }
        Self {
            manual,
            producers_by_output,
            consumers_by_input,
        }
    }

    #[must_use]
    pub fn get_manual(&self, process: ProcessId) -> Option<&ManualCraftDefinition> {
        self.manual.get(&process)
    }

    pub fn definitions(&self) -> impl Iterator<Item = &ManualCraftDefinition> {
        self.manual.values()
    }

    /// Iterates manual processes that directly produce the requested commodity in stable process-ID
    /// order. This is a direct authored edge, not a transitive reachability claim.
    pub fn manual_producers(
        &self,
        output: CommodityKey,
    ) -> impl Iterator<Item = &ManualCraftDefinition> {
        self.producers_by_output
            .get(&output)
            .into_iter()
            .flat_map(|processes| processes.iter())
            .map(|process| {
                self.manual.get(process).unwrap_or_else(|| {
                    unreachable!("crafting producer index references known process")
                })
            })
    }

    /// Iterates manual processes that directly consume the requested commodity in stable process-ID
    /// order. This is a direct authored edge, not a transitive reachability claim.
    pub fn manual_consumers(
        &self,
        input: CommodityKey,
    ) -> impl Iterator<Item = &ManualCraftDefinition> {
        self.consumers_by_input
            .get(&input)
            .into_iter()
            .flat_map(|processes| processes.iter())
            .map(|process| {
                self.manual.get(process).unwrap_or_else(|| {
                    unreachable!("crafting consumer index references known process")
                })
            })
    }

    pub(crate) fn validate_references(
        &self,
        production: &ProductionRegistry,
        materials: &MaterialRegistry,
    ) {
        for definition in self.manual.values() {
            assert!(
                materials.has_commodity(definition.input()),
                "manual craft {} references unknown input commodity {}",
                definition.process().value(),
                definition.input().value()
            );
            let input_form = materials
                .get_form(definition.input().form())
                .unwrap_or_else(|| {
                    panic!(
                        "manual craft {} references unknown input form {}",
                        definition.process().value(),
                        definition.input().form().value()
                    )
                });
            for output in definition.outputs() {
                assert!(
                    materials.has_commodity(output.commodity()),
                    "manual craft {} references unknown output commodity {}",
                    definition.process().value(),
                    output.commodity().value()
                );
                let output_form = materials
                    .get_form(output.commodity().form())
                    .unwrap_or_else(|| {
                        panic!(
                            "manual craft {} references unknown output form {}",
                            definition.process().value(),
                            output.commodity().form().value()
                        )
                    });
                assert_eq!(
                    output_form.phase(),
                    input_form.phase(),
                    "manual craft {} cannot change material phase from {:?} to {:?} without thermal physics",
                    definition.process().value(),
                    input_form.phase(),
                    output_form.phase()
                );
                assert_eq!(
                    output_form.particle_size_policy(),
                    ParticleSizeStatePolicy::Untracked,
                    "manual craft {} output form {} cannot require particle-size state because manual shaping has no authored particulate output distribution",
                    definition.process().value(),
                    output.commodity().form().value()
                );
            }
            let process = production
                .get_process(definition.process())
                .unwrap_or_else(|| {
                    panic!(
                        "manual craft {} has no production definition",
                        definition.process().value()
                    )
                });
            assert!(
                process.capability_requirements().is_empty(),
                "manual craft {} cannot require machine capabilities",
                definition.process().value()
            );
            assert!(
                matches!(process.input_policy(), ProcessInputPolicy::SelectedBatch),
                "manual craft {} must use selected-batch production input policy because crafting owns exact recipe and lot eligibility",
                definition.process().value()
            );
        }
    }
}
