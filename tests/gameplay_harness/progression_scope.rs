//! Executable ordinary primitive progression scope.

use deep_hearth::registry::Registries;

use super::focused_seeds::FocusedProbeCase;
use super::{first_foundry_probe, primitive_liberation, progression_probe};

pub(super) fn run_primitive_progression_scope(registries: &Registries, case: FocusedProbeCase) {
    progression_probe::run_primitive_progression_probe(registries, case);
    primitive_liberation::run_primitive_liberation_probe(registries, case);
    first_foundry_probe::run_first_foundry_probe(registries, case);
}
