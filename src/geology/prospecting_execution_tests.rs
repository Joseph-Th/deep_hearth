//! Contract tests for prospecting execution and acquired evidence.

use super::*;
use crate::content::{MATERIAL_COPPER, MATERIAL_SLAG, build_registries};
use crate::core::state::validate_loaded_state;
use crate::core::time::WorldSeed;
use crate::geology::{
    GeologicalEvidenceConsistency, MaterialAbundanceEstimate, assess_geological_knowledge,
    build_geological_knowledge_map,
};
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;

fn bounds(min_x: i64, max_x: i64) -> VoxelBounds {
    match VoxelBounds::new(VoxelCoord::new(min_x, -16, 0), VoxelCoord::new(max_x, 0, 8)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("prospecting bounds fixture failed: {error}"),
    }
}

fn estimate(material: MaterialId, lower: u32, upper: u32) -> MaterialAbundanceEstimate {
    match MaterialAbundanceEstimate::new(material, lower, upper) {
        Ok(estimate) => estimate,
        Err(error) => panic!("prospecting estimate fixture failed: {error}"),
    }
}

fn make_test_prospecting_resolution(
    region: VoxelBounds,
    evidence: GeologicalEvidenceKind,
    mut findings: Vec<MaterialAbundanceEstimate>,
) -> ProspectingResolution {
    findings.sort_by_key(|finding| finding.material());
    ProspectingResolution {
        region,
        evidence,
        findings,
        excavation_hardness: None,
    }
}

fn record(
    registries: &Registries,
    state: &mut AppState,
    resolution: ProspectingResolution,
) -> GeologicalObservationId {
    let token = match validate_record_prospecting(registries, state, resolution) {
        Ok(token) => token,
        Err(error) => panic!("prospecting fixture validation failed: {error}"),
    };
    match token.commit(state) {
        Ok(id) => id,
        Err(error) => panic!("prospecting fixture commit failed: {error}"),
    }
}

#[test]
fn observations_persist_quantitative_uncertainty_without_exposing_deposit_identity() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_0001));
    let broad = make_test_prospecting_resolution(
        bounds(0, 16),
        GeologicalEvidenceKind::SurfaceExposure,
        vec![estimate(MATERIAL_COPPER, 0, 700_000)],
    );
    let focused = make_test_prospecting_resolution(
        bounds(4, 8),
        GeologicalEvidenceKind::CoreSample,
        vec![estimate(MATERIAL_COPPER, 420_000, 520_000)],
    );
    let broad_id = record(&registries, &mut state, broad);
    if let Err(error) = advance_tick(&registries, &mut state) {
        panic!("prospecting fixture tick failed: {error}");
    }
    let focused_id = record(&registries, &mut state, focused);

    let assessment =
        assess_geological_knowledge(state.geological_knowledge(), bounds(5, 6), MATERIAL_COPPER);
    assert_eq!(assessment.observations(), &[broad_id, focused_id]);
    assert_eq!(
        assessment.consistency(),
        GeologicalEvidenceConsistency::Compatible {
            lower_ppm: 420_000,
            upper_ppm: 520_000,
        }
    );
    assert_eq!(assessment.envelope(), Some((0, 700_000)));
    assert_eq!(assessment.common_evidence_region(), Some(bounds(5, 6)));
    assert_eq!(assessment.common_acquired_region(), Some(bounds(4, 8)));
    assert_eq!(assessment.most_precise(), Some(focused_id));
    assert_eq!(assessment.latest_observed_at(), Some(state.tick()));
    assert_eq!(state.geology().deposits().count(), 0);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn precision_ranking_uses_width_then_footprint_then_recency_then_identity() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_0010));
    let query = bounds(4, 6);

    let wider_small = record(
        &registries,
        &mut state,
        make_test_prospecting_resolution(
            bounds(4, 6),
            GeologicalEvidenceKind::CoreSample,
            vec![estimate(MATERIAL_COPPER, 300_000, 500_000)],
        ),
    );
    let narrower_large = record(
        &registries,
        &mut state,
        make_test_prospecting_resolution(
            bounds(0, 10),
            GeologicalEvidenceKind::CoreSample,
            vec![estimate(MATERIAL_COPPER, 350_000, 450_000)],
        ),
    );
    assert_eq!(
        assess_geological_knowledge(state.geological_knowledge(), query, MATERIAL_COPPER)
            .most_precise(),
        Some(narrower_large),
        "narrower abundance bounds must outrank a smaller footprint"
    );

    let narrower_small = record(
        &registries,
        &mut state,
        make_test_prospecting_resolution(
            bounds(4, 6),
            GeologicalEvidenceKind::CoreSample,
            vec![estimate(MATERIAL_COPPER, 350_000, 450_000)],
        ),
    );
    assert_eq!(
        assess_geological_knowledge(state.geological_knowledge(), query, MATERIAL_COPPER)
            .most_precise(),
        Some(narrower_small),
        "smaller footprint must break an equal-width tie"
    );

    if let Err(error) = advance_tick(&registries, &mut state) {
        panic!("precision-ranking fixture tick failed: {error}");
    }
    let newer = record(
        &registries,
        &mut state,
        make_test_prospecting_resolution(
            bounds(4, 6),
            GeologicalEvidenceKind::CoreSample,
            vec![estimate(MATERIAL_COPPER, 350_000, 450_000)],
        ),
    );
    assert_eq!(
        assess_geological_knowledge(state.geological_knowledge(), query, MATERIAL_COPPER)
            .most_precise(),
        Some(newer),
        "newer evidence must break an equal-width equal-footprint tie"
    );

    let same_tick_later_id = record(
        &registries,
        &mut state,
        make_test_prospecting_resolution(
            bounds(4, 6),
            GeologicalEvidenceKind::CoreSample,
            vec![estimate(MATERIAL_COPPER, 350_000, 450_000)],
        ),
    );
    assert!(same_tick_later_id > newer);
    assert_eq!(
        assess_geological_knowledge(state.geological_knowledge(), query, MATERIAL_COPPER)
            .most_precise(),
        Some(newer),
        "lower persistent identity must break a complete precision tie"
    );
    assert!(wider_small < narrower_large);
}

