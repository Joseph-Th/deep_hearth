//! Timed manual-craft conservation, reserve, persistence, stale-token, and repeated-batch contracts.

use super::*;

#[test]
fn stone_knapping_is_timed_conserved_hand_work() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("manual craft initial accounting failed: {error}"));
    let survival_before = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("manual craft survival state is missing"));
    let resolution = resolve_manual_craft(
        &registries,
        &state,
        &ManualCraftRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
        ),
    )
    .unwrap_or_else(|error| panic!("stone knapping resolution failed: {error}"));
    assert_eq!(resolution.duration(), TickSpan::new(40));
    assert_eq!(
        validate_start_process(&registries, &state, &resolution, source, destination),
        Err(StartProcessError::ManualProcessRequiresPlayerWork {
            process: PROCESS_KNAP_STONE_TOOL,
        })
    );
    let token = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("stone knapping start failed: {error}"));
    let job = token
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stone knapping commit failed: {error}"));
    assert!(matches!(
        state.player_work().active(),
        Some(PlayerWork::ManualProduction { .. })
    ));

    let completes_at = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("stone knapping job disappeared after admission"))
        .completes_at();
    while state.tick() < completes_at {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("stone knapping tick failed: {error}"));
    }
    assert_eq!(state.player_work().active(), None);

    let destination_record = state
        .inventory()
        .get_stockpile(destination)
        .unwrap_or_else(|| panic!("stone knapping destination disappeared"));
    assert_eq!(
        destination_record.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_TOOL)),
        Mass::from_milligrams(800_000)
    );
    assert_eq!(
        destination_record.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_CHIP)),
        Mass::from_milligrams(200_000)
    );
    let matter_after = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("manual craft final accounting failed: {error}"));
    assert_eq!(matter_before.total(), matter_after.total());
    let survival_after = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("manual craft survival state disappeared"));
    let physiology = registries.survival().physiology();
    let exertion = registries
        .crafting()
        .get_manual(PROCESS_KNAP_STONE_TOOL)
        .unwrap_or_else(|| panic!("stone knapping manual definition disappeared"))
        .exertion();
    assert_eq!(
        survival_before.metabolic_energy().nanojoules()
            - survival_after.metabolic_energy().nanojoules(),
        (physiology.basal_energy_cost_per_tick().nanojoules()
            + exertion.energy_cost_per_tick().nanojoules())
            * u128::from(resolution.duration().value()),
        "manual-craft admission duration must equal the exact number of charged active ticks"
    );
    assert_eq!(
        survival_before.hydration().microliters() - survival_after.hydration().microliters(),
        (physiology.hydration_loss_per_tick().microliters()
            + exertion.hydration_loss_per_tick().microliters())
            * resolution.duration().value(),
        "manual-craft hydration budgeting must match realized active-tick cost"
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("stone knapping final audit failed: {error}"));
}

#[test]
fn manual_craft_requires_enough_metabolic_reserve_to_finish() {
    let (registries, state, source, lot, destination) = make_fixture();
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("manual craft reserve serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["player"]["metabolic_energy"] =
        serde_json::json!(1_u64);
    let loaded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("manual craft low-reserve decode failed: {error}"));
    let low_reserve = loaded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("manual craft low-reserve load failed: {error}"));
    let before = low_reserve.clone();

    assert!(matches!(
        validate_start_manual_craft(
            &registries,
            &low_reserve,
            ManualCraftStartRequest::single(
                PROCESS_KNAP_STONE_TOOL,
                source,
                MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
                destination,
            ),
        ),
        Err(StartManualCraftError::Work(
            PlayerWorkStartError::InsufficientMetabolicEnergy { .. }
        ))
    ));
    assert_eq!(low_reserve, before);
}

