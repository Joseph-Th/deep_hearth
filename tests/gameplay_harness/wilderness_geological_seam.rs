//! The generated wilderness copper float points to a *real* mineable seam.
//! No injected ore, pre-owned tools, knowledge records, or special fixture access.

use super::*;
use deep_hearth::content::{
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_STONE_GEOLOGICAL_HAMMER, EQUIPMENT_STONE_PICK,
    FORM_NATIVE_METAL, FORM_ORE, FORM_REINFORCEMENT, FORM_TOOL, MATERIAL_COPPER,
    MINING_METHOD_HAND_PICK, PROSPECTING_DETAILED_FIELD_SURVEY, PROSPECTING_FIELD_INSPECTION,
};
use deep_hearth::equipment::validate_upgrade_equipment;
use deep_hearth::geology::{
    FieldProspectingRequest, GeologicalEvidenceKind, player_prospecting_options,
    validate_start_field_prospecting,
};
use deep_hearth::logistics::{validate_drop_equipment, validate_pickup_equipment};
use deep_hearth::mining::{
    MiningStartError, MiningTargetRequest, resolve_mining_target, validate_start_mining,
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
    }
}
