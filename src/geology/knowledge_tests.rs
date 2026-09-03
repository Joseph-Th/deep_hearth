//! Contract tests for acquired geological knowledge.

use super::*;
use crate::content::{MATERIAL_COPPER, MATERIAL_SLAG, build_registries};
use crate::core::quantity::Pressure;
use crate::spatial::VoxelCoord;

fn bounds() -> VoxelBounds {
    match VoxelBounds::new(VoxelCoord::new(0, -8, 0), VoxelCoord::new(4, -4, 4)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("geological knowledge bounds fixture failed: {error}"),
    }
}

#[test]
fn loaded_validation_rejects_invalid_excavation_hardness_band() {
    let registries = build_registries();
    let id = GeologicalObservationId::new(1);
    let mut state = GeologicalKnowledgeState::new();
    state.next_observation_id = 2;
    state.observations.insert(
        id,
        GeologicalObservationRecord {
            id,
            region: bounds(),
            evidence: GeologicalEvidenceKind::ExcavationSample,
            findings: vec![estimate(MATERIAL_COPPER, 600_000, 800_000)],
            excavation_hardness: Some(ExcavationHardnessEstimate {
                lower: Pressure::from_pascals(600_000_000),
                upper: Pressure::from_pascals(550_000_000),
            }),
            observed_at: SimulationTick::ZERO,
        },
    );
    state
        .observations_by_material
        .insert(MATERIAL_COPPER, BTreeSet::from([id]));

    assert_eq!(
        validate_loaded_geological_knowledge(registries.materials(), &state, SimulationTick::ZERO),
        Err(GeologicalKnowledgeValidationError::InvalidExcavationHardness { observation: id })
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
