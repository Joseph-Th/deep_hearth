//! Surface custody must have permanent support wherever terrain is authoritative.

use super::*;

const SOURCE: VoxelCoord = VoxelCoord::new(5, 100, 5);
const DESTINATION: VoxelCoord = VoxelCoord::new(6, 100, 5);

fn fixture() -> (crate::Registries, AppState) {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WORLD_SEED,
        ChunkCoord::containing(SOURCE),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(5, 99, 5),
        BLOCK_STATE_STONE,
    );
    (registries, state)
}

fn parcel(position: VoxelCoord, kind: SurfaceResourceKind) -> SurfaceResourceSpec {
    SurfaceResourceSpec::new(
        kind,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(100_000),
        TEMPERATURE,
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap()
}

#[test]
fn surface_footing_creation_rejects_unsupported_generated_parcels() {
    let (registries, state) = fixture();
    let before = state.clone();
    for kind in [SurfaceResourceKind::Drop, SurfaceResourceKind::Opportunity] {
        assert_eq!(
            validate_create_surface_resources(&registries, &state, [parcel(DESTINATION, kind)])
                .err(),
            Some(SurfaceResourceCreationError::InvalidResource(
                SurfaceResourceValidationError::RequiresFixedFloor {
                    resource: SurfaceResourceId::new(state.surface().next_resource_id()),
                    voxel: DESTINATION,
                }
            ))
        );
    }
    assert_eq!(state, before);
}

#[test]
fn surface_footing_relocation_and_split_require_supported_destination() {
    let (registries, mut state) = fixture();
    let creation = validate_create_surface_resources(
        &registries,
        &state,
        [parcel(SOURCE, SurfaceResourceKind::Drop)],
    )
    .unwrap();
    let id = creation.first_resource_id();
    creation.commit(&mut state).unwrap();
    let before = state.clone();
    let at = state.tick();
    assert!(!super::super::relocate_scavenged_drop_step(
        &registries,
        &mut state,
        id,
        SOURCE,
        DESTINATION
    ));
    assert_eq!(state, before);
    assert!(
        super::super::split_bulk_scavenged_drop_step(
            &registries,
            &mut state,
            id,
            SOURCE,
            DESTINATION,
            at,
            Mass::from_milligrams(25_000),
        )
        .is_none()
    );
    assert_eq!(state, before);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(6, 99, 5),
        BLOCK_STATE_STONE,
    );
    let total = calculate_matter_accounting(&state).unwrap().total();
    let (daughter, mass) = super::super::split_bulk_scavenged_drop_step(
        &registries,
        &mut state,
        id,
        SOURCE,
        DESTINATION,
        at,
        Mass::from_milligrams(25_000),
    )
    .unwrap();
    assert_eq!(mass, Mass::from_milligrams(25_000));
    assert_eq!(
        state.surface().get(daughter).unwrap().position(),
        DESTINATION
    );
    assert_eq!(
        state.surface().get(id).unwrap().remaining_mass(),
        Mass::from_milligrams(75_000)
    );
    assert!(super::super::relocate_scavenged_drop_step(
        &registries,
        &mut state,
        id,
        SOURCE,
        DESTINATION
    ));
    assert_eq!(state.surface().get(id).unwrap().position(), DESTINATION);
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
            .unwrap()
            .into_state(&registries),
        Ok(state)
    );
}

#[test]
fn surface_footing_trusted_load_rejects_unsupported_matter_but_not_exhausted_history() {
    let (registries, mut state) = fixture();
    let creation = validate_create_surface_resources(
        &registries,
        &state,
        [parcel(SOURCE, SurfaceResourceKind::Opportunity)],
    )
    .unwrap();
    let id = creation.first_resource_id();
    creation.commit(&mut state).unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(5, 99, 5),
        BlockStateId::AIR,
    );
    let expected =
        StateValidationError::Surface(SurfaceResourceValidationError::RequiresFixedFloor {
            resource: id,
            voxel: SOURCE,
        });
    assert_eq!(
        validate_loaded_state(&registries, &state),
        Err(expected.clone())
    );
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
            .unwrap()
            .into_state(&registries),
        Err(LoadError::InvalidState(expected))
    );
    // Once the parcel is depleted, its history no longer occupies the site.
    // Move the exact fixture mass to inventory rather than discarding it.
    let total = calculate_matter_accounting(&state).unwrap().total();
    let mass = Mass::from_milligrams(100_000);
    let stockpile = crate::inventory::add_solid_stockpile_for_test(&mut state, mass).unwrap();
    deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        mass,
        TEMPERATURE,
    )
    .unwrap();
    let revision = state.surface().revision() + 1;
    let at = state.tick();
    state
        .surface_state_mut()
        .apply_gathering(id, mass, at, revision);
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
