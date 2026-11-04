//! Fluid-transfer errors are an actor-facing interface, not Rust debug dumps.

use super::*;

#[test]
fn fluid_transfer_diagnostics_report_real_route_and_thermal_blockers() {
    let invalid = fixture_at(None, VoxelCoord::new(1, 0, 0));
    let before = invalid.state.clone();
    let route_error = validate_gravity_feed_fluid_to_lower_store(
        &invalid.registries,
        &invalid.state,
        invalid.source,
        invalid.destination,
        Volume::from_microliters(100),
    )
    .unwrap_err();
    assert_eq!(
        route_error.to_string(),
        "gravity feed must go directly down one voxel from (0,0,0) to the lower store; destination is at (1,0,0)"
    );
    assert_eq!(invalid.state, before);

    let thermal = fixture(Some((
        Volume::from_microliters(1),
        Temperature::from_millikelvin(293_151),
    )));
    let before = thermal.state.clone();
    let temperature_error = validate_transfer_fluid_between_local_stores(
        &thermal.registries,
        &thermal.state,
        thermal.source,
        thermal.destination,
        Volume::from_microliters(1),
    )
    .unwrap_err();
    assert_eq!(
        temperature_error.to_string(),
        "mixing would require sub-millikelvin rounding: 586301 mK·µL across 2 µL"
    );
    assert_eq!(thermal.state, before);
}

#[test]
fn fluid_transfer_nested_access_error_keeps_its_cause() {
    use crate::logistics::relocate_player_for_test;

    let mut test = fixture(None);
    relocate_player_for_test(&mut test.state, VoxelCoord::new(1, 0, 0));
    let before = test.state.clone();
    let error = validate_transfer_fluid_between_local_stores(
        &test.registries,
        &test.state,
        test.source,
        test.destination,
        Volume::from_microliters(100),
    )
    .unwrap_err();
    let underlying = format!(
        "fluid store {} is at voxel (0,0,0) but player is at (1,0,0)",
        test.source.value()
    );
    assert_eq!(
        error.to_string(),
        format!("source fluid store is not locally accessible: {underlying}")
    );
    assert_eq!(
        std::error::Error::source(&error).map(ToString::to_string),
        Some(underlying)
    );
    assert_eq!(test.state, before);
    assert_eq!(validate_loaded_state(&test.registries, &test.state), Ok(()));
}

#[test]
fn stale_fluid_transfer_reports_its_changed_revision_without_mutating_custody() {
    let mut test = fixture(None);
    let first = validate_transfer_fluid_between_local_stores(
        &test.registries,
        &test.state,
        test.source,
        test.destination,
        Volume::from_microliters(100),
    )
    .unwrap();
    let obsolete = validate_transfer_fluid_between_local_stores(
        &test.registries,
        &test.state,
        test.source,
        test.destination,
        Volume::from_microliters(100),
    )
    .unwrap();
    let expected = test.state.fluid().revision();
    let _ = first.commit(&mut test.state).unwrap();
    let actual = test.state.fluid().revision();
    let after_first = test.state.clone();
    let error = obsolete.commit(&mut test.state).unwrap_err();
    assert_eq!(
        error,
        FluidTransferCommitError::StaleFluidRevision { expected, actual }
    );
    assert_eq!(
        error.to_string(),
        format!("fluid revision changed from {expected} to {actual} before transfer commit")
    );
    assert_eq!(test.state, after_first);
    assert_eq!(validate_loaded_state(&test.registries, &test.state), Ok(()));
}
