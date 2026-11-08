//! Contract tests for exact inventory selection.

use super::*;
use crate::content::{FORM_LOG, FORM_LUMP, MATERIAL_STONE, MATERIAL_WOOD, build_registries};
use crate::core::quantity::Temperature;
use crate::core::state::AppState;
use crate::inventory::state::get_stockpile_mut_or_panic;
use crate::inventory::{
    add_solid_stockpile_for_test, deposit_composed_lot_for_test, deposit_lot_for_test,
};
use crate::material::{
    CommodityKey, CompositionComponent, CompositionConstraint, MaterialComposition,
};

#[path = "selection_tests/exhaustive.rs"]
mod exhaustive;

#[test]
fn implicit_selection_reassigns_earlier_lots_to_satisfy_overlapping_composition_requirements() {
    use crate::core::state::validate_loaded_state;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};

    let registries = build_registries();
    let mut state = AppState::new();
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000)).unwrap();
    let commodity = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let pure = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        commodity,
        Mass::from_milligrams(100),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let mix = |wood, stone| {
        MaterialComposition::new(vec![
            CompositionComponent::new(MATERIAL_WOOD, wood),
            CompositionComponent::new(MATERIAL_STONE, stone),
        ])
        .unwrap()
    };
    let rich = deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        commodity,
        Mass::from_milligrams(100),
        Temperature::from_millikelvin(293_150),
        mix(900_000, 100_000),
    )
    .unwrap();
    let mixed = deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        commodity,
        Mass::from_milligrams(100),
        Temperature::from_millikelvin(293_150),
        mix(800_000, 200_000),
    )
    .unwrap();
    let inputs = [
        MaterialInputSpec::with_constraints(
            commodity,
            Mass::from_milligrams(100),
            vec![CompositionConstraint::new(MATERIAL_WOOD, 900_000, 1_000_000).unwrap()],
        )
        .unwrap(),
        MaterialInputSpec::with_constraints(
            commodity,
            Mass::from_milligrams(100),
            vec![CompositionConstraint::new(MATERIAL_STONE, 100_000, 500_000).unwrap()],
        )
        .unwrap(),
        MaterialInputSpec::pure(commodity, Mass::from_milligrams(100)),
    ];
    let before = state.clone();
    let selection = validate_consumption_selection(state.inventory(), source, &inputs)
        .expect("three physical lots admit a feasible allocation through reassignment");
    assert_eq!(
        selection.lot_slices,
        vec![
            LotSlice {
                lot: rich,
                mass: Mass::from_milligrams(100)
            },
            LotSlice {
                lot: mixed,
                mass: Mass::from_milligrams(100)
            },
            LotSlice {
                lot: pure,
                mass: Mass::from_milligrams(100)
            },
        ]
    );
    assert_eq!(selection.total_consumed(), Mass::from_milligrams(300));
    assert_eq!(
        validate_temperature_limited_consumption_selection(
            state.inventory(),
            source,
            &inputs,
            Temperature::from_millikelvin(293_150),
        ),
        Ok(selection.clone()),
        "temperature-limited callers share complete physical reassignment"
    );

    let partial_inputs = [
        MaterialInputSpec::with_constraints(
            commodity,
            Mass::from_milligrams(80),
            inputs[0].constraints().to_vec(),
        )
        .unwrap(),
        MaterialInputSpec::with_constraints(
            commodity,
            Mass::from_milligrams(120),
            inputs[1].constraints().to_vec(),
        )
        .unwrap(),
        inputs[2].clone(),
    ];
    let partial = validate_consumption_selection(state.inventory(), source, &partial_inputs)
        .expect("a partial residual exchange can distribute one lot among two inputs");
    assert_eq!(
        assert_consumption_parts_match_state(
            state.inventory(),
            source,
            &partial.inputs,
            &partial.lot_slices,
            &partial.consumed_inputs,
        ),
        (),
        "reassigned slices must still pass the canonical inventory integrity replay"
    );
    assert_eq!(
        partial.lot_slices,
        vec![
            LotSlice {
                lot: rich,
                mass: Mass::from_milligrams(80)
            },
            LotSlice {
                lot: rich,
                mass: Mass::from_milligrams(20)
            },
            LotSlice {
                lot: mixed,
                mass: Mass::from_milligrams(100)
            },
            LotSlice {
                lot: pure,
                mass: Mass::from_milligrams(100)
            }
        ]
    );
    assert_eq!(partial.total_consumed(), Mass::from_milligrams(300));
    // A single physical lot can appear in two requirement slices. The actual
    // inventory owner must consume those slices exactly once in aggregate,
    // not reject the duplicate identity or overdraw its original mass.
    let egress =
        crate::inventory::validate_material_egress_from_selection(state.inventory(), partial)
            .unwrap();
    let mut withdrawn = state.inventory().clone();
    crate::inventory::apply_material_egress(&mut withdrawn, egress);
    assert_eq!(
        withdrawn.get_stockpile(source).unwrap().stored_mass(),
        Mass::ZERO
    );
    assert_eq!(withdrawn.lot_ids(source).count(), 0);
    let impossible = [
        inputs[0].clone(),
        inputs[1].clone(),
        MaterialInputSpec::pure(commodity, Mass::from_milligrams(101)),
    ];
    assert_eq!(
        validate_consumption_selection(state.inventory(), source, &impossible),
        Err(ConsumptionSelectionError::InsufficientMass {
            stockpile: source,
            commodity,
            available: Mass::ZERO,
            requested: Mass::from_milligrams(101)
        }),
        "infeasible restricted requirements must still reject rather than double-count a lot"
    );
    assert_eq!(state, before, "selection is a read-only transaction");
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, state);
    assert_eq!(
        validate_consumption_selection(restored.inventory(), source, &inputs),
        Ok(selection),
        "canonical lot identity and the reassignment plan survive trusted persistence"
    );
}