#[test]
fn contradictory_surveys_remain_visible_instead_of_being_averaged() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_0002));
    let first = make_test_prospecting_resolution(
        bounds(0, 8),
        GeologicalEvidenceKind::MagneticSurvey,
        vec![estimate(MATERIAL_COPPER, 700_000, 900_000)],
    );
    let second = make_test_prospecting_resolution(
        bounds(2, 6),
        GeologicalEvidenceKind::ElectricalSurvey,
        vec![estimate(MATERIAL_COPPER, 100_000, 300_000)],
    );
    record(&registries, &mut state, first);
    record(&registries, &mut state, second);

    let assessment =
        assess_geological_knowledge(state.geological_knowledge(), bounds(3, 4), MATERIAL_COPPER);
    assert_eq!(
        assessment.consistency(),
        GeologicalEvidenceConsistency::Conflicting {
            highest_lower_ppm: 700_000,
            lowest_upper_ppm: 300_000,
        }
    );
    assert_eq!(assessment.envelope(), Some((100_000, 900_000)));
}

#[test]
fn nonoverlapping_observations_do_not_leak_into_local_assessment() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_0003));
    let remote = make_test_prospecting_resolution(
        bounds(100, 110),
        GeologicalEvidenceKind::SeismicSurvey,
        vec![estimate(MATERIAL_COPPER, 900_000, 1_000_000)],
    );
    record(&registries, &mut state, remote);

    let assessment =
        assess_geological_knowledge(state.geological_knowledge(), bounds(0, 10), MATERIAL_COPPER);
    assert_eq!(
        assessment.consistency(),
        GeologicalEvidenceConsistency::NoEvidence
    );
    assert!(assessment.observations().is_empty());
    assert_eq!(assessment.envelope(), None);
    assert_eq!(assessment.common_evidence_region(), None);
    assert_eq!(assessment.common_acquired_region(), None);
}

