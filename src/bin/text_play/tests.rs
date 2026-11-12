//! Scriptable play boundary regressions against actual production simulation.

use super::*;
use deep_hearth::core::time::CONTROL_TICKS_PER_SIMULATION_TICK;

fn snapshot(session: &Session) -> Vec<u8> {
    serde_json::to_vec(&SaveEnvelope::new(&session.registries, &session.state)).unwrap()
}

#[test]
fn seeded_opening_is_replayable_and_uses_authoritative_admission() {
    let first = Session::new(24318).unwrap();
    let second = Session::new(24318).unwrap();
    assert_eq!(snapshot(&first), snapshot(&second));
    assert!(first.state.survival().player().is_some());
    assert!(first.state.world().chunks().count() >= 9);
    assert!(first.opening().contains("seed 24318"));
    assert!(first.opening().contains("Condition: vitality"));
    assert!(first.opening().contains("berries at"));
    assert!(first.opening().contains("Visible spring water"));
}

#[test]
fn text_commands_execute_real_multirate_control_ticks() {
    let mut session = Session::new(24318).unwrap();
    let start = snapshot(&session);
    let (frame, continue_playing) = session.command("wait 72").unwrap();
    assert!(continue_playing);
    assert!(frame.contains("Advanced 72 control ticks"));
    assert_eq!(
        session.state.control_tick().value(),
        CONTROL_TICKS_PER_SIMULATION_TICK
    );
    assert_eq!(session.state.tick().value(), 1);
    assert_ne!(snapshot(&session), start);

    let (frame, _) = session.command("use down").unwrap();
    assert!(frame.contains("Use:"));
    assert_eq!(
        session.state.control_tick().value(),
        CONTROL_TICKS_PER_SIMULATION_TICK + 1
    );
}

#[test]
fn walking_to_berries_and_gathering_reaches_real_carried_inventory() {
    let mut session = Session::new(24318).unwrap();
    session.command("e 20").unwrap();
    session.command("wait 10").unwrap();
    let (began, _) = session.command("gather 1").unwrap();
    assert!(began.contains("SurfaceGatheringStarted"));
    let (finished, _) = session.command("wait 1600").unwrap();
    assert!(finished.contains("Gathered:"));
    let (inventory, _) = session.command("inventory").unwrap();
    assert!(inventory.contains("berries | 500 g"));
}

#[test]
fn invalid_commands_and_readouts_do_not_change_the_world() {
    let mut session = Session::new(123).unwrap();
    let before = snapshot(&session);
    assert!(session.command("n -1").is_err());
    assert!(session.command("gather 99").is_err());
    assert!(session.command("slot 0").is_err());
    assert!(session.command("select 1").is_err());
    let (inventory, _) = session.command("inventory").unwrap();
    assert!(inventory.contains("Carried matter"));
    let (look, _) = session.command("look e").unwrap();
    assert!(
        look.contains("Pointed at")
            || look.contains("No targetable block")
            || look.contains("Pointing unavailable")
    );
    assert_eq!(snapshot(&session), before);
}

#[test]
fn save_envelope_roundtrips_through_current_trusted_loader() {
    let mut session = Session::new(8).unwrap();
    session.command("e 20").unwrap();
    let encoded =
        serde_json::to_vec(&SaveEnvelope::new(&session.registries, &session.state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded).unwrap();
    let reloaded = decoded.into_state(&session.registries).unwrap();
    assert_eq!(
        snapshot(&session),
        serde_json::to_vec(&SaveEnvelope::new(&session.registries, &reloaded)).unwrap()
    );
}

#[test]
fn quitting_does_not_advance_simulation() {
    let mut session = Session::new(8).unwrap();
    let before = snapshot(&session);
    let (notice, continue_playing) = session.command("quit").unwrap();
    assert!(!continue_playing);
    assert!(notice.contains("ended"));
    assert_eq!(before, snapshot(&session));
}
