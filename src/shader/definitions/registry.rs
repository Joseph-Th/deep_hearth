//! Owns shader-definition indexing and dependency-graph validation.

use std::collections::{BTreeMap, BTreeSet};

use super::{ShaderDefinition, ShaderId, ShaderProgramKind};

/// Immutable shader source registry with fully validated library graphs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShaderRegistry {
    definitions: BTreeMap<ShaderId, ShaderDefinition>,
}

impl ShaderRegistry {
    pub(crate) fn new(definitions: impl IntoIterator<Item = ShaderDefinition>) -> Self {
        let mut by_id = BTreeMap::new();
        for definition in definitions {
            let id = definition.id();
            assert!(
                by_id.insert(id, definition).is_none(),
                "duplicate shader id {}",
                id.value()
            );
        }
        let registry = Self { definitions: by_id };
        registry.validate_dependencies();
        registry
    }

    #[must_use]
    pub fn get_shader(&self, id: ShaderId) -> Option<&ShaderDefinition> {
        self.definitions.get(&id)
    }

    pub fn program_ids(&self) -> impl Iterator<Item = ShaderId> + '_ {
        self.definitions
            .values()
            .filter_map(|definition| match definition.kind() {
                ShaderProgramKind::Library => None,
                ShaderProgramKind::Render {
                    entry_points: _,
                    pipeline: _,
                    work_budget: _,
                }
                | ShaderProgramKind::Compute {
                    entry_point: _,
                    work_budget: _,
                } => Some(definition.id()),
            })
    }

    fn validate_dependencies(&self) {
        let mut validated = BTreeSet::new();
        for id in self.definitions.keys().copied() {
            let mut visiting = BTreeSet::new();
            self.validate_dependency_branch(id, &mut visiting, &mut validated);
        }
    }

    fn validate_dependency_branch(
        &self,
        id: ShaderId,
        visiting: &mut BTreeSet<ShaderId>,
        validated: &mut BTreeSet<ShaderId>,
    ) {
        if validated.contains(&id) {
            return;
        }
        assert!(
            visiting.insert(id),
            "shader dependency cycle includes id {}",
            id.value()
        );
        let definition = match self.definitions.get(&id) {
            Some(definition) => definition,
            None => panic!("shader registry is missing definition {}", id.value()),
        };
        for dependency in definition.dependencies() {
            let dependency_definition = match self.definitions.get(dependency) {
                Some(dependency) => dependency,
                None => panic!(
                    "shader {} references missing dependency {}",
                    id.value(),
                    dependency.value()
                ),
            };
            assert!(
                matches!(dependency_definition.kind(), ShaderProgramKind::Library),
                "shader {} depends on executable program {}",
                id.value(),
                dependency.value()
            );
            self.validate_dependency_branch(*dependency, visiting, validated);
        }
        let was_visiting = visiting.remove(&id);
        assert!(
            was_visiting,
            "shader dependency traversal lost its active node"
        );
        validated.insert(id);
    }
}
