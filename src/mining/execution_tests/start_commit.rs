//! Late mining-start commit races and atomic stale-owner rejection.

use super::*;

fn remote_geology_spec() -> GeneratedDepositSpec {
    GeneratedDepositSpec::new(
        VoxelBounds::new(VoxelCoord::new(80, -8, 80), VoxelCoord::new(81, -7, 81)).unwrap(),
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        Mass::from_milligrams(100_000),
        Temperature::from_millikelvin(300_000),
        Pressure::from_pascals(350_000_000),
        MaterialComposition::pure(MATERIAL_COPPER),
    )
    .unwrap()
}

#[test]
fn generation_cannot_consume_reserved_mining_extraction_revision() {
    use crate::geology::{
        GenerateGeologicalDepositCommitError, InsertGeneratedDepositError,
        validate_generate_geological_deposit, validate_generate_geological_deposits,
    };

    let (registries, state, deposit, destination, pick) = ordinary_unstarted_mining_fixture();
    let mut state = reload_with_owner_revision(&registries, &state, "geology", u64::MAX - 1);
    let single =
        validate_generate_geological_deposit(&registries, &state, remote_geology_spec()).unwrap();
    let batch = validate_generate_geological_deposits(&registries, &state, [remote_geology_spec()])
        .unwrap();
    let job = validated_start(&registries, &state, deposit, destination, pick)
        .commit(&mut state)
        .unwrap();
    let before = state.clone();
    assert_eq!(
        single.precheck(&state),
        Err(GenerateGeologicalDepositCommitError::RevisionCapacityExhausted)
    );
    assert_eq!(
        batch.precheck(&state),
        Err(GenerateGeologicalDepositCommitError::RevisionCapacityExhausted)
    );
    assert_eq!(
        validate_generate_geological_deposit(&registries, &state, remote_geology_spec()).err(),
        Some(InsertGeneratedDepositError::RevisionExhausted)
    );
    assert_eq!(
        validate_generate_geological_deposits(&registries, &state, [remote_geology_spec()]).err(),
        Some(InsertGeneratedDepositError::RevisionExhausted)
    );
    assert_eq!(state, before);
    let _ = complete_mining_job(
        &registries,
        &mut state,
        job,
        "reserved final geology revision",
    );
    assert_eq!(state.geology().revision(), u64::MAX);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn retained_mining_start_rechecks_geology_capacity_after_remote_generation() {
    use crate::geology::validate_generate_geological_deposit;

    let (registries, state, deposit, destination, pick) = ordinary_unstarted_mining_fixture();
    let mut state = reload_with_owner_revision(&registries, &state, "geology", u64::MAX - 1);
    let token = validated_start(&registries, &state, deposit, destination, pick);
    let generation =
        validate_generate_geological_deposit(&registries, &state, remote_geology_spec()).unwrap();
    generation.precheck(&state).unwrap();
    let _ = generation.apply(&mut state);
    let before = state.clone();
    assert_eq!(
        token.commit(&mut state),
        Err(MiningStartCommitError::GeologyRevisionExhausted)
    );
    assert_eq!(state, before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

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
        let (registries, state, deposit, destination, pick) = ordinary_unstarted_mining_fixture();
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
    let (registries, mut state, deposit, destination, pick) = ordinary_unstarted_mining_fixture();
    let position = state
        .logistics()
        .player()
        .map(|player| player.position())
        .unwrap_or_else(|| panic!("mining stale-structure player disappeared"));
    let support = active_stockpile_support_at(&registries, &mut state, position);
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
        Err(MiningStartCommitError::Work(
            crate::labor::PlayerWorkCommitError::StaleStructureRevision {
                expected,
                actual: expected + 1,
            }
        ))
    );
    assert_eq!(changed, before, "stale structural commit must be atomic");
}
