//! Tests for the sibling mod module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{
    FORM_CHIP, FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL, FORM_ORE, FORM_REINFORCEMENT, FORM_TOOL,
    MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD, PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
    PROCESS_KNAP_STONE_TOOL, PROSPECTING_FIELD_INSPECTION, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
    build_registries,
};
use crate::core::quantity::{Area, Energy, Force, Length, Temperature};
use crate::core::state::{StateValidationError, validate_loaded_state};
use crate::core::time::WorldSeed;
use crate::geology::{FieldProspectingRequest, validate_start_field_prospecting};
use crate::inventory::{
    add_solid_stockpile_for_test, deposit_composed_lot_for_test, deposit_lot_for_test,
    validate_mount_stockpile, validate_unmount_stockpile,
};
use crate::labor::{PlayerWorkValidationError, calculate_player_work_resource_budget};
use crate::material::{CompositionComponent, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::production::{
    ProcessInputError, ProductionAvailabilityChange, ProductionSuspensionReason, StartProcessError,
    validate_start_process,
};
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralElementId, StructuralLifecycle, StructuralLoadKind, add_structural_element,
    materialize_structural_element_for_test, validate_activate_structural_element,
    validate_set_structural_load,
};
use crate::survival::{assess_survival, initialize_player_survival};

fn stone_lump() -> CommodityKey {
    CommodityKey::new(MATERIAL_STONE, FORM_LUMP)
}

#[test]
fn native_copper_reinforcement_rejects_ordinary_ore_form_without_inventing_separation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xC4AF_7009));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("native copper survival setup failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20_000))
        .unwrap_or_else(|error| panic!("native copper source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20_000))
        .unwrap_or_else(|error| panic!("native copper destination failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        Mass::from_milligrams(20_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("ordinary copper ore fixture failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_start_manual_craft(
            &registries,
            &state,
            ManualCraftStartRequest::single(
                PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
                source,
                destination,
            ),
        )
        .err(),
        Some(StartManualCraftError::Resolution(ManualCraftError::Input(
            ProcessInputError::InsufficientMass {
                stockpile: source,
                commodity: CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
                available: Mass::ZERO,
                requested: Mass::from_milligrams(20_000),
            }
        )))
    );
    assert_eq!(state, before);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT))
            }),
        Some(Mass::ZERO)
    );
}

#[test]
fn native_copper_reinforcement_rejects_contaminated_native_metal() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xC4AF_7010));
    initialize_player_survival(&registries, &mut state).unwrap_or_else(|error| {
        panic!("contaminated native copper survival setup failed: {error}")
    });
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20_000))
        .unwrap_or_else(|error| panic!("contaminated native copper source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20_000))
        .unwrap_or_else(|error| panic!("contaminated native copper destination failed: {error}"));
    let mixed = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 900_000),
        CompositionComponent::new(MATERIAL_STONE, 100_000),
    ])
    .unwrap_or_else(|error| panic!("contaminated native copper composition failed: {error}"));
    deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        Mass::from_milligrams(20_000),
        Temperature::from_millikelvin(293_150),
        mixed,
    )
    .unwrap_or_else(|error| panic!("contaminated native copper fixture failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_start_manual_craft(
            &registries,
            &state,
            ManualCraftStartRequest::single(
                PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
                source,
                destination,
            ),
        )
        .err(),
        Some(StartManualCraftError::Resolution(
            ManualCraftError::UnsupportedComposition
        ))
    );
    assert_eq!(state, before);
}

