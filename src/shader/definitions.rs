//! Typed shader modules, bounded work profiles, and dependency validation used by sibling assembly.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

const MAX_SHADER_ID: u16 = 4_095;
const MAX_TEXTURE_SAMPLES_PER_INVOCATION: u8 = 16;
const MAX_PROCEDURAL_NOISE_LAYERS_PER_INVOCATION: u8 = 8;
const MAX_DYNAMIC_LIGHTS_PER_FRAGMENT: u8 = 32;
const MAX_LOOP_ITERATIONS_PER_INVOCATION: u16 = 64;
const MAX_COMPUTE_WORKGROUP_INVOCATIONS: u32 = 1_024;

/// Stable authored identifier for one WGSL library or executable program.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ShaderId(u16);

impl ShaderId {
    #[must_use]
    pub const fn new(value: u16) -> Self {
        assert!(value != 0, "shader id must be nonzero");
        assert!(
            value <= MAX_SHADER_ID,
            "shader id exceeds the dense startup-lookup limit"
        );
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

/// Blend contract an adapter uses when creating a render pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ShaderBlendMode {
    Opaque,
    PremultipliedAlpha,
}

/// Depth attachment behavior required by a render program.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ShaderDepthMode {
    Disabled,
    ReadOnly,
    ReadWrite,
}

/// Semantic color target class required by a render program.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ShaderColorTarget {
    None,
    LinearHdr,
    Display,
}

/// Authored vertex and optional fragment entry-point identifiers for one render program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderEntryPoints {
    vertex: String,
    fragment: Option<String>,
}

impl RenderEntryPoints {
    #[must_use]
    pub fn new(vertex: impl Into<String>, fragment: impl Into<String>) -> Self {
        let vertex = vertex.into();
        let fragment = fragment.into();
        assert!(
            !vertex.trim().is_empty(),
            "render vertex entry point must not be empty"
        );
        assert!(
            !fragment.trim().is_empty(),
            "render fragment entry point must not be empty"
        );
        Self {
            vertex,
            fragment: Some(fragment),
        }
    }

    #[must_use]
    pub fn new_vertex_only(vertex: impl Into<String>) -> Self {
        let vertex = vertex.into();
        assert!(
            !vertex.trim().is_empty(),
            "render vertex entry point must not be empty"
        );
        Self {
            vertex,
            fragment: None,
        }
    }

    #[must_use]
    pub fn vertex(&self) -> &str {
        &self.vertex
    }

    #[must_use]
    pub fn fragment(&self) -> Option<&str> {
        self.fragment.as_deref()
    }
}

/// Fixed-function state required by one renderer-neutral render program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderPipelineProfile {
    blend_mode: ShaderBlendMode,
    depth_mode: ShaderDepthMode,
    color_target: ShaderColorTarget,
}

impl RenderPipelineProfile {
    #[must_use]
    pub const fn new(
        blend_mode: ShaderBlendMode,
        depth_mode: ShaderDepthMode,
        color_target: ShaderColorTarget,
    ) -> Self {
        match color_target {
            ShaderColorTarget::None => match blend_mode {
                ShaderBlendMode::Opaque => {}
                ShaderBlendMode::PremultipliedAlpha => {
                    panic!("a colorless render pipeline cannot enable color blending")
                }
            },
            ShaderColorTarget::LinearHdr | ShaderColorTarget::Display => {}
        }
        Self {
            blend_mode,
            depth_mode,
            color_target,
        }
    }

    #[must_use]
    pub const fn blend_mode(self) -> ShaderBlendMode {
        self.blend_mode
    }

    #[must_use]
    pub const fn depth_mode(self) -> ShaderDepthMode {
        self.depth_mode
    }

    #[must_use]
    pub const fn color_target(self) -> ShaderColorTarget {
        self.color_target
    }
}

/// Authored compute entry point and dispatch-local invocation dimensions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputeEntryPoint {
    name: String,
    workgroup_size: [u16; 3],
}

