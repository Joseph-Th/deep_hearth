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
fn repeated_terrain_excavation_matches_authorized_batches_and_conserves_matter() {
    use deep_hearth::simulation::validate_pointed_terrain_excavation;
    use deep_hearth::spatial::ViewDirection;

    let mut repeated = equipped_for_copper_survey_and_mining();
    run(&mut repeated, "equip 2");
    let before = snapshot(&repeated);
    let preview = run(&mut repeated, "look down");
    assert!(preview.contains("Excavation preview:"), "{preview}");
    assert!(preview.contains("solid terrain remains"), "{preview}");
    assert_eq!(snapshot(&repeated), before);
    for bad in [
        "break down repeat 0",
        "break down repeat 33",
        "break bogus repeat 3",
    ] {
        assert!(repeated.command(bad).is_err(), "{bad}");
        assert_eq!(snapshot(&repeated), before);
    }
    let mut manual = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&before)
            .unwrap()
            .into_state(&repeated.registries)
            .unwrap(),
        seed: None,
    };
    let conserved = calculate_matter_accounting(&repeated.state)
        .unwrap()
        .total();
    let report = run(&mut repeated, "break down repeat 3");
    assert!(
        report.contains("Excavation repeat: 3 batches completed."),
        "{report}"
    );
    assert_eq!(report.matches("Excavated:").count(), 3, "{report}");
    let aim = ViewDirection::new(0, -1_000_000, 0).unwrap();
    for _ in 0..3 {
        let plan = validate_pointed_terrain_excavation(&manual.registries, &manual.state, aim)
            .unwrap()
            .unwrap();
        let _receipt = plan.commit(&mut manual.state).unwrap();
        finish_work(&mut manual);
    }
    assert_eq!(snapshot(&repeated), snapshot(&manual));
    assert_eq!(
        calculate_matter_accounting(&repeated.state)
            .unwrap()
            .total(),
        conserved
    );
    assert_eq!(
        deep_hearth::core::state::validate_loaded_state(&repeated.registries, &repeated.state),
        Ok(())
    );
}

#[test]
fn repeated_mining_matches_explicit_batches_and_conserves_matter() {
    let mut repeated = equipped_for_copper_survey_and_mining();
    run(&mut repeated, "prospect copper 2 with 1");
    finish_work(&mut repeated);
    let before = snapshot(&repeated);
    for invalid in [
        "mine copper 200 with 2 repeat 0",
        "mine copper 200 with 2 repeat 33",
        "mine copper 200 with 99 repeat 3",
    ] {
        assert!(repeated.command(invalid).is_err(), "{invalid}");
        assert_eq!(snapshot(&repeated), before);
    }
    let mut manual = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&before)
            .unwrap()
            .into_state(&repeated.registries)
            .unwrap(),
        seed: None,
    };
    let conserved = calculate_matter_accounting(&repeated.state)
        .unwrap()
        .total();
    let report = run(&mut repeated, "mine copper 200 with 2 repeat 3");
    assert!(
        report.contains("Mining repeat: 3 batches completed"),
        "{report}"
    );
    assert_eq!(
        report.matches("Mining produced: 200 g").count(),
        3,
        "{report}"
    );
    for _ in 0..3 {
        run(&mut manual, "mine copper 200 with 2");
        finish_work(&mut manual);
    }
    assert_eq!(snapshot(&repeated), snapshot(&manual));
    assert_eq!(
        calculate_matter_accounting(&repeated.state)
            .unwrap()
            .total(),
        conserved
    );
    assert!(run(&mut repeated, "inventory").contains("copper ore | 600 g"));
    assert_eq!(
        deep_hearth::core::state::validate_loaded_state(&repeated.registries, &repeated.state),
        Ok(())
    );
}

