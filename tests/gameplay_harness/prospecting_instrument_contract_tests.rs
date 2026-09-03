//! Material-backed prospecting-instrument and channel-survey contracts.

use deep_hearth::content::gameplay_fixture::{
    GeologicalDepositSeed, seed_geological_deposit, seed_lot,
};
use deep_hearth::content::{
    EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER, EQUIPMENT_STONE_GEOLOGICAL_HAMMER, FORM_ORE,
    MATERIAL_COPPER, PROSPECTING_DETAILED_FIELD_SURVEY, PROSPECTING_INDEXED_CHANNEL_SURVEY,
    build_registries,
};
use deep_hearth::core::quantity::{Mass, Pressure};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::WorldSeed;
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
use deep_hearth::mining::{MiningTargetRequest, resolve_mining_target};
use deep_hearth::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};
use deep_hearth::survival::initialize_player_survival;

use super::environment::ROOM_TEMPERATURE;
use super::inventory_support::add_solid_stockpile;
use super::ore_fixture::copper_ore_composition;

fn horizontal_region(start_x: i64, width: i64) -> VoxelBounds {
    VoxelBounds::new(
        VoxelCoord::new(start_x, -1, 0),
        VoxelCoord::new(start_x + width, 0, 1),
    )
    .unwrap_or_else(|error| panic!("prospecting-instrument region failed: {error}"))
}

fn complete_prospecting(
    registries: &deep_hearth::registry::Registries,
    state: &mut AppState,
    duration: u64,
) -> deep_hearth::geology::FieldProspectingOutcome {
    let mut completion = None;
    for elapsed in 1..=duration {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("prospecting-instrument tick failed: {error}"));
        if elapsed < duration {
            assert_eq!(outcome.field_prospecting(), None);
        } else {
            completion = outcome.field_prospecting();
        }
    }
    completion.unwrap_or_else(|| panic!("prospecting-instrument work produced no observation"))
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
    assert_eq!(channel.maximum_region_voxels(), 4);
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
    assert_eq!(
        detailed.excavation_hardness_resolution(),
        Some(Pressure::from_pascals(50_000_000))
    );
    assert!(channel.duration() > detailed.duration());
    assert!(
        channel.duration().value() < detailed.duration().value() * 4,
        "one indexed channel survey must reduce attention versus four repeated detailed point surveys"
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

    let mut state = AppState::new(WorldSeed::new(0x51A2_5A6D_504C_4501));
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
    let detailed_region = horizontal_region(0, 1);
    seed_geological_deposit(
        &registries,
        &mut state,
        GeologicalDepositSeed::new(
            detailed_region,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Mass::from_milligrams(1_000_000),
            ROOM_TEMPERATURE,
            Pressure::from_pascals(350_000_000),
            copper_ore_composition(400_000, 300_000),
        ),
    );
    let channel_region = horizontal_region(10, 4);
    let hidden_target = horizontal_region(12, 1);
    seed_geological_deposit(
        &registries,
        &mut state,
        GeologicalDepositSeed::new(
            hidden_target,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Mass::from_milligrams(1_000_000),
            ROOM_TEMPERATURE,
            Pressure::from_pascals(350_000_000),
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
    let expected_condition_after_detailed =
        detailed_start.work().condition_after().unwrap_or_else(|| {
            panic!("detailed survey lost its validated equipment condition outcome")
        });
    detailed_start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("detailed hammer survey commit failed: {error}"));
    let detailed_outcome =
        complete_prospecting(&registries, &mut state, detailed.duration().value());
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
        Some(
            ExcavationHardnessEstimate::new(
                Pressure::from_pascals(300_000_000),
                Pressure::from_pascals(350_000_000),
            )
            .unwrap_or_else(|error| panic!("detailed hardness expectation failed: {error}"))
        )
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
    let expected_condition_after_channel = channel_start
        .work()
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
    for _ in elapsed_before_save..channel.duration().value() {
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
    assert_eq!(channel_outcome.observation_count(), 4);
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
        Some(
            ExcavationHardnessEstimate::new(
                Pressure::from_pascals(300_000_000),
                Pressure::from_pascals(350_000_000),
            )
            .unwrap_or_else(|error| panic!("channel hardness expectation failed: {error}"))
        )
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
