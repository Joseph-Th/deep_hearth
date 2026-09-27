//! Material-backed prospecting-instrument and channel-survey contracts.

use deep_hearth::content::gameplay_fixture::{
    GeologicalDepositSeed, seed_geological_deposit, seed_lot,
};
use deep_hearth::content::{
    EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER, EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
    EQUIPMENT_TIMBER_CHANNEL_SAMPLING_FRAME, EQUIPMENT_TIMBER_TRIPOD_CORE_DRILL, FORM_ORE,
    MATERIAL_COPPER, PROSPECTING_CHANNEL_COMPOSITE_SURVEY, PROSPECTING_DETAILED_FIELD_SURVEY,
    PROSPECTING_INDEXED_CHANNEL_SURVEY, PROSPECTING_SHALLOW_CORE_SURVEY, build_registries,
};
use deep_hearth::core::quantity::{Mass, Pressure};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::equipment::{
    EquipmentDisassemblyError, validate_assemble_equipment, validate_disassemble_equipment,
    validate_upgrade_equipment,
};
use deep_hearth::geology::{
    ExcavationHardnessEstimate, FieldProspectingRequest, FieldProspectingStartError,
    GeologicalEvidenceKind, validate_start_field_prospecting,
};
use deep_hearth::labor::ProspectingSpatialResolution;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::mining::{
    MiningTargetRequest, MiningTargetResolutionError, resolve_mining_target,
};
use deep_hearth::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};
use deep_hearth::survival::initialize_player_survival;

use super::environment::ROOM_TEMPERATURE;
use super::inventory_support::add_solid_stockpile;
use super::ore_fixture::copper_ore_composition;
use super::prospecting_timing::complete_prospecting_work;

fn horizontal_region(start_x: i64, width: i64) -> VoxelBounds {
    VoxelBounds::new(
        VoxelCoord::new(start_x, -1, 0),
        VoxelCoord::new(start_x + width, 0, 1),
    )
    .unwrap_or_else(|error| panic!("prospecting-instrument region failed: {error}"))
}

#[test]
fn channel_sampling_frame_buys_precise_aggregate_grade_without_spoofing_mining_authority() {
    let registries = build_registries();
    let survey = registries
        .labor()
        .get_prospecting(PROSPECTING_CHANNEL_COMPOSITE_SURVEY)
        .copied()
        .unwrap_or_else(|| panic!("channel-composite survey definition disappeared"));
    assert_eq!(survey.evidence(), GeologicalEvidenceKind::ChannelComposite);
    assert_eq!(survey.maximum_region_voxels(), 16);
    assert_eq!(
        survey.spatial_resolution(),
        ProspectingSpatialResolution::AggregateRegion
    );
    assert_eq!(survey.excavation_hardness_resolution(), None);
    assert_eq!(survey.resource_mass_resolution(), None);

    let frame_definition = registries
        .equipment()
        .get_equipment(EQUIPMENT_TIMBER_CHANNEL_SAMPLING_FRAME)
        .unwrap_or_else(|| panic!("channel-sampling frame definition disappeared"));
    let frame_assembly = frame_definition
        .assembly_profile()
        .unwrap_or_else(|| panic!("channel-sampling frame lost its assembly profile"));
    let mut state = AppState::new();
    let frame_source = add_solid_stockpile(&mut state, frame_assembly.input_mass());
    for input in frame_assembly.inputs() {
        seed_lot(
            &registries,
            &mut state,
            frame_source,
            input.commodity(),
            input.mass(),
            ROOM_TEMPERATURE,
        );
    }
    let region = horizontal_region(40, 16);
    seed_geological_deposit(
        &registries,
        &mut state,
        GeologicalDepositSeed::new(
            region,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Mass::from_milligrams(16_000_000),
            ROOM_TEMPERATURE,
            Pressure::from_pascals(350_000_000),
            copper_ore_composition(420_000, 280_000),
        ),
    );
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("channel-composite matter setup failed: {error}"))
        .total();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("channel-composite survival setup failed: {error}"));
    let frame = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_TIMBER_CHANNEL_SAMPLING_FRAME,
        frame_source,
    )
    .unwrap_or_else(|error| panic!("channel-sampling frame assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("channel-sampling frame assembly commit failed: {error}"));

    let start = validate_start_field_prospecting(
        &registries,
        &state,
        FieldProspectingRequest::new_with_equipment(
            PROSPECTING_CHANNEL_COMPOSITE_SURVEY,
            region,
            MATERIAL_COPPER,
            frame,
        ),
    )
    .unwrap_or_else(|error| panic!("channel-composite survey start failed: {error}"));
    let work = start.work();
    start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("channel-composite survey commit failed: {error}"));
    let outcome =
        complete_prospecting_work(&registries, &mut state, work, "channel-composite survey");
    assert_eq!(outcome.evidence(), GeologicalEvidenceKind::ChannelComposite);
    let mut observations = outcome.observations();
    let observation = observations
        .next()
        .unwrap_or_else(|| panic!("aggregate channel-composite survey emitted no observation"));
    assert!(
        observations.next().is_none(),
        "aggregate channel-composite survey must emit exactly one observation"
    );
    let record = state
        .geological_knowledge()
        .get_observation(observation)
        .unwrap_or_else(|| panic!("channel-composite observation disappeared"));
    let finding = record
        .finding(MATERIAL_COPPER)
        .unwrap_or_else(|| panic!("channel-composite copper finding disappeared"));
    assert!(finding.lower_ppm() > 0);
    assert_eq!(record.excavation_hardness(), None);
    assert_eq!(record.resource_mass(), None);
    assert!(matches!(
        resolve_mining_target(&state, MiningTargetRequest::new(region, MATERIAL_COPPER)),
        Err(MiningTargetResolutionError::EvidenceInsufficientToResolveTarget { .. })
    ));
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("channel-composite final state invalid: {error}"));
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("channel-composite matter audit failed: {error}"))
            .total(),
        matter_before,
        "prospecting instrument construction and use must conserve matter"
    );
}

