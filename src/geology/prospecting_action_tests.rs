//! Behavioral coverage for player-performed field prospecting.

use super::*;
use crate::content::{
    EQUIPMENT_STONE_GEOLOGICAL_HAMMER, FORM_HANDLE, FORM_ORE, FORM_TOOL, MATERIAL_COPPER,
    MATERIAL_STONE, MATERIAL_WOOD, PROSPECTING_DETAILED_FIELD_SURVEY, PROSPECTING_FIELD_INSPECTION,
    PROSPECTING_LOCAL_TRANSECT, PROSPECTING_REGIONAL_RECONNAISSANCE, build_registries,
};
use crate::core::quantity::{Mass, Pressure, Temperature};
use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::core::time::WorldSeed;
use crate::equipment::{EquipmentId, validate_assemble_equipment};
use crate::geology::{GeneratedDepositSpec, insert_generated_deposit};
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::labor::{PlayerWork, PlayerWorkValidationError, ProspectingMethodId};
use crate::material::{CommodityKey, CompositionComponent, MaterialComposition};
use crate::mining::{MiningTargetRequest, MiningTargetResolutionError, resolve_mining_target};
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::simulation::{TickError, advance_tick};
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::survival::{assess_survival, initialize_player_survival};

fn one_voxel(x: i64) -> VoxelBounds {
    VoxelBounds::new(VoxelCoord::new(x, -1, 0), VoxelCoord::new(x + 1, 0, 1))
        .unwrap_or_else(|error| panic!("field prospecting bounds fixture failed: {error}"))
}

#[test]
fn local_transect_reduces_repeated_point_work_without_revealing_an_exact_target() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_2011));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("local-transect survival setup failed: {error}"));
    let region = horizontal_region(50, 4);
    let target_region = one_voxel(52);
    insert_copper(&registries, &mut state, target_region);

    let transect = registries
        .labor()
        .get_prospecting(PROSPECTING_LOCAL_TRANSECT)
        .copied()
        .unwrap_or_else(|| panic!("local-transect definition disappeared"));
    let inspection = registries
        .labor()
        .get_prospecting(PROSPECTING_FIELD_INSPECTION)
        .copied()
        .unwrap_or_else(|| panic!("field-inspection definition disappeared"));
    assert_eq!(transect.maximum_region_voxels(), 4);
    assert_eq!(transect.abundance_uncertainty_ppm(), 75_000);
    assert!(
        transect.duration().value() < inspection.duration().value() * 4,
        "one bounded transect should cost less active time than four independent point inspections"
    );
    assert!(matches!(
        validate_start_field_prospecting(
            &registries,
            &state,
            FieldProspectingRequest::new(PROSPECTING_FIELD_INSPECTION, region, MATERIAL_COPPER),
        ),
        Err(FieldProspectingStartError::RegionTooLarge {
            actual: 4,
            maximum: 1,
        })
    ));

    start_prospecting(&registries, &mut state, PROSPECTING_LOCAL_TRANSECT, region);
    let mut completed = None;
    for _ in 0..transect.duration().value() {
        completed = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("local-transect tick failed: {error}"))
            .field_prospecting();
    }
    let observation = completed.unwrap_or_else(|| panic!("local transect did not complete"));
    assert_eq!(observation.method(), PROSPECTING_LOCAL_TRANSECT);
    assert_eq!(observation.region(), region);
    assert_eq!(
        observation.evidence(),
        GeologicalEvidenceKind::SurfaceExposure
    );
    let finding = state
        .geological_knowledge()
        .get_observation(observation.observation())
        .and_then(|record| record.finding(MATERIAL_COPPER))
        .unwrap_or_else(|| panic!("local-transect finding disappeared"));
    assert_eq!((finding.lower_ppm(), finding.upper_ppm()), (0, 1_000_000));
    assert_eq!(
        resolve_mining_target(
            &state,
            MiningTargetRequest::new(target_region, MATERIAL_COPPER),
        ),
        Err(
            MiningTargetResolutionError::EvidenceInsufficientToResolveTarget {
                material: MATERIAL_COPPER,
                region: target_region,
            }
        ),
        "area evidence must narrow search effort without leaking the exact occupied voxel"
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("local-transect final audit failed: {error}"));
}

#[test]
fn positive_local_transect_stays_area_evidence_even_with_one_hidden_deposit() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_2012));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("positive local-transect survival setup failed: {error}"));
    let region = horizontal_region(60, 4);
    let requested_voxel = one_voxel(62);
    insert_copper(&registries, &mut state, region);

    start_prospecting(&registries, &mut state, PROSPECTING_LOCAL_TRANSECT, region);
    let duration = prospecting_duration(&registries, PROSPECTING_LOCAL_TRANSECT);
    let mut completed = None;
    for _ in 0..duration {
        completed = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("positive local-transect tick failed: {error}"))
            .field_prospecting();
    }
    let observation =
        completed.unwrap_or_else(|| panic!("positive local transect did not complete"));
    let finding = state
        .geological_knowledge()
        .get_observation(observation.observation())
        .and_then(|record| record.finding(MATERIAL_COPPER))
        .unwrap_or_else(|| panic!("positive local-transect finding disappeared"));
    assert_eq!(
        (finding.lower_ppm(), finding.upper_ppm()),
        (925_000, 1_000_000)
    );
    assert_eq!(
        resolve_mining_target(
            &state,
            MiningTargetRequest::new(requested_voxel, MATERIAL_COPPER),
        ),
        Err(
            MiningTargetResolutionError::EvidenceInsufficientToResolveTarget {
                material: MATERIAL_COPPER,
                region,
            }
        ),
        "positive area evidence must require real local refinement instead of revealing the hidden deposit through a narrow query"
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("positive local-transect final audit failed: {error}"));
}

