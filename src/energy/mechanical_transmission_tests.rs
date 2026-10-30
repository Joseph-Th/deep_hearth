//! Physical topology limits for non-teleporting line shafts.

use super::*;

fn roller_set(cells: &[(i64, i64)]) -> BTreeMap<VoxelCoord, StructuralElementId> {
    cells
        .iter()
        .enumerate()
        .map(|(index, &(x, z))| {
            (
                VoxelCoord::new(x, 5, z),
                StructuralElementId::new(u32::try_from(index + 1).unwrap()),
            )
        })
        .collect()
}

#[test]
fn only_an_externally_bounded_contiguous_straight_run_has_endpoint_cells() {
    let rollers = roller_set(&[(1, 2), (2, 2), (3, 2)]);
    let (path, low, high) = discover_shaft_run(&rollers, VoxelCoord::new(1, 5, 2), 1, 0)
        .expect("the whole shaft must resolve from its leftmost roller");
    assert_eq!(path.len(), 3);
    assert_eq!(low, VoxelCoord::new(0, 5, 2));
    assert_eq!(high, VoxelCoord::new(4, 5, 2));
    assert_eq!(
        path,
        [
            StructuralElementId::new(1),
            StructuralElementId::new(2),
            StructuralElementId::new(3),
        ]
    );
    assert!(discover_shaft_run(&rollers, VoxelCoord::new(2, 5, 2), 1, 0).is_none());
    assert!(discover_shaft_run(&rollers, VoxelCoord::new(3, 5, 2), 1, 0).is_none());
    assert_eq!(
        discover_shaft_run(&rollers, VoxelCoord::new(1, 5, 2), 0, 1)
            .unwrap()
            .0
            .len(),
        1,
        "a 90-degree shaft cannot turn without a dedicated gear or coupling",
    );

    let mut gap = rollers;
    gap.remove(&VoxelCoord::new(2, 5, 2));
    let (left, _, left_end) = discover_shaft_run(&gap, VoxelCoord::new(1, 5, 2), 1, 0).unwrap();
    let (right, right_start, _) = discover_shaft_run(&gap, VoxelCoord::new(3, 5, 2), 1, 0).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(right.len(), 1);
    assert_eq!(left_end, VoxelCoord::new(2, 5, 2));
    assert_eq!(right_start, VoxelCoord::new(2, 5, 2));
}

#[test]
fn runs_resolve_on_both_axes_and_do_not_overflow_world_coordinate_limits() {
    let rollers = roller_set(&[(2, 1), (2, 2), (2, 3)]);
    let (path, low, high) = discover_shaft_run(&rollers, VoxelCoord::new(2, 5, 1), 0, 1).unwrap();
    assert_eq!(path.len(), 3);
    assert_eq!(low, VoxelCoord::new(2, 5, 0));
    assert_eq!(high, VoxelCoord::new(2, 5, 4));
    assert!(
        discover_shaft_run(
            &roller_set(&[(i64::MAX, 2)]),
            VoxelCoord::new(i64::MAX, 5, 2),
            1,
            0,
        )
        .is_none()
    );
    assert!(
        discover_shaft_run(
            &roller_set(&[(i64::MIN, 2)]),
            VoxelCoord::new(i64::MIN, 5, 2),
            1,
            0,
        )
        .is_none()
    );
}

#[test]
fn finite_sink_headroom_bounds_delivery_after_physical_roller_losses() {
    assert_eq!(bounded_shaft_transfer(500, 1, 1), Some((2, 1)));
    assert_eq!(bounded_shaft_transfer(500, 1, 3), Some((4, 1)));
    assert_eq!(bounded_shaft_transfer(500, 2, 1), Some((3, 2)));
    assert_eq!(bounded_shaft_transfer(200, 200, 1), Some((200, 160)));
    assert_eq!(bounded_shaft_transfer(2, 100, 3), None);
    assert_eq!(bounded_shaft_transfer(500, 0, 1), None);
    assert_eq!(bounded_shaft_transfer(0, 100, 1), None);
    assert_eq!(
        bounded_shaft_transfer(u128::MAX, 1, 1),
        Some((2, 1)),
        "full-width energy arithmetic must not overflow when the sink is nearly full"
    );
}

#[test]
fn exact_charge_comparison_handles_fractional_ppm_and_full_width_capacity() {
    assert_eq!(
        compare_charge(
            999_999_999_999,
            1_000_000_000_000,
            1_000_000_000_000,
            1_000_000_000_000
        ),
        Ordering::Less
    );
    assert_eq!(
        compare_charge(
            1_000_000_000_000,
            1_000_000_000_000,
            999_999_999_999,
            1_000_000_000_000
        ),
        Ordering::Greater
    );
    assert_eq!(compare_charge(2, 4, 3, 6), Ordering::Equal);
    assert_eq!(
        compare_charge(u128::MAX, u128::MAX, u128::MAX - 1, u128::MAX),
        Ordering::Greater
    );
    assert_eq!(
        compare_charge(u128::MAX - 1, u128::MAX, u128::MAX, u128::MAX),
        Ordering::Less
    );
}