#[test]
fn temperature_limited_reassignment_keeps_hot_lots_out_of_a_feasible_recipe() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000)).unwrap();
    let commodity = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let cool = Temperature::from_millikelvin(293_150);
    let hot = Temperature::from_millikelvin(370_000);
    let pure = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        commodity,
        Mass::from_milligrams(100),
        cool,
    )
    .unwrap();
    let composed = |wood, stone| {
        MaterialComposition::new(vec![
            CompositionComponent::new(MATERIAL_WOOD, wood),
            CompositionComponent::new(MATERIAL_STONE, stone),
        ])
        .unwrap()
    };
    let hot_rich = deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        commodity,
        Mass::from_milligrams(100),
        hot,
        composed(900_000, 100_000),
    )
    .unwrap();
    let mixed = deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        commodity,
        Mass::from_milligrams(100),
        cool,
        composed(800_000, 200_000),
    )
    .unwrap();
    let cool_rich = deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        commodity,
        Mass::from_milligrams(100),
        cool,
        composed(900_000, 100_000),
    )
    .unwrap();
    let inputs = [
        MaterialInputSpec::with_constraints(
            commodity,
            Mass::from_milligrams(100),
            vec![CompositionConstraint::new(MATERIAL_WOOD, 900_000, 1_000_000).unwrap()],
        )
        .unwrap(),
        MaterialInputSpec::with_constraints(
            commodity,
            Mass::from_milligrams(100),
            vec![CompositionConstraint::new(MATERIAL_STONE, 100_000, 500_000).unwrap()],
        )
        .unwrap(),
        MaterialInputSpec::pure(commodity, Mass::from_milligrams(100)),
    ];
    let before = state.clone();
    let unconstrained = validate_consumption_selection(state.inventory(), source, &inputs)
        .expect("hot mixed material is physically available before hand safety is considered");
    assert!(
        unconstrained
            .lot_slices
            .iter()
            .any(|slice| slice.lot == hot_rich),
        "without a temperature limit, the identity-ordered material is usable"
    );
    let safe = validate_temperature_limited_consumption_selection(
        state.inventory(),
        source,
        &inputs,
        crate::material::MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE,
    )
    .expect("cool alternative can satisfy every overlapping constraint");
    assert_eq!(
        safe.lot_slices,
        vec![
            LotSlice {
                lot: cool_rich,
                mass: Mass::from_milligrams(100)
            },
            LotSlice {
                lot: mixed,
                mass: Mass::from_milligrams(100)
            },
            LotSlice {
                lot: pure,
                mass: Mass::from_milligrams(100)
            }
        ]
    );
    assert!(
        safe.consumed_inputs()
            .iter()
            .all(|trace| trace.profile().temperature()
                <= crate::material::MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE)
    );
    assert_eq!(state, before);
}