#[test]
fn disjoint_evidence_inside_a_large_query_is_not_reported_as_a_false_conflict() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_0007));
    let west = make_test_prospecting_resolution(
        bounds(0, 4),
        GeologicalEvidenceKind::CoreSample,
        vec![estimate(MATERIAL_COPPER, 700_000, 900_000)],
    );
    let east = make_test_prospecting_resolution(
        bounds(6, 10),
        GeologicalEvidenceKind::CoreSample,
        vec![estimate(MATERIAL_COPPER, 100_000, 300_000)],
    );
    record(&registries, &mut state, west);
    record(&registries, &mut state, east);

    let assessment =
        assess_geological_knowledge(state.geological_knowledge(), bounds(0, 10), MATERIAL_COPPER);
    assert_eq!(
        assessment.consistency(),
        GeologicalEvidenceConsistency::SpatiallyIncomparable
    );
    assert_eq!(assessment.common_evidence_region(), None);
    assert_eq!(assessment.common_acquired_region(), None);
    assert_eq!(assessment.envelope(), Some((100_000, 900_000)));
    assert_eq!(assessment.observations().len(), 2);
}

#[test]
fn empty_common_overlap_stays_empty_after_later_evidence() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_0008));
    for (region, lower, upper) in [
        (bounds(0, 4), 700_000, 900_000),
        (bounds(6, 10), 100_000, 300_000),
        (bounds(2, 8), 400_000, 500_000),
    ] {
        let resolution = make_test_prospecting_resolution(
            region,
            GeologicalEvidenceKind::CoreSample,
            vec![estimate(MATERIAL_COPPER, lower, upper)],
        );
        record(&registries, &mut state, resolution);
    }

    let assessment =
        assess_geological_knowledge(state.geological_knowledge(), bounds(0, 10), MATERIAL_COPPER);
    assert_eq!(
        assessment.consistency(),
        GeologicalEvidenceConsistency::SpatiallyIncomparable
    );
    assert_eq!(assessment.common_evidence_region(), None);
    assert_eq!(assessment.common_acquired_region(), None);
    assert_eq!(assessment.observations().len(), 3);
}

#[test]
fn regional_geological_map_is_stable_and_omits_remote_only_materials() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_0006));
    let local = make_test_prospecting_resolution(
        bounds(0, 8),
        GeologicalEvidenceKind::CoreSample,
        vec![estimate(MATERIAL_COPPER, 400_000, 500_000)],
    );
    let remote = make_test_prospecting_resolution(
        bounds(100, 108),
        GeologicalEvidenceKind::LaboratoryAssay,
        vec![estimate(MATERIAL_SLAG, 700_000, 800_000)],
    );
    record(&registries, &mut state, local);
    record(&registries, &mut state, remote);

    let map = build_geological_knowledge_map(state.geological_knowledge(), bounds(2, 4));
    assert_eq!(map.region(), bounds(2, 4));
    assert_eq!(map.assessments().len(), 1);
    assert_eq!(map.assessments()[0].material(), MATERIAL_COPPER);
    assert_eq!(
        map.assessments()[0].consistency(),
        GeologicalEvidenceConsistency::Compatible {
            lower_ppm: 400_000,
            upper_ppm: 500_000,
        }
    );
}

#[test]
fn prospecting_rejects_physically_impossible_combined_abundance_minima() {
    let registries = build_registries();
    let state = AppState::new(WorldSeed::new(0x6B00_0009));
    let resolution = make_test_prospecting_resolution(
        bounds(0, 8),
        GeologicalEvidenceKind::LaboratoryAssay,
        vec![
            estimate(MATERIAL_COPPER, 600_000, 900_000),
            estimate(MATERIAL_SLAG, 500_000, 800_000),
        ],
    );

    assert_eq!(
        validate_record_prospecting(&registries, &state, resolution),
        Err(RecordProspectingError::ImpossibleLowerBoundTotal {
            total_ppm: 1_100_000,
        })
    );
    assert_eq!(state.geological_knowledge().observations().count(), 0);
}