#[test]
fn manual_craft_commit_rejects_intervening_survival_change() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    let token = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("manual craft survival-stale validation failed: {error}"));
    let expected = state.survival().revision();
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("manual craft survival-stale tick failed: {error}"));
    let before = state.clone();

    assert_eq!(
        token.commit(&mut state),
        Err(ManualCraftCommitError::Work(
            PlayerWorkCommitError::StaleSurvivalRevision {
                expected,
                actual: state.survival().revision(),
            }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn active_manual_craft_save_requires_enough_metabolic_energy_to_finish() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    let token = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("manual craft save reserve start failed: {error}"));
    let job = token
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("manual craft save reserve commit failed: {error}"));
    let record = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("manual craft save reserve job disappeared"));
    let remaining = record
        .completes_at()
        .checked_duration_since(state.tick())
        .unwrap_or_else(|| panic!("manual craft completion precedes current tick"));
    let exertion = registries
        .crafting()
        .get_manual(PROCESS_KNAP_STONE_TOOL)
        .unwrap_or_else(|| panic!("manual craft save reserve definition disappeared"))
        .exertion();
    let required = calculate_player_work_resource_budget(
        registries.survival().physiology(),
        exertion,
        remaining,
    )
    .unwrap_or_else(|error| panic!("manual craft save reserve budget failed: {error:?}"))
    .metabolic_energy();
    assert!(required > Energy::from_nanojoules(1));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("manual craft save reserve serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["player"]["metabolic_energy"] =
        serde_json::json!(1_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("manual craft save reserve decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::InsufficientMetabolicEnergy {
                available: Energy::from_nanojoules(1),
                required,
            }
        )))
    );
}

#[test]
fn active_manual_craft_save_requires_player_work_revision_for_later_release() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("manual craft revision-load start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("manual craft revision-load commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("manual craft revision-load serialization failed: {error}"));
    encoded["state"]["systems"]["player_work"]["revision"] = serde_json::json!(u64::MAX);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("manual craft revision-load decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::RevisionExhausted
        )))
    );
}

#[test]
fn active_manual_craft_save_requires_survival_revision_capacity_to_finish() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("manual craft survival-revision start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("manual craft survival-revision commit failed: {error}"));

    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("manual craft survival-revision serialization failed: {error}")
        });
    encoded["state"]["systems"]["survival"]["revision"] = serde_json::json!(u64::MAX);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("manual craft survival-revision decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::SurvivalRevisionExhausted
        )))
    );
}

#[test]
fn suspended_manual_craft_loads_with_depleted_reserves_and_does_not_resume_unsafely() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    let support = active_stockpile_support(&registries, &mut state);
    let _ = validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap_or_else(|error| panic!("suspended low-reserve destination mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("suspended low-reserve destination mount commit failed: {error}")
        });
    let job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("suspended low-reserve craft start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("suspended low-reserve craft start commit failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("suspended low-reserve active craft tick failed: {error}"));
    let _ = validate_set_structural_load(
        &registries,
        &state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    )
    .unwrap_or_else(|error| {
        panic!("suspended low-reserve support failure validation failed: {error}")
    })
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("suspended low-reserve support failure commit failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("suspended low-reserve suspension tick failed: {error}"));
    assert_eq!(state.player_work().active(), None);

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("suspended low-reserve serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["player"]["metabolic_energy"] =
        serde_json::json!(1_u64);
    encoded["state"]["systems"]["survival"]["player"]["hydration"] = serde_json::json!(1_u64);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("suspended low-reserve decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("suspended low-reserve state failed trusted load: {error}"));
    assert_eq!(loaded.player_work().active(), None);

    let _ = validate_unmount_stockpile(&registries, &loaded, destination)
        .unwrap_or_else(|error| panic!("suspended low-reserve recovery validation failed: {error}"))
        .commit(&mut loaded)
        .unwrap_or_else(|error| panic!("suspended low-reserve recovery commit failed: {error}"));
    let blocked = advance_tick(&registries, &mut loaded).unwrap_or_else(|error| {
        panic!("suspended low-reserve blocked-resume tick failed: {error}")
    });
    assert!(matches!(
        blocked.production_availability_changes(),
        [ProductionAvailabilityChange::SuspensionReasonChanged {
            job: changed_job,
            previous: ProductionSuspensionReason::OutputSupportUnavailable { stockpile },
            reason: ProductionSuspensionReason::PlayerLaborUnavailable,
        }] if *changed_job == job && *stockpile == destination
    ));
    assert_eq!(loaded.player_work().active(), None);
    assert!(
        loaded
            .production()
            .get_job(job)
            .is_some_and(|record| record.is_suspended())
    );
    assert_eq!(validate_loaded_state(&registries, &loaded), Ok(()));
    let round_trip =
        serde_json::to_vec(&SaveEnvelope::new(&registries, &loaded)).unwrap_or_else(|error| {
            panic!("suspended low-reserve round-trip serialization failed: {error}")
        });
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&round_trip)
        .unwrap_or_else(|error| panic!("suspended low-reserve round-trip decode failed: {error}"));
    decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("suspended low-reserve round-trip load failed: {error}"));
}

