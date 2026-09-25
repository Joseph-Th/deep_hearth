//! Assembles deterministic WGSL dependencies into dense executable program sets.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::Write as _;
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
        let source_capacity = assembled_ids
            .iter()
            .try_fold(0_usize, |total, assembled_id| {
                let definition = self.get_shader(*assembled_id).unwrap_or_else(|| {
                    panic!(
                        "validated shader dependency {} disappeared during assembly sizing",
                        assembled_id.value()
                    )
                });
                total
                    .checked_add(definition.source().len())
                    .and_then(|value| value.checked_add(definition.name().len()))
                    .and_then(|value| value.checked_add(48))
            })
            .unwrap_or_else(|| panic!("assembled shader source size exceeds addressable memory"));
        let mut source = String::with_capacity(source_capacity);
        for assembled_id in assembled_ids {
            let definition = match self.get_shader(assembled_id) {
                Some(definition) => definition,
                None => panic!(
                    "validated shader dependency {} disappeared during assembly",
                    assembled_id.value()
                ),
            };
            writeln!(
                source,
                "// deep_hearth module {}: {}",
                assembled_id.value(),
                definition.name()
            )
            .unwrap_or_else(|_| unreachable!("writing to String cannot fail"));
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
        let lookup_len = self
            .program_ids()
            .map(|id| usize::from(id.value()))
            .max()
            .map_or(0, |maximum_id| maximum_id + 1);
        let mut programs_by_id = vec![None; lookup_len];
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

#[cfg(test)]
#[path = "shader_assembly_tests.rs"]
mod tests;
