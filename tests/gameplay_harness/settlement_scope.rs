//! Ordinary settlement-to-foundry progression scope.

use deep_hearth::registry::Registries;

use super::focused_seeds::FocusedProbeCase;
use super::{first_foundry_probe, settlement_probe};

pub(super) fn run_settlement_progression_scope(registries: &Registries, case: FocusedProbeCase) {
    settlement_probe::run_settlement_probe(registries, case);
    first_foundry_probe::run_first_foundry_probe(registries, case);
}
