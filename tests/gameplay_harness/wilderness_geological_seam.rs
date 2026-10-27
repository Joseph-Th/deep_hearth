//! The generated wilderness copper float points to a *real* mineable seam.
//! No injected ore, pre-owned tools, knowledge records, or special fixture access.

use super::*;
use deep_hearth::content::{
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_STONE_GEOLOGICAL_HAMMER, EQUIPMENT_STONE_PICK,
    FORM_CRUSHED, FORM_NATIVE_METAL, FORM_ORE, FORM_REINFORCEMENT, FORM_TOOL, MATERIAL_COPPER,
    MINING_METHOD_HAND_PICK, PROCESS_HAND_BREAK_ORE, PROCESS_HAND_SORT_NATIVE_COPPER,
    PROSPECTING_DETAILED_FIELD_SURVEY, PROSPECTING_FIELD_INSPECTION,
};
use deep_hearth::equipment::validate_upgrade_equipment;
use deep_hearth::geology::{
    FieldProspectingRequest, GeologicalEvidenceKind, player_prospecting_options,
    validate_start_field_prospecting,
};
use deep_hearth::inventory::{MaterialStackProjectionError, project_material_stacks};
use deep_hearth::logistics::{validate_drop_equipment, validate_pickup_equipment};
use deep_hearth::mining::{
    MiningStartError, MiningTargetRequest, resolve_mining_target, validate_start_mining,
};
use deep_hearth::ore_processing::{
    ManualComminutionResolutionError, resolve_manual_comminution_from_stack,
    resolve_manual_constituent_separation_from_stack, validate_start_manual_comminution,
    validate_start_manual_constituent_separation,
};

pub(super) struct SeamRoute {
    pub(super) junction: VoxelCoord,
    pub(super) forage: VoxelCoord,
    pub(super) water: VoxelCoord,
    pub(super) outcrop: VoxelCoord,
    pub(super) stone: VoxelCoord,
    pub(super) timber: VoxelCoord,
    pub(super) carried: StockpileId,
}

pub(super) struct SeamPayoff {
    pub(super) survey_ticks: u64,
    pub(super) acquisition_ticks: u64,
    pub(super) mining_ticks: u64,
    pub(super) mined_ore: Mass,
    pub(super) hardness_upper_pa: u64,
    pub(super) followup_still_resolvable: bool,
    pub(super) dressed_feed: Mass,
    pub(super) native_recovered: Mass,
    pub(super) crushed_residue: Mass,
    pub(super) dressed_ticks: u64,
    pub(super) alternate_float_gathered: Mass,
    pub(super) alternate_float_remaining: Mass,
    pub(super) alternate_float_ticks: u64,
}

/// The freshly mined ore does not automatically justify hand dressing while
/// a shorter, finite surface-float trip can still supply the immediate need.
/// Compare the actual walk and gathering work from the *same* post-mine state.
fn gather_remaining_float_instead(
    registries: &Registries,
    after_mining: &AppState,
    route: &SeamRoute,
    conserved: deep_hearth::core::quantity::AggregateMass,
) -> (Mass, Mass, u64) {
    let mut alternative = after_mining.clone();
    let started_at = alternative.tick();
    let copper = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
    visit_outcrop(registries, &mut alternative, route, false);
    let source = local_surface_resource(&alternative, copper);
    let before = alternative
        .available_surface_resources()
        .find(|record| record.id() == source)
        .unwrap_or_else(|| panic!("finite copper float is no longer locally observable"))
        .remaining_mass();
    let selected = Mass::from_milligrams(40_000);
    assert!(
        before >= selected,
        "the immediate float alternative must still be genuinely available on every opening seed"
    );
    gather_in_batches(
        registries,
        &mut alternative,
        source,
        selected,
        "remaining native float instead of mineral dressing",
    );
    let remaining = alternative
        .available_surface_resources()
        .find(|record| record.id() == source)
        .unwrap_or_else(|| panic!("the selected surface float lost its persistent identity"))
        .remaining_mass();
    assert_eq!(before.checked_sub(selected), Some(remaining));
    visit_outcrop(registries, &mut alternative, route, true);
    assert_eq!(
        alternative
            .inventory()
            .get_stockpile(route.carried)
            .unwrap()
            .get_mass(copper),
        selected,
        "the cheaper float route must physically deliver its selected copper to the worksite"
    );
    assert_eq!(
        calculate_matter_accounting(&alternative).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(registries, &alternative), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(registries, &alternative)).unwrap();
    let replay: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    assert_eq!(replay.into_state(registries), Ok(alternative.clone()));
    (
        selected,
        remaining,
        alternative.tick().value() - started_at.value(),
    )
}