fn horizontal_region(start_x: i64, width: i64) -> VoxelBounds {
    VoxelBounds::new(
        VoxelCoord::new(start_x, -1, 0),
        VoxelCoord::new(start_x + width, 0, 1),
    )
    .unwrap_or_else(|error| panic!("regional prospecting bounds fixture failed: {error}"))
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

#[test]
fn regional_reconnaissance_trades_precision_for_footprint_then_local_inspection_resolves_target() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_2010));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("regional prospecting survival setup failed: {error}"));
    let region = horizontal_region(40, 4);
    let target_region = one_voxel(42);
    insert_copper(&registries, &mut state, target_region);

    assert!(matches!(
        validate_start_field_prospecting(
            &registries,
            &state,
            FieldProspectingRequest::new(PROSPECTING_FIELD_INSPECTION, region, MATERIAL_COPPER),
        ),
        Err(FieldProspectingStartError::RegionTooLarge {
            actual: 4,
            maximum: 1,
        })
    ));

    start_prospecting(
        &registries,
        &mut state,
        PROSPECTING_REGIONAL_RECONNAISSANCE,
        region,
    );
    let regional_duration = prospecting_duration(&registries, PROSPECTING_REGIONAL_RECONNAISSANCE);
    assert!(
        regional_duration > prospecting_duration(&registries, PROSPECTING_DETAILED_FIELD_SURVEY),
        "regional reconnaissance should trade more elapsed field time for broader coverage"
    );
    let mut completed = None;
    for _ in 0..regional_duration {
        completed = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("regional prospecting tick failed: {error}"))
            .field_prospecting();
    }
    let observation = completed.unwrap_or_else(|| panic!("regional prospecting did not complete"));
    assert_eq!(observation.method(), PROSPECTING_REGIONAL_RECONNAISSANCE);
    assert_eq!(observation.region(), region);
    assert_eq!(
        observation.evidence(),
        GeologicalEvidenceKind::LooseIndicator
    );
    let finding = state
        .geological_knowledge()
        .get_observation(observation.observation())
        .and_then(|record| record.finding(MATERIAL_COPPER))
        .unwrap_or_else(|| panic!("regional prospecting finding disappeared"));
    assert_eq!((finding.lower_ppm(), finding.upper_ppm()), (0, 1_000_000));
    assert_eq!(
        resolve_mining_target(
            &state,
            MiningTargetRequest::new(target_region, MATERIAL_COPPER),
        ),
        Err(
            MiningTargetResolutionError::EvidenceInsufficientToResolveTarget {
                material: MATERIAL_COPPER,
                region: target_region,
            }
        )
    );

    start_inspection(&registries, &mut state, target_region);
    for _ in 0..prospecting_duration(&registries, PROSPECTING_FIELD_INSPECTION) {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("local refinement tick failed: {error}"));
    }
    let target = resolve_mining_target(
        &state,
        MiningTargetRequest::new(target_region, MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("local refinement did not resolve target: {error}"));
    assert_eq!(target.region(), target_region);
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("regional refinement final audit failed: {error}"));
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

fn assemble_sampling_hammer(registries: &Registries, state: &mut AppState) -> EquipmentId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(650_000))
        .unwrap_or_else(|error| panic!("sampling-hammer assembly stockpile failed: {error}"));
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(500_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(150_000),
        ),
    ] {
        deposit_lot_for_test(
            registries,
            state,
            source,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("sampling-hammer assembly material failed: {error}"));
    }
    validate_assemble_equipment(registries, state, EQUIPMENT_STONE_GEOLOGICAL_HAMMER, source)
        .unwrap_or_else(|error| panic!("sampling-hammer assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("sampling-hammer assembly commit failed: {error}"))
}

fn prospecting_duration(registries: &Registries, method: ProspectingMethodId) -> u64 {
    registries
        .labor()
        .get_prospecting(method)
        .unwrap_or_else(|| panic!("prospecting definition {method:?} disappeared"))
        .duration()
        .value()
}

fn inspection_ready_to_complete_fixture() -> (Registries, AppState) {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_E001));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("prospecting exhaustion survival setup failed: {error}"));
    let region = one_voxel(40);
    insert_copper(&registries, &mut state, region);
    start_inspection(&registries, &mut state, region);
    let duration = prospecting_duration(&registries, PROSPECTING_FIELD_INSPECTION);
    for _ in 1..duration {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("prospecting exhaustion setup tick failed: {error}"));
    }
    (registries, state)
}

