//! Replayable ordinary prospecting-to-mining episode for the cold-agent report.

use std::collections::BTreeMap;

use deep_hearth::capability::CapabilityValue;
use deep_hearth::content::gameplay_fixture::{
    GeologicalDepositSeed, seed_geological_deposit, seed_lot,
};
use deep_hearth::content::{
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
    EQUIPMENT_STONE_GEOLOGICAL_HAMMER, EQUIPMENT_STONE_PICK, EQUIPMENT_STONE_QUARRY_PICK,
    FORM_NATIVE_METAL, FORM_ORE, MATERIAL_COPPER, MINING_METHOD_HAND_PICK,
    PROSPECTING_DETAILED_FIELD_SURVEY, PROSPECTING_FIELD_INSPECTION, PROSPECTING_LOCAL_TRANSECT,
};
use deep_hearth::core::quantity::{Mass, Pressure};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::WorldSeed;
use deep_hearth::equipment::{
    EquipmentDefinitionId, EquipmentId, validate_assemble_equipment, validate_upgrade_equipment,
};
use deep_hearth::geology::{
    ExcavationHardnessEstimate, FieldProspectingOutcome, FieldProspectingRequest,
    GeologicalEvidenceKind, validate_start_field_prospecting,
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
use super::manual_craft_planning::manual_craft_plan_for_output;
use super::ore_fixture::copper_ore_composition;
use super::physical_time::format_physical_duration;
use super::seed::mix64;

const CHANNEL_START_X: i64 = 20;
const CHANNEL_COUNT: i64 = 2;

fn add_mass(
    totals: &mut BTreeMap<CommodityKey, Mass>,
    commodity: CommodityKey,
    mass: Mass,
    context: &'static str,
) {
    let next = totals
        .get(&commodity)
        .copied()
        .unwrap_or(Mass::ZERO)
        .checked_add(mass)
        .unwrap_or_else(|| panic!("fieldwork {context} mass overflowed"));
    totals.insert(commodity, next);
}

fn multiplied_mass(mass: Mass, batches: u64, context: &'static str) -> Mass {
    Mass::from_milligrams(
        mass.milligrams()
            .checked_mul(batches)
            .unwrap_or_else(|| panic!("fieldwork {context} mass overflowed")),
    )
}

fn equipment_component_requirements(
    registries: &Registries,
    equipment_definitions: &[EquipmentDefinitionId],
) -> BTreeMap<CommodityKey, Mass> {
    let mut requirements = BTreeMap::new();
    for &equipment in equipment_definitions {
        let assembly = registries
            .equipment()
            .get_equipment(equipment)
            .and_then(|definition| definition.assembly_profile())
            .unwrap_or_else(|| {
                panic!(
                    "fieldwork equipment {} lost its ordinary authored assembly",
                    equipment.value()
                )
            });
        for input in assembly.inputs() {
            add_mass(
                &mut requirements,
                input.commodity(),
                input.mass(),
                "tool-component requirement",
            );
        }
    }
    requirements
}

fn fieldwork_raw_opportunity(registries: &Registries) -> (BTreeMap<CommodityKey, Mass>, Mass) {
    let mut raw = BTreeMap::new();
    let mut parts_capacity = Mass::ZERO;
    for (commodity, required) in equipment_component_requirements(
        registries,
        &[
            EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
            EQUIPMENT_STONE_QUARRY_PICK,
            EQUIPMENT_STONE_PICK,
        ],
    ) {
        let (craft, batches) = manual_craft_plan_for_output(
            registries,
            commodity,
            required,
            "field-tool component planning",
        );
        let consumed = multiplied_mass(craft.input_mass(), batches, "field-tool raw input");
        add_mass(
            &mut raw,
            craft.input(),
            consumed,
            "field-tool raw opportunity",
        );
        parts_capacity = parts_capacity
            .checked_add(consumed)
            .unwrap_or_else(|| panic!("fieldwork parts capacity overflowed"));
    }

    for (target, expected_base) in [
        (
            EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
            EQUIPMENT_STONE_QUARRY_PICK,
        ),
        (EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_STONE_PICK),
    ] {
        let upgrade = registries
            .equipment()
            .get_equipment(target)
            .and_then(|definition| definition.upgrade_profile())
            .unwrap_or_else(|| {
                panic!(
                    "fieldwork reinforced equipment {} lost its authored upgrade",
                    target.value()
                )
            });
        assert_eq!(upgrade.from(), expected_base);
        for input in upgrade.additions().inputs() {
            let (craft, batches) = manual_craft_plan_for_output(
                registries,
                input.commodity(),
                input.mass(),
                "fieldwork reinforcement planning",
            );
            let upgrade_raw =
                multiplied_mass(craft.input_mass(), batches, "reinforcement raw input");
            add_mass(
                &mut raw,
                craft.input(),
                upgrade_raw,
                "reinforcement raw opportunity",
            );
            parts_capacity = parts_capacity
                .checked_add(upgrade_raw)
                .unwrap_or_else(|| panic!("fieldwork reinforcement parts capacity overflowed"));
        }
    }
    (raw, parts_capacity)
}

#[derive(Clone, Copy)]
struct FieldworkMiningLimits {
    base_quarry_hardness: Pressure,
    reinforced_quarry_hardness: Pressure,
    reinforced_pick_hardness: Pressure,
    base_quarry_batch: Mass,
    reinforced_pick_batch: Mass,
}

fn fieldwork_mining_limits(registries: &Registries) -> FieldworkMiningLimits {
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("fieldwork hand-pick mining method disappeared"));
    let resolve = |equipment| {
        registries
            .equipment()
            .get_equipment(equipment)
            .unwrap_or_else(|| {
                panic!(
                    "fieldwork quarry equipment {} disappeared",
                    equipment.value()
                )
            })
    };
    let base = resolve(EQUIPMENT_STONE_QUARRY_PICK);
    let reinforced = resolve(EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK);
    let hard_pick = resolve(EQUIPMENT_COPPER_REINFORCED_PICK);
    let CapabilityValue::Pressure(base_hardness) = base
        .capabilities()
        .get_capability(method.max_hardness_capability())
        .unwrap_or_else(|| panic!("fieldwork stone quarry pick lost mining-hardness capability"))
    else {
        panic!("fieldwork stone quarry hardness capability changed physical kind")
    };
    let CapabilityValue::Pressure(reinforced_hardness) = reinforced
        .capabilities()
        .get_capability(method.max_hardness_capability())
        .unwrap_or_else(|| {
            panic!("fieldwork reinforced quarry pick lost mining-hardness capability")
        })
    else {
        panic!("fieldwork reinforced quarry hardness capability changed physical kind")
    };
    let CapabilityValue::Mass(base_batch) = base
        .capabilities()
        .get_capability(method.max_batch_mass_capability())
        .unwrap_or_else(|| panic!("fieldwork stone quarry pick lost mining-batch capability"))
    else {
        panic!("fieldwork stone quarry batch capability changed physical kind")
    };
    let CapabilityValue::Pressure(hard_pick_hardness) = hard_pick
        .capabilities()
        .get_capability(method.max_hardness_capability())
        .unwrap_or_else(|| panic!("fieldwork reinforced pick lost mining-hardness capability"))
    else {
        panic!("fieldwork reinforced pick hardness capability changed physical kind")
    };
    let CapabilityValue::Mass(hard_pick_batch) = hard_pick
        .capabilities()
        .get_capability(method.max_batch_mass_capability())
        .unwrap_or_else(|| panic!("fieldwork reinforced pick lost mining-batch capability"))
    else {
        panic!("fieldwork reinforced pick batch capability changed physical kind")
    };
    assert!(
        reinforced_hardness > base_hardness,
        "fieldwork requires quarry reinforcement to open a harder geological opportunity"
    );
    assert!(
        hard_pick_hardness > reinforced_hardness,
        "fieldwork requires the light reinforced pick to retain a distinct hard-rock niche"
    );
    FieldworkMiningLimits {
        base_quarry_hardness: base_hardness,
        reinforced_quarry_hardness: reinforced_hardness,
        reinforced_pick_hardness: hard_pick_hardness,
        base_quarry_batch: base_batch,
        reinforced_pick_batch: hard_pick_batch,
    }
}

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

