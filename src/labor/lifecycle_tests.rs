//! Admission contracts for exclusive player-work revision and survival budgeting.

use super::*;
use crate::content::build_registries;
use crate::core::state::AppState;
use crate::core::time::WorldSeed;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::production::ProductionJobId;
use crate::survival::initialize_player_survival;

#[test]
fn player_work_admission_requires_revision_capacity_for_later_release() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1AB0_0001));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("player-work revision fixture survival failed: {error}"));

    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("player-work revision fixture serialization failed: {error}")
        });
    encoded["state"]["systems"]["player_work"]["revision"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("player-work revision fixture decode failed: {error}"));
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("idle near-exhausted player-work revision fixture should load: {error}")
    });
    let before = loaded.clone();

    assert_eq!(
        validate_player_work_start(
            &registries,
            &loaded,
            PlayerWork::ManualProduction {
                job: ProductionJobId::new(1),
            },
            TickSpan::new(1),
            SurvivalExertion::REST,
        )
        .err(),
        Some(PlayerWorkStartError::RevisionExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn player_work_admission_requires_survival_revision_capacity_for_full_interval() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1AB0_0002));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("survival-revision fixture setup failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("survival-revision fixture serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["revision"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("survival-revision fixture decode failed: {error}"));
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("idle near-exhausted survival revision fixture should load: {error}")
    });
    let before = loaded.clone();
    let duration = TickSpan::new(2);

    assert_eq!(
        validate_player_work_start(
            &registries,
            &loaded,
            PlayerWork::ManualProduction {
                job: ProductionJobId::new(1),
            },
            duration,
            SurvivalExertion::REST,
        )
        .err(),
        Some(PlayerWorkStartError::SurvivalRevisionExhausted { duration })
    );
    assert_eq!(loaded, before);
}