#[test]
fn completion_tick_rejects_exhausted_observation_id_without_partial_progress() {
    let (registries, state) = inspection_ready_to_complete_fixture();
    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("prospecting observation-id exhaustion serialization failed: {error}")
        });
    encoded["state"]["systems"]["geological_knowledge"]["next_observation_id"] =
        serde_json::json!(u32::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded).unwrap_or_else(|error| {
        panic!("prospecting observation-id exhaustion decode failed: {error}")
    });
    let mut loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("prospecting observation-id exhaustion fixture should load: {error}")
    });
    let before = loaded.clone();

    assert_eq!(
        advance_tick(&registries, &mut loaded),
        Err(TickError::GeologicalObservationIdExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn completion_tick_rejects_exhausted_knowledge_revision_without_partial_progress() {
    let (registries, state) = inspection_ready_to_complete_fixture();
    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("prospecting knowledge revision exhaustion serialization failed: {error}")
        });
    encoded["state"]["systems"]["geological_knowledge"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded).unwrap_or_else(|error| {
        panic!("prospecting knowledge revision exhaustion decode failed: {error}")
    });
    let mut loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("prospecting knowledge revision exhaustion fixture should load: {error}")
    });
    let before = loaded.clone();

    assert_eq!(
        advance_tick(&registries, &mut loaded),
        Err(TickError::GeologicalKnowledgeRevisionExhausted)
    );
    assert_eq!(loaded, before);
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
    let physiology = registries.survival().physiology();
    let exertion = registries
        .labor()
        .get_prospecting(PROSPECTING_FIELD_INSPECTION)
        .unwrap_or_else(|| panic!("field inspection definition disappeared"))
        .exertion();
    assert_eq!(
        survival_before.metabolic_energy().nanojoules()
            - survival_after.metabolic_energy().nanojoules(),
        (physiology.basal_energy_cost_per_tick().nanojoules()
            + exertion.energy_cost_per_tick().nanojoules())
            * u128::from(field_duration),
        "prospecting admission duration must equal the exact number of charged field-work ticks"
    );
    assert_eq!(
        survival_before.hydration().microliters() - survival_after.hydration().microliters(),
        (physiology.hydration_loss_per_tick().microliters()
            + exertion.hydration_loss_per_tick().microliters())
            * field_duration,
        "prospecting hydration budgeting must match realized field-work cost"
    );
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
        resolve_region_abundance_bounds(&state, region, MATERIAL_COPPER, 25_000),
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
        resolve_region_abundance_bounds(&state, region, MATERIAL_COPPER, 25_000),
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
        resolve_region_abundance_bounds(&state, region, MATERIAL_COPPER, 25_000),
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
        let _ = advance_tick(&registries, &mut state)
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
    let hammer = assemble_sampling_hammer(&registries, &mut state);

    start_prospecting(
        &registries,
        &mut state,
        PROSPECTING_FIELD_INSPECTION,
        region,
    );
    let field_duration = prospecting_duration(&registries, PROSPECTING_FIELD_INSPECTION);
    for _ in 0..field_duration {
        let _ = advance_tick(&registries, &mut state)
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

    validate_start_field_prospecting(
        &registries,
        &state,
        FieldProspectingRequest::new_with_equipment(
            PROSPECTING_DETAILED_FIELD_SURVEY,
            region,
            MATERIAL_COPPER,
            hammer,
        ),
    )
    .unwrap_or_else(|error| panic!("detailed refinement prospecting start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("detailed refinement prospecting commit failed: {error}"));
    let detailed_duration = prospecting_duration(&registries, PROSPECTING_DETAILED_FIELD_SURVEY);
    assert!(detailed_duration > field_duration);
    for _ in 0..detailed_duration {
        let _ = advance_tick(&registries, &mut state)
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
        let _ = advance_tick(&registries, &mut state)
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

#[test]
fn trusted_load_rejects_forged_field_prospecting_duration() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x6B00_2006));
    initialize_player_survival(&registries, &mut state).unwrap_or_else(|error| {
        panic!("prospecting duration-tamper survival setup failed: {error}")
    });
    let region = one_voxel(31);
    insert_copper(&registries, &mut state, region);
    start_inspection(&registries, &mut state, region);

    let mut tampered =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("prospecting duration-tamper serialization failed: {error}")
        });
    let completion = tampered["state"]["systems"]["player_work"]["active"]["Prospecting"]
        ["work"]["completes_at"]
        .as_u64()
        .unwrap_or_else(|| panic!("prospecting completion tick was not serialized as u64"));
    tampered["state"]["systems"]["player_work"]["active"]["Prospecting"]["work"]["completes_at"] =
        serde_json::json!(completion + 1);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("prospecting duration-tamper decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::ProspectingDurationMismatch
        )))
    );
}
