//! Contract tests for deterministic shader assembly.

use super::*;
use crate::shader::{
    RenderEntryPoints, RenderPipelineProfile, ShaderBlendMode, ShaderColorTarget, ShaderDefinition,
    ShaderDepthMode, ShaderWorkBudget,
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