#[test]
fn resource_mass_history_keeps_the_tightest_earlier_physical_bound() {
    use deep_hearth::core::state::StateValidationError;
    use deep_hearth::geology::{GeologicalKnowledgeValidationError, GeologicalObservationId};
    use deep_hearth::persistence::LoadError;

    let mut session = equipped_for_copper_survey_and_mining();
    run(&mut session, "prospect copper 2 with 1");
    finish_work(&mut session);
    let conserved = calculate_matter_accounting(&session.state).unwrap().total();
    let mining = run(&mut session, "mine copper 200 with 2 repeat 32");
    assert!(
        mining.contains("Mining repeat: 32 batches completed."),
        "{mining}"
    );
    for command in [
        "prospect copper 8 with 1",
        "prospect copper 2 with 1",
        "prospect copper 8 with 1",
    ] {
        run(&mut session, command);
        assert!(finish_work(&mut session).contains("Prospecting finished"));
    }
    let report = run(&mut session, "knowledge");
    assert!(report.contains("sampled mass 25000..30000 g"), "{report}");
    assert!(report.contains("sampled mass 20000..25000 g"), "{report}");
    assert_eq!(
        report.matches("sampled mass 22000..23000 g").count(),
        2,
        "{report}"
    );
    assert_eq!(
        calculate_matter_accounting(&session.state).unwrap().total(),
        conserved
    );
    let encoded =
        serde_json::to_vec(&SaveEnvelope::new(&session.registries, &session.state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&session.registries)
        .unwrap();
    assert_eq!(
        serde_json::to_vec(&SaveEnvelope::new(&session.registries, &loaded)).unwrap(),
        encoded
    );

    // The last band is individually possible and fits inside sample 3's
    // coarser band, but sample 2 has already ruled it out permanently.
    let mut forged: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    forged["state"]["systems"]["geological_knowledge"]["observations"]["4"]["resource_mass"] =
        serde_json::json!({"lower": 23_000_000, "upper": 24_000_000});
    let result = serde_json::from_value::<LoadedSaveEnvelope>(forged)
        .unwrap()
        .into_state(&session.registries);
    assert!(
        matches!(result, Err(LoadError::InvalidState(StateValidationError::GeologicalKnowledge(GeologicalKnowledgeValidationError::ResourceMassIncreasesAcrossObservations { earlier, observation, .. }))) if earlier == GeologicalObservationId::new(2) && observation == GeologicalObservationId::new(4)),
        "{result:?}"
    );
}

#[test]
fn mined_copper_ore_can_be_broken_and_sorted_into_finite_native_metal() {
    let mut session = equipped_for_copper_survey_and_mining();
    run(&mut session, "prospect copper 1");
    finish_work(&mut session);
    assert!(
        session.command("mine copper 200 with 2").is_err(),
        "coarse evidence alone must not resolve low-grade copper"
    );
    run(&mut session, "prospect copper 2 with 1");
    finish_work(&mut session);
    run(&mut session, "mine copper 200 with 2");
    finish_work(&mut session);
    let inventory = run(&mut session, "inventory");
    assert!(inventory.contains("copper ore | 200 g"));
    assert!(
        inventory.lines().any(|line| {
            line.contains("copper ore | 200 g |") && line.ends_with("mixed material")
        }),
        "mined mixed ore should be distinguishable without exposing its exact assay: {inventory}"
    );
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
    assert!(started.contains("Use 'wait finish'"), "{started}");
    let checkpoint = snapshot(&session);
    let mut restored = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
            .unwrap()
            .into_state(&session.registries)
            .unwrap(),
        seed: None,
    };
    let done = run(&mut session, "wait finish");
    let replay = run(&mut restored, "wait finish");
    assert!(session.state.player_work().active().is_none());
    assert!(restored.state.player_work().active().is_none());
    assert_eq!(done, replay);
    assert_eq!(snapshot(&session), snapshot(&restored));
    assert!(done.contains("Ore processing completed: hand break ore"));
    assert!(done.contains("Received 100 g crushed copper ore"));

    let inventory = run(&mut session, "inventory");
    assert!(inventory.contains("copper ore | 100 g"));
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
    assert_eq!(native.mass().milligrams(), 6_500);
    assert!(run(&mut session, "inventory").contains("native copper | 6.5 g"));
    assert_eq!(
        calculate_matter_accounting(&session.state).unwrap().total(),
        conserved
    );
    assert!(
        session.command("craft 10").is_err(),
        "one hand-sorted ore batch cannot yield a ready copper reinforcement"
    );
    // Stone mining is limited to 200 g per batch. Four ore-breaking and
    // sorting batches are needed to recover the first 20 g of native metal.
    let unworked = carried_stack(&session, "copper ore");
    run(&mut session, &format!("dress 15 {unworked} 100"));
    finish_work(&mut session);
    let crushed = carried_stack(&session, "crushed copper ore");
    run(&mut session, &format!("dress 13 {crushed} 100"));
    finish_work(&mut session);
    assert!(session.command("craft 10").is_err());
    run(&mut session, "mine copper 200 with 2");
    finish_work(&mut session);
    for _ in 0..2 {
        let unworked = carried_stack(&session, "copper ore");
        run(&mut session, &format!("dress 15 {unworked} 100"));
        finish_work(&mut session);
        let crushed = carried_stack(&session, "crushed copper ore");
        run(&mut session, &format!("dress 13 {crushed} 100"));
        finish_work(&mut session);
    }
    assert!(run(&mut session, "inventory").contains("native copper | 26 g"));
    let crafted = run(&mut session, "craft 10");
    assert!(crafted.contains("Started cold-work native copper reinforcement"));
    let finished = finish_work(&mut session);
    assert!(finished.contains("Craft completed: cold-work native copper reinforcement"));
    let after = run(&mut session, "inventory");
    assert!(after.contains("copper reinforcement | 20 g"));
    assert!(after.contains("native copper | 6 g"));
    assert_eq!(
        calculate_matter_accounting(&session.state).unwrap().total(),
        conserved
    );
}

