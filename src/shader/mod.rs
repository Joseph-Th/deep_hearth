//! Immutable WGSL program definitions and deterministic startup assembly for graphics adapters.

mod definitions;
mod shader_assembly;

pub use definitions::{
    ComputeEntryPoint, RenderEntryPoints, RenderPipelineProfile, ShaderBlendMode,
    ShaderColorTarget, ShaderDefinition, ShaderDepthMode, ShaderId, ShaderProgramKind,
    ShaderRegistry, ShaderWorkBudget,
};
pub use shader_assembly::{BakedShaderProgram, BakedShaderSet, ShaderAssemblyError};
