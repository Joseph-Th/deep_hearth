//! Replayable ordinary prospecting-to-mining episode for the cold-agent report.

use deep_hearth::content::gameplay_fixture::{
    GeologicalDepositSeed, seed_geological_deposit, seed_lot,
};
use deep_hearth::content::{
    EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK, EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
    EQUIPMENT_STONE_QUARRY_PICK, FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL, FORM_ORE, MATERIAL_COPPER,
    MATERIAL_STONE, MATERIAL_WOOD, MINING_METHOD_HAND_PICK, PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
    PROCESS_KNAP_STONE_TOOL, PROCESS_SHAPE_WOOD_HANDLE, PROSPECTING_DETAILED_FIELD_SURVEY,
    PROSPECTING_FIELD_INSPECTION, PROSPECTING_LOCAL_TRANSECT,
};
use deep_hearth::core::quantity::{Mass, Pressure};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::WorldSeed;
use deep_hearth::equipment::{
    EquipmentId, validate_assemble_equipment, validate_upgrade_equipment,
};
use deep_hearth::geology::{
    FieldProspectingOutcome, FieldProspectingRequest, validate_start_field_prospecting,
};
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::mining::{
    MiningStartError, MiningTargetRequest, MiningTargetResolution, MiningTargetResolutionError,
    resolve_mining_target, validate_claim_mining_output, validate_start_mining,
};
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};
use deep_hearth::survival::initialize_player_survival;

use super::environment::ROOM_TEMPERATURE;
use super::focused_runner::focused_probe_role_label;
use super::focused_seeds::FocusedProbeCase;
use super::inventory_support::add_solid_stockpile;
use super::manual_craft_execution::execute_manual_craft_batches;
use super::ore_fixture::copper_ore_composition;
use super::seed::mix64;

const CHANNEL_START_X: i64 = 20;
const CHANNEL_VOXELS: i64 = 4;
const CHANNEL_COUNT: i64 = 2;

fn horizontal_region(start_x: i64, width: i64) -> VoxelBounds {
    VoxelBounds::new(
        VoxelCoord::new(start_x, -1, 0),
        VoxelCoord::new(start_x + width, 0, 1),
    )
    .unwrap_or_else(|error| panic!("fieldwork region failed: {error}"))
}

fn complete_prospecting(
    registries: &Registries,
    state: &mut AppState,
    duration: u64,
    context: &'static str,
) -> FieldProspectingOutcome {
    let mut completion = None;
    for elapsed in 1..=duration {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("fieldwork {context} tick failed: {error}"));
        if elapsed < duration {
            assert_eq!(outcome.field_prospecting(), None);
        } else {
            completion = outcome.field_prospecting();
        }
        assert!(
            outcome.production_completions().is_empty()
                && outcome.ready_mining_jobs().is_empty()
                && outcome.manual_power().is_none(),
            "fieldwork {context} crossed unrelated observable work"
        );
    }
    completion.unwrap_or_else(|| panic!("fieldwork {context} produced no observation"))
}