#[test]
fn implicit_selection_uses_ordered_eligible_lots_and_stops_at_requested_mass() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("implicit selection source fixture failed: {error}"));
    let commodity = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let first = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        commodity,
        Mass::from_milligrams(4),
        Temperature::from_millikelvin(290_000),
    )
    .unwrap_or_else(|error| panic!("first implicit selection lot failed: {error}"));
    let second = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        commodity,
        Mass::from_milligrams(7),
        Temperature::from_millikelvin(300_000),
    )
    .unwrap_or_else(|error| panic!("second implicit selection lot failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        commodity,
        Mass::from_milligrams(50),
        Temperature::from_millikelvin(310_000),
    )
    .unwrap_or_else(|error| panic!("unused implicit selection lot failed: {error}"));

    let selection = validate_consumption_selection(
        state.inventory(),
        source,
        &[MaterialInputSpec::new(commodity, Mass::from_milligrams(6))],
    )
    .unwrap_or_else(|error| panic!("implicit selection validation failed: {error:?}"));

    assert_eq!(selection.total_consumed(), Mass::from_milligrams(6));
    assert_eq!(selection.lot_slices.len(), 2);
    assert_eq!(
        selection.lot_slices[0],
        LotSlice {
            lot: first,
            mass: Mass::from_milligrams(4)
        }
    );
    assert_eq!(
        selection.lot_slices[1],
        LotSlice {
            lot: second,
            mass: Mass::from_milligrams(2)
        }
    );
}

#[test]
fn implicit_selection_shortage_reports_only_composition_eligible_free_mass() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("implicit shortage source fixture failed: {error}"));
    let commodity = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        commodity,
        Mass::from_milligrams(3),
        Temperature::from_millikelvin(300_000),
    )
    .unwrap_or_else(|error| panic!("pure implicit shortage lot failed: {error}"));
    let mixed = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_WOOD, 900_000),
        CompositionComponent::new(MATERIAL_STONE, 100_000),
    ])
    .unwrap_or_else(|error| panic!("implicit shortage composition failed: {error}"));
    deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        commodity,
        Mass::from_milligrams(40),
        Temperature::from_millikelvin(300_000),
        mixed,
    )
    .unwrap_or_else(|error| panic!("mixed implicit shortage lot failed: {error}"));

    assert_eq!(
        validate_consumption_selection(
            state.inventory(),
            source,
            &[MaterialInputSpec::pure(commodity, Mass::from_milligrams(4))],
        ),
        Err(ConsumptionSelectionError::InsufficientMass {
            stockpile: source,
            commodity,
            available: Mass::from_milligrams(3),
            requested: Mass::from_milligrams(4),
        })
    );
}

#[test]
fn temperature_limited_selection_identifies_actual_cooling_blocker_in_multi_input_recipe() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)).unwrap();
    let wood = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let stone = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let warm_wood = Temperature::from_millikelvin(370_000);
    let hot_stone = Temperature::from_millikelvin(400_000);
    for (commodity, temperature) in [
        (wood, warm_wood),
        (wood, Temperature::from_millikelvin(293_150)),
        (stone, hot_stone),
    ] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            commodity,
            Mass::from_milligrams(10),
            temperature,
        )
        .unwrap();
    }
    let before = state.clone();
    let inputs = [
        MaterialInputSpec::pure(wood, Mass::from_milligrams(10)),
        MaterialInputSpec::pure(stone, Mass::from_milligrams(10)),
    ];
    assert_eq!(
        validate_temperature_limited_consumption_selection(
            state.inventory(),
            source,
            &inputs,
            crate::material::MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE,
        ),
        Err(TemperatureLimitedSelectionError::TooHot {
            commodity: stone,
            temperature: hot_stone,
            maximum: crate::material::MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn explicit_selection_binds_partial_lot_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("explicit selection source fixture failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(20),
        Temperature::from_millikelvin(300_000),
    )
    .unwrap_or_else(|error| panic!("explicit selection lot fixture failed: {error}"));
    let before = state.clone();

    let selection = validate_explicit_consumption_selection(
        state.inventory(),
        source,
        &[MaterialLotSelection::new(lot, Mass::from_milligrams(7))],
    )
    .unwrap_or_else(|error| panic!("explicit selection validation failed: {error:?}"));

    assert_eq!(selection.total_consumed(), Mass::from_milligrams(7));
    assert_eq!(selection.consumed_inputs().len(), 1);
    assert_eq!(
        selection.consumed_inputs()[0].mass(),
        Mass::from_milligrams(7)
    );
    assert_eq!(
        selection.consumed_inputs()[0].profile().temperature(),
        Temperature::from_millikelvin(300_000)
    );
    assert_eq!(state, before);
}

#[test]
fn consumption_plan_integrity_rejects_divergent_parallel_representations() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("integrity source fixture failed: {error}"));
    let commodity = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        commodity,
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
    )
    .unwrap_or_else(|error| panic!("integrity source lot failed: {error}"));
    let selection = validate_consumption_selection(
        state.inventory(),
        source,
        &[MaterialInputSpec::new(commodity, Mass::from_milligrams(6))],
    )
    .unwrap_or_else(|error| panic!("integrity selection failed: {error:?}"));

    assert!(
        std::panic::catch_unwind(|| {
            assert_consumption_parts_well_formed(
                &selection.inputs,
                &[],
                &selection.consumed_inputs,
            );
        })
        .is_err()
    );

    let wrong_inputs = [MaterialInputSpec::new(
        CommodityKey::new(MATERIAL_STONE, FORM_LOG),
        Mass::from_milligrams(6),
    )];
    assert!(
        std::panic::catch_unwind(|| {
            assert_consumption_parts_well_formed(
                &wrong_inputs,
                &selection.lot_slices,
                &selection.consumed_inputs,
            );
        })
        .is_err()
    );
    assert_eq!(
        assert_consumption_parts_well_formed(
            &selection.inputs,
            &selection.lot_slices,
            &selection.consumed_inputs,
        ),
        ()
    );

    let mut wrong_trace = selection.clone();
    wrong_trace.consumed_inputs[0].profile.temperature = Temperature::from_millikelvin(301_000);
    assert!(
        std::panic::catch_unwind(|| {
            assert_consumption_parts_match_state(
                state.inventory(),
                source,
                &wrong_trace.inputs,
                &wrong_trace.lot_slices,
                &wrong_trace.consumed_inputs,
            );
        })
        .is_err()
    );
    assert_eq!(
        assert_consumption_parts_match_state(
            state.inventory(),
            source,
            &selection.inputs,
            &selection.lot_slices,
            &selection.consumed_inputs,
        ),
        ()
    );
}