fn craft_equipment_components(
    registries: &Registries,
    state: &mut AppState,
    raw: deep_hearth::inventory::StockpileId,
    parts: deep_hearth::inventory::StockpileId,
    equipment_definitions: &[EquipmentDefinitionId],
    context: &'static str,
) -> u64 {
    let mut ticks = 0_u64;
    for (commodity, required) in equipment_component_requirements(registries, equipment_definitions)
    {
        let (craft, batches) =
            manual_craft_plan_for_output(registries, commodity, required, context);
        let duration = execute_manual_craft_batches(
            registries,
            state,
            craft.process(),
            raw,
            parts,
            batches,
            context,
        );
        ticks = ticks
            .checked_add(duration.value())
            .unwrap_or_else(|| panic!("fieldwork tool preparation duration overflowed"));
    }
    ticks
}

fn assemble_reinforced_hard_pick(
    registries: &Registries,
    state: &mut AppState,
    raw: deep_hearth::inventory::StockpileId,
    parts: deep_hearth::inventory::StockpileId,
) -> (EquipmentId, u64) {
    let component_ticks = craft_equipment_components(
        registries,
        state,
        raw,
        parts,
        &[EQUIPMENT_STONE_PICK],
        "fieldwork hard-pick components",
    );
    let pick = validate_assemble_equipment(registries, state, EQUIPMENT_STONE_PICK, parts)
        .unwrap_or_else(|error| panic!("fieldwork hard-pick assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("fieldwork hard-pick assembly commit failed: {error}"));
    let reinforcement_ticks = craft_upgrade_additions(
        registries,
        state,
        raw,
        parts,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        "fieldwork hard-pick reinforcement",
    );
    let upgraded = validate_upgrade_equipment(
        registries,
        state,
        pick,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        parts,
    )
    .unwrap_or_else(|error| panic!("fieldwork hard-pick upgrade failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("fieldwork hard-pick upgrade commit failed: {error}"));
    assert_eq!(upgraded, pick);
    (
        pick,
        component_ticks
            .checked_add(reinforcement_ticks)
            .unwrap_or_else(|| panic!("fieldwork hard-pick adaptation duration overflowed")),
    )
}

fn assemble_sampling_hammer(
    registries: &Registries,
    state: &mut AppState,
    raw: deep_hearth::inventory::StockpileId,
    parts: deep_hearth::inventory::StockpileId,
) -> (EquipmentId, u64) {
    let setup_ticks = craft_equipment_components(
        registries,
        state,
        raw,
        parts,
        &[EQUIPMENT_STONE_GEOLOGICAL_HAMMER],
        "fieldwork sampling-hammer components",
    );
    let hammer =
        validate_assemble_equipment(registries, state, EQUIPMENT_STONE_GEOLOGICAL_HAMMER, parts)
            .unwrap_or_else(|error| panic!("fieldwork sampling-hammer assembly failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| {
                panic!("fieldwork sampling-hammer assembly commit failed: {error}")
            });
    (hammer, setup_ticks)
}

fn assemble_quarry_pick(
    registries: &Registries,
    state: &mut AppState,
    raw: deep_hearth::inventory::StockpileId,
    parts: deep_hearth::inventory::StockpileId,
) -> (EquipmentId, u64) {
    let setup_ticks = craft_equipment_components(
        registries,
        state,
        raw,
        parts,
        &[EQUIPMENT_STONE_QUARRY_PICK],
        "fieldwork quarry-pick components",
    );
    let quarry = validate_assemble_equipment(registries, state, EQUIPMENT_STONE_QUARRY_PICK, parts)
        .unwrap_or_else(|error| panic!("fieldwork quarry-pick assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("fieldwork quarry-pick assembly commit failed: {error}"));
    (quarry, setup_ticks)
}

fn craft_upgrade_additions(
    registries: &Registries,
    state: &mut AppState,
    raw: deep_hearth::inventory::StockpileId,
    parts: deep_hearth::inventory::StockpileId,
    target: EquipmentDefinitionId,
    context: &'static str,
) -> u64 {
    let upgrade = registries
        .equipment()
        .get_equipment(target)
        .and_then(|definition| definition.upgrade_profile())
        .unwrap_or_else(|| {
            panic!(
                "fieldwork reinforced equipment {} lost its authored upgrade",
                target.value()
            )
        });
    let mut ticks = 0_u64;
    for input in upgrade.additions().inputs() {
        let (craft, batches) =
            manual_craft_plan_for_output(registries, input.commodity(), input.mass(), context);
        let duration = execute_manual_craft_batches(
            registries,
            state,
            craft.process(),
            raw,
            parts,
            batches,
            context,
        );
        ticks = ticks
            .checked_add(duration.value())
            .unwrap_or_else(|| panic!("fieldwork reinforcement duration overflowed"));
    }
    ticks
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

pub(super) fn run_fieldwork_probe(registries: &Registries, case: FocusedProbeCase) {
    let seed = case.seed();
    let channel_voxels = i64::try_from(
        registries
            .labor()
            .get_prospecting(PROSPECTING_LOCAL_TRANSECT)
            .map(|definition| definition.maximum_region_voxels())
            .unwrap_or_else(|| panic!("fieldwork local-transect definition disappeared")),
    )
    .unwrap_or_else(|_| panic!("fieldwork transect span exceeds coordinate range"));
    assert!(channel_voxels > 0);
    let hidden_channel =
        i64::try_from(mix64(seed ^ 0x4649_454C_4443_484E) % u64::try_from(CHANNEL_COUNT).unwrap())
            .unwrap_or_else(|_| unreachable!("fieldwork channel is bounded"));
    let hidden_slot =
        i64::try_from(mix64(seed ^ 0x4649_454C_4453_4C4F) % u64::try_from(channel_voxels).unwrap())
            .unwrap_or_else(|_| unreachable!("fieldwork slot is bounded"));
    let mining_limits = fieldwork_mining_limits(registries);
    let hardness_tier = mix64(seed ^ 0x4649_454C_4448_4152) % 3;
    let base_pa = mining_limits.base_quarry_hardness.pascals();
    let reinforced_quarry_pa = mining_limits.reinforced_quarry_hardness.pascals();
    let reinforced_pick_pa = mining_limits.reinforced_pick_hardness.pascals();
    let (geology_label, excavation_hardness) = match hardness_tier {
        0 => {
            let floor = base_pa.saturating_mul(3) / 4;
            let span = base_pa - floor;
            (
                "quarry-soft",
                Pressure::from_pascals(floor + mix64(seed ^ 0x4649_454C_4453_4F46) % (span + 1)),
            )
        }
        1 => {
            let gap = reinforced_quarry_pa
                .checked_sub(base_pa)
                .unwrap_or_else(|| {
                    unreachable!("reinforced quarry hardness exceeds base hardness")
                });
            (
                "quarry-reinforcement",
                Pressure::from_pascals(base_pa + 1 + mix64(seed ^ 0x4649_454C_444D_4544) % gap),
            )
        }
        2 => {
            let gap = reinforced_pick_pa
                .checked_sub(reinforced_quarry_pa)
                .unwrap_or_else(|| {
                    unreachable!("reinforced pick hardness exceeds reinforced quarry hardness")
                });
            (
                "hard-pick-specialist",
                Pressure::from_pascals(
                    reinforced_quarry_pa + 1 + mix64(seed ^ 0x4649_454C_4448_4152) % gap,
                ),
            )
        }
        _ => unreachable!("three fieldwork hardness tiers are exhaustive"),
    };
    let copper_ppm = 350_000 + (mix64(seed ^ 0x4649_454C_4447_5241) % 300_001) as u32;
    let clay_share_ppm = (mix64(seed ^ 0x4649_454C_4443_4C41) % 600_001) as u32;
    let minimum_mine_mass = (mining_limits.base_quarry_batch.milligrams() / 2).max(1);
    let requested_mine_mass = Mass::from_milligrams(
        minimum_mine_mass
            + mix64(seed ^ 0x4649_454C_444D_4153)
                % (mining_limits.base_quarry_batch.milligrams() - minimum_mine_mass + 1),
    );
    let deposit_mass = mining_limits
        .base_quarry_batch
        .checked_add(mining_limits.base_quarry_batch)
        .unwrap_or_else(|| panic!("fieldwork deposit mass overflowed"));

    let mut state = AppState::new(WorldSeed::new(seed ^ 0x4649_454C_4457_524C));
    let (raw_opportunity, parts_capacity) = fieldwork_raw_opportunity(registries);
    let raw_capacity = raw_opportunity
        .values()
        .copied()
        .try_fold(Mass::ZERO, |total, mass| total.checked_add(mass))
        .unwrap_or_else(|| panic!("fieldwork raw opportunity capacity overflowed"));
    let raw = add_solid_stockpile(&mut state, raw_capacity);
    for (commodity, mass) in raw_opportunity {
        seed_lot(
            registries,
            &mut state,
            raw,
            commodity,
            mass,
            ROOM_TEMPERATURE,
        );
    }
    let parts = add_solid_stockpile(&mut state, parts_capacity);
    let destination = add_solid_stockpile(&mut state, deposit_mass);
    let hidden_region = horizontal_region(
        CHANNEL_START_X + hidden_channel * channel_voxels + hidden_slot,
        1,
    );
    seed_geological_deposit(
        registries,
        &mut state,
        GeologicalDepositSeed::new(
            hidden_region,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            deposit_mass,
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

    let (hammer, sampling_setup_ticks) =
        assemble_sampling_hammer(registries, &mut state, raw, parts);
    let (target, observed_hardness, transects, field_inspections, detailed_surveys) =
        localize_target(registries, &mut state, hammer, channel_voxels);
    assert!(
        observed_hardness.lower() <= excavation_hardness
            && observed_hardness.upper() >= excavation_hardness,
        "actor-visible hardness band must conservatively contain diagnostic geological truth"
    );
    let mining_start = if observed_hardness.upper() <= mining_limits.base_quarry_hardness {
        let (quarry, tool_prep_ticks) = assemble_quarry_pick(registries, &mut state, raw, parts);
        let start = validate_start_mining(
            registries,
            &state,
            MINING_METHOD_HAND_PICK,
            target,
            destination,
            quarry,
            requested_mine_mass,
        )
        .unwrap_or_else(|error| {
            panic!("sample-selected stone quarry pick unexpectedly failed mining: {error}")
        });
        (
            start,
            quarry,
            "stone-quarry",
            "sampled-hardness-base-quarry",
            tool_prep_ticks,
            requested_mine_mass,
        )
    } else if observed_hardness.upper() <= mining_limits.reinforced_quarry_hardness {
        let (mut quarry, quarry_ticks) = assemble_quarry_pick(registries, &mut state, raw, parts);
        let reinforcement_ticks = craft_upgrade_additions(
            registries,
            &mut state,
            raw,
            parts,
            EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
            "fieldwork sampled-hardness quarry reinforcement",
        );
        quarry = validate_upgrade_equipment(
            registries,
            &state,
            quarry,
            EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
            parts,
        )
        .unwrap_or_else(|error| panic!("fieldwork quarry reinforcement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("fieldwork quarry reinforcement commit failed: {error}"));
        let tool_prep_ticks = quarry_ticks
            .checked_add(reinforcement_ticks)
            .unwrap_or_else(|| panic!("fieldwork quarry tool-prep duration overflowed"));
        let start = validate_start_mining(
            registries,
            &state,
            MINING_METHOD_HAND_PICK,
            target,
            destination,
            quarry,
            requested_mine_mass,
        )
        .unwrap_or_else(|error| {
            panic!("sample-selected reinforced quarry unexpectedly failed mining: {error}")
        });
        (
            start,
            quarry,
            "copper-reinforced-quarry",
            "sampled-hardness-quarry-upgrade",
            tool_prep_ticks,
            requested_mine_mass,
        )
    } else {
        assert!(
            observed_hardness.upper() <= mining_limits.reinforced_pick_hardness,
            "fieldwork sampled hardness exceeds every ordinary extraction tool in the episode"
        );
        let (hard_pick, tool_prep_ticks) =
            assemble_reinforced_hard_pick(registries, &mut state, raw, parts);
        match validate_start_mining(
            registries,
            &state,
            MINING_METHOD_HAND_PICK,
            target,
            destination,
            hard_pick,
            requested_mine_mass,
        ) {
            Ok(start) => (
                start,
                hard_pick,
                "copper-reinforced-hard-pick",
                "sampled-hardness-hard-pick",
                tool_prep_ticks,
                requested_mine_mass,
            ),
            Err(MiningStartError::BatchTooLarge { maximum, .. }) => {
                assert_eq!(
                    maximum, mining_limits.reinforced_pick_batch,
                    "hard-pick batch blocker must expose the current authored specialist limit"
                );
                let start = validate_start_mining(
                    registries,
                    &state,
                    MINING_METHOD_HAND_PICK,
                    target,
                    destination,
                    hard_pick,
                    maximum,
                )
                .unwrap_or_else(|error| panic!("batch-adapted hard-pick mining failed: {error}"));
                (
                    start,
                    hard_pick,
                    "copper-reinforced-hard-pick",
                    "sampled-hardness-hard-pick+batch-limit",
                    tool_prep_ticks,
                    maximum,
                )
            }
            Err(error) => panic!("sample-selected hard-pick mining failed unexpectedly: {error}"),
        }
    };
    let (start, mining_equipment, quarry_label, adaptation, tool_prep_ticks, extracted_mass) =
        mining_start;
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
    assert_eq!(receipt.output().mass(), extracted_mass);
    assert_eq!(
        state
            .equipment()
            .get_equipment(mining_equipment)
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
    let sampling_setup_time = format_physical_duration(registries, sampling_setup_ticks);
    let tool_prep_time = format_physical_duration(registries, tool_prep_ticks);
    let mining_time = format_physical_duration(registries, mining_ticks);

    reviewln!(
        "FIELDWORK EXPERIENCE seed=0x{seed:016X} sample={} search=compare-local-transects->cheap-inspection->targeted-survey channels={} transects={} selected-channel=observed-strongest field-inspections={} detailed-surveys={} target=acquired-evidence observed-hardness={}..{}Pa geology={geology_label} tool={quarry_label} adaptation={adaptation} sampling-setup={}t/{sampling_setup_time} tool-prep={}t/{tool_prep_time} retained-native-copper={}mg requested={}mg mining={}mg duration={}t/{mining_time} condition={}ppm->{}ppm output-grade={}ppm matter=conserved",
        focused_probe_role_label(case.role()),
        CHANNEL_COUNT,
        transects,
        field_inspections,
        detailed_surveys,
        observed_hardness.lower().pascals(),
        observed_hardness.upper().pascals(),
        sampling_setup_ticks,
        tool_prep_ticks,
        retained_native_copper.milligrams(),
        requested_mine_mass.milligrams(),
        extracted_mass.milligrams(),
        mining_ticks,
        condition_before.parts_per_million(),
        condition_after.parts_per_million(),
        receipt
            .output()
            .composition()
            .parts_per_million(MATERIAL_COPPER),
    );
}
