//! Exhaustive small-case oracle for overlapping material-custody requirements.
//!
//! The oracle assigns one milligram at a time, deliberately independently of the
//! production allocator's capacitated residual-flow implementation.

use super::*;
use crate::inventory::MaterialLotId;

fn can_allocate_units(
    inputs: &[MaterialInputSpec; 3],
    compositions: &[MaterialComposition; 4],
    eligible: &[bool; 4],
    remaining: &mut [u8; 4],
    request: usize,
    units_left: u64,
) -> bool {
    if request == inputs.len() {
        return true;
    }
    if units_left == 0 {
        let next = request + 1;
        return next == inputs.len()
            || can_allocate_units(
                inputs,
                compositions,
                eligible,
                remaining,
                next,
                inputs[next].mass().milligrams(),
            );
    }
    for index in 0..remaining.len() {
        if remaining[index] == 0
            || !eligible[index]
            || !inputs[request].is_satisfied_by(&compositions[index])
        {
            continue;
        }
        remaining[index] -= 1;
        if can_allocate_units(
            inputs,
            compositions,
            eligible,
            remaining,
            request,
            units_left - 1,
        ) {
            return true;
        }
        remaining[index] += 1;
    }
    false
}

fn oracle_can_allocate(
    inputs: &[MaterialInputSpec; 3],
    compositions: &[MaterialComposition; 4],
    eligible: &[bool; 4],
) -> bool {
    can_allocate_units(
        inputs,
        compositions,
        eligible,
        &mut [2; 4],
        0,
        inputs[0].mass().milligrams(),
    )
}

fn assert_authorized_exact_lot_custody(
    state: &AppState,
    inputs: &[MaterialInputSpec; 3],
    slices: &[LotSlice],
    permitted: &[bool; 4],
    lots: &[MaterialLotId; 4],
) {
    let mut request = 0;
    let mut remaining = inputs[request].mass().milligrams();
    let mut allocated = [0_u64; 4];
    for slice in slices {
        let lot_index = lots
            .iter()
            .position(|lot| *lot == slice.lot)
            .expect("allocation may only use these four physical lots");
        assert!(
            permitted[lot_index],
            "excluded lots must not enter an allocation"
        );
        let lot = state.inventory().get_lot(slice.lot).unwrap();
        assert!(
            inputs[request].is_satisfied_by(lot.composition()),
            "each assigned material must satisfy its own requirement"
        );
        assert!(slice.mass.milligrams() <= remaining);
        remaining -= slice.mass.milligrams();
        allocated[lot_index] += slice.mass.milligrams();
        if remaining == 0 {
            request += 1;
            if request < inputs.len() {
                remaining = inputs[request].mass().milligrams();
            }
        }
    }
    assert_eq!(
        request,
        inputs.len(),
        "every requirement must receive its exact mass"
    );
    assert!(
        allocated.iter().all(|mass| *mass <= 2),
        "a lot cannot be consumed beyond its exact physical mass"
    );
}

#[test]
fn overlapping_composition_allocation_matches_independent_exact_unit_oracle() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)).unwrap();
    let commodity = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let compositions = [1_000_000, 900_000, 800_000, 700_000].map(|wood| {
        if wood == 1_000_000 {
            MaterialComposition::pure(MATERIAL_WOOD)
        } else {
            MaterialComposition::new(vec![
                CompositionComponent::new(MATERIAL_WOOD, wood),
                CompositionComponent::new(MATERIAL_STONE, 1_000_000 - wood),
            ])
            .unwrap()
        }
    });
    let temperatures = [293_150, 294_150, 370_000, 293_150].map(Temperature::from_millikelvin);
    let mut lot_ids = Vec::new();
    for (composition, temperature) in compositions.iter().zip(temperatures) {
        lot_ids.push(
            deposit_composed_lot_for_test(
                &registries,
                &mut state,
                source,
                commodity,
                Mass::from_milligrams(2),
                temperature,
                composition.clone(),
            )
            .unwrap(),
        );
    }
    let lots: [MaterialLotId; 4] = lot_ids.try_into().unwrap();
    let two = Mass::from_milligrams(2);
    let templates = [
        MaterialInputSpec::new(commodity, two),
        MaterialInputSpec::pure(commodity, two),
        MaterialInputSpec::with_constraints(
            commodity,
            two,
            vec![CompositionConstraint::new(MATERIAL_WOOD, 900_000, 1_000_000).unwrap()],
        )
        .unwrap(),
        MaterialInputSpec::with_constraints(
            commodity,
            two,
            vec![CompositionConstraint::new(MATERIAL_STONE, 200_000, 500_000).unwrap()],
        )
        .unwrap(),
    ];
    let unchanged = state.clone();
    // All 4³ ordered requirements, evaluated both with and without the hot third lot.
    for code in 0..64_usize {
        let inputs =
            std::array::from_fn(|place| templates[(code / 4_usize.pow(place as u32)) % 4].clone());
        for limit_hot in [false, true] {
            let eligible = std::array::from_fn(|index| !limit_hot || index != 2);
            let feasible = oracle_can_allocate(&inputs, &compositions, &eligible);
            let selected = if limit_hot {
                validate_temperature_limited_consumption_selection(
                    state.inventory(),
                    source,
                    &inputs,
                    crate::material::MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE,
                )
                .map_err(|_| ())
            } else {
                validate_consumption_selection(state.inventory(), source, &inputs).map_err(|_| ())
            };
            assert_eq!(
                selected.is_ok(),
                feasible,
                "case {code}, hot-lot exclusion {limit_hot}: selection must match independent feasibility"
            );
            if let Ok(selection) = selected {
                assert_authorized_exact_lot_custody(
                    &state,
                    &inputs,
                    &selection.lot_slices,
                    &eligible,
                    &lots,
                );
            }
        }
    }
    assert_eq!(
        state, unchanged,
        "planning must not mutate material custody"
    );
}
