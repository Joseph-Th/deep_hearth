//! Behavioral coverage for player-performed field prospecting.

use super::*;
use crate::content::{
    FORM_ORE, MATERIAL_COPPER, MATERIAL_STONE, PROSPECTING_DETAILED_FIELD_SURVEY,
    PROSPECTING_FIELD_INSPECTION, build_registries,
};
use crate::core::quantity::{Mass, Pressure, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::core::time::WorldSeed;
use crate::geology::{GeneratedDepositSpec, insert_generated_deposit};
use crate::labor::{PlayerWork, ProspectingMethodId};
use crate::material::{CommodityKey, CompositionComponent, MaterialComposition};
use crate::mining::{MiningTargetRequest, MiningTargetResolutionError, resolve_mining_target};
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::survival::{assess_survival, initialize_player_survival};

fn one_voxel(x: i64) -> VoxelBounds {
    VoxelBounds::new(VoxelCoord::new(x, -1, 0), VoxelCoord::new(x + 1, 0, 1))
        .unwrap_or_else(|error| panic!("field prospecting bounds fixture failed: {error}"))
}

fn insert_copper(registries: &Registries, state: &mut AppState, region: VoxelBounds) {
    let spec = GeneratedDepositSpec::new(
        region,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
        Pressure::from_pascals(350_000_000),
        MaterialComposition::pure(MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("field prospecting deposit fixture failed: {error}"));
    insert_generated_deposit(registries, state, spec)
        .unwrap_or_else(|error| panic!("field prospecting deposit insertion failed: {error}"));
}

fn insert_low_grade_copper(registries: &Registries, state: &mut AppState, region: VoxelBounds) {
    let composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 75_000),
        CompositionComponent::new(MATERIAL_STONE, 925_000),
    ])
    .unwrap_or_else(|error| panic!("low-grade prospecting composition failed: {error}"));
    let spec = GeneratedDepositSpec::new(
        region,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
        Pressure::from_pascals(350_000_000),
        composition,
    )
    .unwrap_or_else(|error| panic!("low-grade prospecting deposit fixture failed: {error}"));
    insert_generated_deposit(registries, state, spec)
        .unwrap_or_else(|error| panic!("low-grade prospecting deposit insertion failed: {error}"));
}

fn start_prospecting(
    registries: &Registries,
    state: &mut AppState,
    method: ProspectingMethodId,
    region: VoxelBounds,
) {
    validate_start_field_prospecting(
        registries,
        state,
        FieldProspectingRequest::new(method, region, MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("field prospecting start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("field prospecting commit failed: {error}"));
}

fn start_inspection(registries: &Registries, state: &mut AppState, region: VoxelBounds) {
    start_prospecting(registries, state, PROSPECTING_FIELD_INSPECTION, region);
}

fn prospecting_duration(registries: &Registries, method: ProspectingMethodId) -> u64 {
    registries
        .labor()
        .get_prospecting(method)
        .unwrap_or_else(|| panic!("prospecting definition {method:?} disappeared"))
        .duration()
        .value()
}

#[test]
fn field_inspection_is_timed_survival_costed_and_records_uncertain_evidence() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_2001));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("field prospecting survival setup failed: {error}"));
    let region = one_voxel(0);
    insert_copper(&registries, &mut state, region);
    let survival_before = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("field prospecting survival state disappeared"));

    start_inspection(&registries, &mut state, region);
    assert!(matches!(
        state.player_work().active(),
        Some(PlayerWork::Prospecting { .. })
    ));
    assert_eq!(state.geological_knowledge().observations().count(), 0);

    let field_duration = prospecting_duration(&registries, PROSPECTING_FIELD_INSPECTION);
    for _ in 1..field_duration {
        let outcome = advance_tick(&registries, &mut state).unwrap_or_else(|error| {
            panic!("field prospecting pre-completion tick failed: {error}")
        });
        assert_eq!(outcome.field_prospecting(), None);
    }
    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("field prospecting completion tick failed: {error}"));
    let observation = outcome
        .field_prospecting()
        .unwrap_or_else(|| panic!("field prospecting completion produced no observation"));
    assert_eq!(observation.method(), PROSPECTING_FIELD_INSPECTION);
    assert_eq!(observation.region(), region);
    assert_eq!(observation.material(), MATERIAL_COPPER);
    assert_eq!(
        observation.evidence(),
        GeologicalEvidenceKind::SurfaceExposure
    );
    assert_eq!(state.player_work().active(), None);

    let record = state
        .geological_knowledge()
        .get_observation(observation.observation())
        .unwrap_or_else(|| panic!("field prospecting observation disappeared"));
    let finding = record
        .finding(MATERIAL_COPPER)
        .unwrap_or_else(|| panic!("field prospecting copper finding disappeared"));
    assert_eq!(finding.lower_ppm(), 900_000);
    assert_eq!(finding.upper_ppm(), 1_000_000);
    let survival_after = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("field prospecting final survival state disappeared"));
    assert!(survival_after.metabolic_energy() < survival_before.metabolic_energy());
    assert!(survival_after.hydration() < survival_before.hydration());
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("field prospecting final audit failed: {error}"));
}

