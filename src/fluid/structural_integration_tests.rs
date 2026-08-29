//! Contract tests for fluid-owned structural loads.

use super::*;
use crate::content::{
    FORM_LOG, MATERIAL_CHARCOAL, MATERIAL_WATER, MATERIAL_WOOD,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, make_test_registries_with_fluids,
};
use crate::core::quantity::{Area, Volume};
use crate::core::state::{StateValidationError, validate_loaded_state};
use crate::core::time::WorldSeed;
use crate::fluid::{
    FluidDefinition, FluidDefinitionId, FluidEgressError, FluidValidationError, add_fluid_store,
    add_fluid_store_with_contents_for_fixture, validate_fluid_egress,
};
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralLoadKind, StructuralMutationError, add_structural_element,
    materialize_structural_element_for_test, validate_activate_structural_element,
    validate_remove_structural_element, validate_set_structural_load,
};

const TEST_FLUID: FluidDefinitionId = FluidDefinitionId::new(941_001);
const TEST_TEMPERATURE: crate::core::quantity::Temperature =
    crate::core::quantity::Temperature::from_millikelvin(293_150);

fn registries_with_material(material: crate::material::MaterialId) -> Registries {
    make_test_registries_with_fluids(vec![FluidDefinition::new(
        TEST_FLUID,
        "structural fluid fixture",
        material,
    )])
}

#[test]
fn trusted_load_rejects_absolute_zero_fluid_contents() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x9410_0009));
    let store = add_filled(&registries, &mut state, 1_000);
    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("zero-temperature fluid tamper serialization failed: {error}")
        });
    encoded["state"]["systems"]["fluid"]["records"][store.value().to_string()]["contents"]["temperature"] =
        serde_json::json!(0_u32);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("zero-temperature fluid tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Fluid(
            FluidValidationError::ZeroStoredTemperature { store }
        )))
    );
}

fn registries() -> Registries {
    registries_with_material(MATERIAL_WATER)
}

#[test]
fn fluid_fixture_rejects_absolute_zero_contents_without_mutation() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x9410_000A));
    let before = state.clone();

    assert_eq!(
        add_fluid_store_with_contents_for_fixture(
            &registries,
            &mut state,
            Volume::from_microliters(1_000),
            TEST_FLUID,
            Volume::from_microliters(1_000),
            crate::core::quantity::Temperature::ZERO,
        ),
        Err(super::super::fixture_execution::AddFluidStoreError::InitialTemperatureZero)
    );
    assert_eq!(state, before);
}

fn bounds(x: i64) -> VoxelBounds {
    match VoxelBounds::new(VoxelCoord::new(x, 0, 0), VoxelCoord::new(x + 1, 1, 1)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("fluid structural bounds fixture failed: {error}"),
    }
}

fn add_active_support(
    registries: &Registries,
    state: &mut AppState,
    x: i64,
) -> StructuralElementId {
    let element = match add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            bounds(x),
            crate::core::quantity::Length::from_micrometers(1),
            Area::from_square_millimeters(1_000),
        ),
        true,
    ) {
        Ok(element) => element,
        Err(error) => panic!("fluid structural support fixture failed: {error}"),
    };
    materialize_structural_element_for_test(registries, state, element, FORM_LOG);
    let activation = match validate_activate_structural_element(registries, state, element) {
        Ok(token) => token,
        Err(error) => panic!("fluid structural activation fixture failed: {error}"),
    };
    if let Err(error) = activation.commit(state) {
        panic!("fluid structural activation commit failed: {error}");
    }
    element
}

fn add_filled(
    registries: &Registries,
    state: &mut AppState,
    volume_microliters: u64,
) -> FluidStoreId {
    match add_fluid_store_with_contents_for_fixture(
        registries,
        state,
        Volume::from_microliters(volume_microliters),
        TEST_FLUID,
        Volume::from_microliters(volume_microliters),
        TEST_TEMPERATURE,
    ) {
        Ok(store) => store,
        Err(error) => panic!("fluid structural filled-store fixture failed: {error}"),
    }
}

fn mount(
    registries: &Registries,
    state: &mut AppState,
    store: FluidStoreId,
    support: StructuralElementId,
) -> FluidSupportOutcome {
    let token = match validate_mount_fluid_store(registries, state, store, support) {
        Ok(token) => token,
        Err(error) => panic!("fluid support mount validation failed: {error}"),
    };
    match token.commit(state) {
        Ok(outcome) => outcome,
        Err(error) => panic!("fluid support mount commit failed: {error}"),
    }
}