#[test]
fn shallow_core_drill_turns_expensive_local_work_into_mining_ready_persistent_evidence() {
    let registries = build_registries();
    let survey = registries
        .labor()
        .get_prospecting(PROSPECTING_SHALLOW_CORE_SURVEY)
        .copied()
        .unwrap_or_else(|| panic!("shallow-core survey definition disappeared"));
    assert_eq!(survey.evidence(), GeologicalEvidenceKind::CoreSample);
    assert_eq!(survey.maximum_region_voxels(), 1);
    assert!(survey.excavation_hardness_resolution().is_some());
    assert!(survey.resource_mass_resolution().is_some());

    let drill_definition = registries
        .equipment()
        .get_equipment(EQUIPMENT_TIMBER_TRIPOD_CORE_DRILL)
        .unwrap_or_else(|| panic!("tripod core drill definition disappeared"));
    let drill_assembly = drill_definition
        .assembly_profile()
        .unwrap_or_else(|| panic!("tripod core drill lost its assembly profile"));

    let mut state = AppState::new();
    let drill_source = add_solid_stockpile(&mut state, drill_assembly.input_mass());
    for input in drill_assembly.inputs() {
        seed_lot(
            &registries,
            &mut state,
            drill_source,
            input.commodity(),
            input.mass(),
            ROOM_TEMPERATURE,
        );
    }
    let region = horizontal_region(80, 1);
    seed_geological_deposit(
        &registries,
        &mut state,
        GeologicalDepositSeed::new(
            region,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Mass::from_milligrams(2_000_000),
            ROOM_TEMPERATURE,
            Pressure::from_pascals(375_000_000),
            copper_ore_composition(460_000, 260_000),
        ),
    );
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("shallow-core matter setup failed: {error}"))
        .total();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("shallow-core survival setup failed: {error}"));

    let drill = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_TIMBER_TRIPOD_CORE_DRILL,
        drill_source,
    )
    .unwrap_or_else(|error| panic!("tripod core drill assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("tripod core drill assembly commit failed: {error}"));
    let condition_before = state
        .equipment()
        .get_equipment(drill)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("assembled tripod core drill disappeared"));

    let start = validate_start_field_prospecting(
        &registries,
        &state,
        FieldProspectingRequest::new_with_equipment(
            PROSPECTING_SHALLOW_CORE_SURVEY,
            region,
            MATERIAL_COPPER,
            drill,
        ),
    )
    .unwrap_or_else(|error| panic!("shallow-core survey start failed: {error}"));
    let work = start.work();
    start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("shallow-core survey commit failed: {error}"));
    let outcome = complete_prospecting_work(&registries, &mut state, work, "shallow-core survey");
    assert_eq!(outcome.evidence(), GeologicalEvidenceKind::CoreSample);
    let mut observations = outcome.observations();
    let observation = observations
        .next()
        .unwrap_or_else(|| panic!("shallow-core survey produced no observation"));
    assert!(
        observations.next().is_none(),
        "one shallow-core survey must produce exactly one observation"
    );
    let record = state
        .geological_knowledge()
        .get_observation(observation)
        .unwrap_or_else(|| panic!("shallow-core observation disappeared"));
    let finding = record
        .finding(MATERIAL_COPPER)
        .unwrap_or_else(|| panic!("shallow-core copper finding disappeared"));
    assert!(finding.lower_ppm() > 0);
    assert!(record.excavation_hardness().is_some());
    assert!(record.resource_mass().is_some());
    assert!(
        state
            .equipment()
            .get_equipment(drill)
            .is_some_and(|equipment| equipment.condition() < condition_before),
        "core drilling must wear the physical drill"
    );

    let resolved_target =
        resolve_mining_target(&state, MiningTargetRequest::new(region, MATERIAL_COPPER))
            .unwrap_or_else(|error| {
                panic!("core evidence did not resolve its sampled target: {error}")
            });
    assert_eq!(resolved_target.region(), region);

    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("shallow-core final state invalid: {error}"));
    let encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("shallow-core save encoding failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("shallow-core save decoding failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("shallow-core trusted load failed: {error}"));
    let loaded_record = loaded
        .geological_knowledge()
        .get_observation(observation)
        .unwrap_or_else(|| panic!("shallow-core observation disappeared across save/load"));
    assert_eq!(loaded_record.evidence(), GeologicalEvidenceKind::CoreSample);
    assert!(loaded_record.excavation_hardness().is_some());
    assert!(loaded_record.resource_mass().is_some());
    assert_eq!(
        calculate_matter_accounting(&loaded)
            .unwrap_or_else(|error| panic!("shallow-core matter audit failed: {error}"))
            .total(),
        matter_before,
        "core-drill construction, sampling, and persistence must conserve matter"
    );
}