#[test]
fn empty_ground_produces_uncertain_negative_evidence_without_hidden_presence_oracle() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_2002));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("empty prospecting survival setup failed: {error}"));
    let region = one_voxel(10);
    start_inspection(&registries, &mut state, region);
    let mut completed = None;
    for _ in 0..prospecting_duration(&registries, PROSPECTING_FIELD_INSPECTION) {
        completed = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("empty prospecting tick failed: {error}"))
            .field_prospecting();
    }
    let observation = completed.unwrap_or_else(|| panic!("empty prospecting did not complete"));
    let finding = state
        .geological_knowledge()
        .get_observation(observation.observation())
        .and_then(|record| record.finding(MATERIAL_COPPER))
        .unwrap_or_else(|| panic!("empty prospecting finding disappeared"));
    assert_eq!(finding.lower_ppm(), 0);
    assert_eq!(finding.upper_ppm(), 100_000);
}

#[test]
fn regional_abundance_includes_uncovered_ground_in_lower_bound() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_2007));
    let west = one_voxel(0);
    let region = VoxelBounds::new(VoxelCoord::new(0, -1, 0), VoxelCoord::new(2, 0, 1))
        .unwrap_or_else(|error| panic!("regional prospecting bounds failed: {error}"));
    insert_copper(&registries, &mut state, west);

    assert_eq!(
        resolve_local_abundance_bounds(&state, region, MATERIAL_COPPER, 25_000),
        (0, 1_000_000)
    );
}

#[test]
fn adjacent_deposits_jointly_cover_regional_abundance() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_2008));
    let region = VoxelBounds::new(VoxelCoord::new(0, -1, 0), VoxelCoord::new(2, 0, 1))
        .unwrap_or_else(|error| panic!("joint prospecting bounds failed: {error}"));
    insert_low_grade_copper(&registries, &mut state, one_voxel(0));
    insert_copper(&registries, &mut state, one_voxel(1));

    assert_eq!(
        resolve_local_abundance_bounds(&state, region, MATERIAL_COPPER, 25_000),
        (50_000, 1_000_000)
    );
}

#[test]
fn overlapping_deposits_all_contribute_to_regional_abundance_range() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_2009));
    let region = one_voxel(0);
    insert_low_grade_copper(&registries, &mut state, region);
    insert_copper(&registries, &mut state, region);

    assert_eq!(
        resolve_local_abundance_bounds(&state, region, MATERIAL_COPPER, 25_000),
        (50_000, 1_000_000)
    );
}

#[test]
fn field_inspection_rejects_region_larger_than_authored_local_footprint() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_2003));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("large prospecting survival setup failed: {error}"));
    let region = VoxelBounds::new(VoxelCoord::new(0, -1, 0), VoxelCoord::new(2, 0, 1))
        .unwrap_or_else(|error| panic!("large prospecting bounds failed: {error}"));

    assert!(matches!(
        validate_start_field_prospecting(
            &registries,
            &state,
            FieldProspectingRequest::new(PROSPECTING_FIELD_INSPECTION, region, MATERIAL_COPPER,),
        ),
        Err(FieldProspectingStartError::RegionTooLarge {
            actual: 2,
            maximum: 1,
        })
    ));
    assert_eq!(state.player_work().active(), None);
    assert_eq!(state.geological_knowledge().observations().count(), 0);
}