#[test]
fn fractional_mining_and_hand_dressing_preserve_subgram_matter_and_save_replay() {
    let mut session = equipped_for_copper_survey_and_mining();
    run(&mut session, "prospect copper 1");
    finish_work(&mut session);
    run(&mut session, "prospect copper 2 with 1");
    finish_work(&mut session);
    let before = snapshot(&session);
    for command in [
        "mine copper 0 with 2",
        "mine copper 0.0001 with 2",
        "mine copper .125 with 2",
        "mine copper 18446744073709551.616 with 2",
        "dress 15 1 0.0001",
        "dress 15 1 1e3",
    ] {
        assert!(session.command(command).is_err(), "{command}");
    }
    assert_eq!(
        snapshot(&session),
        before,
        "invalid mass cannot spend time or matter"
    );

    let conserved = calculate_matter_accounting(&session.state).unwrap().total();
    assert!(
        run(&mut session, "mine copper 0.125 with 2").contains("Started 0.125 g copper extraction")
    );
    finish_work(&mut session);
    assert!(run(&mut session, "inventory").contains("copper ore | 0.125 g"));

    let ore = carried_stack(&session, "copper ore");
    let started = run(&mut session, &format!("dress 15 {ore} 0.125"));
    assert!(started.contains("0.125 g from stack"), "{started}");
    let saved_during_work = snapshot(&session);
    let mut restored = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&saved_during_work)
            .unwrap()
            .into_state(&session.registries)
            .unwrap(),
        seed: None,
    };
    assert_eq!(finish_work(&mut session), finish_work(&mut restored));
    assert_eq!(snapshot(&session), snapshot(&restored));
    assert!(run(&mut session, "inventory").contains("crushed copper ore | 0.125 g"));

    let crushed = carried_stack(&session, "crushed copper ore");
    let started = run(&mut session, &format!("dress 13 {crushed} 0.125"));
    assert!(started.contains("0.125 g from stack"), "{started}");
    let completed = finish_work(&mut session);
    assert!(completed.contains("Ore processing completed: hand sort native copper"));
    assert!(run(&mut session, "inventory").contains("native copper | 0.008 g"));
    assert_eq!(
        calculate_matter_accounting(&session.state).unwrap().total(),
        conserved
    );
    assert_eq!(
        deep_hearth::core::state::validate_loaded_state(&session.registries, &session.state),
        Ok(())
    );
}

