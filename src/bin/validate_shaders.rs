//! Minimal CI/local entry point for built-in Naga shader validation.

use std::fmt::{Display, Formatter};
use std::process::ExitCode;

use deep_hearth::shader::{ShaderId, ShaderProgramKind};
use naga::ShaderStage;
use naga::valid::{Capabilities, ValidationFlags, Validator};

#[derive(Clone, Debug, PartialEq, Eq)]
enum ShaderValidationError {
    ProgramCount {
        expected: usize,
        actual: usize,
    },
    MissingProgram {
        shader: ShaderId,
    },
    Parse {
        shader: ShaderId,
        message: String,
    },
    Validation {
        shader: ShaderId,
        message: String,
    },
    ExecutableLibrary {
        shader: ShaderId,
    },
    MissingEntryPoint {
        shader: ShaderId,
        stage: &'static str,
        entry_point: String,
    },
    UnexpectedFragmentEntryPoint {
        shader: ShaderId,
    },
}

impl Display for ShaderValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProgramCount { expected, actual } => write!(
                formatter,
                "built-in shader bake produced {actual} executable programs; expected {expected}"
            ),
            Self::MissingProgram { shader } => write!(
                formatter,
                "built-in executable shader {} was not baked",
                shader.value()
            ),
            Self::Parse { shader, message } => write!(
                formatter,
                "shader {} failed WGSL parsing: {message}",
                shader.value()
            ),
            Self::Validation { shader, message } => write!(
                formatter,
                "shader {} failed WGSL validation: {message}",
                shader.value()
            ),
            Self::ExecutableLibrary { shader } => write!(
                formatter,
                "executable shader {} baked as a library",
                shader.value()
            ),
            Self::MissingEntryPoint {
                shader,
                stage,
                entry_point,
            } => write!(
                formatter,
                "shader {} is missing {stage} entry point {entry_point}",
                shader.value()
            ),
            Self::UnexpectedFragmentEntryPoint { shader } => write!(
                formatter,
                "vertex-only shader {} unexpectedly contains a fragment entry point",
                shader.value()
            ),
        }
    }
}

fn require_entry_point(
    shader: ShaderId,
    module: &naga::Module,
    stage: ShaderStage,
    stage_name: &'static str,
    entry_point: &str,
) -> Result<(), ShaderValidationError> {
    if module
        .entry_points
        .iter()
        .any(|entry| entry.stage == stage && entry.name == entry_point)
    {
        return Ok(());
    }
    Err(ShaderValidationError::MissingEntryPoint {
        shader,
        stage: stage_name,
        entry_point: entry_point.to_owned(),
    })
}

fn validate_builtin_shader_programs() -> Result<usize, ShaderValidationError> {
    let registries = deep_hearth::build_registries();
    let shader_registry = registries.shaders();
    let executable_programs = shader_registry.program_ids().collect::<Vec<_>>();
    let baked = shader_registry.bake_shader_set();
    let actual = baked.program_count();
    if actual != executable_programs.len() {
        return Err(ShaderValidationError::ProgramCount {
            expected: executable_programs.len(),
            actual,
        });
    }

    for shader in executable_programs.iter().copied() {
        let program = baked
            .get_program(shader)
            .ok_or(ShaderValidationError::MissingProgram { shader })?;
        let module = naga::front::wgsl::parse_str(program.source()).map_err(|error| {
            ShaderValidationError::Parse {
                shader,
                message: error.emit_to_string(program.source()),
            }
        })?;
        Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .map_err(|error| ShaderValidationError::Validation {
                shader,
                message: format!("{error:?}"),
            })?;

        match program.kind() {
            ShaderProgramKind::Library => {
                return Err(ShaderValidationError::ExecutableLibrary { shader });
            }
            ShaderProgramKind::Render {
                entry_points,
                pipeline: _,
                work_budget: _,
            } => {
                require_entry_point(
                    shader,
                    &module,
                    ShaderStage::Vertex,
                    "vertex",
                    entry_points.vertex(),
                )?;
                if let Some(fragment) = entry_points.fragment() {
                    require_entry_point(
                        shader,
                        &module,
                        ShaderStage::Fragment,
                        "fragment",
                        fragment,
                    )?;
                } else if module
                    .entry_points
                    .iter()
                    .any(|entry| entry.stage == ShaderStage::Fragment)
                {
                    return Err(ShaderValidationError::UnexpectedFragmentEntryPoint { shader });
                }
            }
            ShaderProgramKind::Compute {
                entry_point,
                work_budget: _,
            } => require_entry_point(
                shader,
                &module,
                ShaderStage::Compute,
                "compute",
                entry_point.name(),
            )?,
        }
    }
    Ok(executable_programs.len())
}

fn main() -> ExitCode {
    match validate_builtin_shader_programs() {
        Ok(programs) => {
            println!("SHADERS PASS programs={programs}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("SHADERS FAIL {error}");
            ExitCode::FAILURE
        }
    }
}
