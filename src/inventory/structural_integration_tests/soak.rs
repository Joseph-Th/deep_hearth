//! Long-horizon supported-transfer invariant and deterministic-replay coverage.

use super::*;

#[cfg(feature = "test-soak")]
fn run_supported_transfer_soak() -> AppState {
    let registries = build_registries();
    let mut state = AppState::new();
    let left_support = active_support(&registries, &mut state, 0);
    let right_support = active_support(&registries, &mut state, 2);
    let left = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(10),
        Mass::from_milligrams(1),
    );
    let right = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(10),
        Mass::ZERO,
    );
    let _ = mount(&registries, &mut state, left, left_support);
    let _ = mount(&registries, &mut state, right, right_support);

    for step in 0..1_000_u64 {
        let (source, destination) = if step.is_multiple_of(2) {
            (left, right)
        } else {
            (right, left)
        };
        let transfer = match validate_material_relocation_for_test(
            &registries,
            &state,
            source,
            destination,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(1),
        ) {
            Ok(transfer) => transfer,
            Err(error) => {
                panic!("supported transfer soak validation failed at {step}: {error}")
            }
        };
        if let Err(error) = transfer.commit(&mut state) {
            panic!("supported transfer soak commit failed at {step}: {error}");
        }
        if step.is_multiple_of(113) {
            assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
        }
    }
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    state
}

#[cfg(feature = "test-soak")]
#[test]
#[ignore = "long-horizon soak"]
fn supported_transfer_soak_preserves_invariants_and_deterministic_replay() {
    let first = run_supported_transfer_soak();
    let second = run_supported_transfer_soak();
    assert_eq!(first, second);
}
