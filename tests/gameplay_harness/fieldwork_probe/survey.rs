//! Coarse-to-fine geological localization using canonical prospecting work.

use deep_hearth::content::{
    MATERIAL_COPPER, PROSPECTING_DETAILED_FIELD_SURVEY, PROSPECTING_FIELD_INSPECTION,
    PROSPECTING_INDEXED_CHANNEL_SURVEY, PROSPECTING_LOCAL_TRANSECT,
};
use deep_hearth::core::state::AppState;
use deep_hearth::equipment::EquipmentId;
use deep_hearth::geology::{
    ExcavationHardnessEstimate, FieldProspectingOutcome, FieldProspectingRequest,
    GeologicalEvidenceKind, ResourceMassEstimate, validate_start_field_prospecting,
};
use deep_hearth::mining::{
    MiningTargetRequest, MiningTargetResolution, MiningTargetResolutionError, resolve_mining_target,
};
use deep_hearth::registry::Registries;
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};

use super::super::prospecting_timing::complete_prospecting_work;

pub(super) const CHANNEL_START_X: i64 = 20;
pub(super) const SECONDARY_CHANNEL_START_X: i64 = 100;
pub(super) const TERTIARY_CHANNEL_START_X: i64 = 180;
pub(super) const QUATERNARY_CHANNEL_START_X: i64 = 260;
const QUINARY_CHANNEL_START_X: i64 = 340;
const SENARY_CHANNEL_START_X: i64 = 420;
const SEPTENARY_CHANNEL_START_X: i64 = 500;
pub(super) const FOLLOWUP_CHANNEL_STARTS: [i64; 6] = [
    SECONDARY_CHANNEL_START_X,
    TERTIARY_CHANNEL_START_X,
    QUATERNARY_CHANNEL_START_X,
    QUINARY_CHANNEL_START_X,
    SENARY_CHANNEL_START_X,
    SEPTENARY_CHANNEL_START_X,
];
pub(super) const CHANNEL_COUNT: i64 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FieldworkSurveyStrategy {
    PointSearch,
    IndexedChannel,
}

impl FieldworkSurveyStrategy {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::PointSearch => "point-search",
            Self::IndexedChannel => "indexed-channel",
        }
    }
}

pub(super) struct FieldworkLocalization {
    pub(super) target: MiningTargetResolution,
    pub(super) hardness: ExcavationHardnessEstimate,
    pub(super) resource_mass: ResourceMassEstimate,
    pub(super) transects: u64,
    pub(super) field_inspections: u64,
    pub(super) detailed_surveys: u64,
    pub(super) indexed_surveys: u64,
}

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
    channel_start_x: i64,
    strategy: FieldworkSurveyStrategy,
) -> FieldworkLocalization {
    let mut selected_channel = None::<(i64, u32, u32)>;
    let mut transects = 0_u64;
    for channel_index in 0..CHANNEL_COUNT {
        let channel_start = channel_start_x + channel_index * channel_voxels;
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
        let evidence = (finding.lower_ppm(), finding.upper_ppm());
        if selected_channel.is_none_or(|(_selected_start, selected_lower, selected_upper)| {
            evidence > (selected_lower, selected_upper)
        }) {
            selected_channel = Some((channel_start, evidence.0, evidence.1));
        }
    }
    // Broad transects can conservatively retain a zero lower bound even over the best channel.
    // The actor therefore ranks only the acquired bounds relative to one another instead of
    // reverse-engineering hidden truth from the method's authored uncertainty constant.
    let (selected_channel_start, _selected_channel_lower, _selected_channel_upper) =
        selected_channel
            .unwrap_or_else(|| unreachable!("fieldwork evaluates at least one candidate channel"));
    let first_point = horizontal_region(selected_channel_start, 1);
    assert!(matches!(
        resolve_mining_target(
            state,
            MiningTargetRequest::new(first_point, MATERIAL_COPPER),
        ),
        Err(MiningTargetResolutionError::EvidenceInsufficientToResolveTarget { .. })
    ));

    if strategy == FieldworkSurveyStrategy::IndexedChannel {
        let channel = horizontal_region(selected_channel_start, channel_voxels);
        let indexed = run_survey(
            registries,
            state,
            PROSPECTING_INDEXED_CHANNEL_SURVEY,
            channel,
            Some(hammer),
            "indexed channel survey",
        );
        for observation in indexed.observations() {
            let record = state
                .geological_knowledge()
                .get_observation(observation)
                .unwrap_or_else(|| panic!("fieldwork indexed observation disappeared"));
            let finding = record
                .finding(MATERIAL_COPPER)
                .unwrap_or_else(|| panic!("fieldwork indexed copper finding disappeared"));
            if finding.lower_ppm() == 0 {
                continue;
            }
            let hardness = record.excavation_hardness().unwrap_or_else(|| {
                panic!("fieldwork indexed positive sample produced no hardness estimate")
            });
            let resource_mass = record.resource_mass().unwrap_or_else(|| {
                panic!("fieldwork indexed localized sample produced no resource-mass estimate")
            });
            let target = resolve_mining_target(
                state,
                MiningTargetRequest::new(record.region(), MATERIAL_COPPER),
            )
            .unwrap_or_else(|error| panic!("indexed evidence did not resolve target: {error}"));
            return FieldworkLocalization {
                target,
                hardness,
                resource_mass,
                transects,
                field_inspections: 0,
                detailed_surveys: 0,
                indexed_surveys: 1,
            };
        }
        panic!("fieldwork indexed search exhausted the promising channel without a target");
    }

    let mut detailed_surveys = 0_u64;
    for (inspection_index, offset) in (0..channel_voxels).enumerate() {
        let field_inspections = u64::try_from(inspection_index + 1)
            .unwrap_or_else(|_| unreachable!("bounded fieldwork inspection count fits u64"));
        let point = horizontal_region(selected_channel_start + offset, 1);
        let inspection = run_survey(
            registries,
            state,
            PROSPECTING_FIELD_INSPECTION,
            point,
            None,
            "fixed-order field inspection",
        );
        let inspection_finding = state
            .geological_knowledge()
            .get_observation(inspection.observation())
            .and_then(|record| record.finding(MATERIAL_COPPER))
            .unwrap_or_else(|| panic!("fieldwork inspection copper finding disappeared"));
        if inspection_finding.lower_ppm() == 0 {
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
        let resource_mass = detailed_record.resource_mass().unwrap_or_else(|| {
            panic!("fieldwork localized physical sample produced no resource-mass estimate")
        });
        let target = resolve_mining_target(state, MiningTargetRequest::new(point, MATERIAL_COPPER))
            .unwrap_or_else(|error| {
                panic!("positive detailed evidence did not resolve target: {error}")
            });
        return FieldworkLocalization {
            target,
            hardness,
            resource_mass,
            transects,
            field_inspections,
            detailed_surveys,
            indexed_surveys: 0,
        };
    }
    panic!("fieldwork coarse-to-fine search exhausted the promising channel without a target")
}
