//! Deterministic WGSL dependency composition and dense program baking for sibling definitions.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

use super::{ShaderId, ShaderProgramKind, ShaderRegistry};

/// Fully assembled executable WGSL program ready for adapter compilation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BakedShaderProgram {
    id: ShaderId,
    name: String,
    kind: ShaderProgramKind,
    source: String,
}

impl BakedShaderProgram {
    #[must_use]
    pub const fn id(&self) -> ShaderId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn kind(&self) -> &ShaderProgramKind {
        &self.kind
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
}

/// Dense startup-baked executable shader lookup keyed directly by stable ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BakedShaderSet {
    programs_by_id: Vec<Option<BakedShaderProgram>>,
}

impl BakedShaderSet {
    #[must_use]
    pub fn get_program(&self, id: ShaderId) -> Option<&BakedShaderProgram> {
        self.programs_by_id
            .get(usize::from(id.value()))
            .and_then(Option::as_ref)
    }

    #[must_use]
    pub fn program_count(&self) -> usize {
        self.programs_by_id
            .iter()
            .filter(|program| program.is_some())
            .count()
    }
}

/// Requested shader assembly cannot produce an executable program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShaderAssemblyError {
    UnknownShader { shader: ShaderId },
    LibraryIsNotExecutable { shader: ShaderId },
}

impl Display for ShaderAssemblyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownShader { shader } => {
                write!(formatter, "unknown shader {}", shader.value())
            }
            Self::LibraryIsNotExecutable { shader } => write!(
                formatter,
                "shader {} is a library and cannot be baked as an executable program",
                shader.value()
            ),
        }
    }
}

impl Error for ShaderAssemblyError {}

impl ShaderRegistry {
    /// Assembles one executable program and all transitive libraries exactly once in stable order.
    pub fn assemble_program(
        &self,
        id: ShaderId,
    ) -> Result<BakedShaderProgram, ShaderAssemblyError> {
        let root = self
            .get_shader(id)
            .ok_or(ShaderAssemblyError::UnknownShader { shader: id })?;
        if matches!(root.kind(), ShaderProgramKind::Library) {
            return Err(ShaderAssemblyError::LibraryIsNotExecutable { shader: id });
        }

        let mut assembled_ids = Vec::new();
        let mut visited = BTreeSet::new();
        collect_dependencies(self, id, &mut visited, &mut assembled_ids);
        let mut source = String::new();
        for assembled_id in assembled_ids {
            let definition = match self.get_shader(assembled_id) {
                Some(definition) => definition,
                None => panic!(
                    "validated shader dependency {} disappeared during assembly",
                    assembled_id.value()
                ),
            };
            source.push_str("// deep_hearth module ");
            source.push_str(&assembled_id.value().to_string());
            source.push_str(": ");
            source.push_str(definition.name());
            source.push('\n');
            source.push_str(definition.source());
            source.push_str("\n\n");
        }

        Ok(BakedShaderProgram {
            id,
            name: root.name().to_owned(),
            kind: root.kind().clone(),
            source,
        })
    }

    /// Preassembles every executable program into one bounded dense lookup.
    #[must_use]
    pub fn bake_shader_set(&self) -> BakedShaderSet {
        let maximum_id = self
            .program_ids()
            .map(|id| usize::from(id.value()))
            .max()
            .unwrap_or(0);
        let mut programs_by_id = vec![None; maximum_id + 1];
        for id in self.program_ids() {
            let program = match self.assemble_program(id) {
                Ok(program) => program,
                Err(error) => panic!("validated shader program failed assembly: {error}"),
            };
            programs_by_id[usize::from(id.value())] = Some(program);
        }
        BakedShaderSet { programs_by_id }
    }
}

fn collect_dependencies(
    registry: &ShaderRegistry,
    id: ShaderId,
    visited: &mut BTreeSet<ShaderId>,
    assembled: &mut Vec<ShaderId>,
) {
    if !visited.insert(id) {
        return;
    }
    let definition = match registry.get_shader(id) {
        Some(definition) => definition,
        None => panic!(
            "validated shader dependency {} disappeared during traversal",
            id.value()
        ),
    };
    for dependency in definition.dependencies() {
        collect_dependencies(registry, *dependency, visited, assembled);
    }
    assembled.push(id);
}

#[cfg(all(
    test,
    any(not(feature = "test-unit-sharded"), feature = "test-unit-render")
))]
mod tests {
    use super::*;
    use crate::shader::{
        RenderEntryPoints, RenderPipelineProfile, ShaderBlendMode, ShaderColorTarget,
        ShaderDefinition, ShaderDepthMode, ShaderWorkBudget,
    };

    const COMMON: ShaderId = ShaderId::new(1);
    const LIGHTING: ShaderId = ShaderId::new(2);
    const PROGRAM: ShaderId = ShaderId::new(3);

    fn registry() -> ShaderRegistry {
        ShaderRegistry::new([
            ShaderDefinition::new_library(COMMON, "common", Vec::new(), "const COMMON: u32 = 1u;"),
            ShaderDefinition::new_library(
                LIGHTING,
                "lighting",
                vec![COMMON],
                "const LIGHTING: u32 = COMMON;",
            ),
            ShaderDefinition::new_render(
                PROGRAM,
                "program",
                vec![COMMON, LIGHTING],
                "@vertex fn vs_main() -> @builtin(position) vec4<f32> { return vec4<f32>(); }\n@fragment fn fs_main() -> @location(0) vec4<f32> { return vec4<f32>(); }",
                RenderEntryPoints::new("vs_main", "fs_main"),
                RenderPipelineProfile::new(
                    ShaderBlendMode::Opaque,
                    ShaderDepthMode::ReadWrite,
                    ShaderColorTarget::LinearHdr,
                ),
                ShaderWorkBudget::new(0, 0, 0, 0),
            ),
        ])
    }

    #[test]
    fn assembly_is_transitive_deduplicated_and_stably_ordered() {
        let registry = registry();

        let first = match registry.assemble_program(PROGRAM) {
            Ok(program) => program,
            Err(error) => panic!("shader assembly fixture failed: {error}"),
        };
        let second = registry.bake_shader_set();

        assert_eq!(first.source().matches("deep_hearth module 1:").count(), 1);
        let common_position = match first.source().find("module 1:") {
            Some(position) => position,
            None => panic!("assembled fixture is missing its common module"),
        };
        let lighting_position = match first.source().find("module 2:") {
            Some(position) => position,
            None => panic!("assembled fixture is missing its lighting module"),
        };
        let program_position = match first.source().find("module 3:") {
            Some(position) => position,
            None => panic!("assembled fixture is missing its executable module"),
        };
        assert!(common_position < lighting_position);
        assert!(lighting_position < program_position);
        assert_eq!(second.get_program(PROGRAM), Some(&first));
        assert_eq!(second.program_count(), 1);
    }

    #[test]
    fn library_cannot_be_requested_as_an_executable_program() {
        assert_eq!(
            registry().assemble_program(COMMON),
            Err(ShaderAssemblyError::LibraryIsNotExecutable { shader: COMMON })
        );
    }
}
