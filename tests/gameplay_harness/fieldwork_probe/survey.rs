//! Coarse-to-fine geological localization using canonical prospecting work.

use super::*;

pub(super) const CHANNEL_START_X: i64 = 20;
pub(super) const CHANNEL_COUNT: i64 = 2;

pub(super) fn horizontal_region(start_x: i64, width: i64) -> VoxelBounds {
    VoxelBounds::new(
        VoxelCoord::new(start_x, -1, 0),
        VoxelCoord::new(start_x + width, 0, 1),
    )
    .unwrap_or_else(|error| panic!("fieldwork region failed: {error}"))
}

fn run_survey(
    registries: &Registries,
    state: &mut AppState,
    method: deep_hearth::labor::ProspectingMethodId,
    region: VoxelBounds,
    equipment: Option<EquipmentId>,
    context: &'static str,
) -> FieldProspectingOutcome {
    let request = match equipment {
        Some(equipment) => {
            FieldProspectingRequest::new_with_equipment(method, region, MATERIAL_COPPER, equipment)
        }
        None => FieldProspectingRequest::new(method, region, MATERIAL_COPPER),
    };
    let start = validate_start_field_prospecting(registries, state, request)
        .unwrap_or_else(|error| panic!("fieldwork {context} start failed: {error}"));
    let work = start.work();
    let expected_condition = work.condition_after();
    start
        .commit(state)
        .unwrap_or_else(|error| panic!("fieldwork {context} commit failed: {error}"));
    let outcome = complete_prospecting_work(registries, state, work, context);
    match (equipment, expected_condition) {
        (Some(equipment), Some(expected_condition)) => assert_eq!(
            state
                .equipment()
                .get_equipment(equipment)
                .map(|record| record.condition()),
            Some(expected_condition),
            "fieldwork {context} wear diverged from its validated prospecting work"
        ),
        (None, None) => {}
        _ => panic!("fieldwork {context} equipment/wear resolution disagreed"),
    }
    outcome
}

pub(super) fn localize_target(
    registries: &Registries,
    state: &mut AppState,
    hammer: EquipmentId,
    channel_voxels: i64,
) -> (
    MiningTargetResolution,
    ExcavationHardnessEstimate,
    u64,
    u64,
    u64,
) {
    let transect_uncertainty = registries
        .labor()
        .get_prospecting(PROSPECTING_LOCAL_TRANSECT)
        .map(|definition| definition.abundance_uncertainty_ppm())
        .unwrap_or_else(|| panic!("fieldwork local-transect definition disappeared"));
    let mut selected_channel = None::<(i64, u32)>;
    let mut transects = 0_u64;
    for channel_index in 0..CHANNEL_COUNT {
        let channel_start = CHANNEL_START_X + channel_index * channel_voxels;
        let channel = horizontal_region(channel_start, channel_voxels);
        let outcome = run_survey(
            registries,
            state,
            PROSPECTING_LOCAL_TRANSECT,
            channel,
            None,
            "candidate local transect",
        );
        transects += 1;
        let finding = state
            .geological_knowledge()
            .get_observation(outcome.observation())
            .and_then(|record| record.finding(MATERIAL_COPPER))
            .unwrap_or_else(|| panic!("fieldwork local-transect copper finding disappeared"));
        if selected_channel
            .is_none_or(|(_selected_start, selected_upper)| finding.upper_ppm() > selected_upper)
        {
            selected_channel = Some((channel_start, finding.upper_ppm()));
        }
    }
    let (selected_channel_start, selected_channel_upper) = selected_channel
        .unwrap_or_else(|| unreachable!("fieldwork evaluates at least one candidate channel"));
    assert!(
        selected_channel_upper > transect_uncertainty,
        "fieldwork selected channel must contain a signal above transect uncertainty"
    );
    let first_point = horizontal_region(selected_channel_start, 1);
    assert!(matches!(
        resolve_mining_target(
            state,
            MiningTargetRequest::new(first_point, MATERIAL_COPPER),
        ),
        Err(MiningTargetResolutionError::EvidenceInsufficientToResolveTarget { .. })
    ));

    let inspection_uncertainty = registries
        .labor()
        .get_prospecting(PROSPECTING_FIELD_INSPECTION)
        .map(|definition| definition.abundance_uncertainty_ppm())
        .unwrap_or_else(|| panic!("fieldwork inspection definition disappeared"));
    let mut field_inspections = 0_u64;
    let mut detailed_surveys = 0_u64;
    for offset in 0..channel_voxels {
        let point = horizontal_region(selected_channel_start + offset, 1);
        let inspection = run_survey(
            registries,
            state,
            PROSPECTING_FIELD_INSPECTION,
            point,
            None,
            "fixed-order field inspection",
        );
        field_inspections += 1;
        let inspection_finding = state
            .geological_knowledge()
            .get_observation(inspection.observation())
            .and_then(|record| record.finding(MATERIAL_COPPER))
            .unwrap_or_else(|| panic!("fieldwork inspection copper finding disappeared"));
        if inspection_finding.upper_ppm() <= inspection_uncertainty {
            continue;
        }
        let detailed = run_survey(
            registries,
            state,
            PROSPECTING_DETAILED_FIELD_SURVEY,
            point,
            Some(hammer),
            "targeted detailed survey",
        );
        detailed_surveys += 1;
        let detailed_record = state
            .geological_knowledge()
            .get_observation(detailed.observation())
            .unwrap_or_else(|| panic!("fieldwork detailed observation disappeared"));
        assert_eq!(
            detailed_record.evidence(),
            GeologicalEvidenceKind::ExcavationSample,
            "fieldwork physical sampling must identify its acquired evidence as an excavation sample"
        );
        let detailed_finding = detailed_record
            .finding(MATERIAL_COPPER)
            .unwrap_or_else(|| panic!("fieldwork detailed copper finding disappeared"));
        assert!(
            detailed_finding.lower_ppm() > 0,
            "fieldwork coarse positive signal must remain positive after detailed refinement"
        );
        let hardness = detailed_record.excavation_hardness().unwrap_or_else(|| {
            panic!("fieldwork detailed physical sample produced no excavation-hardness estimate")
        });
        let target = resolve_mining_target(state, MiningTargetRequest::new(point, MATERIAL_COPPER))
            .unwrap_or_else(|error| {
                panic!("positive detailed evidence did not resolve target: {error}")
            });
        return (
            target,
            hardness,
            transects,
            field_inspections,
            detailed_surveys,
        );
    }
    panic!("fieldwork coarse-to-fine search exhausted the promising channel without a target")
}
