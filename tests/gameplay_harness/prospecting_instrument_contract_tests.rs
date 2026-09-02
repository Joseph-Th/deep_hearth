//! Material-backed prospecting-instrument and channel-survey contracts.

use deep_hearth::content::gameplay_fixture::{
    GeologicalDepositSeed, seed_geological_deposit, seed_lot,
};
use deep_hearth::content::{
    EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER, EQUIPMENT_STONE_GEOLOGICAL_HAMMER, FORM_HANDLE,
    FORM_ORE, FORM_REINFORCEMENT, FORM_TOOL, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
    PROSPECTING_DETAILED_FIELD_SURVEY, PROSPECTING_INDEXED_CHANNEL_SURVEY, build_registries,
};
use deep_hearth::core::quantity::{Mass, Pressure};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::WorldSeed;
use deep_hearth::equipment::{
    EquipmentDisassemblyError, validate_assemble_equipment, validate_disassemble_equipment,
    validate_upgrade_equipment,
};
use deep_hearth::geology::{
    FieldProspectingRequest, FieldProspectingStartError, validate_start_field_prospecting,
};
use deep_hearth::maintenance::Condition;
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

fn horizontal_region(start_x: i64, width: i64) -> VoxelBounds {
    VoxelBounds::new(
        VoxelCoord::new(start_x, -1, 0),
        VoxelCoord::new(start_x + width, 0, 1),
    )
    .unwrap_or_else(|error| panic!("prospecting-instrument region failed: {error}"))
}

fn expected_condition_after(before: Condition, wear_ppm_per_tick: u32, duration: u64) -> Condition {
    let total_wear = u64::from(wear_ppm_per_tick)
        .checked_mul(duration)
        .unwrap_or_else(|| panic!("prospecting-instrument wear calculation overflowed"));
    let remaining = u64::from(before.parts_per_million())
        .checked_sub(total_wear)
        .unwrap_or_else(|| panic!("prospecting-instrument fixture exceeds tool lifetime"));
    Condition::new(
        u32::try_from(remaining)
            .unwrap_or_else(|_| unreachable!("bounded condition remains within u32")),
    )
    .unwrap_or_else(|error| panic!("prospecting-instrument condition failed: {error}"))
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
        channel.abundance_uncertainty_ppm(),
        detailed.abundance_uncertainty_ppm()
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

    let mut state = AppState::new(WorldSeed::new(0x51A2_5A6D_504C_4501));
    let hammer_source = add_solid_stockpile(&mut state, Mass::from_milligrams(650_000));
    seed_lot(
        &registries,
        &mut state,
        hammer_source,
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(500_000),
        ROOM_TEMPERATURE,
    );
    seed_lot(
        &registries,
        &mut state,
        hammer_source,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        Mass::from_milligrams(150_000),
        ROOM_TEMPERATURE,
    );
    let reinforcement_source = add_solid_stockpile(&mut state, Mass::from_milligrams(20_000));
    seed_lot(
        &registries,
        &mut state,
        reinforcement_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        Mass::from_milligrams(20_000),
        ROOM_TEMPERATURE,
    );
    let recovery = add_solid_stockpile(&mut state, Mass::from_milligrams(670_000));
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

    let condition_before_detailed = state
        .equipment()
        .get_equipment(hammer)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("sampling hammer disappeared before detailed survey"));
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
    detailed_start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("detailed hammer survey commit failed: {error}"));
    let detailed_outcome =
        complete_prospecting(&registries, &mut state, detailed.duration().value());
    assert_eq!(detailed_outcome.region(), detailed_region);
    let condition_after_detailed = state
        .equipment()
        .get_equipment(hammer)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("sampling hammer disappeared after detailed survey"));
    assert_eq!(
        condition_after_detailed,
        expected_condition_after(
            condition_before_detailed,
            detailed_tool.condition_wear_ppm_per_active_tick(),
            detailed.duration().value(),
        )
    );

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

    let condition_before_channel = condition_after_detailed;
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
    let condition_after_channel = state
        .equipment()
        .get_equipment(hammer)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("reinforced hammer disappeared after channel survey"));
    assert_eq!(
        condition_after_channel,
        expected_condition_after(
            condition_before_channel,
            channel_tool.condition_wear_ppm_per_active_tick(),
            channel.duration().value(),
        )
    );
    assert_eq!(
        resolve_mining_target(
            &state,
            MiningTargetRequest::new(hidden_target, MATERIAL_COPPER),
        ),
        Err(
            MiningTargetResolutionError::EvidenceInsufficientToResolveTarget {
                material: MATERIAL_COPPER,
                region: hidden_target,
            }
        ),
        "channel evidence must narrow a bounded area without revealing the exact hidden ore voxel"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("prospecting-instrument matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("prospecting-instrument final state audit failed: {error}"));
}