#[test]
fn consumption_plan_integrity_rejects_corrupt_source_aggregates_and_lot_index() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("aggregate integrity source fixture failed: {error}"));
    let commodity = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        commodity,
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
    )
    .unwrap_or_else(|error| panic!("aggregate integrity source lot failed: {error}"));
    let selection = validate_consumption_selection(
        state.inventory(),
        source,
        &[MaterialInputSpec::new(commodity, Mass::from_milligrams(6))],
    )
    .unwrap_or_else(|error| panic!("aggregate integrity selection failed: {error:?}"));

    let mut corrupt_aggregate = state.clone();
    let aggregate_record =
        get_stockpile_mut_or_panic(corrupt_aggregate.inventory_state_mut(), source);
    aggregate_record
        .contents
        .insert(commodity, Mass::from_milligrams(5));
    assert!(
        std::panic::catch_unwind(|| {
            assert_consumption_parts_match_state(
                corrupt_aggregate.inventory(),
                source,
                &selection.inputs,
                &selection.lot_slices,
                &selection.consumed_inputs,
            );
        })
        .is_err()
    );

    let mut corrupt_index = state.clone();
    corrupt_index
        .inventory_state_mut()
        .remove_lot_index(source, commodity, lot);
    assert!(
        std::panic::catch_unwind(|| {
            assert_consumption_parts_match_state(
                corrupt_index.inventory(),
                source,
                &selection.inputs,
                &selection.lot_slices,
                &selection.consumed_inputs,
            );
        })
        .is_err()
    );
}

#[test]
fn explicit_selection_rejects_duplicate_lot_and_wrong_source() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("explicit selection source fixture failed: {error}"));
    let other = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("explicit selection secondary fixture failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(20),
        Temperature::from_millikelvin(300_000),
    )
    .unwrap_or_else(|error| panic!("explicit selection lot fixture failed: {error}"));
    let before = state.clone();
    let slice = MaterialLotSelection::new(lot, Mass::from_milligrams(5));

    assert_eq!(
        validate_explicit_consumption_selection(state.inventory(), source, &[slice, slice],),
        Err(ExplicitConsumptionSelectionError::DuplicateLot { lot })
    );
    assert_eq!(
        validate_explicit_consumption_selection(state.inventory(), other, &[slice]),
        Err(ExplicitConsumptionSelectionError::LotOwnedElsewhere {
            lot,
            requested_source: other,
            actual_source: source,
        })
    );
    assert_eq!(state, before);
}