impl ComputeEntryPoint {
    #[must_use]
    pub fn new(name: impl Into<String>, workgroup_size: [u16; 3]) -> Self {
        let name = name.into();
        assert!(
            !name.trim().is_empty(),
            "compute entry point must not be empty"
        );
        assert!(
            workgroup_size.iter().all(|dimension| *dimension != 0),
            "compute workgroup dimensions must be nonzero"
        );
        let invocation_count = workgroup_size
            .iter()
            .map(|dimension| u64::from(*dimension))
            .product::<u64>();
        assert!(
            invocation_count <= u64::from(MAX_COMPUTE_WORKGROUP_INVOCATIONS),
            "compute workgroup exceeds the portable invocation limit"
        );
        Self {
            name,
            workgroup_size,
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn workgroup_size(&self) -> [u16; 3] {
        self.workgroup_size
    }
}

/// Auditable upper bounds for expensive work in one shader invocation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShaderWorkBudget {
    texture_samples: u8,
    procedural_noise_layers: u8,
    dynamic_lights: u8,
    loop_iterations: u16,
}

impl ShaderWorkBudget {
    #[must_use]
    pub const fn new(
        texture_samples: u8,
        procedural_noise_layers: u8,
        dynamic_lights: u8,
        loop_iterations: u16,
    ) -> Self {
        assert!(
            texture_samples <= MAX_TEXTURE_SAMPLES_PER_INVOCATION,
            "shader texture-sample budget exceeds the project limit"
        );
        assert!(
            procedural_noise_layers <= MAX_PROCEDURAL_NOISE_LAYERS_PER_INVOCATION,
            "shader procedural-noise budget exceeds the project limit"
        );
        assert!(
            dynamic_lights <= MAX_DYNAMIC_LIGHTS_PER_FRAGMENT,
            "shader dynamic-light budget exceeds the project limit"
        );
        assert!(
            loop_iterations <= MAX_LOOP_ITERATIONS_PER_INVOCATION,
            "shader loop budget exceeds the project limit"
        );
        Self {
            texture_samples,
            procedural_noise_layers,
            dynamic_lights,
            loop_iterations,
        }
    }

    #[must_use]
    pub const fn texture_samples(self) -> u8 {
        self.texture_samples
    }

    #[must_use]
    pub const fn procedural_noise_layers(self) -> u8 {
        self.procedural_noise_layers
    }

    #[must_use]
    pub const fn dynamic_lights(self) -> u8 {
        self.dynamic_lights
    }

    #[must_use]
    pub const fn loop_iterations(self) -> u16 {
        self.loop_iterations
    }
}

/// Whether one source module is a shared library or an executable GPU program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShaderProgramKind {
    Library,
    Render {
        entry_points: RenderEntryPoints,
        pipeline: RenderPipelineProfile,
        work_budget: ShaderWorkBudget,
    },
    Compute {
        entry_point: ComputeEntryPoint,
        work_budget: ShaderWorkBudget,
    },
}

impl ShaderProgramKind {
    #[must_use]
    pub const fn work_budget(&self) -> Option<ShaderWorkBudget> {
        match self {
            Self::Library => None,
            Self::Render {
                entry_points: _,
                pipeline: _,
                work_budget,
            }
            | Self::Compute {
                entry_point: _,
                work_budget,
            } => Some(*work_budget),
        }
    }
}

/// Immutable authored WGSL source module with canonical library dependencies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShaderDefinition {
    id: ShaderId,
    name: String,
    kind: ShaderProgramKind,
    dependencies: Vec<ShaderId>,
    source: String,
}

impl ShaderDefinition {
    #[must_use]
    pub fn new_library(
        id: ShaderId,
        name: impl Into<String>,
        dependencies: Vec<ShaderId>,
        source: impl Into<String>,
    ) -> Self {
        Self::new(id, name, ShaderProgramKind::Library, dependencies, source)
    }

    #[must_use]
    pub fn new_render(
        id: ShaderId,
        name: impl Into<String>,
        dependencies: Vec<ShaderId>,
        source: impl Into<String>,
        entry_points: RenderEntryPoints,
        pipeline: RenderPipelineProfile,
        work_budget: ShaderWorkBudget,
    ) -> Self {
        match pipeline.color_target() {
            ShaderColorTarget::None => {}
            ShaderColorTarget::LinearHdr | ShaderColorTarget::Display => assert!(
                entry_points.fragment().is_some(),
                "a color render pipeline requires a fragment entry point"
            ),
        }
        Self::new(
            id,
            name,
            ShaderProgramKind::Render {
                entry_points,
                pipeline,
                work_budget,
            },
            dependencies,
            source,
        )
    }

    #[must_use]
    pub fn new_compute(
        id: ShaderId,
        name: impl Into<String>,
        dependencies: Vec<ShaderId>,
        source: impl Into<String>,
        entry_point: ComputeEntryPoint,
        work_budget: ShaderWorkBudget,
    ) -> Self {
        Self::new(
            id,
            name,
            ShaderProgramKind::Compute {
                entry_point,
                work_budget,
            },
            dependencies,
            source,
        )
    }

    fn new(
        id: ShaderId,
        name: impl Into<String>,
        kind: ShaderProgramKind,
        mut dependencies: Vec<ShaderId>,
        source: impl Into<String>,
    ) -> Self {
        let name = name.into();
        let source = source.into();
        assert!(!name.trim().is_empty(), "shader name must not be empty");
        assert!(!source.trim().is_empty(), "shader source must not be empty");
        dependencies.sort_unstable();
        for pair in dependencies.windows(2) {
            assert_ne!(
                pair[0],
                pair[1],
                "shader {} repeats dependency {}",
                id.value(),
                pair[0].value()
            );
        }
        Self {
            id,
            name,
            kind,
            dependencies,
            source,
        }
    }

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
    pub fn dependencies(&self) -> &[ShaderId] {
        &self.dependencies
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
}

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

    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        Self::default()
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

#[cfg(test)]
#[path = "definitions_tests.rs"]
mod tests;