#[test]
fn mounted_fluid_uses_material_density_for_structural_weight() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x9410_0001));
    let support = add_active_support(&registries, &mut state, 0);
    let store = add_filled(&registries, &mut state, 1_000_000);

    let outcome = mount(&registries, &mut state, store, support);
    let expected = match calculate_aggregate_weight_force_ceiling(
        AggregateMass::from_milligrams(1_000_000),
        registries.core().gravity(),
    ) {
        Some(force) => force,
        None => panic!("fluid structural fixture weight overflowed"),
    };

    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::Fluid)),
        Some(expected)
    );
    assert_eq!(
        state
            .fluid()
            .get_store(store)
            .and_then(|record| record.supported_by()),
        Some(support)
    );
    assert!(outcome.structural_analysis().is_some());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn fluid_mass_rounding_occurs_after_support_local_aggregation() {
    let registries = registries_with_material(MATERIAL_CHARCOAL);
    let mut state = AppState::new(WorldSeed::new(0x9410_0002));
    let support = add_active_support(&registries, &mut state, 0);
    for _ in 0..4 {
        let store = add_filled(&registries, &mut state, 1);
        let _ = mount(&registries, &mut state, store, support);
    }

    assert_eq!(
        supported_mass_numerator(&registries, &state, support, &BTreeMap::new(), None),
        Ok(1_000)
    );
    assert_eq!(numerator_to_mass(1_000).milligrams(), 1);
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::Fluid)),
        Some(Force::from_millinewtons(1))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn direct_fluid_load_write_and_supported_member_removal_are_blocked() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x9410_0003));
    let support = add_active_support(&registries, &mut state, 0);
    let store = add_filled(&registries, &mut state, 1_000);
    let _ = mount(&registries, &mut state, store, support);

    assert_eq!(
        validate_set_structural_load(
            &registries,
            &state,
            support,
            StructuralLoadKind::Fluid,
            Force::ZERO,
        ),
        Err(StructuralMutationError::LoadOwnedBySubsystem {
            kind: StructuralLoadKind::Fluid,
        })
    );
    assert_eq!(
        validate_remove_structural_element(&registries, &state, support),
        Err(StructuralMutationError::ElementSupportsFluidStore {
            element: support,
            store,
        })
    );
}

#[test]
fn failed_support_can_be_drained_and_rejects_new_mounts() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x9410_0004));
    let support = add_active_support(&registries, &mut state, 0);
    let source = add_filled(&registries, &mut state, 5_000_000_000);
    let outcome = mount(&registries, &mut state, source, support);
    assert!(
        outcome
            .structural_analysis()
            .is_some_and(|analysis| !analysis.damage_events().is_empty())
    );
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Failed)
    );
    let drain = match validate_fluid_egress(
        &registries,
        &state,
        source,
        Volume::from_microliters(1_000_000),
    ) {
        Ok(token) => token,
        Err(error) => panic!("failed-support drain validation failed: {error:?}"),
    };
    if let Err(error) = drain.commit(&mut state) {
        panic!("failed-support drain commit failed: {error}");
    }
    assert_eq!(
        state
            .fluid()
            .get_store(source)
            .map(|record| record.stored_volume()),
        Some(Volume::from_microliters(4_999_000_000))
    );
    let incoming = add_filled(&registries, &mut state, 1);
    assert!(matches!(
        validate_mount_fluid_store(&registries, &state, incoming, support),
        Err(FluidSupportError::TargetNotActive {
            element,
            lifecycle: StructuralLifecycle::Failed,
        }) if element == support
    ));

    let unmount = match validate_unmount_fluid_store(&registries, &state, source) {
        Ok(token) => token,
        Err(error) => panic!("failed-support unmount validation failed: {error}"),
    };
    if let Err(error) = unmount.commit(&mut state) {
        panic!("failed-support unmount commit failed: {error}");
    }
    assert_eq!(
        state
            .fluid()
            .get_store(source)
            .and_then(|record| record.supported_by()),
        None
    );
}