#[test]
fn stale_observation_commit_is_atomic() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_0004));
    let first = make_test_prospecting_resolution(
        bounds(0, 4),
        GeologicalEvidenceKind::LooseIndicator,
        vec![estimate(MATERIAL_COPPER, 0, 200_000)],
    );
    let second = make_test_prospecting_resolution(
        bounds(4, 8),
        GeologicalEvidenceKind::PannedConcentrate,
        vec![estimate(MATERIAL_SLAG, 100_000, 400_000)],
    );
    let stale = match validate_record_prospecting(&registries, &state, first) {
        Ok(token) => token,
        Err(error) => panic!("stale prospecting validation failed: {error}"),
    };
    record(&registries, &mut state, second);
    let before = state.clone();

    assert_eq!(
        stale.commit(&mut state),
        Err(ProspectingCommitError::StaleKnowledgeRevision {
            expected: 0,
            actual: 1,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn prospecting_round_trip_preserves_deterministic_continuation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_0005));
    let initial = make_test_prospecting_resolution(
        bounds(0, 16),
        GeologicalEvidenceKind::SurfaceExposure,
        vec![
            estimate(MATERIAL_COPPER, 0, 600_000),
            estimate(MATERIAL_SLAG, 0, 800_000),
        ],
    );
    record(&registries, &mut state, initial);
    if let Err(error) = advance_tick(&registries, &mut state) {
        panic!("prospecting round-trip tick failed: {error}");
    }

    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("prospecting save serialization failed: {error}"),
    };
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("prospecting save deserialization failed: {error}"),
    };
    let mut loaded = match decoded.into_state(&registries) {
        Ok(loaded) => loaded,
        Err(error) => panic!("prospecting loaded-state validation failed: {error}"),
    };
    assert_eq!(loaded, state);

    let continuation_for_state = make_test_prospecting_resolution(
        bounds(2, 6),
        GeologicalEvidenceKind::LaboratoryAssay,
        vec![estimate(MATERIAL_COPPER, 430_000, 450_000)],
    );
    let continuation_for_loaded = make_test_prospecting_resolution(
        bounds(2, 6),
        GeologicalEvidenceKind::LaboratoryAssay,
        vec![estimate(MATERIAL_COPPER, 430_000, 450_000)],
    );
    record(&registries, &mut state, continuation_for_state);
    record(&registries, &mut loaded, continuation_for_loaded);
    assert_eq!(loaded, state);
}

#[cfg(feature = "test-soak")]
fn run_prospecting_soak(seed: WorldSeed) -> AppState {
    let registries = build_registries();
    let mut state = AppState::new(seed);
    for step in 0_u32..2_000 {
        let x = i64::from(step % 64);
        let material = if step.is_multiple_of(2) {
            MATERIAL_COPPER
        } else {
            MATERIAL_SLAG
        };
        let center = (step.wrapping_mul(7919)) % 900_000;
        let lower = center.saturating_sub(25_000);
        let upper = center.saturating_add(25_000).min(1_000_000);
        let resolution = make_test_prospecting_resolution(
            bounds(x, x + 2),
            GeologicalEvidenceKind::CoreSample,
            vec![estimate(material, lower, upper)],
        );
        record(&registries, &mut state, resolution);
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("prospecting soak tick failed at step {step}: {error}");
        }
        if step.is_multiple_of(97)
            && let Err(error) = validate_loaded_state(&registries, &state)
        {
            panic!("prospecting soak exhaustive audit failed at step {step}: {error}");
        }
    }
    assert_eq!(state.geological_knowledge().observations().count(), 2_000);
    assert_eq!(
        state
            .geological_knowledge()
            .observation_ids_for_material(MATERIAL_COPPER)
            .count(),
        1_000
    );
    assert_eq!(
        state
            .geological_knowledge()
            .observation_ids_for_material(MATERIAL_SLAG)
            .count(),
        1_000
    );
    state
}

#[cfg(feature = "test-soak")]
#[test]
#[ignore = "long-horizon soak"]
fn prospecting_soak_preserves_indexes_persistence_invariants_and_replay() {
    let seed = WorldSeed::new(0x6B00_5000);
    let first = run_prospecting_soak(seed);
    let second = run_prospecting_soak(seed);
    assert_eq!(first, second);
}
