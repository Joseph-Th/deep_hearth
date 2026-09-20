//! Support-driven suspension, attention release, and deterministic resume contracts.

use super::*;

#[test]
fn manual_craft_output_support_failure_pauses_work_and_exertion_until_recovered() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    let support = active_stockpile_support(&registries, &mut state);
    let _ = validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap_or_else(|error| panic!("manual craft destination mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("manual craft destination mount commit failed: {error}"));
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
    .unwrap_or_else(|error| panic!("supported manual craft start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("supported manual craft start commit failed: {error}"));

    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("supported manual craft active tick failed: {error}"));
    let before_pause = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("manual craft survival state disappeared before suspension"));
    let _ = validate_set_structural_load(
        &registries,
        &state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    )
    .unwrap_or_else(|error| panic!("manual craft support failure validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("manual craft support failure commit failed: {error}"));
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Failed)
    );

    let paused = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("manual craft suspension tick failed: {error}"));
    assert!(matches!(
        paused.production_availability_changes(),
        [ProductionAvailabilityChange::Suspended {
            job: suspended_job,
            reason: ProductionSuspensionReason::OutputSupportUnavailable { stockpile },
            ..
        }] if *suspended_job == job && *stockpile == destination
    ));
    assert_eq!(state.player_work().active(), None);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let after_pause = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("manual craft survival state disappeared after suspension"));
    let physiology = registries.survival().physiology();
    assert_eq!(
        before_pause
            .metabolic_energy()
            .checked_sub(after_pause.metabolic_energy()),
        Some(physiology.basal_energy_cost_per_tick())
    );
    assert_eq!(
        before_pause
            .hydration()
            .checked_sub(after_pause.hydration()),
        Some(physiology.hydration_loss_per_tick())
    );

    let _ = validate_unmount_stockpile(&registries, &state, destination)
        .unwrap_or_else(|error| {
            panic!("suspended manual craft destination unmount failed: {error}")
        })
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("suspended manual craft destination unmount commit failed: {error}")
        });
    let before_resume = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("manual craft survival state disappeared before resume"));
    let resumed = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("manual craft resume tick failed: {error}"));
    assert!(matches!(
        resumed.production_availability_changes(),
        [ProductionAvailabilityChange::Resumed {
            job: resumed_job,
            reason: ProductionSuspensionReason::OutputSupportUnavailable { stockpile },
            ..
        }] if *resumed_job == job && *stockpile == destination
    ));
    let after_resume = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("manual craft survival state disappeared after resume"));
    let exertion = registries
        .crafting()
        .get_manual(PROCESS_KNAP_STONE_TOOL)
        .unwrap_or_else(|| panic!("manual craft definition disappeared"))
        .exertion();
    assert_eq!(
        before_resume
            .metabolic_energy()
            .checked_sub(after_resume.metabolic_energy()),
        physiology
            .basal_energy_cost_per_tick()
            .checked_add(exertion.energy_cost_per_tick())
    );
    assert_eq!(
        before_resume
            .hydration()
            .checked_sub(after_resume.hydration()),
        physiology
            .hydration_loss_per_tick()
            .checked_add(exertion.hydration_loss_per_tick())
    );
    assert_eq!(
        state.player_work().active(),
        Some(PlayerWork::ManualProduction { job })
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn suspended_manual_craft_releases_attention_and_waits_while_other_player_work_runs() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    let support = active_stockpile_support(&registries, &mut state);
    let _ = validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap_or_else(|error| {
            panic!("manual craft parallel-work destination mount failed: {error}")
        })
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("manual craft parallel-work destination mount commit failed: {error}")
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
    .unwrap_or_else(|error| panic!("manual craft parallel-work start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("manual craft parallel-work start commit failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("manual craft parallel-work active tick failed: {error}"));
    let _ = validate_set_structural_load(
        &registries,
        &state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    )
    .unwrap_or_else(|error| panic!("manual craft parallel-work support failure failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| {
        panic!("manual craft parallel-work support failure commit failed: {error}")
    });
    let _ = advance_tick(&registries, &mut state).unwrap_or_else(|error| {
        panic!("manual craft parallel-work suspension tick failed: {error}")
    });
    assert_eq!(state.player_work().active(), None);

    let region = VoxelBounds::new(VoxelCoord::new(10, 0, 0), VoxelCoord::new(11, 1, 1))
        .unwrap_or_else(|error| {
            panic!("manual craft parallel-work prospecting bounds failed: {error}")
        });
    let prospecting = validate_start_field_prospecting(
        &registries,
        &state,
        FieldProspectingRequest::new(PROSPECTING_FIELD_INSPECTION, region, MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("manual craft parallel-work prospecting start failed: {error}"));
    let prospecting_work = prospecting.work();
    prospecting.commit(&mut state).unwrap_or_else(|error| {
        panic!("manual craft parallel-work prospecting commit failed: {error}")
    });
    assert!(matches!(
        state.player_work().active(),
        Some(PlayerWork::Prospecting { .. })
    ));

    let _ = validate_unmount_stockpile(&registries, &state, destination)
        .unwrap_or_else(|error| {
            panic!("manual craft parallel-work recovery validation failed: {error}")
        })
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("manual craft parallel-work recovery commit failed: {error}")
        });
    let blocked = advance_tick(&registries, &mut state).unwrap_or_else(|error| {
        panic!("manual craft parallel-work blocked-resume tick failed: {error}")
    });
    assert!(matches!(
        blocked.production_availability_changes(),
        [ProductionAvailabilityChange::SuspensionReasonChanged {
            job: changed_job,
            previous: ProductionSuspensionReason::OutputSupportUnavailable { stockpile },
            reason: ProductionSuspensionReason::PlayerLaborUnavailable,
        }] if *changed_job == job && *stockpile == destination
    ));
    assert!(matches!(
        state.player_work().active(),
        Some(PlayerWork::Prospecting { .. })
    ));

    while state.tick() < prospecting_work.completes_at() {
        let _ = advance_tick(&registries, &mut state).unwrap_or_else(|error| {
            panic!("manual craft parallel-work prospecting tick failed: {error}")
        });
    }
    assert_eq!(state.player_work().active(), None);

    let resumed = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("manual craft parallel-work resume tick failed: {error}"));
    assert!(matches!(
        resumed.production_availability_changes(),
        [ProductionAvailabilityChange::Resumed {
            job: resumed_job,
            reason: ProductionSuspensionReason::PlayerLaborUnavailable,
            ..
        }] if *resumed_job == job
    ));
    assert_eq!(
        state.player_work().active(),
        Some(PlayerWork::ManualProduction { job })
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn simultaneously_recoverable_manual_crafts_resume_one_at_a_time_in_job_order() {
    let (registries, mut state, source, first_lot, first_destination) = make_fixture();
    let second_lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        stone_lump(),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("second manual craft input failed: {error}"));
    let second_destination =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000))
            .unwrap_or_else(|error| panic!("second manual craft destination failed: {error}"));
    let first_support = active_stockpile_support_at(&registries, &mut state, 0);
    let second_support = active_stockpile_support_at(&registries, &mut state, 10);
    for (destination, support) in [
        (first_destination, first_support),
        (second_destination, second_support),
    ] {
        let _ = validate_mount_stockpile(&registries, &state, destination, support)
            .unwrap_or_else(|error| panic!("manual craft destination mount failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("manual craft destination mount commit failed: {error}")
            });
    }

    let first = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(first_lot, Mass::from_milligrams(1_000_000)),
            first_destination,
        ),
    )
    .unwrap_or_else(|error| panic!("first serial-resume craft start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("first serial-resume craft commit failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("first serial-resume active tick failed: {error}"));
    let _ = validate_set_structural_load(
        &registries,
        &state,
        first_support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    )
    .unwrap_or_else(|error| panic!("first serial-resume support failure failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("first serial-resume support failure commit failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("first serial-resume suspension tick failed: {error}"));
    assert_eq!(state.player_work().active(), None);

    let second = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(second_lot, Mass::from_milligrams(1_000_000)),
            second_destination,
        ),
    )
    .unwrap_or_else(|error| panic!("second serial-resume craft start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("second serial-resume craft commit failed: {error}"));
    let _ = validate_set_structural_load(
        &registries,
        &state,
        second_support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    )
    .unwrap_or_else(|error| panic!("second serial-resume support failure failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("second serial-resume support failure commit failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("second serial-resume suspension tick failed: {error}"));
    assert_eq!(state.player_work().active(), None);

    for destination in [first_destination, second_destination] {
        let _ = validate_unmount_stockpile(&registries, &state, destination)
            .unwrap_or_else(|error| panic!("serial-resume destination recovery failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("serial-resume destination recovery commit failed: {error}")
            });
    }

    let first_resume_at = state.tick();
    let first_remaining = state
        .production()
        .get_job(first)
        .and_then(|job| job.suspension())
        .map(|suspension| suspension.remaining_active_time())
        .unwrap_or_else(|| panic!("first serial-resume job was not suspended before recovery"));
    let first_due = first_resume_at
        .checked_add_span(first_remaining)
        .unwrap_or_else(|| panic!("first serial-resume completion tick overflowed"));
    let recovered = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("serial-resume arbitration tick failed: {error}"));
    assert_eq!(
        recovered.production_availability_changes(),
        &[
            ProductionAvailabilityChange::Resumed {
                job: first,
                reason: ProductionSuspensionReason::OutputSupportUnavailable {
                    stockpile: first_destination,
                },
                resumed_at: first_resume_at,
                scheduled_completion: first_due,
            },
            ProductionAvailabilityChange::SuspensionReasonChanged {
                job: second,
                previous: ProductionSuspensionReason::OutputSupportUnavailable {
                    stockpile: second_destination,
                },
                reason: ProductionSuspensionReason::PlayerLaborUnavailable,
            },
        ]
    );
    assert_eq!(
        state.player_work().active(),
        Some(PlayerWork::ManualProduction { job: first })
    );
    assert_eq!(
        state
            .production()
            .get_job(second)
            .and_then(|job| job.suspension())
            .map(|suspension| suspension.reason()),
        Some(ProductionSuspensionReason::PlayerLaborUnavailable)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    while state.production().get_job(first).is_some() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("first serial-resume completion failed: {error}"));
    }
    assert_eq!(state.player_work().active(), None);
    let second_resume = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("second serial-resume arbitration failed: {error}"));
    assert!(matches!(
        second_resume.production_availability_changes(),
        [ProductionAvailabilityChange::Resumed {
            job,
            reason: ProductionSuspensionReason::PlayerLaborUnavailable,
            ..
        }] if *job == second
    ));
    assert_eq!(
        state.player_work().active(),
        Some(PlayerWork::ManualProduction { job: second })
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn one_tick_manual_craft_resume_completes_without_leaking_player_work() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    let support = active_stockpile_support(&registries, &mut state);
    let _ = validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap_or_else(|error| panic!("one-tick resume destination mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("one-tick resume destination mount commit failed: {error}"));
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
    .unwrap_or_else(|error| panic!("one-tick resume craft start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("one-tick resume craft start commit failed: {error}"));
    for _ in 0..39 {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("one-tick resume active craft tick failed: {error}"));
    }
    let _ = validate_set_structural_load(
        &registries,
        &state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    )
    .unwrap_or_else(|error| panic!("one-tick resume support failure validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("one-tick resume support failure commit failed: {error}"));
    let paused = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("one-tick resume suspension tick failed: {error}"));
    assert!(matches!(
        paused.production_availability_changes(),
        [ProductionAvailabilityChange::Suspended {
            job: suspended_job,
            remaining_active_time,
            ..
        }] if *suspended_job == job && *remaining_active_time == TickSpan::new(1)
    ));
    assert_eq!(state.player_work().active(), None);

    let _ = validate_unmount_stockpile(&registries, &state, destination)
        .unwrap_or_else(|error| panic!("one-tick resume recovery validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("one-tick resume recovery commit failed: {error}"));
    let before_resume = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("one-tick resume survival state disappeared"));
    let completed = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("one-tick resume completion tick failed: {error}"));
    assert!(matches!(
        completed.production_availability_changes(),
        [ProductionAvailabilityChange::Resumed {
            job: resumed_job,
            scheduled_completion,
            ..
        }] if *resumed_job == job && *scheduled_completion == state.tick()
    ));
    assert_eq!(
        completed
            .production_completions()
            .iter()
            .map(|completion| completion.job())
            .collect::<Vec<_>>(),
        vec![job]
    );
    assert!(state.production().get_job(job).is_none());
    assert_eq!(state.player_work().active(), None);
    let after_resume = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("one-tick resume post-completion survival state disappeared"));
    let physiology = registries.survival().physiology();
    let exertion = registries
        .crafting()
        .get_manual(PROCESS_KNAP_STONE_TOOL)
        .unwrap_or_else(|| panic!("one-tick resume craft definition disappeared"))
        .exertion();
    assert_eq!(
        before_resume
            .metabolic_energy()
            .checked_sub(after_resume.metabolic_energy()),
        physiology
            .basal_energy_cost_per_tick()
            .checked_add(exertion.energy_cost_per_tick())
    );
    assert_eq!(
        before_resume
            .hydration()
            .checked_sub(after_resume.hydration()),
        physiology
            .hydration_loss_per_tick()
            .checked_add(exertion.hydration_loss_per_tick())
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