#[test]
fn reinforced_sampling_hammer_turns_repeated_point_work_into_bounded_channel_evidence() {
    let registries = build_registries();
    let detailed = registries
        .labor()
        .get_prospecting(PROSPECTING_DETAILED_FIELD_SURVEY)
        .copied()
        .unwrap_or_else(|| panic!("detailed survey definition disappeared"));
    let channel = registries
        .labor()
        .get_prospecting(PROSPECTING_INDEXED_CHANNEL_SURVEY)
        .copied()
        .unwrap_or_else(|| panic!("indexed channel survey definition disappeared"));
    assert_eq!(detailed.maximum_region_voxels(), 1);
    assert!(channel.maximum_region_voxels() > detailed.maximum_region_voxels());
    assert_eq!(
        detailed.spatial_resolution(),
        ProspectingSpatialResolution::AggregateRegion
    );
    assert_eq!(
        channel.spatial_resolution(),
        ProspectingSpatialResolution::PerVoxel
    );
    assert_eq!(
        channel.abundance_uncertainty_ppm(),
        detailed.abundance_uncertainty_ppm()
    );
    assert_eq!(
        channel.excavation_hardness_resolution(),
        detailed.excavation_hardness_resolution()
    );
    let hardness_resolution = detailed
        .excavation_hardness_resolution()
        .unwrap_or_else(|| panic!("detailed sampling lost hardness resolution"));
    assert!(
        !hardness_resolution.is_zero(),
        "detailed sampling must provide finite nonzero hardness resolution"
    );
    assert!(channel.duration() > detailed.duration());
    assert!(
        channel.duration().value()
            < detailed
                .duration()
                .value()
                .checked_mul(
                    u64::try_from(channel.maximum_region_voxels())
                        .unwrap_or_else(|_| panic!("channel survey voxel count exceeds u64")),
                )
                .unwrap_or_else(|| panic!("repeated point-survey duration overflowed")),
        "one indexed channel survey must reduce attention versus repeated detailed point surveys"
    );
    let detailed_tool = detailed
        .equipment()
        .unwrap_or_else(|| panic!("detailed survey lost its sampling-instrument requirement"));
    let channel_tool = channel
        .equipment()
        .unwrap_or_else(|| panic!("channel survey lost its sampling-instrument requirement"));
    assert!(detailed_tool.accepts(EQUIPMENT_STONE_GEOLOGICAL_HAMMER));
    assert!(detailed_tool.accepts(EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER));
    assert!(!channel_tool.accepts(EQUIPMENT_STONE_GEOLOGICAL_HAMMER));
    assert!(channel_tool.accepts(EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER));
    let base_wear = detailed_tool
        .condition_wear_ppm_per_active_tick(EQUIPMENT_STONE_GEOLOGICAL_HAMMER)
        .unwrap_or_else(|| panic!("stone hammer lost detailed-sampling wear"));
    let reinforced_wear = detailed_tool
        .condition_wear_ppm_per_active_tick(EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER)
        .unwrap_or_else(|| panic!("reinforced hammer lost detailed-sampling wear"));
    assert_eq!(
        reinforced_wear
            .checked_mul(2)
            .unwrap_or_else(|| panic!("reinforced sampling wear ratio overflowed")),
        base_wear,
        "copper reinforcement must halve detailed-sampling wear"
    );

    let base_hammer = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_GEOLOGICAL_HAMMER)
        .unwrap_or_else(|| panic!("stone geological hammer definition disappeared"));
    let base_assembly = base_hammer
        .assembly_profile()
        .unwrap_or_else(|| panic!("stone geological hammer lost authored assembly"));
    let reinforced_hammer = registries
        .equipment()
        .get_equipment(EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER)
        .unwrap_or_else(|| panic!("reinforced geological hammer definition disappeared"));
    let reinforcement = reinforced_hammer
        .upgrade_profile()
        .unwrap_or_else(|| panic!("reinforced geological hammer lost authored upgrade"));
    assert_eq!(reinforcement.from(), EQUIPMENT_STONE_GEOLOGICAL_HAMMER);

    let mut state = AppState::new();
    let hammer_source = add_solid_stockpile(&mut state, base_assembly.input_mass());
    for input in base_assembly.inputs() {
        seed_lot(
            &registries,
            &mut state,
            hammer_source,
            input.commodity(),
            input.mass(),
            ROOM_TEMPERATURE,
        );
    }
    let reinforcement_source =
        add_solid_stockpile(&mut state, reinforcement.additions().input_mass());
    for input in reinforcement.additions().inputs() {
        seed_lot(
            &registries,
            &mut state,
            reinforcement_source,
            input.commodity(),
            input.mass(),
            ROOM_TEMPERATURE,
        );
    }
    let recovery = add_solid_stockpile(&mut state, reinforced_hammer.mass());
    let deposit_hardness = Pressure::from_pascals(350_000_000);
    let expected_hardness = ExcavationHardnessEstimate::new(
        deposit_hardness
            .checked_sub(hardness_resolution)
            .unwrap_or_else(|| panic!("sampling hardness resolution exceeds fixture hardness")),
        deposit_hardness,
    )
    .unwrap_or_else(|error| panic!("sampling hardness expectation failed: {error}"));
    let detailed_region = horizontal_region(0, 1);
    seed_geological_deposit(
        &registries,
        &mut state,
        GeologicalDepositSeed::new(
            detailed_region,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Mass::from_milligrams(1_000_000),
            ROOM_TEMPERATURE,
            deposit_hardness,
            copper_ore_composition(400_000, 300_000),
        ),
    );
    let channel_width = i64::try_from(channel.maximum_region_voxels())
        .unwrap_or_else(|_| panic!("channel survey width exceeds i64"));
    let channel_region = horizontal_region(10, channel_width);
    let hidden_target = horizontal_region(10 + channel_width / 2, 1);
    seed_geological_deposit(
        &registries,
        &mut state,
        GeologicalDepositSeed::new(
            hidden_target,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Mass::from_milligrams(1_000_000),
            ROOM_TEMPERATURE,
            deposit_hardness,
            copper_ore_composition(500_000, 250_000),
        ),
    );
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("prospecting-instrument matter setup failed: {error}"))
        .total();

    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("prospecting-instrument survival setup failed: {error}"));
    let hammer = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
        hammer_source,
    )
    .unwrap_or_else(|error| panic!("sampling-hammer assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("sampling-hammer assembly commit failed: {error}"));

    assert!(matches!(
        validate_start_field_prospecting(
            &registries,
            &state,
            FieldProspectingRequest::new(
                PROSPECTING_DETAILED_FIELD_SURVEY,
                detailed_region,
                MATERIAL_COPPER,
            ),
        ),
        Err(FieldProspectingStartError::EquipmentRequired {
            method: PROSPECTING_DETAILED_FIELD_SURVEY,
        })
    ));
    assert!(matches!(
        validate_start_field_prospecting(
            &registries,
            &state,
            FieldProspectingRequest::new_with_equipment(
                PROSPECTING_INDEXED_CHANNEL_SURVEY,
                channel_region,
                MATERIAL_COPPER,
                hammer,
            ),
        ),
        Err(FieldProspectingStartError::EquipmentDefinitionNotAccepted {
            method: PROSPECTING_INDEXED_CHANNEL_SURVEY,
            equipment,
        }) if equipment == hammer
    ));

    let detailed_start = validate_start_field_prospecting(
        &registries,
        &state,
        FieldProspectingRequest::new_with_equipment(
            PROSPECTING_DETAILED_FIELD_SURVEY,
            detailed_region,
            MATERIAL_COPPER,
            hammer,
        ),
    )
    .unwrap_or_else(|error| panic!("detailed hammer survey failed: {error}"));
    let detailed_work = detailed_start.work();
    let expected_condition_after_detailed = detailed_work.condition_after().unwrap_or_else(|| {
        panic!("detailed survey lost its validated equipment condition outcome")
    });
    detailed_start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("detailed hammer survey commit failed: {error}"));
    let detailed_outcome = complete_prospecting_work(
        &registries,
        &mut state,
        detailed_work,
        "detailed hammer survey",
    );
    assert_eq!(detailed_outcome.region(), detailed_region);
    assert_eq!(
        detailed_outcome.evidence(),
        GeologicalEvidenceKind::ExcavationSample
    );
    let detailed_record = state
        .geological_knowledge()
        .get_observation(detailed_outcome.observation())
        .unwrap_or_else(|| panic!("detailed hardness observation disappeared"));
    assert_eq!(
        detailed_record.excavation_hardness(),
        Some(expected_hardness)
    );
    let condition_after_detailed = state
        .equipment()
        .get_equipment(hammer)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("sampling hammer disappeared after detailed survey"));
    assert_eq!(condition_after_detailed, expected_condition_after_detailed);

    let upgraded = validate_upgrade_equipment(
        &registries,
        &state,
        hammer,
        EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER,
        reinforcement_source,
    )
    .unwrap_or_else(|error| panic!("sampling-hammer reinforcement failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("sampling-hammer reinforcement commit failed: {error}"));
    assert_eq!(upgraded, hammer);
    assert_eq!(
        state
            .equipment()
            .get_equipment(hammer)
            .map(|record| record.condition()),
        Some(condition_after_detailed),
        "sampling-hammer reinforcement must preserve prior wear"
    );
    let reinforced_detailed_start = validate_start_field_prospecting(
        &registries,
        &state,
        FieldProspectingRequest::new_with_equipment(
            PROSPECTING_DETAILED_FIELD_SURVEY,
            detailed_region,
            MATERIAL_COPPER,
            hammer,
        ),
    )
    .unwrap_or_else(|error| panic!("reinforced detailed hammer survey failed: {error}"));
    let reinforced_detailed_after = reinforced_detailed_start
        .work()
        .condition_after()
        .unwrap_or_else(|| panic!("reinforced detailed survey lost its condition outcome"));
    assert_eq!(
        u64::from(
            condition_after_detailed
                .parts_per_million()
                .checked_sub(reinforced_detailed_after.parts_per_million())
                .unwrap_or_else(|| {
                    panic!("reinforced detailed survey cannot improve equipment condition")
                }),
        ),
        u64::from(reinforced_wear)
            .checked_mul(detailed.duration().value())
            .unwrap_or_else(|| panic!("reinforced detailed-survey wear overflowed")),
        "reinforced hammer must apply its authored lower wear through canonical detailed-survey admission"
    );

    let channel_start = validate_start_field_prospecting(
        &registries,
        &state,
        FieldProspectingRequest::new_with_equipment(
            PROSPECTING_INDEXED_CHANNEL_SURVEY,
            channel_region,
            MATERIAL_COPPER,
            hammer,
        ),
    )
    .unwrap_or_else(|error| panic!("indexed channel survey failed: {error}"));
    let channel_work = channel_start.work();
    let expected_condition_after_channel = channel_work
        .condition_after()
        .unwrap_or_else(|| panic!("channel survey lost its validated equipment condition outcome"));
    channel_start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("indexed channel survey commit failed: {error}"));
    assert!(matches!(
        validate_disassemble_equipment(&registries, &state, hammer, recovery),
        Err(EquipmentDisassemblyError::EquipmentBusyProspecting { equipment, .. })
            if equipment == hammer
    ));
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("active channel-survey state audit failed: {error}"));

    let elapsed_before_save = 11_u64;
    for _ in 0..elapsed_before_save {
        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("channel survey pre-save tick failed: {error}"));
        assert_eq!(outcome.field_prospecting(), None);
    }
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("channel survey serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("channel survey decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("channel survey load failed: {error}"));
    assert_eq!(loaded, state);

    let mut channel_outcome = None;
    while state.tick() < channel_work.completes_at() {
        let expected = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("channel survey source tick failed: {error}"));
        let actual = advance_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("channel survey loaded tick failed: {error}"));
        assert_eq!(actual, expected);
        channel_outcome = expected.field_prospecting();
    }
    assert_eq!(loaded, state);
    let channel_outcome =
        channel_outcome.unwrap_or_else(|| panic!("indexed channel survey produced no observation"));
    assert_eq!(channel_outcome.region(), channel_region);
    assert_eq!(
        u128::from(channel_outcome.observation_count()),
        channel.maximum_region_voxels()
    );
    let channel_observations = channel_outcome
        .observations()
        .map(|observation| {
            state
                .geological_knowledge()
                .get_observation(observation)
                .unwrap_or_else(|| panic!("indexed channel observation disappeared"))
        })
        .collect::<Vec<_>>();
    assert!(
        channel_observations
            .iter()
            .all(|observation| observation.region().voxel_count() == Some(1)),
        "indexed channel survey must persist one acquired evidence record per covered voxel"
    );
    assert!(
        channel_observations
            .iter()
            .any(|observation| observation.region() == hidden_target),
        "indexed channel survey must include the explicitly covered target voxel"
    );
    let target_observation = channel_observations
        .iter()
        .find(|observation| observation.region() == hidden_target)
        .copied()
        .unwrap_or_else(|| panic!("indexed target observation disappeared"));
    assert_eq!(
        target_observation.excavation_hardness(),
        Some(expected_hardness)
    );
    assert!(
        channel_observations
            .iter()
            .filter(|observation| observation.region() != hidden_target)
            .all(|observation| observation.excavation_hardness().is_none()),
        "indexed blank cells must not leak hidden geological presence through a hardness side channel"
    );
    let condition_after_channel = state
        .equipment()
        .get_equipment(hammer)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("reinforced hammer disappeared after channel survey"));
    assert_eq!(condition_after_channel, expected_condition_after_channel);
    let resolved_target = resolve_mining_target(
        &state,
        MiningTargetRequest::new(hidden_target, MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| {
        panic!("indexed channel evidence did not resolve covered target: {error}")
    });
    assert_eq!(resolved_target.region(), hidden_target);
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("prospecting-instrument matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("prospecting-instrument final state audit failed: {error}"));
}