#[test]
fn completed_field_inspection_provides_the_evidence_required_for_mining_target_resolution() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_2004));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("target prospecting survival setup failed: {error}"));
    let region = one_voxel(20);
    insert_copper(&registries, &mut state, region);
    assert!(
        resolve_mining_target(&state, MiningTargetRequest::new(region, MATERIAL_COPPER)).is_err()
    );

    start_inspection(&registries, &mut state, region);
    for _ in 0..prospecting_duration(&registries, PROSPECTING_FIELD_INSPECTION) {
        advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("target prospecting tick failed: {error}"));
    }
    let target = resolve_mining_target(&state, MiningTargetRequest::new(region, MATERIAL_COPPER))
        .unwrap_or_else(|error| panic!("field evidence did not resolve mining target: {error}"));
    assert_eq!(target.region(), region);
    assert_eq!(target.material(), MATERIAL_COPPER);
}

#[test]
fn detailed_field_survey_refines_ambiguous_surface_evidence_into_a_mining_target() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_2006));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("refinement prospecting survival setup failed: {error}"));
    let region = one_voxel(25);
    insert_low_grade_copper(&registries, &mut state, region);
    let request = MiningTargetRequest::new(region, MATERIAL_COPPER);

    start_prospecting(
        &registries,
        &mut state,
        PROSPECTING_FIELD_INSPECTION,
        region,
    );
    let field_duration = prospecting_duration(&registries, PROSPECTING_FIELD_INSPECTION);
    for _ in 0..field_duration {
        advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("surface refinement prospecting tick failed: {error}"));
    }
    assert_eq!(
        resolve_mining_target(&state, request),
        Err(
            MiningTargetResolutionError::EvidenceInsufficientToResolveTarget {
                material: MATERIAL_COPPER,
                region,
            }
        )
    );
    let surface = state
        .geological_knowledge()
        .observations()
        .last()
        .and_then(|record| record.finding(MATERIAL_COPPER))
        .unwrap_or_else(|| panic!("surface refinement finding disappeared"));
    assert_eq!((surface.lower_ppm(), surface.upper_ppm()), (0, 175_000));

    start_prospecting(
        &registries,
        &mut state,
        PROSPECTING_DETAILED_FIELD_SURVEY,
        region,
    );
    let detailed_duration = prospecting_duration(&registries, PROSPECTING_DETAILED_FIELD_SURVEY);
    assert!(detailed_duration > field_duration);
    for _ in 0..detailed_duration {
        advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("detailed refinement prospecting tick failed: {error}"));
    }
    let detailed = state
        .geological_knowledge()
        .observations()
        .last()
        .and_then(|record| record.finding(MATERIAL_COPPER))
        .unwrap_or_else(|| panic!("detailed refinement finding disappeared"));
    assert_eq!(
        (detailed.lower_ppm(), detailed.upper_ppm()),
        (50_000, 100_000)
    );
    let target = resolve_mining_target(&state, request).unwrap_or_else(|error| {
        panic!("detailed surface evidence did not resolve target: {error}")
    });
    assert_eq!(target.region(), region);
    assert_eq!(target.material(), MATERIAL_COPPER);
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("refined prospecting final audit failed: {error}"));
}

#[test]
fn in_progress_field_inspection_round_trip_preserves_deterministic_continuation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_2005));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("round-trip prospecting survival setup failed: {error}"));
    let region = one_voxel(30);
    insert_copper(&registries, &mut state, region);
    start_inspection(&registries, &mut state, region);
    let field_duration = prospecting_duration(&registries, PROSPECTING_FIELD_INSPECTION);
    let pre_save_ticks = 7;
    assert!(pre_save_ticks < field_duration);
    for _ in 0..pre_save_ticks {
        advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("round-trip prospecting pre-save tick failed: {error}"));
    }

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("round-trip prospecting serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("round-trip prospecting decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("round-trip prospecting load failed: {error}"));
    assert_eq!(loaded, state);

    for _ in pre_save_ticks..field_duration {
        let expected = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("round-trip prospecting source tick failed: {error}"));
        let actual = advance_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("round-trip prospecting loaded tick failed: {error}"));
        assert_eq!(actual, expected);
    }
    assert_eq!(loaded, state);
}