#[test]
fn stale_manual_craft_token_reports_labor_revision_conflict_after_prior_work_finishes() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    let first = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("first manual craft validation failed: {error}"));
    let stale = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("stale manual craft validation failed: {error}"));
    first
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("first manual craft commit failed: {error}"));
    for _ in 0..40 {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("manual craft completion tick failed: {error}"));
    }

    let error = stale
        .commit(&mut state)
        .err()
        .unwrap_or_else(|| panic!("stale manual craft token unexpectedly committed"));

    assert_eq!(
        error,
        ManualCraftCommitError::Work(PlayerWorkCommitError::StaleRevision {
            expected: 0,
            actual: 2,
        })
    );
    assert_eq!(state.player_work().active(), None);
}

#[test]
fn manual_craft_load_audit_rejects_forged_duration() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    let token = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("manual craft tamper start failed: {error}"));
    let job = token
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("manual craft tamper commit failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("manual craft tamper serialization failed: {error}"));
    encoded["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["active_duration"] =
        serde_json::json!(41_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("manual craft tamper decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::CraftingJob(
            CraftingJobValidationError::DurationMismatch {
                job,
                stored: TickSpan::new(41),
                required: TickSpan::new(40),
            }
        )))
    );

    let mut coordinated = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| {
            panic!("manual craft coordinated tamper serialization failed: {error}")
        });
    coordinated["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["active_duration"] =
        serde_json::json!(39_u64);
    coordinated["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["completes_at"] =
        serde_json::json!(39_u64);
    let coordinated: LoadedSaveEnvelope = serde_json::from_value(coordinated)
        .unwrap_or_else(|error| panic!("manual craft coordinated tamper decode failed: {error}"));

    assert_eq!(
        coordinated.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::CraftingJob(
            CraftingJobValidationError::DurationMismatch {
                job,
                stored: TickSpan::new(39),
                required: TickSpan::new(40),
            }
        )))
    );
}

#[test]
fn in_progress_timber_chest_joinery_round_trip_preserves_deterministic_continuation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xC4AF_7016));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("timber chest joinery survival setup failed: {error}"));
    let chest_mass = Mass::from_milligrams(2_400_000);
    let source = add_solid_stockpile_for_test(&mut state, chest_mass)
        .unwrap_or_else(|error| panic!("timber chest joinery source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, chest_mass)
        .unwrap_or_else(|error| panic!("timber chest joinery destination failed: {error}"));
    let boards = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        chest_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("timber chest joinery board fixture failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("timber chest joinery initial matter audit failed: {error}"))
        .total();
    let job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_ASSEMBLE_TIMBER_CHEST,
            source,
            MaterialLotSelection::new(boards, chest_mass),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("timber chest joinery start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("timber chest joinery start commit failed: {error}"));
    assert_eq!(
        state
            .production()
            .get_job(job)
            .map(|record| record.active_duration()),
        Some(TickSpan::new(80))
    );
    for _ in 0..20 {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("timber chest joinery pre-save tick failed: {error}"));
    }

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("timber chest joinery serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("timber chest joinery decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("timber chest joinery trusted load failed: {error}"));
    assert_eq!(loaded, state);

    for _ in 20..80 {
        let expected = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("timber chest joinery source tick failed: {error}"));
        let actual = advance_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("timber chest joinery loaded tick failed: {error}"));
        assert_eq!(actual, expected);
    }
    assert_eq!(loaded, state);
    assert_eq!(state.player_work().active(), None);
    assert!(state.production().get_job(job).is_none());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))),
        Some(Mass::ZERO)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY))
            }),
        Some(chest_mass)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!(
                "timber chest joinery final matter audit failed: {error}"
            ))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("timber chest joinery final state audit failed: {error}"));
}