#[test]
fn repeated_dressing_matches_individual_batches_and_preserves_matter() {
    let mut repeated = equipped_for_copper_survey_and_mining();
    run(&mut repeated, "prospect copper 1");
    finish_work(&mut repeated);
    run(&mut repeated, "prospect copper 2 with 1");
    finish_work(&mut repeated);
    run(&mut repeated, "mine copper 200 with 2 repeat 2");
    let ore = carried_stack(&repeated, "copper ore");
    let before = snapshot(&repeated);
    for invalid in ["0", "33", "no"] {
        assert!(
            repeated
                .command(&format!("dress 15 {ore} 100 repeat {invalid}"))
                .is_err()
        );
        assert_eq!(snapshot(&repeated), before);
    }
    let mut manual = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&before)
            .unwrap()
            .into_state(&repeated.registries)
            .unwrap(),
        seed: None,
    };
    let conserved = calculate_matter_accounting(&repeated.state)
        .unwrap()
        .total();
    let report = run(&mut repeated, &format!("dress 15 {ore} 100 repeat 3"));
    assert!(
        report.contains("Dressing repeat: 3 batches completed."),
        "{report}"
    );
    for _ in 0..3 {
        let index = carried_stack(&manual, "copper ore");
        run(&mut manual, &format!("dress 15 {index} 100"));
        finish_work(&mut manual);
    }
    assert_eq!(snapshot(&repeated), snapshot(&manual));
    let crushed = carried_stack(&repeated, "crushed copper ore");
    let report = run(&mut repeated, &format!("dress 13 {crushed} 100 repeat 4"));
    assert!(
        report.contains("Dressing repeat: 3 batches completed."),
        "{report}"
    );
    assert!(report.contains("selected stack exhausted"), "{report}");
    for _ in 0..3 {
        let index = carried_stack(&manual, "crushed copper ore");
        run(&mut manual, &format!("dress 13 {index} 100"));
        finish_work(&mut manual);
    }
    assert_eq!(snapshot(&repeated), snapshot(&manual));
    assert_eq!(
        calculate_matter_accounting(&repeated.state)
            .unwrap()
            .total(),
        conserved
    );
    assert_eq!(
        deep_hearth::core::state::validate_loaded_state(&repeated.registries, &repeated.state),
        Ok(())
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
    // Stone tools, acquired evidence and hand dressing precede first copper.
    move_along(
        &mut session,
        &["e 20", "wait 10", "e 20", "wait 10", "e 20", "wait 10"],
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
    assert!(run(&mut session, "assemble 6").contains("knapped stone pick"));
    run(&mut session, "take 1");
    let inventory = run(&mut session, "inventory");
    assert!(inventory.contains("Carried equipment 1: stone geological sampling hammer"));
    assert!(inventory.contains("Carried equipment 2: knapped stone pick"));
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
    let after_survey = snapshot(&session);
    let broad_details = run(&mut session, "knowledge 2");
    assert!(broad_details.contains("Sample #1: region (9,2,10) to (11,3,12) exclusive"));
    assert!(broad_details.contains("SurfaceExposure"));
    assert!(broad_details.contains("ppm"));
    assert_eq!(snapshot(&session), after_survey);
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
fn ordinary_wilderness_stone_pick_survey_and_ore_conserves_matter_and_replays() {
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
        .is_err(),
        "coarse exposure cannot resolve the sparse weathered copper; a precise stone-hammer survey is required"
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
    let first_sample = local.find("Sample #1:").expect("first acquired sample");
    let second_sample = local.find("Sample #2:").expect("second acquired sample");
    assert!(
        first_sample < second_sample,
        "knowledge must retain acquired ID order"
    );
    assert_eq!(local.matches("Sample #1:").count(), 1);
    assert_eq!(local.matches("Sample #2:").count(), 1);
    assert!(local.contains("hardness 400..450 MPa"));
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
    let started = run(&mut session, "mine copper 200 with 2");
    assert!(started.contains("Started 200 g copper extraction"));
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
    assert!(inventory.contains("copper ore | 200 g"));
    assert!(inventory.contains("knapped stone pick | 1000 g | condition 99%"));
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
