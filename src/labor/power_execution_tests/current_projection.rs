//! Current-state manual-power envelope contracts.

use super::*;
use crate::labor::{
    ManualPowerDestinationTargetAssessment, ManualPowerDestinationTargetBlocker,
    ManualPowerDestinationTargetRequest, ManualPowerEnergyEnvelopeRequest,
    assess_manual_power_destination_target, assess_manual_power_energy_envelope,
};

#[test]
fn envelope_handles_larger_charge_becoming_feasible_after_passive_sink_recovery() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("manual-power envelope survival setup failed: {error}"));
    let crank = assemble_crank_fixture(&registries, &mut state, EQUIPMENT_STONE_HAND_CRANK, false);
    let setup_crank = assemble_crank_fixture(
        &registries,
        &mut state,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        true,
    );
    let drive = assemble_flywheel_fixture(&registries, &mut state);
    let initial = Energy::from_nanojoules(323_000_000_000);
    validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, setup_crank, drive, initial),
    )
    .unwrap_or_else(|error| panic!("manual-power envelope setup charge failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("manual-power envelope setup charge commit failed: {error}"));
    advance_exact(&registries, &mut state, 1);
    assert_eq!(
        state
            .energy()
            .get_store(drive)
            .map(EnergyStoreRecord::stored),
        Some(initial)
    );

    let shorter = Energy::from_nanojoules(178_000_000_000);
    assert!(matches!(
        validate_start_manual_power(
            &registries,
            &state,
            ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, shorter),
        ),
        Err(ManualPowerError::EnergySink(
            EnergySinkError::InsufficientCapacity { .. }
        ))
    ));

    let longer = Energy::from_nanojoules(180_500_000_000);
    let exact = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, longer),
    )
    .unwrap_or_else(|error| panic!("longer passive-recovery charge should be feasible: {error}"));
    assert_eq!(
        exact
            .work()
            .completes_at()
            .checked_duration_since(exact.work().started_at()),
        Some(TickSpan::new(2))
    );

    let envelope = assess_manual_power_energy_envelope(
        &registries,
        &state,
        ManualPowerEnergyEnvelopeRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, longer),
    )
    .unwrap_or_else(|error| panic!("manual-power current envelope failed: {error}"));
    assert_eq!(envelope.requested_limit(), longer);
    assert_eq!(envelope.maximum_energy(), longer);
    assert_eq!(
        envelope.maximum_destination_energy(),
        Energy::from_nanojoules(496_400_000_000)
    );
    assert_eq!(
        envelope.energy_for_maximum_destination(),
        Energy::from_nanojoules(177_000_000_000)
    );

    let target = Energy::from_nanojoules(480_000_000_000);
    let target_projection = assess_manual_power_destination_target(
        &registries,
        &state,
        ManualPowerDestinationTargetRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, target),
    )
    .unwrap_or_else(|error| panic!("manual-power destination target failed: {error}"));
    let ManualPowerDestinationTargetAssessment::Feasible(target_projection) = target_projection
    else {
        panic!("480 J destination target should be reachable")
    };
    assert_eq!(
        target_projection.generated_energy(),
        Energy::from_nanojoules(160_600_000_000),
        "target charge must replace the 3.6 J dissipated during its one active tick"
    );
    assert_eq!(target_projection.destination_energy_after(), target);

    let target_start = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            crank,
            drive,
            target_projection.generated_energy(),
        ),
    )
    .unwrap_or_else(|error| panic!("projected destination target failed admission: {error}"));
    target_start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("projected destination target failed commit: {error}"));
    advance_exact(&registries, &mut state, 1);
    assert_eq!(
        state
            .energy()
            .get_store(drive)
            .map(EnergyStoreRecord::stored),
        Some(target)
    );
}

#[test]
fn destination_target_reports_capacity_ordering_that_prevents_a_full_post_tick_store() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("manual-power target survival setup failed: {error}"));
    let crank = assemble_crank_fixture(&registries, &mut state, EQUIPMENT_STONE_HAND_CRANK, false);
    let setup_crank = assemble_crank_fixture(
        &registries,
        &mut state,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        true,
    );
    let drive = assemble_flywheel_fixture(&registries, &mut state);
    validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            setup_crank,
            drive,
            Energy::from_nanojoules(323_000_000_000),
        ),
    )
    .unwrap_or_else(|error| panic!("manual-power target setup charge failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("manual-power target setup commit failed: {error}"));
    advance_exact(&registries, &mut state, 1);

    assert_eq!(
        assess_manual_power_destination_target(
            &registries,
            &state,
            ManualPowerDestinationTargetRequest::new(
                MANUAL_POWER_HAND_CRANK,
                crank,
                drive,
                Energy::from_nanojoules(500_000_000_000),
            ),
        )
        .unwrap_or_else(|error| panic!("manual-power full-store target failed: {error}")),
        ManualPowerDestinationTargetAssessment::Blocked(
            ManualPowerDestinationTargetBlocker::DestinationCapacity
        )
    );
}

#[test]
fn envelope_respects_requested_survival_floor_without_mutating_state() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state).unwrap_or_else(|error| {
        panic!("manual-power reserve-floor survival setup failed: {error}")
    });
    let crank = assemble_crank_fixture(&registries, &mut state, EQUIPMENT_STONE_HAND_CRANK, false);
    let drive = assemble_flywheel_fixture(&registries, &mut state);
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("manual-power reserve-floor player disappeared"));
    let requested = Energy::from_nanojoules(1_000_000_000);
    let _ = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested),
    )
    .unwrap_or_else(|error| {
        panic!("unconstrained manual-power request should be feasible: {error}")
    });
    let before = state.clone();

    let projected = assess_manual_power_energy_envelope(
        &registries,
        &state,
        ManualPowerEnergyEnvelopeRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested)
            .with_minimum_reserves(player.metabolic_energy(), player.hydration()),
    )
    .unwrap_or_else(|error| panic!("manual-power reserve-floor envelope failed: {error}"));

    assert_eq!(projected.maximum_energy(), Energy::ZERO);
    assert_eq!(state, before, "manual-power planning must remain read-only");
}

#[test]
fn envelope_selection_requires_fresh_canonical_admission() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state).unwrap_or_else(|error| {
        panic!("manual-power stale-envelope survival setup failed: {error}")
    });
    let crank = assemble_crank_fixture(&registries, &mut state, EQUIPMENT_STONE_HAND_CRANK, false);
    let drive = assemble_flywheel_fixture(&registries, &mut state);
    let requested = Energy::from_nanojoules(1_000_000_000);
    let projected = assess_manual_power_energy_envelope(
        &registries,
        &state,
        ManualPowerEnergyEnvelopeRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested),
    )
    .unwrap_or_else(|error| panic!("manual-power stale-envelope projection failed: {error}"));
    assert_eq!(projected.maximum_energy(), requested);

    validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested),
    )
    .unwrap_or_else(|error| panic!("manual-power stale-envelope setup start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("manual-power stale-envelope setup commit failed: {error}"));

    assert!(matches!(
        validate_start_manual_power(
            &registries,
            &state,
            ManualPowerRequest::new(
                MANUAL_POWER_HAND_CRANK,
                crank,
                drive,
                projected.maximum_energy(),
            ),
        ),
        Err(ManualPowerError::EnergySink(
            EnergySinkError::StoreBusyManualPower { store }
        )) if store == drive
    ));
}