fn assemble_field_tools(
    registries: &Registries,
    state: &mut AppState,
    raw: deep_hearth::inventory::StockpileId,
    parts: deep_hearth::inventory::StockpileId,
) -> (EquipmentId, EquipmentId, u64) {
    let stone = execute_manual_craft_batches(
        registries,
        state,
        PROCESS_KNAP_STONE_TOOL,
        raw,
        parts,
        3,
        "fieldwork stone tool preparation",
    );
    let handles = execute_manual_craft_batches(
        registries,
        state,
        PROCESS_SHAPE_WOOD_HANDLE,
        raw,
        parts,
        3,
        "fieldwork handle preparation",
    );
    let hammer =
        validate_assemble_equipment(registries, state, EQUIPMENT_STONE_GEOLOGICAL_HAMMER, parts)
            .unwrap_or_else(|error| panic!("fieldwork sampling-hammer assembly failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| {
                panic!("fieldwork sampling-hammer assembly commit failed: {error}")
            });
    let quarry = validate_assemble_equipment(registries, state, EQUIPMENT_STONE_QUARRY_PICK, parts)
        .unwrap_or_else(|error| panic!("fieldwork quarry-pick assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("fieldwork quarry-pick assembly commit failed: {error}"));
    let setup_ticks = stone
        .value()
        .checked_add(handles.value())
        .unwrap_or_else(|| panic!("fieldwork tool preparation duration overflowed"));
    (hammer, quarry, setup_ticks)
}

fn run_survey(
    registries: &Registries,
    state: &mut AppState,
    method: deep_hearth::labor::ProspectingMethodId,
    region: VoxelBounds,
    equipment: Option<EquipmentId>,
    context: &'static str,
) -> FieldProspectingOutcome {
    let definition = registries
        .labor()
        .get_prospecting(method)
        .copied()
        .unwrap_or_else(|| panic!("fieldwork {context} prospecting definition disappeared"));
    let request = match equipment {
        Some(equipment) => {
            FieldProspectingRequest::new_with_equipment(method, region, MATERIAL_COPPER, equipment)
        }
        None => FieldProspectingRequest::new(method, region, MATERIAL_COPPER),
    };
    let start = validate_start_field_prospecting(registries, state, request)
        .unwrap_or_else(|error| panic!("fieldwork {context} start failed: {error}"));
    let expected_condition = start.work().condition_after();
    start
        .commit(state)
        .unwrap_or_else(|error| panic!("fieldwork {context} commit failed: {error}"));
    let outcome = complete_prospecting(registries, state, definition.duration().value(), context);
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

fn localize_target(
    registries: &Registries,
    state: &mut AppState,
    hammer: EquipmentId,
) -> (MiningTargetResolution, u64, u64, u64) {
    let transect_uncertainty = registries
        .labor()
        .get_prospecting(PROSPECTING_LOCAL_TRANSECT)
        .map(|definition| definition.abundance_uncertainty_ppm())
        .unwrap_or_else(|| panic!("fieldwork local-transect definition disappeared"));
    let mut selected_channel = None::<(i64, u32)>;
    let mut transects = 0_u64;
    for channel_index in 0..CHANNEL_COUNT {
        let channel_start = CHANNEL_START_X + channel_index * CHANNEL_VOXELS;
        let channel = horizontal_region(channel_start, CHANNEL_VOXELS);
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
    for offset in 0..CHANNEL_VOXELS {
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
        let detailed_finding = state
            .geological_knowledge()
            .get_observation(detailed.observation())
            .and_then(|record| record.finding(MATERIAL_COPPER))
            .unwrap_or_else(|| panic!("fieldwork detailed copper finding disappeared"));
        assert!(
            detailed_finding.lower_ppm() > 0,
            "fieldwork coarse positive signal must remain positive after detailed refinement"
        );
        let target = resolve_mining_target(state, MiningTargetRequest::new(point, MATERIAL_COPPER))
            .unwrap_or_else(|error| {
                panic!("positive detailed evidence did not resolve target: {error}")
            });
        return (target, transects, field_inspections, detailed_surveys);
    }
    panic!("fieldwork coarse-to-fine search exhausted the promising channel without a target")
}

pub(super) fn run_fieldwork_probe(registries: &Registries, case: FocusedProbeCase) {
    let seed = case.seed();
    let hidden_channel =
        i64::try_from(mix64(seed ^ 0x4649_454C_4443_484E) % u64::try_from(CHANNEL_COUNT).unwrap())
            .unwrap_or_else(|_| unreachable!("fieldwork channel is bounded"));
    let hidden_slot =
        i64::try_from(mix64(seed ^ 0x4649_454C_4453_4C4F) % u64::try_from(CHANNEL_VOXELS).unwrap())
            .unwrap_or_else(|_| unreachable!("fieldwork slot is bounded"));
    let hard_opportunity = mix64(seed ^ 0x4649_454C_4448_4152).is_multiple_of(2);
    let excavation_hardness = Pressure::from_pascals(if hard_opportunity {
        550_000_000
    } else {
        450_000_000
    });
    let copper_ppm = 350_000 + (mix64(seed ^ 0x4649_454C_4447_5241) % 300_001) as u32;
    let clay_share_ppm = (mix64(seed ^ 0x4649_454C_4443_4C41) % 600_001) as u32;
    let mine_mass = Mass::from_milligrams(300_000 + mix64(seed ^ 0x4649_454C_444D_4153) % 150_001);

    let mut state = AppState::new(WorldSeed::new(seed ^ 0x4649_454C_4457_524C));
    let raw = add_solid_stockpile(&mut state, Mass::from_milligrams(6_040_000));
    seed_lot(
        registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(3_000_000),
        ROOM_TEMPERATURE,
    );
    seed_lot(
        registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(3_000_000),
        ROOM_TEMPERATURE,
    );
    seed_lot(
        registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        Mass::from_milligrams(40_000),
        ROOM_TEMPERATURE,
    );
    let parts = add_solid_stockpile(&mut state, Mass::from_milligrams(6_040_000));
    let destination = add_solid_stockpile(&mut state, Mass::from_milligrams(1_000_000));
    let hidden_region = horizontal_region(
        CHANNEL_START_X + hidden_channel * CHANNEL_VOXELS + hidden_slot,
        1,
    );
    seed_geological_deposit(
        registries,
        &mut state,
        GeologicalDepositSeed::new(
            hidden_region,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Mass::from_milligrams(1_000_000),
            ROOM_TEMPERATURE,
            excavation_hardness,
            copper_ore_composition(copper_ppm, clay_share_ppm),
        ),
    );
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("fieldwork initial matter audit failed: {error}"))
        .total();
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("fieldwork survival setup failed: {error}"));

    let (hammer, mut quarry, setup_ticks) =
        assemble_field_tools(registries, &mut state, raw, parts);
    let (target, transects, field_inspections, detailed_surveys) =
        localize_target(registries, &mut state, hammer);
    let mining_start = match validate_start_mining(
        registries,
        &state,
        MINING_METHOD_HAND_PICK,
        target,
        destination,
        quarry,
        mine_mass,
    ) {
        Ok(start) => (start, "stone-quarry", "none", 0_u64),
        Err(MiningStartError::TargetTooHard { .. }) => {
            let reinforcement_ticks = execute_manual_craft_batches(
                registries,
                &mut state,
                PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
                raw,
                parts,
                1,
                "fieldwork hardness-response reinforcement",
            )
            .value();
            quarry = validate_upgrade_equipment(
                registries,
                &state,
                quarry,
                EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
                parts,
            )
            .unwrap_or_else(|error| panic!("fieldwork quarry reinforcement failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("fieldwork quarry reinforcement commit failed: {error}")
            });
            let start = validate_start_mining(
                registries,
                &state,
                MINING_METHOD_HAND_PICK,
                target,
                destination,
                quarry,
                mine_mass,
            )
            .unwrap_or_else(|error| panic!("reinforced fieldwork quarry mining failed: {error}"));
            (
                start,
                "copper-reinforced-quarry",
                "hardness-blocker",
                reinforcement_ticks,
            )
        }
        Err(error) => panic!("fieldwork quarry mining failed unexpectedly: {error}"),
    };
    let (start, quarry_label, adaptation, adaptation_ticks) = mining_start;
    let job = start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("fieldwork mining start commit failed: {error}"));
    let record = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("fieldwork mining job disappeared after start"));
    let mining_ticks = record
        .completes_at()
        .value()
        .checked_sub(record.started_at().value())
        .unwrap_or_else(|| panic!("fieldwork mining duration underflowed"));
    let condition_before = record.equipment_condition_before();
    let condition_after = record.equipment_condition_after();
    for elapsed in 1..=mining_ticks {
        let outcome = advance_tick(registries, &mut state)
            .unwrap_or_else(|error| panic!("fieldwork mining tick failed: {error}"));
        assert_eq!(
            outcome.ready_mining_jobs().contains(&job),
            elapsed == mining_ticks,
            "fieldwork mining readiness diverged from its authoritative schedule"
        );
        assert!(
            outcome.production_completions().is_empty()
                && outcome.manual_power().is_none()
                && outcome.field_prospecting().is_none(),
            "fieldwork mining crossed unrelated observable work"
        );
    }
    let receipt = validate_claim_mining_output(registries, &state, job)
        .unwrap_or_else(|error| panic!("fieldwork mining claim validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("fieldwork mining claim commit failed: {error}"));
    assert_eq!(receipt.output().mass(), mine_mass);
    assert_eq!(
        state
            .equipment()
            .get_equipment(quarry)
            .map(|record| record.condition()),
        Some(condition_after)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("fieldwork final matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("fieldwork final state invalid: {error}"));
    let retained_native_copper = state
        .inventory()
        .get_stockpile(raw)
        .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL)))
        .unwrap_or_else(|| panic!("fieldwork raw stockpile disappeared"));

    std::println!(
        "FIELDWORK EXPERIENCE seed=0x{seed:016X} sample={} search=compare-local-transects->cheap-inspection->targeted-survey channels={} transects={} selected-channel=observed-strongest field-inspections={} detailed-surveys={} target=acquired-evidence quarry={quarry_label} adaptation={adaptation} setup={}t adaptation-work={}t retained-native-copper={}mg mining={}mg duration={}t condition={}ppm->{}ppm output-grade={}ppm matter=conserved",
        focused_probe_role_label(case.role()),
        CHANNEL_COUNT,
        transects,
        field_inspections,
        detailed_surveys,
        setup_ticks,
        adaptation_ticks,
        retained_native_copper.milligrams(),
        mine_mass.milligrams(),
        mining_ticks,
        condition_before.parts_per_million(),
        condition_after.parts_per_million(),
        receipt
            .output()
            .composition()
            .parts_per_million(MATERIAL_COPPER),
    );
}