#[test]
fn fluid_support_change_rejects_stale_fluid_owner_before_structural_mutation() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x9410_0006));
    let support = add_active_support(&registries, &mut state, 0);
    let store = add_filled(&registries, &mut state, 1_000_000);
    let token = match validate_mount_fluid_store(&registries, &state, store, support) {
        Ok(token) => token,
        Err(error) => panic!("stale fluid support setup failed: {error}"),
    };
    let structure_before = state.structures().clone();
    if let Err(error) = add_fluid_store(&mut state, Volume::from_microliters(1)) {
        panic!("stale fluid support owner mutation failed: {error}");
    }

    assert!(matches!(
        token.commit(&mut state),
        Err(FluidSupportCommitError::StaleFluidRevision {
            expected: _expected,
            actual: _actual,
        })
    ));
    assert_eq!(state.structures(), &structure_before);
    assert_eq!(
        state
            .fluid()
            .get_store(store)
            .and_then(|record| record.supported_by()),
        None
    );
}

#[test]
fn fluid_mount_rejects_exhausted_fluid_revision_without_structural_mutation() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x9410_E001));
    let support = add_active_support(&registries, &mut state, 0);
    let store = add_filled(&registries, &mut state, 1_000_000);
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("fluid mount exhaustion serialization failed: {error}"));
    encoded["state"]["systems"]["fluid"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("fluid mount exhaustion decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("fluid mount exhaustion fixture should load: {error}"));
    let before = loaded.clone();

    assert_eq!(
        validate_mount_fluid_store(&registries, &loaded, store, support).err(),
        Some(FluidSupportError::FluidRevisionExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn fluid_egress_rejects_exhausted_revision_without_withdrawing_volume() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x9410_E002));
    let store = add_filled(&registries, &mut state, 1_000_000);
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("fluid egress exhaustion serialization failed: {error}"));
    encoded["state"]["systems"]["fluid"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("fluid egress exhaustion decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("fluid egress exhaustion fixture should load: {error}"));
    let before = loaded.clone();

    assert_eq!(
        validate_fluid_egress(&registries, &loaded, store, Volume::from_microliters(1),).err(),
        Some(FluidEgressError::RevisionExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn supported_fluid_round_trip_preserves_support_index_and_derived_load() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x9410_0007));
    let support = add_active_support(&registries, &mut state, 0);
    let store = add_filled(&registries, &mut state, 1_000_000);
    let _ = mount(&registries, &mut state, store, support);
    let expected_load = state
        .structures()
        .get_element(support)
        .map(|record| record.load(StructuralLoadKind::Fluid));

    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("supported fluid save serialization failed: {error}"),
    };
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("supported fluid save decode failed: {error}"),
    };
    let loaded = match decoded.into_state(&registries) {
        Ok(loaded) => loaded,
        Err(error) => panic!("supported fluid save validation failed: {error}"),
    };

    assert_eq!(loaded, state);
    assert_eq!(
        loaded
            .fluid()
            .get_store(store)
            .and_then(|record| record.supported_by()),
        Some(support)
    );
    assert_eq!(
        loaded
            .structures()
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::Fluid)),
        expected_load
    );
}

#[test]
fn tampered_fluid_derived_load_is_rejected_on_load() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x9410_0008));
    let support = add_active_support(&registries, &mut state, 0);
    let store = add_filled(&registries, &mut state, 1_000_000);
    let _ = mount(&registries, &mut state, store, support);

    let expected = match state.structures().get_element(support) {
        Some(record) => record.load(StructuralLoadKind::Fluid),
        None => panic!("fluid load tamper support disappeared"),
    };
    let wrong = Force::from_millinewtons(expected.millinewtons() + 1);
    let mut wrong_load = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("fluid load tamper serialization failed: {error}"),
    };
    wrong_load["state"]["systems"]["structures"]["elements"][support.value().to_string()]["loads"]
        ["Fluid"] = serde_json::json!(wrong.millinewtons());
    let wrong_load: LoadedSaveEnvelope = match serde_json::from_value(wrong_load) {
        Ok(decoded) => decoded,
        Err(error) => panic!("fluid load tamper failed decode: {error}"),
    };
    assert_eq!(
        wrong_load.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::FluidStructuralLoad(
                FluidStructuralLoadError::ExistingLoadMismatch {
                    element: support,
                    stored: wrong,
                    expected,
                }
            )
        ))
    );
}