#[test]
fn double_wall_chest_joinery_round_trip_preserves_full_cost_and_output() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xC4AF_7017));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("double-wall chest survival setup failed: {error}"));
    let body_mass = Mass::from_milligrams(4_000_000);
    let source = add_solid_stockpile_for_test(&mut state, body_mass)
        .unwrap_or_else(|error| panic!("double-wall chest source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, body_mass)
        .unwrap_or_else(|error| panic!("double-wall chest destination failed: {error}"));
    let boards = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        body_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("double-wall chest board fixture failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("double-wall chest matter setup failed: {error}"))
        .total();
    let job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_ASSEMBLE_DOUBLE_WALL_TIMBER_CHEST,
            source,
            MaterialLotSelection::new(boards, body_mass),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("double-wall chest joinery start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("double-wall chest joinery commit failed: {error}"));
    assert_eq!(
        state
            .production()
            .get_job(job)
            .map(|record| record.active_duration()),
        Some(TickSpan::new(120))
    );
    for _ in 0..30 {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("double-wall chest pre-save tick failed: {error}"));
    }
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("double-wall chest serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("double-wall chest decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("double-wall chest trusted load failed: {error}"));
    assert_eq!(loaded, state);

    for _ in 30..120 {
        let expected = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("double-wall chest source tick failed: {error}"));
        let actual = advance_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("double-wall chest loaded tick failed: {error}"));
        assert_eq!(actual, expected);
    }
    assert_eq!(loaded, state);
    assert_eq!(state.player_work().active(), None);
    assert!(state.production().get_job(job).is_none());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))),
        Some(Mass::ZERO)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(
                    MATERIAL_WOOD,
                    FORM_DOUBLE_WALL_CHEST_BODY,
                ))
            }),
        Some(body_mass)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("double-wall chest matter final failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn repeated_manual_craft_batches_share_one_labor_job_without_discounting_work() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    let merged_lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        stone_lump(),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("batch craft second stone fixture failed: {error}"));
    assert_eq!(
        merged_lot, lot,
        "identical manual-craft input must retain one merged persistent lot identity"
    );
    let craft = ManualCraftRequest::single(
        PROCESS_KNAP_STONE_TOOL,
        source,
        MaterialLotSelection::new(lot, Mass::from_milligrams(2_000_000)),
    );
    let resolution = resolve_manual_craft(&registries, &state, &craft)
        .unwrap_or_else(|error| panic!("batch craft resolution failed: {error}"));

    assert_eq!(resolution.input_mass(), Mass::from_milligrams(2_000_000));
    assert_eq!(resolution.duration(), TickSpan::new(80));
    assert_eq!(
        resolution
            .outputs()
            .iter()
            .map(|output| (output.commodity(), output.mass()))
            .collect::<Vec<_>>(),
        vec![
            (
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(1_600_000),
            ),
            (
                CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                Mass::from_milligrams(400_000),
            ),
        ]
    );

    let token = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::new(craft, destination),
    )
    .unwrap_or_else(|error| panic!("batch craft start failed: {error}"));
    let job = token
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("batch craft commit failed: {error}"));
    assert_eq!(
        state
            .production()
            .get_job(job)
            .map(|record| record.active_duration()),
        Some(TickSpan::new(80))
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("batch craft running audit failed: {error}"));
}
