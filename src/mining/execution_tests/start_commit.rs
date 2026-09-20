//! Late mining-start commit races and atomic stale-owner rejection.

use super::*;

fn reload_with_owner_revision(
    registries: &Registries,
    state: &AppState,
    owner: &str,
    revision: u64,
) -> AppState {
    let mut encoded = serde_json::to_value(SaveEnvelope::new(registries, state))
        .unwrap_or_else(|error| panic!("mining stale-owner serialization failed: {error}"));
    encoded["state"]["systems"][owner]["revision"] = serde_json::json!(revision);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining stale-owner decode failed: {error}"));
    decoded.into_state(registries).unwrap_or_else(|error| {
        panic!("mining stale-owner fixture must remain a valid current-schema state: {error}")
    })
}

fn validated_start(
    registries: &Registries,
    state: &AppState,
    deposit: GeologicalDepositId,
    destination: StockpileId,
    pick: EquipmentId,
) -> ValidatedMiningStart {
    validate_known_mining(
        registries,
        state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining stale-owner validation failed: {error}"))
}

#[test]
fn mining_start_commit_rejects_stale_owner_revisions_atomically() {
    for owner in ["inventory", "equipment", "mining"] {
        let (registries, state, deposit, destination, pick) = unstarted_mining_fixture();
        let token = validated_start(&registries, &state, deposit, destination, pick);
        let (expected, error) = match owner {
            "inventory" => {
                let expected = state.inventory().revision();
                (
                    expected,
                    MiningStartCommitError::StaleInventory {
                        expected,
                        actual: expected + 1,
                    },
                )
            }
            "equipment" => {
                let expected = state.equipment().revision();
                (
                    expected,
                    MiningStartCommitError::StaleEquipment {
                        expected,
                        actual: expected + 1,
                    },
                )
            }
            "mining" => {
                let expected = state.mining().revision();
                (
                    expected,
                    MiningStartCommitError::StaleMining {
                        expected,
                        actual: expected + 1,
                    },
                )
            }
            _ => unreachable!("bounded stale-owner table contains only known owners"),
        };
        let mut changed = reload_with_owner_revision(&registries, &state, owner, expected + 1);
        let before = changed.clone();

        assert_eq!(token.commit(&mut changed), Err(error));
        assert_eq!(changed, before, "stale {owner} commit must be atomic");
    }
}

#[test]
fn mining_start_commit_rejects_stale_structural_support_revision_atomically() {
    let (registries, mut state, deposit, destination, pick) = unstarted_mining_fixture();
    let support = active_stockpile_support(&registries, &mut state);
    let _ = validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap_or_else(|error| panic!("mining stale-structure mount validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("mining stale-structure mount commit failed: {error}"));
    let token = validated_start(&registries, &state, deposit, destination, pick);
    let expected = state.structures().revision();
    let mut changed = reload_with_owner_revision(&registries, &state, "structures", expected + 1);
    let before = changed.clone();

    assert_eq!(
        token.commit(&mut changed),
        Err(MiningStartCommitError::StaleStructure {
            expected,
            actual: expected + 1,
        })
    );
    assert_eq!(changed, before, "stale structural commit must be atomic");
}
