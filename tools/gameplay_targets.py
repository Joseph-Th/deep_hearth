"""Single source of truth for focused gameplay verification targets."""

from __future__ import annotations

from dataclasses import dataclass


GAMEPLAY_CONTRACTS_TARGET = "gameplay_contracts"
GAMEPLAY_AUDIT_TARGET = "gameplay_audit"
GAMEPLAY_AGENCY_TARGET = "gameplay_agency"
GAMEPLAY_FEATURE = "test-gameplay"
GAMEPLAY_VARIATION_ENV = "DEEP_HEARTH_GAMEPLAY_VARIATION_SEED"
GAMEPLAY_BEHAVIOR_ENV = "DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED"
GAMEPLAY_VARIATION_SCOPE_ENV = "DEEP_HEARTH_GAMEPLAY_VARIATION_SCOPE"
GAMEPLAY_REPORT_MODE_ENV = "DEEP_HEARTH_GAMEPLAY_REPORT"

EVIDENCE_ORDINARY_EXACT_LOCAL = "ordinary-exact-local-after-disclosed-bootstrap"
EVIDENCE_ORDINARY_SPATIAL_PROXY = "ordinary-system-spatial-proxy"
EVIDENCE_CONTROLLED_CAPABILITY = "controlled-capability"
EVIDENCE_COUNTERFACTUAL = "counterfactual"


@dataclass(frozen=True)
class GameplayScopeSpec:
    target: str
    test: str
    uses_behavior_seed: bool = False
    evidence_mode: str = EVIDENCE_ORDINARY_EXACT_LOCAL


GAMEPLAY_SCOPE_SPECS = {
    "workshop": GameplayScopeSpec(
        "gameplay_workshop",
        "gameplay_harness_gate",
        True,
        EVIDENCE_CONTROLLED_CAPABILITY,
    ),
    "survival": GameplayScopeSpec(
        "gameplay_survival",
        "gameplay_survival_provisioning_probe",
        True,
    ),
    "progression": GameplayScopeSpec(
        "gameplay_progression",
        "gameplay_primitive_progression_probe",
        evidence_mode=EVIDENCE_ORDINARY_SPATIAL_PROXY,
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
    "fieldwork": GameplayScopeSpec(
        "gameplay_fieldwork",
        "gameplay_fieldwork_probe",
        True,
        EVIDENCE_ORDINARY_SPATIAL_PROXY,
    ),
    "power-provider": GameplayScopeSpec(
        "gameplay_power",
        "gameplay_power_provider_probe",
        True,
    ),
    "ore": GameplayScopeSpec(
        "gameplay_ore",
        "gameplay_ore_preparation_probe",
        evidence_mode=EVIDENCE_CONTROLLED_CAPABILITY,
    ),
    "foundry": GameplayScopeSpec(
        "gameplay_foundry",
        "gameplay_foundry_probe",
        evidence_mode=EVIDENCE_CONTROLLED_CAPABILITY,
    ),
}

GAMEPLAY_TARGETS = {scope: spec.target for scope, spec in GAMEPLAY_SCOPE_SPECS.items()}
GAMEPLAY_TESTS = {scope: spec.test for scope, spec in GAMEPLAY_SCOPE_SPECS.items()}
GAMEPLAY_SCOPE_CONTRACT_TARGETS = {
    "workshop": "gameplay_workshop_contracts",
    "survival": "gameplay_survival_contracts",
    "progression": "gameplay_progression_contracts",
    "liberation": "gameplay_liberation_contracts",
    "settlement": "gameplay_settlement_contracts",
    "foundry-bootstrap": "gameplay_foundry_bootstrap_contracts",
    "woodworking": "gameplay_woodworking_contracts",
    "fieldwork": "gameplay_fieldwork_contracts",
    "power-provider": "gameplay_power_contracts",
    "ore": "gameplay_ore_contracts",
    "foundry": "gameplay_foundry_contracts",
}
GAMEPLAY_PROSPECTING_CONTRACT_TARGET = "gameplay_prospecting_contracts"
GAMEPLAY_OWNER_CONTRACT_TARGETS = tuple(
    sorted(
        (
            *GAMEPLAY_SCOPE_CONTRACT_TARGETS.values(),
            GAMEPLAY_PROSPECTING_CONTRACT_TARGET,
        )
    )
)
GAMEPLAY_CARGO_TEST_TARGETS = tuple(
    sorted(
        {
            GAMEPLAY_CONTRACTS_TARGET,
            GAMEPLAY_AUDIT_TARGET,
            GAMEPLAY_AGENCY_TARGET,
            *GAMEPLAY_TARGETS.values(),
            *GAMEPLAY_OWNER_CONTRACT_TARGETS,
        }
    )
)
GAMEPLAY_PROBE_TARGETS = {spec.test: spec.target for spec in GAMEPLAY_SCOPE_SPECS.values()}
GAMEPLAY_PROBE_TARGETS["gameplay_agency_counterfactuals"] = GAMEPLAY_AGENCY_TARGET
GAMEPLAY_PROBE_SCOPES = {
    spec.test: scope for scope, spec in GAMEPLAY_SCOPE_SPECS.items()
}
GAMEPLAY_PROBE_SCOPES["gameplay_agency_counterfactuals"] = "agency"
GAMEPLAY_REPORT_SCOPE_BY_PROBE = {
    "workshop": "workshop",
    "agency": "agency",
    "survival": "survival",
    "primitive-progression": "progression",
    "primitive-liberation": "liberation",
    "settlement": "settlement",
    "foundry-bootstrap": "foundry-bootstrap",
    "woodworking": "woodworking",
    "fieldwork": "fieldwork",
    "power-provider": "power-provider",
    "ore": "ore",
    "foundry": "foundry",
}


def gameplay_evidence_mode(scope: str) -> str:
    if scope == "agency":
        return EVIDENCE_COUNTERFACTUAL
    return GAMEPLAY_SCOPE_SPECS[scope].evidence_mode
GAMEPLAY_PROBE_TESTS = frozenset(GAMEPLAY_PROBE_TARGETS)
GAMEPLAY_BEHAVIOR_PROBE_TESTS = frozenset(
    spec.test for spec in GAMEPLAY_SCOPE_SPECS.values() if spec.uses_behavior_seed
)
GAMEPLAY_ROUTINE_VARIATION_SCOPES = (*GAMEPLAY_SCOPE_SPECS, "agency")