#[test]
fn friction_aware_equalization_never_reverses_a_real_charge_gradient() {
    assert_eq!(equalizing_shaft_draw(100, 1, 100, 100, 99, 100), None);
    assert_eq!(
        equalizing_shaft_draw(100, 1, 100, 100, 97, 100),
        Some((2, 1))
    );
    assert_eq!(
        equalizing_shaft_draw(100, 1, 100, 100, 92, 100),
        Some((4, 3))
    );
    // Unequal capacities must compare fractional charge, not absolute stored work.
    assert_eq!(equalizing_shaft_draw(100, 1, 100, 100, 198, 200), None);
    assert_eq!(
        equalizing_shaft_draw(100, 1, 100, 100, 194, 200),
        Some((2, 1))
    );
    // The full-width bound and comparison must also work where cross products overflow.
    assert_eq!(
        equalizing_shaft_draw(100, 1, u128::MAX, u128::MAX, u128::MAX - 1, u128::MAX),
        None
    );
}

#[test]
fn near_full_mechanical_store_equalizes_without_reversing_or_creating_work() {
    use crate::content::make_test_registries_with_energy_store;
    use crate::core::quantity::Power;
    use crate::energy::{
        EnergyStoreDefinition, EnergyStoreDefinitionId, add_energy_store_with_initial_for_fixture,
    };

    let definition = EnergyStoreDefinitionId::new(990_678);
    let capacity = Energy::from_nanojoules(1_000_000_000);
    let registries =
        make_test_registries_with_energy_store(EnergyStoreDefinition::new_with_transfer_limits(
            definition,
            "small precision shaft store",
            EnergyCarrier::Mechanical,
            capacity,
            Power::from_microwatts(1_000),
            Power::from_microwatts(1_000),
        ));
    let mut state = AppState::new();
    let source =
        add_energy_store_with_initial_for_fixture(&registries, &mut state, definition, capacity)
            .unwrap();
    let destination = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        definition,
        Energy::from_nanojoules(capacity.nanojoules() - 3),
    )
    .unwrap();
    let passive =
        crate::energy::passive_dissipation::decide_passive_energy_dissipation(&registries, &state);
    let transfer = plan_pair(
        &registries,
        &state,
        &passive,
        StructuralElementId::new(1),
        1,
        source,
        destination,
    )
    .expect("a three-nanojoule charge gap can equalize with one-nanojoule delivery");
    assert_eq!(transfer.drawn(), Energy::from_nanojoules(2));
    assert_eq!(transfer.delivered(), Energy::from_nanojoules(1));
    assert_eq!(transfer.lost(), Energy::from_nanojoules(1));
    let before_revision = state.energy().revision();
    let applied = apply_mechanical_transmission(
        &mut state,
        MechanicalTransmissionPlan {
            transfers: vec![transfer],
        },
    );
    assert_eq!(applied, vec![transfer]);
    assert_eq!(state.energy().revision(), before_revision + 1);
    assert_eq!(
        state.energy().get_store(source).unwrap().stored(),
        Energy::from_nanojoules(capacity.nanojoules() - 2)
    );
    assert_eq!(
        state.energy().get_store(destination).unwrap().stored(),
        Energy::from_nanojoules(capacity.nanojoules() - 2)
    );
    let subsequent = plan_pair(
        &registries,
        &state,
        &crate::energy::passive_dissipation::decide_passive_energy_dissipation(&registries, &state),
        StructuralElementId::new(1),
        1,
        source,
        destination,
    );
    assert!(
        subsequent.is_none(),
        "equalized shafts must not ping-pong and burn work"
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn one_nanojoule_charge_difference_does_not_start_a_lossy_oscillation() {
    use crate::content::make_test_registries_with_energy_store;
    use crate::core::quantity::Power;
    use crate::energy::{
        EnergyStoreDefinition, EnergyStoreDefinitionId, add_energy_store_with_initial_for_fixture,
    };

    let definition = EnergyStoreDefinitionId::new(990_679);
    let capacity = Energy::from_nanojoules(1_000_000_000_000);
    let registries =
        make_test_registries_with_energy_store(EnergyStoreDefinition::new_with_transfer_limits(
            definition,
            "high capacity shaft store",
            EnergyCarrier::Mechanical,
            capacity,
            Power::from_microwatts(1_000),
            Power::from_microwatts(1_000),
        ));
    let mut state = AppState::new();
    let source =
        add_energy_store_with_initial_for_fixture(&registries, &mut state, definition, capacity)
            .unwrap();
    let destination = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        definition,
        Energy::from_nanojoules(capacity.nanojoules() - 1),
    )
    .unwrap();
    let passive =
        crate::energy::passive_dissipation::decide_passive_energy_dissipation(&registries, &state);
    assert!(
        plan_pair(
            &registries,
            &state,
            &passive,
            StructuralElementId::new(1),
            1,
            source,
            destination,
        )
        .is_none(),
        "a shaft cannot spend two nanojoules to move one and reverse a one-nanojoule gradient"
    );
}
