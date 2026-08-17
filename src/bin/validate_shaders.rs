//! Minimal CI/local entry point for built-in Naga shader validation.

use std::process::ExitCode;

fn main() -> ExitCode {
    match deep_hearth::content::validate_builtin_shader_programs() {
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
