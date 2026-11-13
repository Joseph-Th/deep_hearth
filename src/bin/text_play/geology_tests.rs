//! End-to-end evidence acquisition and finite ore extraction in the ordinary seeded world.

use super::*;
use deep_hearth::content::MATERIAL_COPPER;
use deep_hearth::geology::{GeologicalEvidenceConsistency, assess_geological_knowledge};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::mining::{MiningTargetRequest, resolve_mining_target};

fn carried_stack(session: &Session, material_name: &str) -> usize {
    let inventory = project_player_inventory(&session.registries, &session.state).unwrap();
    inventory
        .carried()
        .stacks()
        .iter()
        .position(|stack| {
            session
                .registries
                .materials()
                .commodity_name(stack.commodity())
                == Some(material_name)
        })
        .map_or_else(
            || panic!("carried stack {material_name} was not acquired"),
            |index| index + 1,
        )
}

#[test]
fn mined_copper_ore_can_be_broken_and_sorted_into_finite_native_metal() {
    let mut session = equipped_for_copper_survey_and_mining();
    run(&mut session, "prospect copper 1");
    finish_work(&mut session);
    run(&mut session, "mine copper 300 with 2");
    finish_work(&mut session);
    let inventory = run(&mut session, "inventory");
    assert!(inventory.contains("copper ore | 300 g"));
    let first = carried_stack(&session, "copper ore");
    let catalog = run(&mut session, "ore");
    assert!(catalog.contains("15. "));
    assert!(catalog.contains("13. "));
    assert!(catalog.contains(&format!("Candidate stack {first}: copper ore")));
    let prior = snapshot(&session);
    for bad in [
        "dress 0 1 100",
        "dress 999 1 100",
        "dress 15 0 100",
        "dress 15 999 100",
        "dress 15 1 0",
        "dress 15 1 9999999999999999999",
    ] {
        assert!(session.command(bad).is_err(), "{bad}");
    }
    assert!(
        session.command(&format!("dress 15 {first} 200")).is_err(),
        "hand ore breaking has a 100 g maximum batch"
    );
    assert!(
        session.command(&format!("dress 13 {first} 100")).is_err(),
        "unbroken ore does not qualify for hand sorting"
    );
    assert_eq!(snapshot(&session), prior);
    let conserved = calculate_matter_accounting(&session.state).unwrap().total();
    let started = run(&mut session, &format!("dress 15 {first} 100"));
    assert!(started.contains("100 g from stack"));
    let checkpoint = snapshot(&session);
    let mut restored = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
            .unwrap()
            .into_state(&session.registries)
            .unwrap(),
        seed: None,
    };
    let done = finish_work(&mut session);
    let replay = finish_work(&mut restored);
    assert_eq!(done, replay);
    assert_eq!(snapshot(&session), snapshot(&restored));
    assert!(done.contains("Ore processing completed: hand break ore"));
    assert!(done.contains("Received 100 g crushed copper ore"));

    let inventory = run(&mut session, "inventory");
    assert!(inventory.contains("copper ore | 200 g"));
    assert!(inventory.contains("crushed copper ore | 100 g"));
    let crushed = carried_stack(&session, "crushed copper ore");
    let started = run(&mut session, &format!("dress 13 {crushed} 100"));
    assert!(started.contains("100 g from stack"));
    let sorting_checkpoint = snapshot(&session);
    let mut replay_sorting = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&sorting_checkpoint)
            .unwrap()
            .into_state(&session.registries)
            .unwrap(),
        seed: None,
    };
    let finished = finish_work(&mut session);
    let replayed = finish_work(&mut replay_sorting);
    assert!(finished.contains("Ore processing completed: hand sort native copper"));
    assert_eq!(finished, replayed);
    assert_eq!(snapshot(&session), snapshot(&replay_sorting));
    let inv = project_player_inventory(&session.registries, &session.state).unwrap();
    let native = inv
        .carried()
        .stacks()
        .iter()
        .find(|stack| {
            session
                .registries
                .materials()
                .commodity_name(stack.commodity())
                == Some("native copper")
        })
        .expect("hand sorting must return physically recovered native copper");
    assert_eq!(native.mass().milligrams(), 32_500);
    assert!(run(&mut session, "inventory").contains("native copper | 32 g"));
    assert_eq!(
        calculate_matter_accounting(&session.state).unwrap().total(),
        conserved
    );
    let crafted = run(&mut session, "craft 10");
    assert!(crafted.contains("Started cold-work native copper reinforcement"));
    let finished = finish_work(&mut session);
    assert!(finished.contains("Craft completed: cold-work native copper reinforcement"));
    let after = run(&mut session, "inventory");
    assert!(after.contains("copper reinforcement | 20 g"));
    assert!(after.contains("native copper | 12 g"));
    assert_eq!(
        calculate_matter_accounting(&session.state).unwrap().total(),
        conserved
    );
}