fn make_fixture() -> (Registries, AppState, StockpileId, StockpileId) {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xC4AF_7001));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("manual craft survival initialization failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000))
        .unwrap_or_else(|error| panic!("manual craft source fixture failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000))
        .unwrap_or_else(|error| panic!("manual craft destination fixture failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        stone_lump(),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("manual craft stone fixture failed: {error}"));
    (registries, state, source, destination)
}

fn active_stockpile_support(registries: &Registries, state: &mut AppState) -> StructuralElementId {
    let bounds = VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(1, 1, 1))
        .unwrap_or_else(|error| panic!("manual craft support bounds failed: {error}"));
    let support = add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            bounds,
            Length::from_micrometers(1),
            Area::from_square_millimeters(1_000),
        ),
        true,
    )
    .unwrap_or_else(|error| panic!("manual craft support allocation failed: {error}"));
    materialize_structural_element_for_test(registries, state, support, FORM_LOG);
    validate_activate_structural_element(registries, state, support)
        .unwrap_or_else(|error| panic!("manual craft support activation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("manual craft support activation commit failed: {error}"));
    support
}

#[test]
fn manual_craft_output_support_failure_pauses_work_and_exertion_until_recovered() {
    let (registries, mut state, source, destination) = make_fixture();
    let support = active_stockpile_support(&registries, &mut state);
    validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap_or_else(|error| panic!("manual craft destination mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("manual craft destination mount commit failed: {error}"));
    let job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(PROCESS_KNAP_STONE_TOOL, source, destination),
    )
    .unwrap_or_else(|error| panic!("supported manual craft start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("supported manual craft start commit failed: {error}"));

    advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("supported manual craft active tick failed: {error}"));
    let before_pause = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("manual craft survival state disappeared before suspension"));
    validate_set_structural_load(
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

    validate_unmount_stockpile(&registries, &state, destination)
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
        Some(PlayerWork::ManualCraft { job })
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn suspended_manual_craft_releases_attention_and_waits_while_other_player_work_runs() {
    let (registries, mut state, source, destination) = make_fixture();
    let support = active_stockpile_support(&registries, &mut state);
    validate_mount_stockpile(&registries, &state, destination, support)
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
        ManualCraftStartRequest::single(PROCESS_KNAP_STONE_TOOL, source, destination),
    )
    .unwrap_or_else(|error| panic!("manual craft parallel-work start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("manual craft parallel-work start commit failed: {error}"));
    advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("manual craft parallel-work active tick failed: {error}"));
    validate_set_structural_load(
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
    advance_tick(&registries, &mut state).unwrap_or_else(|error| {
        panic!("manual craft parallel-work suspension tick failed: {error}")
    });
    assert_eq!(state.player_work().active(), None);

    let region = VoxelBounds::new(VoxelCoord::new(10, 0, 0), VoxelCoord::new(11, 1, 1))
        .unwrap_or_else(|error| {
            panic!("manual craft parallel-work prospecting bounds failed: {error}")
        });
    validate_start_field_prospecting(
        &registries,
        &state,
        FieldProspectingRequest::new(PROSPECTING_FIELD_INSPECTION, region, MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("manual craft parallel-work prospecting start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| {
        panic!("manual craft parallel-work prospecting commit failed: {error}")
    });
    assert!(matches!(
        state.player_work().active(),
        Some(PlayerWork::Prospecting { .. })
    ));

    validate_unmount_stockpile(&registries, &state, destination)
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

    let prospecting_duration = registries
        .labor()
        .get_prospecting(PROSPECTING_FIELD_INSPECTION)
        .unwrap_or_else(|| panic!("manual craft parallel-work prospecting definition disappeared"))
        .duration()
        .value();
    for _ in 1..prospecting_duration {
        advance_tick(&registries, &mut state).unwrap_or_else(|error| {
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
        Some(PlayerWork::ManualCraft { job })
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn one_tick_manual_craft_resume_completes_without_leaking_player_work() {
    let (registries, mut state, source, destination) = make_fixture();
    let support = active_stockpile_support(&registries, &mut state);
    validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap_or_else(|error| panic!("one-tick resume destination mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("one-tick resume destination mount commit failed: {error}"));
    let job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(PROCESS_KNAP_STONE_TOOL, source, destination),
    )
    .unwrap_or_else(|error| panic!("one-tick resume craft start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("one-tick resume craft start commit failed: {error}"));
    for _ in 0..39 {
        advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("one-tick resume active craft tick failed: {error}"));
    }
    validate_set_structural_load(
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

    validate_unmount_stockpile(&registries, &state, destination)
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

#[test]
fn stone_knapping_is_timed_conserved_hand_work() {
    let (registries, mut state, source, destination) = make_fixture();
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("manual craft initial accounting failed: {error}"));
    let survival_before = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("manual craft survival state is missing"));
    let resolution = resolve_manual_craft(
        &registries,
        &state,
        ManualCraftRequest::single(PROCESS_KNAP_STONE_TOOL, source),
    )
    .unwrap_or_else(|error| panic!("stone knapping resolution failed: {error}"));
    assert_eq!(resolution.duration(), TickSpan::new(40));
    assert_eq!(
        validate_start_process(&registries, &state, &resolution, source, destination),
        Err(StartProcessError::ManualCraftRequiresPlayerWork {
            process: PROCESS_KNAP_STONE_TOOL,
        })
    );
    let token = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(PROCESS_KNAP_STONE_TOOL, source, destination),
    )
    .unwrap_or_else(|error| panic!("stone knapping start failed: {error}"));
    token
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stone knapping commit failed: {error}"));
    assert!(matches!(
        state.player_work().active(),
        Some(PlayerWork::ManualCraft { .. })
    ));

    for _ in 0..resolution.duration().value() {
        advance_tick(&registries, &mut state)
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
    assert!(
        assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("manual craft survival state disappeared"))
            .metabolic_energy()
            < survival_before.metabolic_energy()
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("stone knapping final audit failed: {error}"));
}

#[test]
fn manual_craft_requires_enough_metabolic_reserve_to_finish() {
    let (registries, state, source, destination) = make_fixture();
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
            ManualCraftStartRequest::single(PROCESS_KNAP_STONE_TOOL, source, destination),
        ),
        Err(StartManualCraftError::Work(
            PlayerWorkStartError::InsufficientMetabolicEnergy { .. }
        ))
    ));
    assert_eq!(low_reserve, before);
}

#[test]
fn manual_craft_commit_rejects_intervening_survival_change() {
    let (registries, mut state, source, destination) = make_fixture();
    let token = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(PROCESS_KNAP_STONE_TOOL, source, destination),
    )
    .unwrap_or_else(|error| panic!("manual craft survival-stale validation failed: {error}"));
    let expected = state.survival().revision();
    advance_tick(&registries, &mut state)
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
    let (registries, mut state, source, destination) = make_fixture();
    let token = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(PROCESS_KNAP_STONE_TOOL, source, destination),
    )
    .unwrap_or_else(|error| panic!("manual craft save reserve start failed: {error}"));
    let job = token
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("manual craft save reserve commit failed: {error}"));
    let record = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("manual craft save reserve job disappeared"));
    let remaining = TickSpan::new(record.completes_at().value() - state.tick().value());
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
fn suspended_manual_craft_loads_with_depleted_reserves_and_does_not_resume_unsafely() {
    let (registries, mut state, source, destination) = make_fixture();
    let support = active_stockpile_support(&registries, &mut state);
    validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap_or_else(|error| panic!("suspended low-reserve destination mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("suspended low-reserve destination mount commit failed: {error}")
        });
    let job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(PROCESS_KNAP_STONE_TOOL, source, destination),
    )
    .unwrap_or_else(|error| panic!("suspended low-reserve craft start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("suspended low-reserve craft start commit failed: {error}"));
    advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("suspended low-reserve active craft tick failed: {error}"));
    validate_set_structural_load(
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
    advance_tick(&registries, &mut state)
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

    validate_unmount_stockpile(&registries, &loaded, destination)
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
    let (registries, mut state, source, destination) = make_fixture();
    let first = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(PROCESS_KNAP_STONE_TOOL, source, destination),
    )
    .unwrap_or_else(|error| panic!("first manual craft validation failed: {error}"));
    let stale = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(PROCESS_KNAP_STONE_TOOL, source, destination),
    )
    .unwrap_or_else(|error| panic!("stale manual craft validation failed: {error}"));
    first
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("first manual craft commit failed: {error}"));
    for _ in 0..40 {
        advance_tick(&registries, &mut state)
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
    let (registries, mut state, source, destination) = make_fixture();
    let token = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(PROCESS_KNAP_STONE_TOOL, source, destination),
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
        Err(LoadError::InvalidState(
            StateValidationError::ManualCraftJob(ManualCraftJobValidationError::DurationMismatch {
                job,
                stored: TickSpan::new(41),
                required: TickSpan::new(40),
            })
        ))
    );
}

#[test]
fn repeated_manual_craft_batches_share_one_labor_job_without_discounting_work() {
    let (registries, mut state, source, destination) = make_fixture();
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        stone_lump(),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("batch craft second stone fixture failed: {error}"));
    let batches =
        NonZeroU64::new(2).unwrap_or_else(|| panic!("batch craft count fixture must be nonzero"));
    let craft = ManualCraftRequest::new(PROCESS_KNAP_STONE_TOOL, source, batches);
    let resolution = resolve_manual_craft(&registries, &state, craft)
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