/// Process only explicitly selected ore from this player's carried custody.
/// Both output streams stay represented in the carried stockpile, including
/// the rejected gangue. No test-owned lot, ground bin, or equipment is spawned.
fn hand_dress_first_copper(
    registries: &Registries,
    state: &mut AppState,
    carried: StockpileId,
) -> (Mass, Mass, Mass, u64) {
    let ore = CommodityKey::new(MATERIAL_COPPER, FORM_ORE);
    let crushed = CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED);
    let gangue = CommodityKey::new(MATERIAL_STONE, FORM_CRUSHED);
    let native = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
    let feed = Mass::from_milligrams(100_000);
    let before_native = state
        .inventory()
        .get_stockpile(carried)
        .unwrap()
        .get_mass(native);
    let beginning = state.tick();
    let ore_stack = project_material_stacks(registries, state, carried)
        .unwrap_or_else(|error| panic!("actor-visible mined-ore catalog failed: {error}"))
        .into_iter()
        .find(|stack| stack.commodity() == ore && stack.mass() >= feed)
        .expect("player cannot select the genuinely mined ore from the familiar carried stacks");
    let broken = resolve_manual_comminution_from_stack(
        registries,
        state,
        PROCESS_HAND_BREAK_ORE,
        &ore_stack,
        feed,
    )
    .unwrap_or_else(|error| panic!("hand-breaking mined ore was rejected: {error}"));
    let breaking_job =
        validate_start_manual_comminution(registries, state, &broken, carried, carried)
            .unwrap_or_else(|error| panic!("hand-breaking mined ore start rejected: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("hand-breaking mined ore commit failed: {error}"));
    let breaking_finish = state
        .production()
        .get_job(breaking_job)
        .unwrap()
        .completes_at();
    advance_to(
        registries,
        state,
        breaking_finish,
        TickEventAllowance {
            production_jobs: &[breaking_job],
            ..TickEventAllowance::default()
        },
        "hand-breaking first owned geological ore",
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(crushed),
        feed
    );
    assert!(
        matches!(
            resolve_manual_comminution_from_stack(
                registries,
                state,
                PROCESS_HAND_BREAK_ORE,
                &ore_stack,
                feed,
            ),
            Err(ManualComminutionResolutionError::Stack(
                MaterialStackProjectionError::StaleInventoryRevision { .. }
            ))
        ),
        "the old player stack choice must be stale after ore actually changes form"
    );
    let crushed_stack = project_material_stacks(registries, state, carried)
        .unwrap_or_else(|error| panic!("actor-visible crushed-ore catalog failed: {error}"))
        .into_iter()
        .find(|stack| stack.commodity() == crushed && stack.mass() >= feed)
        .expect("player cannot select the hand-broken rock from the familiar carried stacks");
    let sorted = resolve_manual_constituent_separation_from_stack(
        registries,
        state,
        PROCESS_HAND_SORT_NATIVE_COPPER,
        &crushed_stack,
        feed,
    )
    .unwrap_or_else(|error| panic!("hand-sorting owned crushed ore was rejected: {error}"));
    let recovered = sorted.target_mass();
    let residue = sorted.residue_mass();
    assert_eq!(recovered.checked_add(residue), Some(feed));
    let sorting_job = validate_start_manual_constituent_separation(
        registries, state, &sorted, carried, carried, carried,
    )
    .unwrap_or_else(|error| panic!("hand-sorting mined ore start rejected: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("hand-sorting mined ore commit failed: {error}"));
    let sorting_finish = state
        .production()
        .get_job(sorting_job)
        .unwrap()
        .completes_at();
    advance_to(
        registries,
        state,
        sorting_finish,
        TickEventAllowance {
            production_jobs: &[sorting_job],
            ..TickEventAllowance::default()
        },
        "hand-sorting native copper from personally mined rock",
    );
    let pack = state.inventory().get_stockpile(carried).unwrap();
    assert_eq!(
        pack.get_mass(ore),
        feed,
        "undressed ore must remain available"
    );
    assert_eq!(
        pack.get_mass(gangue),
        residue,
        "hand sorting must retain the rock-hosted, copper-bearing crushed residue"
    );
    assert_eq!(
        pack.get_mass(crushed),
        Mass::ZERO,
        "the selected ore feed was sorted"
    );
    let gangue_lot = state
        .inventory()
        .lot_ids(carried)
        .find_map(|id| {
            state
                .inventory()
                .get_lot(id)
                .filter(|lot| lot.commodity() == gangue)
        })
        .expect("sorted mixed rock must remain physically in the player's inventory");
    assert!(
        gangue_lot.composition().parts_per_million(MATERIAL_COPPER) > 0,
        "hand sorting must not silently destroy copper still bound into the reject"
    );
    assert_eq!(
        pack.get_mass(native).checked_sub(before_native),
        Some(recovered),
        "recovered native copper must enter this player's actual carried inventory"
    );
    (
        feed,
        recovered,
        residue,
        state.tick().value() - beginning.value(),
    )
}

fn visit_outcrop(
    registries: &Registries,
    state: &mut AppState,
    route: &SeamRoute,
    returning: bool,
) {
    let itinerary = if returning {
        [route.water, route.forage, route.junction]
    } else {
        [route.forage, route.water, route.outcrop]
    };
    for voxel in itinerary {
        walk_continuous_body_adjacent_and_settle(
            registries,
            state,
            voxel,
            "seam inspection travel",
        );
    }
}

fn acquired_survey(
    registries: &Registries,
    state: &mut AppState,
    region: VoxelBounds,
    method: deep_hearth::labor::ProspectingMethodId,
    equipment: Option<deep_hearth::equipment::EquipmentId>,
) -> u64 {
    let option = player_prospecting_options(registries, state, region)
        .unwrap_or_else(|error| panic!("generated-world survey catalog failed: {error}"))
        .into_iter()
        .find(|option| option.method() == method)
        .unwrap_or_else(|| panic!("requested geological survey technique is not visible"));
    assert_eq!(option.requires_equipment(), equipment.is_some());
    if let Some(id) = equipment {
        assert!(
            option
                .instruments()
                .iter()
                .any(|instrument| instrument.equipment() == id)
        );
    }
    let request = equipment.map_or_else(
        || FieldProspectingRequest::new(method, region, MATERIAL_COPPER),
        |id| FieldProspectingRequest::new_with_equipment(method, region, MATERIAL_COPPER, id),
    );
    let admitted = validate_start_field_prospecting(registries, state, request)
        .unwrap_or_else(|error| panic!("generated-world survey admission rejected: {error}"));
    let finish = admitted.work().completes_at();
    let began = state.tick();
    admitted
        .commit(state)
        .unwrap_or_else(|error| panic!("generated-world survey commit rejected: {error}"));
    advance_to(
        registries,
        state,
        finish,
        TickEventAllowance {
            field_prospecting: true,
            ..TickEventAllowance::default()
        },
        "observed copper outcrop survey",
    );
    finish.value() - began.value()
}

pub(super) fn exercise_generated_copper_seam(
    registries: &Registries,
    source: &AppState,
    route: SeamRoute,
    conserved: deep_hearth::core::quantity::AggregateMass,
) -> SeamPayoff {
    let mut state = source.clone();
    let stone = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let native = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
    let ore = CommodityKey::new(MATERIAL_COPPER, FORM_ORE);
    let reinforcement = CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT);
    let region = VoxelBounds::new(
        route.outcrop,
        VoxelCoord::new(
            route.outcrop.x() + 1,
            route.outcrop.y() + 1,
            route.outcrop.z() + 1,
        ),
    )
    .unwrap();
    assert!(
        state.geological_knowledge().observations().next().is_none(),
        "opening actor must have no unexplained prior knowledge of the hidden seam"
    );
    visit_outcrop(registries, &mut state, &route, false);
    let mut survey_ticks = acquired_survey(
        registries,
        &mut state,
        region,
        PROSPECTING_FIELD_INSPECTION,
        None,
    );
    let inspection = state
        .geological_knowledge()
        .observations()
        .last()
        .unwrap_or_else(|| panic!("opening copper outcrop inspection recorded no knowledge"));
    assert_eq!(
        inspection.evidence(),
        GeologicalEvidenceKind::SurfaceExposure
    );
    assert!(
        inspection
            .finding(MATERIAL_COPPER)
            .is_some_and(|finding| finding.lower_ppm() > 0),
        "the first geological investment must respond to acquired copper evidence"
    );
    let copper_source = local_surface_resource(&state, native);
    gather_in_batches(
        registries,
        &mut state,
        copper_source,
        Mass::from_milligrams(20_000),
        "seam prospecting cold-work float",
    );
    visit_outcrop(registries, &mut state, &route, true);
    let acquisition_started = state.tick();
    for (voxel, commodity, amount, label) in [
        (
            route.stone,
            stone,
            2_000_000,
            "stone sampling and mining tools",
        ),
        (
            route.timber,
            timber,
            2_000_000,
            "timber sampling and mining handles",
        ),
    ] {
        walk_continuous_body_adjacent_and_settle(registries, &mut state, voxel, label);
        let source = local_surface_resource(&state, commodity);
        gather_in_batches(
            registries,
            &mut state,
            source,
            Mass::from_milligrams(amount),
            label,
        );
        walk_continuous_body_adjacent_and_settle(
            registries,
            &mut state,
            route.junction,
            "return sampling equipment components",
        );
    }
    craft_in_place(
        registries,
        &mut state,
        stone,
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        2,
        "fieldwork sampling and extraction heads",
    );
    craft_in_place(
        registries,
        &mut state,
        timber,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        2,
        "fieldwork sampling and extraction handles",
    );
    let assemble = |state: &mut AppState, definition| {
        let ready = player_equipment_assembly_options(registries, state, route.carried)
            .unwrap_or_else(|error| panic!("fieldwork player equipment catalog rejected: {error}"))
            .into_iter()
            .find(|option| option.definition() == definition)
            .unwrap_or_else(|| panic!("fieldwork player equipment vanished from catalog"));
        assert!(ready.is_ready());
        let equipment = validate_assemble_player_equipment_option(registries, state, &ready)
            .unwrap_or_else(|error| panic!("fieldwork tool assembly rejected: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("fieldwork tool assembly failed: {error}"));
        validate_pickup_equipment(registries, state, equipment)
            .unwrap_or_else(|error| panic!("fieldwork tool pickup rejected: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("fieldwork tool pickup failed: {error}"));
        equipment
    };
    let hammer = assemble(&mut state, EQUIPMENT_STONE_GEOLOGICAL_HAMMER);
    let pick = assemble(&mut state, EQUIPMENT_STONE_PICK);
    let acquisition_ticks = state.tick().value() - acquisition_started.value();
    visit_outcrop(registries, &mut state, &route, false);
    survey_ticks += acquired_survey(
        registries,
        &mut state,
        region,
        PROSPECTING_DETAILED_FIELD_SURVEY,
        Some(hammer),
    );
    let acquired_hardness = state
        .geological_knowledge()
        .observations()
        .last()
        .and_then(|record| record.excavation_hardness())
        .unwrap_or_else(|| panic!("detailed local observation did not disclose hardness"));
    let target = resolve_mining_target(&state, MiningTargetRequest::new(region, MATERIAL_COPPER))
        .unwrap_or_else(|error| panic!("actor-acquired evidence cannot authorize a seam: {error}"));
    let requested = Mass::from_milligrams(200_000);
    assert!(
        matches!(
            validate_start_mining(
                registries,
                &state,
                MINING_METHOD_HAND_PICK,
                target,
                route.carried,
                pick,
                requested
            ),
            Err(MiningStartError::ExcavationHardnessEvidenceExceedsCapability { .. })
        ),
        "the player must actually encounter the harder-seam limit before reinforcing their stone pick"
    );
    visit_outcrop(registries, &mut state, &route, true);
    craft_in_place(
        registries,
        &mut state,
        native,
        reinforcement,
        1,
        "reinforcement after discovered hard-seam limit",
    );
    validate_drop_equipment(registries, &state, pick)
        .unwrap_or_else(|error| panic!("reinforced pick worksite drop rejected: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("reinforced pick worksite drop failed: {error}"));
    let upgraded = validate_upgrade_equipment(
        registries,
        &state,
        pick,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        route.carried,
    )
    .unwrap_or_else(|error| panic!("acquired copper pick upgrade was rejected: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("acquired copper pick upgrade failed: {error}"));
    assert_eq!(upgraded, pick);
    validate_pickup_equipment(registries, &state, pick)
        .unwrap_or_else(|error| panic!("reinforced pick return-to-field pickup rejected: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("reinforced pick return-to-field pickup failed: {error}"));
    visit_outcrop(registries, &mut state, &route, false);
    let upgraded_target =
        resolve_mining_target(&state, MiningTargetRequest::new(region, MATERIAL_COPPER))
            .unwrap_or_else(|error| {
                panic!("surveyed seam target disappeared after return: {error}")
            });
    let begin = state.tick();
    let job = validate_start_mining(
        registries,
        &state,
        MINING_METHOD_HAND_PICK,
        upgraded_target,
        route.carried,
        pick,
        requested,
    )
    .unwrap_or_else(|error| panic!("reinforced pick cannot mine the acquired seam: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("reinforced pick mining start failed: {error}"));
    let completed_at = state.mining().get_job(job).unwrap().completes_at();
    advance_to(
        registries,
        &mut state,
        completed_at,
        TickEventAllowance {
            mining_jobs: &[job],
            ..TickEventAllowance::default()
        },
        "first hard-seam copper ore extraction",
    );
    let mining_ticks = state.tick().value() - begin.value();
    assert_eq!(
        state
            .inventory()
            .get_stockpile(route.carried)
            .unwrap()
            .get_mass(ore),
        requested
    );
    visit_outcrop(registries, &mut state, &route, true);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(route.carried)
            .unwrap()
            .get_mass(ore),
        requested,
        "mined ore must physically return to the worksite in the player's carried inventory"
    );
    let (alternate_float_gathered, alternate_float_remaining, alternate_float_ticks) =
        gather_remaining_float_instead(registries, &state, &route, conserved);
    let (dressed_feed, native_recovered, crushed_residue, dressed_ticks) =
        hand_dress_first_copper(registries, &mut state, route.carried);
    assert!(
        alternate_float_ticks < dressed_ticks && alternate_float_gathered > native_recovered,
        "while finite exposed copper remains, the player must not mistake slower ore dressing for the cheapest immediate native-metal supply"
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(registries, &state)).unwrap();
    let replay: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    assert_eq!(replay.into_state(registries), Ok(state.clone()));
    let followup_still_resolvable =
        resolve_mining_target(&state, MiningTargetRequest::new(region, MATERIAL_COPPER)).is_ok();
    assert!(
        followup_still_resolvable,
        "the actor's acquired survey evidence must still support a next mining order"
    );
    SeamPayoff {
        survey_ticks,
        acquisition_ticks,
        mining_ticks,
        mined_ore: requested,
        hardness_upper_pa: acquired_hardness.upper().pascals(),
        followup_still_resolvable,
        dressed_feed,
        native_recovered,
        crushed_residue,
        dressed_ticks,
        alternate_float_gathered,
        alternate_float_remaining,
        alternate_float_ticks,
    }
}
