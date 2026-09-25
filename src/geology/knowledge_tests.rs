//! Contract tests for acquired geological knowledge.

use std::collections::BTreeSet;

use super::*;
use crate::content::{
    FORM_ORE, MATERIAL_COPPER, MATERIAL_SLAG, PROSPECTING_DETAILED_FIELD_SURVEY, build_registries,
};
use crate::core::quantity::{Mass, Pressure, Temperature};
use crate::core::state::{
    AppState, StateValidationError, apply_clock_advance, validate_loaded_state,
};
use crate::core::time::SimulationTick;
use crate::geology::{GeneratedDepositSpec, GeologicalDepositId, insert_generated_deposit};
use crate::material::{CommodityKey, MaterialComposition, MaterialId};
use crate::registry::Registries;
use crate::spatial::{VoxelBounds, VoxelCoord};

fn bounds() -> VoxelBounds {
    match VoxelBounds::new(VoxelCoord::new(0, -8, 0), VoxelCoord::new(4, -4, 4)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("geological knowledge bounds fixture failed: {error}"),
    }
}

fn physical_bounds() -> VoxelBounds {
    VoxelBounds::new(VoxelCoord::new(0, -8, 0), VoxelCoord::new(1, -7, 1))
        .unwrap_or_else(|error| panic!("physical knowledge bounds fixture failed: {error}"))
}