fn snapshot(session: &Session) -> Vec<u8> {
    serde_json::to_vec(&SaveEnvelope::new(&session.registries, &session.state)).unwrap()
}

fn run(session: &mut Session, command: &str) -> String {
    session
        .command(command)
        .unwrap_or_else(|error| panic!("{command}: {error}"))
        .0
}

fn finish_work(session: &mut Session) -> String {
    let mut output = String::new();
    for _ in 0..25 {
        if session.state.player_work().active().is_none() {
            return output;
        }
        output.push_str(&run(session, "wait work"));
    }
    panic!("ordinary player work did not finish after 25 bounded waits");
}

fn move_along(session: &mut Session, controls: &[&str]) {
    for command in controls {
        run(session, command);
    }
}

fn equipped_for_copper_survey_and_mining() -> Session {
    let mut session = Session::new(24318).unwrap();
    // Visit the finite float first. The reinforcement is made, not conjured.
    move_along(
        &mut session,
        &["e 20", "wait 10", "s 20", "wait 10", "s 20", "wait 10"],
    );
    run(&mut session, "gather 2 20");
    finish_work(&mut session);
    run(&mut session, "craft 10");
    finish_work(&mut session);

    // Knapped tool heads and the two wood handles use physical gathered supply.
    move_along(
        &mut session,
        &[
            "n 20", "wait 10", "e 20", "wait 10", "n 20", "wait 10", "e 20", "wait 10",
        ],
    );
    run(&mut session, "gather 1 2000");
    finish_work(&mut session);
    run(&mut session, "craft 7 2");
    finish_work(&mut session);
    move_along(&mut session, &["w 20", "wait 10", "s 20", "wait 10"]);
    run(&mut session, "gather 1 2000");
    finish_work(&mut session);
    run(&mut session, "craft 8 2");
    finish_work(&mut session);

    assert!(run(&mut session, "assemble 21").contains("stone geological sampling hammer"));
    run(&mut session, "take 1");
    assert!(run(&mut session, "assemble 8").contains("copper-reinforced stone pick"));
    run(&mut session, "take 1");
    let inventory = run(&mut session, "inventory");
    assert!(inventory.contains("Carried equipment 1: stone geological sampling hammer"));
    assert!(inventory.contains("Carried equipment 2: copper-reinforced stone pick"));
    move_along(&mut session, &["w 20", "wait 10", "s 20", "wait 10"]);
    assert!(run(&mut session, "status").contains("At (9,2,10)"));
    run(&mut session, "wait boundary");
    session
}

#[test]
fn field_prospecting_uses_acquired_evidence_and_cannot_invent_locality() {
    let mut session = Session::new(24318).unwrap();
    let before = snapshot(&session);
    assert!(run(&mut session, "knowledge").contains("(none acquired"));
    let all = run(&mut session, "surveys");
    assert!(all.contains("1. SurfaceExposure"));
    assert!(all.contains("2. ExcavationSample"));
    assert!(all.contains("none usable here"));
    let broad = run(&mut session, "surveys 2");
    assert!(!broad.contains("1. SurfaceExposure"));
    assert!(broad.contains("4. SurfaceExposure"));
    for invalid in [
        "surveys 0",
        "surveys 5",
        "knowledge 0",
        "prospect nonsense 1",
        "prospect copper 0",
        "prospect copper 1 2 with",
        "prospect copper 1 5",
        "prospect copper 2 with 1",
        "prospect copper 1 with 999",
    ] {
        assert!(session.command(invalid).is_err(), "{invalid}");
    }
    assert_eq!(
        snapshot(&session),
        before,
        "readouts and rejected choices must be pure"
    );

    let mut session = equipped_for_copper_survey_and_mining();
    let original = snapshot(&session);
    let catalog = run(&mut session, "surveys");
    assert!(catalog.contains("with 1 (condition 100% -> 99%)"));
    assert!(catalog.contains("stone geological sampling hammer"));
    assert_eq!(snapshot(&session), original);
    let wide = run(&mut session, "prospect copper 4 2");
    assert!(wide.contains("2 x 2 voxels"));
    let _ = finish_work(&mut session);
    let broad_knowledge = run(&mut session, "knowledge");
    assert!(broad_knowledge.contains("copper | 1 observations | abundance"));
    let here = session.state.logistics().player().unwrap().position();
    let region = deep_hearth::spatial::VoxelBounds::new(
        here,
        deep_hearth::spatial::VoxelCoord::new(here.x() + 1, here.y() + 1, here.z() + 1),
    )
    .unwrap();
    let assessment = assess_geological_knowledge(
        session.state.geological_knowledge(),
        region,
        MATERIAL_COPPER,
    );
    assert!(matches!(
        assessment.consistency(),
        GeologicalEvidenceConsistency::Compatible { .. }
    ));
    assert!(
        resolve_mining_target(
            &session.state,
            MiningTargetRequest::new(region, MATERIAL_COPPER)
        )
        .is_err(),
        "2 x 2 aggregate evidence cannot be narrowed into an unobserved one-voxel deposit"
    );
    let before = snapshot(&session);
    assert!(session.command("mine copper 300 with 2").is_err());
    assert_eq!(snapshot(&session), before);
}

