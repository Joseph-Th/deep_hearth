//! Long-horizon inventory transaction replay across custody and form changes.

use super::*;
use crate::content::{FORM_CHIP, FORM_LOG, MATERIAL_WOOD, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::core::time::WorldSeed;
use crate::inventory::{
    MaterialIngressEntry, add_solid_stockpile_for_test, apply_material_ingress,
    deposit_lot_for_test, validate_consumption_selection, validate_material_ingress,
    validate_material_transfer_for_test,
};
use crate::material::{CommodityKey, MaterialInputSpec};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};

const OPERATIONS: u64 = 5_000;

fn commodity(form: crate::material::FormId) -> CommodityKey {
    CommodityKey::new(MATERIAL_WOOD, form)
}

fn next_random(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state
}

fn run_transaction_soak(seed: WorldSeed) -> AppState {
    let registries = build_registries();
    let mut state = AppState::new(seed);
    let stockpiles = [
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10_000)),
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10_000)),
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10_000)),
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10_000)),
    ]
    .map(|result| {
        result.unwrap_or_else(|error| panic!("transaction soak stockpile failed: {error}"))
    });
    for (stockpile, mass) in stockpiles.into_iter().zip([1_000, 750, 500, 250]) {
        deposit_lot_for_test(
            &registries,
            &mut state,
            stockpile,
            commodity(FORM_LOG),
            Mass::from_milligrams(mass),
            Temperature::from_millikelvin(300_000),
        )
        .unwrap_or_else(|error| panic!("transaction soak seed material failed: {error}"));
    }
    let initial_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("transaction soak initial accounting failed: {error:?}"))
        .total();
    let mut random = seed.value() ^ 0xD00D_2026_1A70_5000;
    let mut committed = [0_u64; 5];

    for step in 1..=OPERATIONS {
        let value = next_random(&mut random);
        let source = stockpiles[((value >> 8) % 4) as usize];
        let destination = stockpiles[((value >> 16) % 4) as usize];
        let requested = Mass::from_milligrams(1 + ((value >> 24) % 25));
        let operation = (value >> 32) % 5;

        match operation {
            0 if source != destination => {
                if let Ok(transfer) = validate_material_transfer_for_test(
                    &registries,
                    &state,
                    source,
                    destination,
                    commodity(FORM_LOG),
                    requested,
                ) {
                    transfer.commit(&mut state).unwrap_or_else(|error| {
                        panic!("transaction soak transfer failed at step {step}: {error}")
                    });
                    committed[0] += 1;
                }
            }
            1 if source != destination => {
                let inputs = [MaterialInputSpec::new(commodity(FORM_LOG), requested)];
                if let Ok(selection) =
                    validate_consumption_selection(state.inventory(), source, &inputs)
                    && let Ok(relocation) = validate_material_relocation_from_selection(
                        &registries,
                        &state,
                        destination,
                        selection,
                    )
                {
                    relocation.commit(&mut state).unwrap_or_else(|error| {
                        panic!("transaction soak relocation failed at step {step}: {error:?}")
                    });
                    committed[1] += 1;
                }
            }
            2 if source != destination => {
                let inputs = [MaterialInputSpec::new(commodity(FORM_LOG), requested)];
                if let Ok(selection) =
                    validate_consumption_selection(state.inventory(), source, &inputs)
                {
                    let egress =
                        validate_material_egress_from_selection(state.inventory(), selection)
                            .unwrap_or_else(|error| {
                                panic!("transaction soak egress failed at step {step}: {error:?}")
                            });
                    let traces = egress.consumed_inputs().to_vec();
                    apply_material_egress(state.inventory_state_mut(), egress);
                    let ingress = validate_material_ingress(
                        &registries,
                        state.inventory(),
                        destination,
                        traces.iter().map(MaterialIngressEntry::from_consumed_trace),
                        state.tick(),
                    )
                    .unwrap_or_else(|error| {
                        panic!("transaction soak ingress failed at step {step}: {error:?}")
                    });
                    apply_material_ingress(state.inventory_state_mut(), ingress);
                    committed[2] += 1;
                }
            }
            3 => {
                let inputs = [MaterialInputSpec::new(commodity(FORM_LOG), requested)];
                if let Ok(selection) =
                    validate_consumption_selection(state.inventory(), source, &inputs)
                    && let Ok(reform) = validate_material_reform_from_selection(
                        &registries,
                        &state,
                        destination,
                        commodity(FORM_CHIP),
                        selection,
                    )
                {
                    reform.commit(&mut state).unwrap_or_else(|error| {
                        panic!("transaction soak log reform failed at step {step}: {error:?}")
                    });
                    committed[3] += 1;
                }
            }
            4 => {
                let inputs = [MaterialInputSpec::new(commodity(FORM_CHIP), requested)];
                if let Ok(selection) =
                    validate_consumption_selection(state.inventory(), source, &inputs)
                    && let Ok(reform) = validate_material_reform_from_selection(
                        &registries,
                        &state,
                        destination,
                        commodity(FORM_LOG),
                        selection,
                    )
                {
                    reform.commit(&mut state).unwrap_or_else(|error| {
                        panic!("transaction soak chip reform failed at step {step}: {error:?}")
                    });
                    committed[4] += 1;
                }
            }
            _ => {}
        }

        if step.is_multiple_of(101) {
            validate_loaded_state(&registries, &state).unwrap_or_else(|error| {
                panic!("transaction soak invariant audit failed at step {step}: {error}")
            });
            assert_eq!(
                calculate_matter_accounting(&state)
                    .unwrap_or_else(|error| {
                        panic!("transaction soak accounting failed at step {step}: {error:?}")
                    })
                    .total(),
                initial_matter,
                "transaction soak changed matter at step {step}"
            );
        }
        if step.is_multiple_of(997) {
            let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
                .unwrap_or_else(|error| {
                    panic!("transaction soak save failed at step {step}: {error}")
                });
            let decoded: LoadedSaveEnvelope =
                serde_json::from_slice(&encoded).unwrap_or_else(|error| {
                    panic!("transaction soak decode failed at step {step}: {error}")
                });
            state = decoded.into_state(&registries).unwrap_or_else(|error| {
                panic!("transaction soak trusted load failed at step {step}: {error}")
            });
        }
    }

    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("transaction soak final invariant audit failed: {error}"));
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("transaction soak final accounting failed: {error:?}"))
            .total(),
        initial_matter
    );
    assert!(
        state.inventory().lots().count() <= stockpiles.len() * 2,
        "transaction soak produced unbounded lot fragmentation"
    );
    assert!(
        committed.into_iter().all(|count| count != 0),
        "transaction soak must successfully exercise every transaction family: {committed:?}"
    );
    state
}

#[test]
#[ignore = "long-horizon soak"]
fn randomized_inventory_transaction_soak_preserves_conservation_persistence_and_replay() {
    let seed = WorldSeed::new(0x1A70_5000_D00D_2026);
    let first = run_transaction_soak(seed);
    let second = run_transaction_soak(seed);
    assert_eq!(first, second);
}