fn insert_copper_deposit(
    registries: &Registries,
    app: &mut AppState,
    mass: Mass,
) -> GeologicalDepositId {
    insert_generated_deposit(
        registries,
        app,
        GeneratedDepositSpec::new(
            physical_bounds(),
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            mass,
            Temperature::from_millikelvin(293_150),
            Pressure::from_pascals(350_000_000),
            MaterialComposition::pure(MATERIAL_COPPER),
        )
        .unwrap_or_else(|error| panic!("knowledge deposit fixture failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("knowledge deposit insertion failed: {error}"))
}

fn valid_hardness() -> ExcavationHardnessEstimate {
    ExcavationHardnessEstimate::new(
        Pressure::from_pascals(300_000_000),
        Pressure::from_pascals(350_000_000),
    )
    .unwrap_or_else(|error| panic!("geological hardness fixture failed: {error}"))
}

fn knowledge_with_hardness(
    evidence: GeologicalEvidenceKind,
    findings: Vec<MaterialAbundanceEstimate>,
) -> (GeologicalKnowledgeState, GeologicalObservationId) {
    let id = GeologicalObservationId::new(1);
    let mut state = GeologicalKnowledgeState::new();
    state.next_observation_id = 2;
    for finding in &findings {
        state
            .observations_by_material
            .entry(finding.material())
            .or_default()
            .insert(id);
    }
    state.observations.insert(
        id,
        GeologicalObservationRecord {
            id,
            region: physical_bounds(),
            evidence,
            findings,
            excavation_hardness: Some(valid_hardness()),
            resource_mass: None,
            observed_at: SimulationTick::ZERO,
        },
    );
    (state, id)
}

fn knowledge_with_resource_mass(
    resource_mass: ResourceMassEstimate,
) -> (GeologicalKnowledgeState, GeologicalObservationId) {
    let id = GeologicalObservationId::new(1);
    let mut state = GeologicalKnowledgeState::new();
    state.next_observation_id = 2;
    let finding = estimate(MATERIAL_COPPER, 975_000, 1_000_000);
    state
        .observations_by_material
        .entry(MATERIAL_COPPER)
        .or_default()
        .insert(id);
    state.observations.insert(
        id,
        GeologicalObservationRecord {
            id,
            region: physical_bounds(),
            evidence: GeologicalEvidenceKind::ExcavationSample,
            findings: vec![finding],
            excavation_hardness: Some(valid_hardness()),
            resource_mass: Some(resource_mass),
            observed_at: SimulationTick::ZERO,
        },
    );
    (state, id)
}

#[test]
fn loaded_state_rejects_abundance_precision_no_authored_method_can_emit() {
    let registries = build_registries();
    let mut app = AppState::new();
    let _ = insert_copper_deposit(&registries, &mut app, Mass::from_milligrams(1_000_000));
    let id = GeologicalObservationId::new(1);
    let finding = estimate(MATERIAL_COPPER, 999_999, 1_000_000);
    let mut knowledge = GeologicalKnowledgeState::new();
    knowledge.next_observation_id = 2;
    knowledge
        .observations_by_material
        .entry(MATERIAL_COPPER)
        .or_default()
        .insert(id);
    knowledge.observations.insert(
        id,
        GeologicalObservationRecord {
            id,
            region: physical_bounds(),
            evidence: GeologicalEvidenceKind::SurfaceExposure,
            findings: vec![finding],
            excavation_hardness: None,
            resource_mass: None,
            observed_at: SimulationTick::ZERO,
        },
    );
    *app.geological_knowledge_state_mut() = knowledge;

    assert_eq!(
        validate_loaded_state(&registries, &app),
        Err(StateValidationError::GeologicalKnowledge(
            GeologicalKnowledgeValidationError::ObservationCannotMatchAuthoredMethod {
                observation: id,
                evidence: GeologicalEvidenceKind::SurfaceExposure,
            }
        ))
    );
}

#[test]
fn loaded_state_rejects_definite_physical_sample_missing_authored_hardness() {
    let registries = build_registries();
    let mut app = AppState::new();
    let _ = insert_copper_deposit(&registries, &mut app, Mass::from_milligrams(1_000_000));
    let id = GeologicalObservationId::new(1);
    let finding = estimate(MATERIAL_COPPER, 975_000, 1_000_000);
    let mut knowledge = GeologicalKnowledgeState::new();
    knowledge.next_observation_id = 2;
    knowledge
        .observations_by_material
        .entry(MATERIAL_COPPER)
        .or_default()
        .insert(id);
    knowledge.observations.insert(
        id,
        GeologicalObservationRecord {
            id,
            region: physical_bounds(),
            evidence: GeologicalEvidenceKind::ExcavationSample,
            findings: vec![finding],
            excavation_hardness: None,
            resource_mass: None,
            observed_at: SimulationTick::ZERO,
        },
    );
    *app.geological_knowledge_state_mut() = knowledge;

    assert_eq!(
        validate_loaded_state(&registries, &app),
        Err(StateValidationError::GeologicalKnowledge(
            GeologicalKnowledgeValidationError::ObservationCannotMatchAuthoredMethod {
                observation: id,
                evidence: GeologicalEvidenceKind::ExcavationSample,
            }
        ))
    );
}

#[test]
fn loaded_validation_rejects_hardness_on_nonphysical_evidence() {
    let registries = build_registries();
    let (state, id) = knowledge_with_hardness(
        GeologicalEvidenceKind::MagneticSurvey,
        vec![estimate(MATERIAL_COPPER, 600_000, 800_000)],
    );

    assert_eq!(
        validate_loaded_geological_knowledge(registries.materials(), &state, SimulationTick::ZERO),
        Err(
            GeologicalKnowledgeValidationError::ExcavationHardnessUnsupportedEvidence {
                observation: id,
                evidence: GeologicalEvidenceKind::MagneticSurvey,
            }
        )
    );
}

#[test]
fn loaded_validation_rejects_hardness_when_target_presence_is_uncertain() {
    let registries = build_registries();
    let (state, id) = knowledge_with_hardness(
        GeologicalEvidenceKind::ExcavationSample,
        vec![estimate(MATERIAL_COPPER, 0, 800_000)],
    );

    assert_eq!(
        validate_loaded_geological_knowledge(registries.materials(), &state, SimulationTick::ZERO),
        Err(
            GeologicalKnowledgeValidationError::ExcavationHardnessWithoutDefinitePresence {
                observation: id,
                material: MATERIAL_COPPER,
            }
        )
    );
}

#[test]
fn loaded_validation_rejects_ambiguous_multi_material_hardness() {
    let registries = build_registries();
    let (state, id) = knowledge_with_hardness(
        GeologicalEvidenceKind::CoreSample,
        vec![
            estimate(MATERIAL_COPPER, 300_000, 500_000),
            estimate(MATERIAL_SLAG, 300_000, 500_000),
        ],
    );

    assert_eq!(
        validate_loaded_geological_knowledge(registries.materials(), &state, SimulationTick::ZERO),
        Err(
            GeologicalKnowledgeValidationError::ExcavationHardnessAmbiguousFindings {
                observation: id,
                count: 2,
            }
        )
    );
}

#[test]
fn loaded_state_rejects_hardness_band_that_excludes_live_matching_deposit() {
    let registries = build_registries();
    let mut app = AppState::new();
    let deposit = insert_generated_deposit(
        &registries,
        &mut app,
        GeneratedDepositSpec::new(
            bounds(),
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Mass::from_milligrams(1_000_000),
            Temperature::from_millikelvin(293_150),
            Pressure::from_pascals(500_000_000),
            MaterialComposition::pure(MATERIAL_COPPER),
        )
        .unwrap_or_else(|error| panic!("knowledge/live-geology deposit fixture failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("knowledge/live-geology deposit insertion failed: {error}"));
    let (knowledge, observation) = knowledge_with_hardness(
        GeologicalEvidenceKind::ExcavationSample,
        vec![estimate(MATERIAL_COPPER, 975_000, 1_000_000)],
    );
    *app.geological_knowledge_state_mut() = knowledge;

    assert_eq!(
        validate_loaded_state(&registries, &app),
        Err(StateValidationError::GeologicalKnowledge(
            GeologicalKnowledgeValidationError::ExcavationHardnessContradictsLiveDeposit {
                observation,
                deposit,
                lower: Pressure::from_pascals(300_000_000),
                upper: Pressure::from_pascals(350_000_000),
                actual: Pressure::from_pascals(500_000_000),
            }
        ))
    );
}

#[test]
fn loaded_state_rejects_physical_hardness_without_historical_body() {
    let registries = build_registries();
    let mut app = AppState::new();
    let (knowledge, observation) = knowledge_with_hardness(
        GeologicalEvidenceKind::ExcavationSample,
        vec![estimate(MATERIAL_COPPER, 975_000, 1_000_000)],
    );
    *app.geological_knowledge_state_mut() = knowledge;

    assert_eq!(
        validate_loaded_state(&registries, &app),
        Err(StateValidationError::GeologicalKnowledge(
            GeologicalKnowledgeValidationError::ExcavationHardnessCannotMatchHistoricalDeposit {
                observation,
                material: MATERIAL_COPPER,
                lower: Pressure::from_pascals(300_000_000),
                upper: Pressure::from_pascals(350_000_000),
            }
        ))
    );
}

#[test]
fn later_generated_body_does_not_retroactively_validate_older_hardness() {
    let registries = build_registries();
    let mut app = AppState::new();
    let (knowledge, observation) = knowledge_with_hardness(
        GeologicalEvidenceKind::ExcavationSample,
        vec![estimate(MATERIAL_COPPER, 975_000, 1_000_000)],
    );
    *app.geological_knowledge_state_mut() = knowledge;
    apply_clock_advance(&mut app, SimulationTick::new(1));
    let _ = insert_copper_deposit(&registries, &mut app, Mass::from_milligrams(1_000_000));

    assert_eq!(
        validate_loaded_state(&registries, &app),
        Err(StateValidationError::GeologicalKnowledge(
            GeologicalKnowledgeValidationError::ExcavationHardnessCannotMatchHistoricalDeposit {
                observation,
                material: MATERIAL_COPPER,
                lower: Pressure::from_pascals(300_000_000),
                upper: Pressure::from_pascals(350_000_000),
            }
        ))
    );
}

#[test]
fn depleted_body_can_still_support_historical_hardness_evidence() {
    let registries = build_registries();
    let mut app = AppState::new();
    let deposit = insert_copper_deposit(&registries, &mut app, Mass::from_milligrams(1_000_000));
    let (knowledge, _) = knowledge_with_hardness(
        GeologicalEvidenceKind::ExcavationSample,
        vec![estimate(MATERIAL_COPPER, 975_000, 1_000_000)],
    );
    *app.geological_knowledge_state_mut() = knowledge;
    apply_clock_advance(&mut app, SimulationTick::new(1));
    let next_revision = app
        .geology()
        .revision()
        .checked_add(1)
        .unwrap_or_else(|| panic!("historical hardness geology revision overflowed"));
    app.geology_state_mut().apply_extraction(
        deposit,
        Mass::from_milligrams(1_000_000),
        next_revision,
    );

    assert_eq!(validate_loaded_state(&registries, &app), Ok(()));
}

#[test]
fn loaded_state_rejects_representationally_clipped_hardness_precision() {
    let registries = build_registries();
    let detailed = registries
        .labor()
        .get_prospecting(PROSPECTING_DETAILED_FIELD_SURVEY)
        .copied()
        .unwrap_or_else(|| panic!("detailed prospecting definition disappeared"));
    let resolution = detailed
        .excavation_hardness_resolution()
        .unwrap_or_else(|| panic!("detailed prospecting hardness resolution disappeared"));
    let actual_pa = u64::MAX - 100;
    let old_clipped_lower =
        (actual_pa.saturating_sub(1) / resolution.pascals()) * resolution.pascals();
    assert!(u64::MAX - old_clipped_lower < resolution.pascals());
    let narrow = ExcavationHardnessEstimate::new(
        Pressure::from_pascals(old_clipped_lower),
        Pressure::from_pascals(u64::MAX),
    )
    .unwrap_or_else(|error| panic!("clipped hardness fixture failed: {error}"));
    let mut app = AppState::new();
    let _ = insert_generated_deposit(
        &registries,
        &mut app,
        GeneratedDepositSpec::new(
            physical_bounds(),
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Mass::from_milligrams(1_000_000),
            Temperature::from_millikelvin(293_150),
            Pressure::from_pascals(actual_pa),
            MaterialComposition::pure(MATERIAL_COPPER),
        )
        .unwrap_or_else(|error| panic!("ceiling hardness deposit fixture failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("ceiling hardness deposit insertion failed: {error}"));
    let (mut knowledge, observation) = knowledge_with_hardness(
        GeologicalEvidenceKind::ExcavationSample,
        vec![estimate(MATERIAL_COPPER, 975_000, 1_000_000)],
    );
    knowledge
        .observations
        .get_mut(&observation)
        .unwrap_or_else(|| panic!("ceiling hardness observation disappeared"))
        .excavation_hardness = Some(narrow);
    *app.geological_knowledge_state_mut() = knowledge;

    assert_eq!(
        validate_loaded_state(&registries, &app),
        Err(StateValidationError::GeologicalKnowledge(
            GeologicalKnowledgeValidationError::ObservationCannotMatchAuthoredMethod {
                observation,
                evidence: GeologicalEvidenceKind::ExcavationSample,
            }
        ))
    );
}

#[test]
fn loaded_state_rejects_resource_mass_without_exact_historical_body() {
    let registries = build_registries();
    let mut app = AppState::new();
    let _ = insert_generated_deposit(
        &registries,
        &mut app,
        GeneratedDepositSpec::new(
            bounds(),
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Mass::from_milligrams(4_500_000),
            Temperature::from_millikelvin(293_150),
            Pressure::from_pascals(350_000_000),
            MaterialComposition::pure(MATERIAL_COPPER),
        )
        .unwrap_or_else(|error| panic!("nonlocalized resource deposit fixture failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("nonlocalized resource deposit insertion failed: {error}"));
    let resource_mass = ResourceMassEstimate::new(
        Mass::from_milligrams(4_000_000),
        Mass::from_milligrams(5_000_000),
    )
    .unwrap_or_else(|error| panic!("missing-body resource fixture failed: {error}"));
    let (knowledge, observation) = knowledge_with_resource_mass(resource_mass);
    *app.geological_knowledge_state_mut() = knowledge;

    assert_eq!(
        validate_loaded_state(&registries, &app),
        Err(StateValidationError::GeologicalKnowledge(
            GeologicalKnowledgeValidationError::ResourceMassCannotMatchHistoricalDeposit {
                observation,
                material: MATERIAL_COPPER,
                lower: resource_mass.lower(),
                upper: resource_mass.upper(),
            }
        ))
    );
}

#[test]
fn loaded_state_rejects_resource_mass_outside_possible_historical_range() {
    let registries = build_registries();
    for (deposit_mass, lower, upper) in [
        (5_000_000, 1_000_000, 2_000_000),
        (1_000_000, 2_000_000, 3_000_000),
        (4_000_000, 3_000_000, 4_000_000),
    ] {
        let mut app = AppState::new();
        let _ = insert_copper_deposit(&registries, &mut app, Mass::from_milligrams(deposit_mass));
        let resource_mass =
            ResourceMassEstimate::new(Mass::from_milligrams(lower), Mass::from_milligrams(upper))
                .unwrap_or_else(|error| panic!("range resource fixture failed: {error}"));
        let (knowledge, observation) = knowledge_with_resource_mass(resource_mass);
        *app.geological_knowledge_state_mut() = knowledge;

        assert_eq!(
            validate_loaded_state(&registries, &app),
            Err(StateValidationError::GeologicalKnowledge(
                GeologicalKnowledgeValidationError::ResourceMassCannotMatchHistoricalDeposit {
                    observation,
                    material: MATERIAL_COPPER,
                    lower: resource_mass.lower(),
                    upper: resource_mass.upper(),
                }
            ))
        );
    }
}

#[test]
fn depleted_body_can_still_support_historical_resource_mass_evidence() {
    let registries = build_registries();
    let mut app = AppState::new();
    let deposit = insert_copper_deposit(&registries, &mut app, Mass::from_milligrams(4_500_000));
    let resource_mass = ResourceMassEstimate::new(
        Mass::from_milligrams(4_000_000),
        Mass::from_milligrams(5_000_000),
    )
    .unwrap_or_else(|error| panic!("historical resource fixture failed: {error}"));
    let (knowledge, _) = knowledge_with_resource_mass(resource_mass);
    *app.geological_knowledge_state_mut() = knowledge;
    apply_clock_advance(&mut app, SimulationTick::new(1));
    let next_revision = app
        .geology()
        .revision()
        .checked_add(1)
        .unwrap_or_else(|| panic!("historical resource geology revision overflowed"));
    app.geology_state_mut().apply_extraction(
        deposit,
        Mass::from_milligrams(4_500_000),
        next_revision,
    );

    assert_eq!(validate_loaded_state(&registries, &app), Ok(()));
}

#[test]
fn loaded_state_rejects_representationally_clipped_resource_mass_precision() {
    let registries = build_registries();
    let detailed = registries
        .labor()
        .get_prospecting(PROSPECTING_DETAILED_FIELD_SURVEY)
        .copied()
        .unwrap_or_else(|| panic!("detailed prospecting definition disappeared"));
    let resolution = detailed
        .resource_mass_resolution()
        .unwrap_or_else(|| panic!("detailed prospecting resource resolution disappeared"));
    let actual_mg = u64::MAX - 100;
    let old_clipped_lower = (actual_mg / resolution.milligrams()) * resolution.milligrams();
    assert!(u64::MAX - old_clipped_lower < resolution.milligrams());
    let narrow = ResourceMassEstimate::new(
        Mass::from_milligrams(old_clipped_lower),
        Mass::from_milligrams(u64::MAX),
    )
    .unwrap_or_else(|error| panic!("clipped resource-mass fixture failed: {error}"));
    let mut app = AppState::new();
    let _ = insert_copper_deposit(&registries, &mut app, Mass::from_milligrams(actual_mg));
    let (knowledge, observation) = knowledge_with_resource_mass(narrow);
    *app.geological_knowledge_state_mut() = knowledge;

    assert_eq!(
        validate_loaded_state(&registries, &app),
        Err(StateValidationError::GeologicalKnowledge(
            GeologicalKnowledgeValidationError::ObservationCannotMatchAuthoredMethod {
                observation,
                evidence: GeologicalEvidenceKind::ExcavationSample,
            }
        ))
    );
}

#[test]
fn excavation_hardness_deserialization_enforces_canonical_bounds() {
    let valid = valid_hardness();
    let encoded = serde_json::to_value(valid)
        .unwrap_or_else(|error| panic!("hardness estimate serialization failed: {error}"));
    let decoded: ExcavationHardnessEstimate = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("hardness estimate deserialization failed: {error}"));
    assert_eq!(decoded, valid);

    assert!(
        serde_json::from_value::<ExcavationHardnessEstimate>(serde_json::json!({
            "lower": Pressure::from_pascals(600_000_000).pascals(),
            "upper": Pressure::from_pascals(550_000_000).pascals(),
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<ExcavationHardnessEstimate>(serde_json::json!({
            "lower": 0,
            "upper": 0,
        }))
        .is_err()
    );
}

fn estimate(material: MaterialId, lower: u32, upper: u32) -> MaterialAbundanceEstimate {
    match MaterialAbundanceEstimate::new(material, lower, upper) {
        Ok(estimate) => estimate,
        Err(error) => panic!("geological knowledge estimate fixture failed: {error}"),
    }
}

#[test]
fn abundance_estimate_rejects_invalid_fraction_bounds() {
    assert_eq!(
        MaterialAbundanceEstimate::new(MATERIAL_COPPER, 700_000, 600_000),
        Err(MaterialAbundanceEstimateError::InvertedBounds {
            lower_ppm: 700_000,
            upper_ppm: 600_000,
        })
    );
    assert_eq!(
        MaterialAbundanceEstimate::new(MATERIAL_COPPER, 0, 1_000_001),
        Err(MaterialAbundanceEstimateError::AboveUnity {
            bound: AbundanceBound::Upper,
            value: 1_000_001,
        })
    );
}

#[test]
fn abundance_estimate_deserialization_replays_fraction_bounds() {
    let inverted = format!(
        r#"{{"material":{},"lower_ppm":700000,"upper_ppm":600000}}"#,
        MATERIAL_COPPER.value()
    );
    assert!(
        serde_json::from_str::<MaterialAbundanceEstimate>(&inverted).is_err(),
        "deserialization must not bypass inverted abundance bounds"
    );

    let above_unity = format!(
        r#"{{"material":{},"lower_ppm":0,"upper_ppm":1000001}}"#,
        MATERIAL_COPPER.value()
    );
    assert!(
        serde_json::from_str::<MaterialAbundanceEstimate>(&above_unity).is_err(),
        "deserialization must not bypass normalized abundance bounds"
    );
}

#[test]
fn loaded_validation_rejects_missing_material_index_entry() {
    let registries = build_registries();
    let id = GeologicalObservationId::new(1);
    let mut state = GeologicalKnowledgeState::new();
    state.next_observation_id = 2;
    state.observations.insert(
        id,
        GeologicalObservationRecord {
            id,
            region: bounds(),
            evidence: GeologicalEvidenceKind::CoreSample,
            findings: vec![estimate(MATERIAL_COPPER, 600_000, 800_000)],
            excavation_hardness: None,
            resource_mass: None,
            observed_at: SimulationTick::ZERO,
        },
    );

    assert_eq!(
        validate_loaded_geological_knowledge(registries.materials(), &state, SimulationTick::ZERO,),
        Err(
            GeologicalKnowledgeValidationError::MissingMaterialIndexEntry {
                observation: id,
                material: MATERIAL_COPPER,
            }
        )
    );
}

#[test]
fn loaded_validation_rejects_noncanonical_duplicate_material_findings() {
    let registries = build_registries();
    let id = GeologicalObservationId::new(1);
    let mut state = GeologicalKnowledgeState::new();
    state.next_observation_id = 2;
    state.observations.insert(
        id,
        GeologicalObservationRecord {
            id,
            region: bounds(),
            evidence: GeologicalEvidenceKind::LaboratoryAssay,
            findings: vec![
                estimate(MATERIAL_COPPER, 500_000, 600_000),
                estimate(MATERIAL_COPPER, 550_000, 650_000),
            ],
            excavation_hardness: None,
            resource_mass: None,
            observed_at: SimulationTick::ZERO,
        },
    );
    state
        .observations_by_material
        .insert(MATERIAL_COPPER, BTreeSet::from([id]));

    assert_eq!(
        validate_loaded_geological_knowledge(registries.materials(), &state, SimulationTick::ZERO,),
        Err(GeologicalKnowledgeValidationError::FindingsNotCanonical {
            observation: id,
            previous: MATERIAL_COPPER,
            current: MATERIAL_COPPER,
        })
    );
}

#[test]
fn loaded_validation_rejects_impossible_combined_abundance_minima() {
    let registries = build_registries();
    let id = GeologicalObservationId::new(1);
    let mut findings = vec![
        estimate(MATERIAL_COPPER, 600_000, 900_000),
        estimate(MATERIAL_SLAG, 500_000, 800_000),
    ];
    findings.sort_by_key(|finding| finding.material());
    let mut state = GeologicalKnowledgeState::new();
    state.next_observation_id = 2;
    state.observations.insert(
        id,
        GeologicalObservationRecord {
            id,
            region: bounds(),
            evidence: GeologicalEvidenceKind::LaboratoryAssay,
            findings,
            excavation_hardness: None,
            resource_mass: None,
            observed_at: SimulationTick::ZERO,
        },
    );

    assert_eq!(
        validate_loaded_geological_knowledge(registries.materials(), &state, SimulationTick::ZERO,),
        Err(
            GeologicalKnowledgeValidationError::ImpossibleLowerBoundTotal {
                observation: id,
                total_ppm: 1_100_000,
            }
        )
    );
}

#[test]
fn material_index_validation_checks_both_directions() {
    let registries = build_registries();
    let id = GeologicalObservationId::new(1);
    let mut state = GeologicalKnowledgeState::new();
    state.next_observation_id = 2;
    state.observations.insert(
        id,
        GeologicalObservationRecord {
            id,
            region: bounds(),
            evidence: GeologicalEvidenceKind::MagneticSurvey,
            findings: vec![estimate(MATERIAL_COPPER, 100_000, 900_000)],
            excavation_hardness: None,
            resource_mass: None,
            observed_at: SimulationTick::ZERO,
        },
    );
    state
        .observations_by_material
        .insert(MATERIAL_COPPER, BTreeSet::from([id]));
    state
        .observations_by_material
        .insert(MATERIAL_SLAG, BTreeSet::from([id]));

    assert_eq!(
        validate_loaded_geological_knowledge(registries.materials(), &state, SimulationTick::ZERO,),
        Err(GeologicalKnowledgeValidationError::IndexMaterialMismatch {
            material: MATERIAL_SLAG,
            observation: id,
        })
    );
}