#[test]
fn ordinary_wilderness_survey_to_reinforced_pick_ore_conserves_matter_and_replays() {
    let mut session = equipped_for_copper_survey_and_mining();
    let before = snapshot(&session);
    assert!(session.command("mine copper 300 with 2").is_err());
    assert_eq!(snapshot(&session), before);
    run(&mut session, "prospect copper 1");
    let completed = finish_work(&mut session);
    assert!(completed.contains("Prospecting finished"));
    let first = run(&mut session, "knowledge");
    assert!(first.contains("copper | 1 observations | abundance"));
    assert!(!first.contains("hardness"));
    let here = session.state.logistics().player().unwrap().position();
    let exact = deep_hearth::spatial::VoxelBounds::new(
        here,
        deep_hearth::spatial::VoxelCoord::new(here.x() + 1, here.y() + 1, here.z() + 1),
    )
    .unwrap();
    assert!(
        resolve_mining_target(
            &session.state,
            MiningTargetRequest::new(exact, MATERIAL_COPPER)
        )
        .is_ok(),
        "one-voxel acquired exposure can authorize a target even before the optional hardness survey"
    );

    run(&mut session, "prospect copper 2 with 1");
    let evidence_saved = snapshot(&session);
    let mut replay = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&evidence_saved)
            .unwrap()
            .into_state(&session.registries)
            .unwrap(),
        seed: None,
    };
    let finished = finish_work(&mut session);
    let reproduced = finish_work(&mut replay);
    assert_eq!(finished, reproduced);
    assert_eq!(snapshot(&session), snapshot(&replay));
    assert!(finished.contains("Prospecting finished"));
    let local = run(&mut session, "knowledge");
    assert!(local.contains("copper | 2 observations"));
    assert!(local.contains("hardness 600..650 MPa"));
    assert!(local.contains("surveyed mass 25000..30000 g"));
    assert!(local.contains("historical acquired measurements"));
    let read_snapshot = snapshot(&session);
    for invalid in [
        "mine copper 0 with 2",
        "mine copper 500 with 2",
        "mine copper 300 with 1",
        "mine copper 300 with 3",
        "mine copper 300 with 2 using 0",
        "mine copper 300 with 2 using 99",
        "mine uranium 300 with 2",
    ] {
        assert!(session.command(invalid).is_err(), "{invalid}");
    }
    assert_eq!(snapshot(&session), read_snapshot);

    let conserved = calculate_matter_accounting(&session.state).unwrap().total();
    let started = run(&mut session, "mine copper 300 with 2");
    assert!(started.contains("Started 300 g copper extraction"));
    assert!(started.contains("Work: hand pick mining; scheduled completion in"));
    let mining_saved = snapshot(&session);
    let mut restored = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&mining_saved)
            .unwrap()
            .into_state(&session.registries)
            .unwrap(),
        seed: None,
    };
    assert_eq!(snapshot(&restored), mining_saved);
    let mined = finish_work(&mut session);
    let replicated = finish_work(&mut restored);
    assert_eq!(mined, replicated);
    assert_eq!(snapshot(&session), snapshot(&restored));
    assert!(mined.contains("Mining produced"));
    let inventory = run(&mut session, "inventory");
    assert!(inventory.contains("copper ore | 300 g"));
    assert!(inventory.contains("copper-reinforced stone pick | 1020 g | condition 99%"));
    assert_eq!(
        calculate_matter_accounting(&session.state).unwrap().total(),
        conserved
    );
    assert!(
        run(&mut session, "knowledge").contains("surveyed mass 25000..30000 g"),
        "surveyed mass must remain dated knowledge, not reveal current hidden reserves"
    );

    move_along(&mut session, &["n 20", "wait 10"]);
    let remote_snapshot = snapshot(&session);
    assert!(session.command("mine copper 100 with 2").is_err());
    assert_eq!(snapshot(&session), remote_snapshot);
}
