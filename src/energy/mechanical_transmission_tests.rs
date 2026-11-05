//! Physical topology limits for non-teleporting line shafts.

use super::*;

#[path = "mechanical_transmission_tests/crossing.rs"]
mod crossing;
#[path = "mechanical_transmission_tests/crowded.rs"]
mod crowded;

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
fn overlapping_narrow_rollers_cannot_become_an_arbitrary_transmission_path() {
    let disputed = VoxelCoord::new(2, 5, 2);
    let first = VoxelCoord::new(1, 5, 2);
    let last = VoxelCoord::new(3, 5, 2);
    let rollers = unique_roller_cells([
        (first, StructuralElementId::new(1)),
        (disputed, StructuralElementId::new(2)),
        (disputed, StructuralElementId::new(3)),
        (disputed, StructuralElementId::new(4)),
        (last, StructuralElementId::new(5)),
    ]);
    assert!(!rollers.contains_key(&disputed));
    assert_eq!(rollers.len(), 2);
    assert_eq!(
        discover_shaft_run(&rollers, first, 1, 0).unwrap().0.len(),
        1
    );
    assert_eq!(discover_shaft_run(&rollers, last, 1, 0).unwrap().0.len(), 1);
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
    assert_eq!(bounded_shaft_transfer(10, 10, 3), Some((9, 4)));
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
fn quantized_roller_delivery_never_consumes_an_equivalent_but_wasteful_extra_draw() {
    // Three rollers transmit four nanojoules from either nine or ten input
    // nanojoules. Only nine must leave the finite source.
    assert_eq!(delivered_after_rollers(9, 3), 4);
    assert_eq!(delivered_after_rollers(10, 3), 4);
    assert_eq!(equalizing_shaft_draw(10, 3, 100, 100, 0, 100), Some((9, 4)));

    for segments in 1..=5 {
        for draw in 1..=500_u128 {
            let delivered = delivered_after_rollers(draw, segments);
            if delivered == 0 {
                continue;
            }
            let minimum = minimum_draw_for_delivery(delivered, segments);
            assert!(minimum <= draw);
            assert_eq!(delivered_after_rollers(minimum, segments), delivered);
            assert!(
                minimum == 1 || delivered_after_rollers(minimum - 1, segments) < delivered,
                "a smaller draw must never produce the same work"
            );

            for headroom in 1..=10_u128 {
                let (bounded_draw, bounded_delivery) =
                    bounded_shaft_transfer(draw, headroom, segments).unwrap();
                assert!(bounded_draw <= draw && bounded_delivery <= headroom);
                assert_eq!(
                    bounded_delivery,
                    delivered.min(headroom),
                    "a bounded sink must receive all the representable permitted work"
                );
                assert_eq!(
                    delivered_after_rollers(bounded_draw, segments),
                    bounded_delivery
                );
                assert!(
                    bounded_draw == 1
                        || delivered_after_rollers(bounded_draw - 1, segments) < bounded_delivery,
                    "no smaller intake can deliver the same requested work"
                );
            }
        }
    }
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
fn receiver_power_limit_applies_to_delivered_shaft_work_not_pre_loss_draw() {
    use crate::content::make_test_registries_with_energy_store;
    use crate::core::quantity::Power;
    use crate::energy::{
        EnergyStoreDefinition, EnergyStoreDefinitionId, add_energy_store_with_initial_for_fixture,
    };

    let definition = EnergyStoreDefinitionId::new(990_681);
    let registries =
        make_test_registries_with_energy_store(EnergyStoreDefinition::new_with_transfer_limits(
            definition,
            "mechanical store with restricted input power",
            EnergyCarrier::Mechanical,
            Energy::from_nanojoules(1_000_000_000_000),
            Power::from_microwatts(10_000_000),
            Power::from_microwatts(50_000_000),
        ));
    let mut state = AppState::new();
    let source_energy = registries
        .energy()
        .get_store(definition)
        .unwrap()
        .capacity();
    let source = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        definition,
        source_energy,
    )
    .unwrap();
    let destination = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        definition,
        Energy::ZERO,
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
    .expect("shaft can supply the receiver's complete 10 W input rating");

    // One 3.6-second tick delivers 36 J after a single 80%-efficient roller.
    // The source must actually supply 45 J, not the former incorrect 36 J cap.
    assert_eq!(transfer.drawn(), Energy::from_nanojoules(45_000_000_000));
    assert_eq!(
        transfer.delivered(),
        Energy::from_nanojoules(36_000_000_000)
    );
    assert_eq!(transfer.lost(), Energy::from_nanojoules(9_000_000_000));
    let two_roller_transfer = plan_pair(
        &registries,
        &state,
        &passive,
        StructuralElementId::new(2),
        2,
        source,
        destination,
    )
    .expect("the source also supports the receiver's full input through two rollers");
    assert_eq!(
        two_roller_transfer.drawn(),
        Energy::from_nanojoules(56_250_000_000)
    );
    assert_eq!(
        two_roller_transfer.delivered(),
        Energy::from_nanojoules(36_000_000_000)
    );
    assert_eq!(
        two_roller_transfer.lost(),
        Energy::from_nanojoules(20_250_000_000)
    );
    apply_mechanical_transmission(
        &mut state,
        MechanicalTransmissionPlan {
            transfers: vec![transfer],
        },
    );
    assert_eq!(
        state.energy().get_store(source).unwrap().stored(),
        source_energy.checked_sub(transfer.drawn()).unwrap()
    );
    assert_eq!(
        state.energy().get_store(destination).unwrap().stored(),
        transfer.delivered()
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn co_located_shaft_receivers_prioritize_useful_work_over_store_identity() {
    use crate::content::{
        ENERGY_MECHANICAL_SMALL_DRIVE, FORM_HANDLE, MATERIAL_WOOD,
        STRUCTURAL_PROFILE_TIMBER_DRIVE_COUPLING, build_registries,
    };
    use crate::core::quantity::{Area, Length};
    use crate::core::state::validate_loaded_state;
    use crate::energy::add_energy_store_with_initial_for_fixture;
    use crate::logistics::place_energy_store_for_fixture;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    use crate::simulation::advance_tick;
    use crate::spatial::VoxelBounds;
    use crate::structural::{
        add_structural_element, make_test_structural_geometry,
        materialize_structural_element_for_test, validate_activate_structural_element,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let definition = ENERGY_MECHANICAL_SMALL_DRIVE;
    let capacity = registries
        .energy()
        .get_store(definition)
        .unwrap()
        .capacity();
    let source =
        add_energy_store_with_initial_for_fixture(&registries, &mut state, definition, capacity)
            .unwrap();
    let nearly_full = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        definition,
        capacity
            .checked_sub(Energy::from_nanojoules(10_000_000_000))
            .unwrap(),
    )
    .unwrap();
    let empty = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        definition,
        Energy::ZERO,
    )
    .unwrap();
    place_energy_store_for_fixture(&mut state, source, VoxelCoord::new(0, 0, 0));
    place_energy_store_for_fixture(&mut state, nearly_full, VoxelCoord::new(2, 0, 0));
    place_energy_store_for_fixture(&mut state, empty, VoxelCoord::new(2, 0, 0));
    let roller = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_TIMBER_DRIVE_COUPLING,
        MATERIAL_WOOD,
        make_test_structural_geometry(
            VoxelBounds::new(VoxelCoord::new(1, 0, 0), VoxelCoord::new(2, 1, 1)).unwrap(),
            Length::from_micrometers(1_000_000),
            Area::from_square_millimeters(300),
        ),
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, roller, FORM_HANDLE);
    let _ = validate_activate_structural_element(&registries, &state, roller)
        .unwrap()
        .commit(&mut state)
        .unwrap();

    let passive =
        crate::energy::passive_dissipation::decide_passive_energy_dissipation(&registries, &state);
    let restricted = plan_pair(
        &registries,
        &state,
        &passive,
        roller,
        1,
        source,
        nearly_full,
    )
    .expect("near-full receiver still permits a small transmission");
    let productive = plan_pair(&registries, &state, &passive, roller, 1, source, empty)
        .expect("empty receiver admits more transferred work");
    assert!(productive.delivered() > restricted.delivered());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let before_source = state.energy().get_store(source).unwrap().stored();
    let before_restricted = state.energy().get_store(nearly_full).unwrap().stored();
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let receipt = advance_tick(&registries, &mut state).unwrap();
    assert_eq!(receipt, advance_tick(&registries, &mut replay).unwrap());
    assert_eq!(state, replay);
    assert_eq!(receipt.mechanical_transmissions().len(), 1);
    let transfer = receipt.mechanical_transmissions()[0];
    assert_eq!(transfer, productive);
    assert_eq!(transfer.source(), source);
    assert_eq!(transfer.destination(), empty);
    assert_eq!(
        state.energy().get_store(nearly_full).unwrap().stored(),
        before_restricted
    );
    assert_eq!(
        before_source.checked_sub(state.energy().get_store(source).unwrap().stored()),
        Some(transfer.drawn()),
    );
    assert_eq!(
        state.energy().get_store(empty).unwrap().stored(),
        transfer.delivered()
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
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
