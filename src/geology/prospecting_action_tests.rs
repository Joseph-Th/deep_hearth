//! Behavioral coverage for player-performed field prospecting.

use super::*;
use crate::content::{FORM_ORE, MATERIAL_COPPER, PROSPECTING_FIELD_INSPECTION, build_registries};
use crate::core::quantity::{Mass, Pressure, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::core::time::WorldSeed;
use crate::geology::{GeneratedDepositSpec, insert_generated_deposit};
use crate::labor::PlayerWork;
use crate::material::{CommodityKey, MaterialComposition};
use crate::mining::{MiningTargetRequest, resolve_mining_target};
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

fn start_inspection(registries: &Registries, state: &mut AppState, region: VoxelBounds) {
    validate_start_field_prospecting(
        registries,
        state,
        FieldProspectingRequest::new(PROSPECTING_FIELD_INSPECTION, region, MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("field prospecting start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("field prospecting commit failed: {error}"));
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

    for _ in 1..24 {
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
    for _ in 0..24 {
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
    for _ in 0..24 {
        advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("target prospecting tick failed: {error}"));
    }
    let target = resolve_mining_target(&state, MiningTargetRequest::new(region, MATERIAL_COPPER))
        .unwrap_or_else(|error| panic!("field evidence did not resolve mining target: {error}"));
    assert_eq!(target.region(), region);
    assert_eq!(target.material(), MATERIAL_COPPER);
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
    for _ in 0..7 {
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

    for _ in 7..24 {
        let expected = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("round-trip prospecting source tick failed: {error}"));
        let actual = advance_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("round-trip prospecting loaded tick failed: {error}"));
        assert_eq!(actual, expected);
    }
    assert_eq!(loaded, state);
}
