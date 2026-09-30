"""Single source of truth for focused gameplay verification targets."""

from __future__ import annotations

from dataclasses import dataclass


GAMEPLAY_CONTRACTS_TARGET = "gameplay_contracts"
GAMEPLAY_AUDIT_TARGET = "gameplay_audit"
GAMEPLAY_FEATURE = "test-gameplay"
GAMEPLAY_VARIATION_ENV = "DEEP_HEARTH_GAMEPLAY_VARIATION_SEED"
GAMEPLAY_BEHAVIOR_ENV = "DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED"
GAMEPLAY_REPORT_MODE_ENV = "DEEP_HEARTH_GAMEPLAY_REPORT"


@dataclass(frozen=True)
class GameplayScopeSpec:
    target: str
    test: str
    uses_behavior_seed: bool = False


GAMEPLAY_SCOPE_SPECS = {
    "workshop": GameplayScopeSpec("gameplay_workshop", "gameplay_harness_gate", True),
    "survival": GameplayScopeSpec(
        "gameplay_survival",
        "gameplay_survival_provisioning_probe",
        True,
    ),
    "progression": GameplayScopeSpec(
        "gameplay_progression",
        "gameplay_primitive_progression_probe",
    ),
    "liberation": GameplayScopeSpec(
        "gameplay_liberation",
        "gameplay_primitive_liberation_probe",
    ),
    "settlement": GameplayScopeSpec(
        "gameplay_settlement",
        "gameplay_settlement_probe",
        True,
    ),
    "foundry-bootstrap": GameplayScopeSpec(
        "gameplay_foundry_bootstrap",
        "gameplay_foundry_bootstrap_probe",
    ),
    "woodworking": GameplayScopeSpec(
        "gameplay_woodworking",
        "gameplay_woodworking_probe",
        True,
    ),
    "fieldwork": GameplayScopeSpec("gameplay_fieldwork", "gameplay_fieldwork_probe", True),
    "power-provider": GameplayScopeSpec(
        "gameplay_power",
        "gameplay_power_provider_probe",
        True,
    ),
    "ore": GameplayScopeSpec("gameplay_ore", "gameplay_ore_preparation_probe"),
    "foundry": GameplayScopeSpec("gameplay_foundry", "gameplay_foundry_probe"),
}

GAMEPLAY_TARGETS = {scope: spec.target for scope, spec in GAMEPLAY_SCOPE_SPECS.items()}
GAMEPLAY_TESTS = {scope: spec.test for scope, spec in GAMEPLAY_SCOPE_SPECS.items()}
GAMEPLAY_PROBE_TARGETS = {spec.test: spec.target for spec in GAMEPLAY_SCOPE_SPECS.values()}
GAMEPLAY_PROBE_TARGETS["gameplay_agency_counterfactuals"] = GAMEPLAY_AUDIT_TARGET
GAMEPLAY_PROBE_TESTS = frozenset(GAMEPLAY_PROBE_TARGETS)
GAMEPLAY_BEHAVIOR_PROBE_TESTS = frozenset(
    spec.test for spec in GAMEPLAY_SCOPE_SPECS.values() if spec.uses_behavior_seed
)
