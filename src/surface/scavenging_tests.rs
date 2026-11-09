//! Conservation, particle-size fidelity, and save replay for scavenged surface abrasion.

use super::*;
use crate::content::{
    FORM_LUMP, FORM_TAILINGS, MATERIAL_HEMATITE, MATERIAL_STONE, build_registries,
};
use crate::core::quantity::{Length, Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::inventory::{MaterialLotProfile, MaterialLotProvenance};
use crate::material::{
    CommodityKey, MaterialComposition, MaterialLotSpec, ParticleSizeClass,
    ParticleSizeDistribution, ParticleSizeRange,
};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};

const WORKSITE: VoxelCoord = VoxelCoord::new(0, 0, 0);
const ADJACENT: VoxelCoord = VoxelCoord::new(1, 0, 0);

fn sized_residue(
    registries: &crate::registry::Registries,
    state: &mut AppState,
) -> (SurfaceResourceId, ParticleSizeDistribution) {
    let distribution = ParticleSizeDistribution::new(vec![
        ParticleSizeClass::new(
            ParticleSizeRange::new(Length::from_micrometers(100), Length::from_micrometers(499))
                .unwrap(),
            3,
        )
        .unwrap(),
        ParticleSizeClass::new(
            ParticleSizeRange::new(
                Length::from_micrometers(500),
                Length::from_micrometers(2_000),
            )
            .unwrap(),
            7,
        )
        .unwrap(),
    ])
    .unwrap();
    let lot = MaterialLotSpec::with_composition_and_particle_size(
        CommodityKey::new(MATERIAL_HEMATITE, FORM_TAILINGS),
        Mass::from_milligrams(20_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_HEMATITE),
        distribution.clone(),
    )
    .unwrap();
    let spec = SurfaceResourceSpec::from_existing_material(
        SurfaceResourceKind::Drop,
        WORKSITE,
        lot.mass(),
        MaterialLotProfile::from_spec(&lot),
        MaterialLotProvenance::single(state.tick()),
        0,
    )
    .unwrap();
    let creation = validate_create_surface_resources(registries, state, [spec]).unwrap();
    let id = creation.first_resource_id();
    creation.commit(state).unwrap();
    (id, distribution)
}

fn assert_valid_replay(registries: &crate::registry::Registries, state: &AppState) {
    assert_eq!(validate_loaded_state(registries, state), Ok(()));
    let bytes = serde_json::to_vec(&SaveEnvelope::new(registries, state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(registries)
        .unwrap();
    assert_eq!(&loaded, state);
}

#[test]
fn isolated_abrasion_keeps_the_required_weighted_particulate_profile() {
    let registries = build_registries();
    let mut state = AppState::new();
    let (head, distribution) = sized_residue(&registries, &mut state);
    let initial = calculate_matter_accounting(&state).unwrap().total();

    let tick = state.tick();
    assert!(abrade_scavenged_tool_head(
        &registries,
        &mut state,
        head,
        WORKSITE,
        tick,
        Mass::from_milligrams(1_000),
        FORM_TAILINGS,
    ));
    let source = state.surface().get(head).unwrap();
    assert_eq!(source.remaining_mass(), Mass::from_milligrams(19_000));
    assert_eq!(
        source.material_profile().particle_size_distribution(),
        Some(&distribution)
    );
    let shed = state
        .surface()
        .resources()
        .find(|record| record.id() != head)
        .expect("real abrasion must create recoverable residue");
    assert_eq!(shed.remaining_mass(), Mass::from_milligrams(1_000));
    assert_eq!(
        shed.commodity(),
        CommodityKey::new(MATERIAL_HEMATITE, FORM_TAILINGS)
    );
    assert_eq!(
        shed.material_profile().particle_size_distribution(),
        Some(&distribution)
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial
    );
    assert_valid_replay(&registries, &state);
}

#[test]
fn bulk_split_and_same_form_head_wear_share_exact_residue_physics() {
    let registries = build_registries();
    let mut state = AppState::new();
    let (head, distribution) = sized_residue(&registries, &mut state);
    let rock = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        WORKSITE,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(100_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let creation = validate_create_surface_resources(&registries, &state, [rock]).unwrap();
    let source = creation.first_resource_id();
    creation.commit(&mut state).unwrap();
    let initial = calculate_matter_accounting(&state).unwrap().total();

    let tick = state.tick();
    let (daughter, mass) = split_bulk_scavenged_drop_with_head_wear(
        &registries,
        &mut state,
        ScavengedPortionRequest::new(
            source,
            WORKSITE,
            ADJACENT,
            tick,
            Mass::from_milligrams(40_000),
        ),
        head,
        Mass::from_milligrams(1_000),
        FORM_TAILINGS,
    )
    .expect("local split and finite head wear must both settle");
    assert_eq!(mass, Mass::from_milligrams(40_000));
    assert_eq!(
        state.surface().get(source).unwrap().remaining_mass(),
        Mass::from_milligrams(60_000)
    );
    assert_eq!(state.surface().get(daughter).unwrap().position(), ADJACENT);
    assert_eq!(
        state.surface().get(head).unwrap().remaining_mass(),
        Mass::from_milligrams(19_000)
    );
    let shed = state
        .surface()
        .resources()
        .find(|record| record.id() != source && record.id() != head && record.id() != daughter)
        .expect("tool wear must create separate material custody");
    assert_eq!(shed.position(), WORKSITE);
    assert_eq!(shed.remaining_mass(), Mass::from_milligrams(1_000));
    assert_eq!(
        shed.material_profile().particle_size_distribution(),
        Some(&distribution)
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial
    );
    assert_valid_replay(&registries, &state);
}
