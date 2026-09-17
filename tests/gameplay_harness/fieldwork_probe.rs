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
use deep_hearth::core::quantity::{Energy, Mass, Pressure, Volume};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::WorldSeed;
use deep_hearth::crafting::resolve_manual_craft;
use deep_hearth::equipment::{
    EquipmentDefinitionId, EquipmentId, validate_assemble_equipment, validate_upgrade_equipment,
};
use deep_hearth::geology::{
    ExcavationHardnessEstimate, FieldProspectingOutcome, FieldProspectingRequest,
    GeologicalEvidenceKind, validate_start_field_prospecting,
};
use deep_hearth::inventory::StockpileId;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::mining::{
    MiningOrderRequest, MiningStartError, MiningTargetRequest, MiningTargetResolution,
    MiningTargetResolutionError, resolve_mining_order, resolve_mining_target,
    validate_claim_mining_output, validate_start_mining,
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
use super::manual_craft_planning::{
    manual_craft_plan_for_available_output, manual_craft_topology_plan_for_output,
};
use super::manual_craft_selection::{
    first_sufficient_pure_temperature, select_manual_craft_request,
};
use super::ore_fixture::copper_ore_composition;
use super::physical_time::format_physical_duration;
use super::prospecting_timing::complete_prospecting_work;
use super::seed::mix64;

#[path = "fieldwork_probe/extraction.rs"]
mod extraction;
use extraction::{FieldworkExtractionOrder, FieldworkStop, execute_fieldwork_extraction};

#[cfg(test)]
#[path = "fieldwork_probe/supply_tests.rs"]
mod supply_tests;

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
        let (craft, batches) = manual_craft_topology_plan_for_output(
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
            let (craft, batches) = manual_craft_topology_plan_for_output(
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
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FieldworkTool {
    base: EquipmentDefinitionId,
    target: EquipmentDefinitionId,
    label: &'static str,
}

// A bounded actor family, not an exhaustive equipment catalog. Equal observable costs prefer
// the light stone pick, then its reinforcement, then the corresponding heavy quarry tools.
const FIELDWORK_TOOLS: [FieldworkTool; 4] = [
    FieldworkTool {
        base: EQUIPMENT_STONE_PICK,
        target: EQUIPMENT_STONE_PICK,
        label: "stone-pick",
    },
    FieldworkTool {
        base: EQUIPMENT_STONE_PICK,
        target: EQUIPMENT_COPPER_REINFORCED_PICK,
        label: "copper-reinforced-hard-pick",
    },
    FieldworkTool {
        base: EQUIPMENT_STONE_QUARRY_PICK,
        target: EQUIPMENT_STONE_QUARRY_PICK,
        label: "stone-quarry",
    },
    FieldworkTool {
        base: EQUIPMENT_STONE_QUARRY_PICK,
        target: EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
        label: "copper-reinforced-quarry",
    },
];

#[derive(Clone, Debug)]
struct FieldworkToolEstimate {
    tool: FieldworkTool,
    preparation_ticks: u64,
    order_ticks: u64,
    batch: Mass,
    raw: BTreeMap<CommodityKey, Mass>,
}

impl FieldworkToolEstimate {
    fn total_ticks(&self) -> u64 {
        self.preparation_ticks
            .checked_add(self.order_ticks)
            .unwrap_or_else(|| panic!("fieldwork estimated attention overflowed"))
    }

    fn policy_key(&self) -> (u64, Mass, Mass) {
        let copper = self
            .raw
            .get(&CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL))
            .copied()
            .unwrap_or(Mass::ZERO);
        let raw = self
            .raw
            .values()
            .copied()
            .try_fold(Mass::ZERO, Mass::checked_add)
            .unwrap_or_else(|| panic!("fieldwork raw estimate overflowed"));
        (self.total_ticks(), copper, raw)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum FieldworkToolBlocker {
    AcquiredHardness {
        upper: Pressure,
        maximum: Pressure,
    },
    RawInput {
        commodity: CommodityKey,
        required: Mass,
    },
    Order(deep_hearth::mining::MiningOrderError),
}

fn estimate_tool_preparation(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    tool: FieldworkTool,
) -> Result<(u64, BTreeMap<CommodityKey, Mass>), FieldworkToolBlocker> {
    // Assembly and upgrade execute as separate crafts. Preserve their batch rounding rather
    // than merging a shared component into one cheaper hypothetical preparation step.
    let mut requirements: Vec<_> = equipment_component_requirements(registries, &[tool.base])
        .into_iter()
        .collect();
    if tool.target != tool.base {
        let upgrade = registries
            .equipment()
            .get_equipment(tool.target)
            .and_then(|definition| definition.upgrade_profile())
            .unwrap_or_else(|| panic!("fieldwork upgrade disappeared"));
        assert_eq!(upgrade.from(), tool.base);
        for input in upgrade.additions().inputs() {
            requirements.push((input.commodity(), input.mass()));
        }
    }
    let mut raw_required = BTreeMap::new();
    let mut ticks = 0_u64;
    for (commodity, required) in requirements {
        // The declared raw-tool family uses its equipment-free topology route. Missing raw
        // inputs exclude this route; they do not prove that every possible salvage route fails.
        let (craft, batches) = manual_craft_topology_plan_for_output(
            registries,
            commodity,
            required,
            "fieldwork pre-action components",
        );
        let consumed = multiplied_mass(craft.input_mass(), batches, "planned raw input");
        add_mass(
            &mut raw_required,
            craft.input(),
            consumed,
            "planned cumulative raw input",
        );
        let cumulative = raw_required[&craft.input()];
        if first_sufficient_pure_temperature(
            state,
            raw,
            craft.input(),
            cumulative,
            "fieldwork pre-action raw availability",
        )
        .is_none()
        {
            return Err(FieldworkToolBlocker::RawInput {
                commodity: craft.input(),
                required: cumulative,
            });
        }
        let request = select_manual_craft_request(
            registries,
            state,
            craft.process(),
            raw,
            batches,
            "fieldwork pre-action craft",
        );
        let resolution =
            resolve_manual_craft(registries, state, &request).unwrap_or_else(|error| {
                panic!("fieldwork pre-action craft resolution failed: {error}")
            });
        ticks = ticks
            .checked_add(resolution.duration().value())
            .unwrap_or_else(|| panic!("fieldwork preparation estimate overflowed"));
    }
    Ok((ticks, raw_required))
}

fn estimate_fieldwork_tool(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    tool: FieldworkTool,
    observed_upper: Pressure,
    order: Mass,
) -> Result<FieldworkToolEstimate, FieldworkToolBlocker> {
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("fieldwork mining method disappeared"));
    let definition = registries
        .equipment()
        .get_equipment(tool.target)
        .unwrap_or_else(|| panic!("fieldwork candidate disappeared"));
    let capabilities = definition.capabilities();
    let CapabilityValue::Pressure(maximum) = capabilities
        .get_capability(method.max_hardness_capability())
        .unwrap_or_else(|| panic!("fieldwork candidate hardness disappeared"))
    else {
        panic!("fieldwork hardness kind changed")
    };
    if observed_upper > maximum {
        return Err(FieldworkToolBlocker::AcquiredHardness {
            upper: observed_upper,
            maximum,
        });
    }
    let CapabilityValue::Mass(batch) = capabilities
        .get_capability(method.max_batch_mass_capability())
        .unwrap_or_else(|| panic!("fieldwork candidate batch disappeared"))
    else {
        panic!("fieldwork batch kind changed")
    };
    // Planning uses the same sequential wear and per-batch rounding as mining admission.
    // The bounded projection promises effort, not hidden supply or current authorization.
    let projection = resolve_mining_order(
        registries.core().physical_tick_duration(),
        method,
        definition,
        MiningOrderRequest::new(
            deep_hearth::maintenance::Condition::PRISTINE,
            observed_upper,
            order,
            batch,
            256,
        ),
    )
    .map_err(FieldworkToolBlocker::Order)?;
    let (preparation_ticks, raw) = estimate_tool_preparation(registries, state, raw, tool)?;
    Ok(FieldworkToolEstimate {
        tool,
        preparation_ticks,
        order_ticks: projection.duration().value(),
        batch,
        raw,
    })
}

fn choose_fieldwork_tool(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    observed_upper: Pressure,
    order: Mass,
) -> Option<FieldworkToolEstimate> {
    let mut selected: Option<FieldworkToolEstimate> = None;
    for tool in FIELDWORK_TOOLS {
        let estimate = estimate_fieldwork_tool(registries, state, raw, tool, observed_upper, order);
        reviewln!(
            "FIELDWORK CANDIDATE tick={} tool={} observed-upper={}Pa order={}mg estimate={estimate:?} scope=four-raw-build-tools authorization=not-yet assumptions=no-service,unknown-deposit-reserve",
            state.tick().value(),
            tool.label,
            observed_upper.pascals(),
            order.milligrams()
        );
        let Ok(estimate) = estimate else { continue };
        if selected
            .as_ref()
            .is_none_or(|best| estimate.policy_key() < best.policy_key())
        {
            selected = Some(estimate);
        }
    }
    selected
}

fn horizontal_region(start_x: i64, width: i64) -> VoxelBounds {
    VoxelBounds::new(
        VoxelCoord::new(start_x, -1, 0),
        VoxelCoord::new(start_x + width, 0, 1),
    )
    .unwrap_or_else(|error| panic!("fieldwork region failed: {error}"))
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
        let (craft, batches, source) = manual_craft_plan_for_available_output(
            registries,
            state,
            &[raw],
            commodity,
            required,
            context,
        );
        let duration = execute_manual_craft_batches(
            registries,
            state,
            craft.process(),
            source,
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

fn assemble_fieldwork_tool(
    registries: &Registries,
    state: &mut AppState,
    raw: deep_hearth::inventory::StockpileId,
    parts: deep_hearth::inventory::StockpileId,
    tool: FieldworkTool,
) -> (EquipmentId, u64) {
    let component_ticks = craft_equipment_components(
        registries,
        state,
        raw,
        parts,
        &[tool.base],
        "fieldwork selected-tool components",
    );
    let pick = validate_assemble_equipment(registries, state, tool.base, parts)
        .unwrap_or_else(|error| panic!("fieldwork hard-pick assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("fieldwork hard-pick assembly commit failed: {error}"));
    if tool.target == tool.base {
        return (pick, component_ticks);
    }
    let reinforcement_ticks = craft_upgrade_additions(
        registries,
        state,
        raw,
        parts,
        tool.target,
        "fieldwork selected-tool reinforcement",
    );
    let upgraded = validate_upgrade_equipment(registries, state, pick, tool.target, parts)
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
        let (craft, batches, source) = manual_craft_plan_for_available_output(
            registries,
            state,
            &[raw],
            input.commodity(),
            input.mass(),
            context,
        );
        let duration = execute_manual_craft_batches(
            registries,
            state,
            craft.process(),
            source,
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

/// Preserves the executed follow-up order after a typed batch-cap adaptation.
///
/// The exploration report exposed a capped hard-pick batch silently becoming the whole work
/// order. This regression keeps the requested mass, not the tool limit, as the player goal.
#[test]
fn batch_capped_mining_finishes_the_requested_order() {
    let registries = deep_hearth::content::build_registries();
    for (seed, expected) in [
        (1, EQUIPMENT_COPPER_REINFORCED_PICK),
        (2, EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK),
        (3, EQUIPMENT_COPPER_REINFORCED_PICK),
    ] {
        let FieldworkEpisode {
            tool,
            projected_ticks,
            extraction:
                extraction::FieldworkExtraction {
                    ticks: actual_ticks,
                    batches,
                    stop,
                    ..
                },
            ..
        } = run_fieldwork_order(
            &registries,
            FocusedProbeCase::new(
                seed,
                None,
                super::focused_seeds::FocusedProbeRole::ExplicitReplay,
            ),
            fieldwork_order(&registries, seed),
        );
        assert_eq!(tool, expected, "maintained report seed={seed}");
        assert_eq!(stop, FieldworkStop::OrderComplete);
        assert!(
            batches > 1,
            "the requested order must outlive its first claim"
        );
        assert_eq!(
            actual_ticks, projected_ticks,
            "wear-adjusted effort must match execution"
        );
    }
}

#[test]
fn preparation_cost_selects_light_tools_for_short_orders() {
    let registries = deep_hearth::content::build_registries();
    for (seed, expected) in [
        (1, EQUIPMENT_COPPER_REINFORCED_PICK),
        (2, EQUIPMENT_STONE_PICK),
        (3, EQUIPMENT_COPPER_REINFORCED_PICK),
    ] {
        let FieldworkEpisode {
            tool,
            preparation_ticks,
            extraction:
                extraction::FieldworkExtraction {
                    ticks: mining_ticks,
                    batches,
                    stop,
                    ..
                },
            ..
        } = run_fieldwork_order(
            &registries,
            FocusedProbeCase::new(
                seed,
                None,
                super::focused_seeds::FocusedProbeRole::ExplicitReplay,
            ),
            short_fieldwork_order(fieldwork_mining_limits(&registries).base_quarry_batch, seed),
        );
        assert_eq!(tool, expected);
        assert!(preparation_ticks > mining_ticks);
        assert!(batches > 1);
        assert_eq!(stop, FieldworkStop::OrderComplete);
    }
}

#[cfg(test)]
fn fieldwork_planning_fixture(
    registries: &Registries,
    include_copper: bool,
) -> (AppState, StockpileId) {
    let mut state = AppState::new(WorldSeed::new(71));
    let (raw_opportunity, capacity) = fieldwork_raw_opportunity(registries);
    let raw = add_solid_stockpile(&mut state, capacity);
    for (commodity, mass) in raw_opportunity {
        if include_copper || commodity.material() != MATERIAL_COPPER {
            seed_lot(
                registries,
                &mut state,
                raw,
                commodity,
                mass,
                ROOM_TEMPERATURE,
            );
        }
    }
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("fieldwork planning survival failed: {error}"));
    (state, raw)
}

#[test]
fn candidate_frame_respects_visible_hardness_and_finite_copper() {
    let registries = deep_hearth::content::build_registries();
    let limits = fieldwork_mining_limits(&registries);
    let (state, raw) = fieldwork_planning_fixture(&registries, false);
    let before = state.clone();
    let selected = choose_fieldwork_tool(
        &registries,
        &state,
        raw,
        limits.base_quarry_hardness,
        limits.base_quarry_batch,
    )
    .unwrap_or_else(|| panic!("stone route remains available"));
    assert_eq!(selected.tool.target, EQUIPMENT_STONE_PICK);
    assert!(
        matches!(estimate_fieldwork_tool(&registries, &state, raw, FIELDWORK_TOOLS[1],
        limits.reinforced_quarry_hardness, limits.base_quarry_batch),
        Err(FieldworkToolBlocker::RawInput { commodity, .. }) if commodity.material() == MATERIAL_COPPER)
    );
    assert!(
        matches!(estimate_fieldwork_tool(&registries, &state, raw, FIELDWORK_TOOLS[2],
        limits.reinforced_quarry_hardness, limits.base_quarry_batch),
        Err(FieldworkToolBlocker::AcquiredHardness { upper, maximum })
            if upper == limits.reinforced_quarry_hardness && maximum == limits.base_quarry_hardness)
    );
    assert!(
        choose_fieldwork_tool(
            &registries,
            &state,
            raw,
            limits.reinforced_quarry_hardness,
            limits.base_quarry_batch
        )
        .is_none()
    );
    assert_eq!(
        state, before,
        "pre-action comparison must not mutate or execute hypothetical worlds"
    );
}

#[test]
fn wear_adjusted_order_can_favor_the_lighter_reinforced_tool() {
    let registries = deep_hearth::content::build_registries();
    // An explicit visible work order, not an inference from hidden deposit reserves.
    let order = multiplied_mass(
        fieldwork_mining_limits(&registries).base_quarry_batch,
        40,
        "large-order regression",
    );
    let FieldworkEpisode {
        tool,
        preparation_ticks,
        projected_ticks: projected_order_ticks,
        extraction:
            extraction::FieldworkExtraction {
                ticks: mining_ticks,
                batches,
                condition_after,
                stop,
                ..
            },
        ..
    } = run_fieldwork_order(
        &registries,
        FocusedProbeCase::new(
            2,
            None,
            super::focused_seeds::FocusedProbeRole::ExplicitReplay,
        ),
        order,
    );
    assert_eq!(tool, EQUIPMENT_COPPER_REINFORCED_PICK);
    assert_eq!(stop, FieldworkStop::OrderComplete);
    assert!(mining_ticks > preparation_ticks);
    assert_eq!(mining_ticks, projected_order_ticks);
    let (state, raw) = fieldwork_planning_fixture(&registries, true);
    let quarry = estimate_fieldwork_tool(
        &registries,
        &state,
        raw,
        FIELDWORK_TOOLS[2],
        fieldwork_mining_limits(&registries).base_quarry_hardness,
        order,
    )
    .unwrap_or_else(|error| panic!("quarry comparison failed: {error:?}"));
    assert!(
        preparation_ticks + mining_ticks < quarry.total_ticks(),
        "the selected lighter tool must finish sooner than the old pristine-policy quarry choice"
    );
    assert!(condition_after < deep_hearth::maintenance::Condition::PRISTINE);
    assert!(batches > 1);
}

/// Controlled world generation, independent of demand and tool capabilities. The explicit salt
/// retains rich seeds 1–3; seed 6 is maintained shallow coverage. Neither reserve nor tier is
/// exposed to the actor. These are scenario opportunities, not runtime regional generation.
fn fieldwork_supply(seed: u64) -> Mass {
    let variation = mix64(seed ^ 0x4649_454C_4452_5356);
    let milligrams = if mix64(seed ^ 0x4649_454C_4453_5554).is_multiple_of(2) {
        32_000_000 + variation % 32_000_001
    } else {
        25_000 + variation % 175_001
    };
    Mass::from_milligrams(milligrams)
}

/// Visible scenario demand, sampled independently of hidden geology and never inferred from a
/// deposit's reserve. The long horizon is an explicit extraction order, not downstream demand
/// that the ordinary game has yet demonstrated.
fn fieldwork_order(registries: &Registries, seed: u64) -> Mass {
    let batch = fieldwork_mining_limits(registries).base_quarry_batch;
    if mix64(seed ^ 0x4649_454C_4444_454D).is_multiple_of(2) {
        return short_fieldwork_order(batch, seed);
    }
    let minimum = multiplied_mass(batch, 32, "long-order minimum");
    let span = multiplied_mass(batch, 16, "long-order variation");
    minimum
        .checked_add(Mass::from_milligrams(
            mix64(seed ^ 0x4649_454C_444D_4153) % (span.milligrams() + 1),
        ))
        .unwrap_or_else(|| panic!("fieldwork long order overflowed"))
}

fn short_fieldwork_order(batch: Mass, seed: u64) -> Mass {
    let minimum = (batch.milligrams() / 2).max(1);
    Mass::from_milligrams(
        minimum + mix64(seed ^ 0x4649_454C_444D_4153) % (batch.milligrams() - minimum + 1),
    )
}

#[cfg(not(test))]
pub(super) fn run_fieldwork_probe(registries: &Registries, case: FocusedProbeCase) {
    let episode = run_fieldwork_order(registries, case, fieldwork_order(registries, case.seed()));
    reviewln!(
        "FIELDWORK ENDPOINT seed=0x{:016X} tool={} observed-hardness={}..{}Pa preparation={}t projected-order={}t actual-extraction={}t extracted={}mg outcome={}",
        case.seed(),
        episode.tool.value(),
        episode.observed_hardness.lower().pascals(),
        episode.observed_hardness.upper().pascals(),
        episode.preparation_ticks,
        episode.projected_ticks,
        episode.extraction.ticks,
        episode.extraction.extracted.milligrams(),
        episode.extraction.stop.outcome(),
    );
}

struct FieldworkEpisode {
    tool: EquipmentDefinitionId,
    preparation_ticks: u64,
    projected_ticks: u64,
    observed_hardness: ExcavationHardnessEstimate,
    extraction: extraction::FieldworkExtraction,
}

fn run_fieldwork_order(
    registries: &Registries,
    case: FocusedProbeCase,
    requested_mine_mass: Mass,
) -> FieldworkEpisode {
    run_fieldwork_with_supply(
        registries,
        case,
        requested_mine_mass,
        fieldwork_supply(case.seed()),
    )
}

// Controlled supply overrides belong to regression setup, never to candidate selection.
fn run_fieldwork_with_supply(
    registries: &Registries,
    case: FocusedProbeCase,
    requested_mine_mass: Mass,
    deposit_mass: Mass,
) -> FieldworkEpisode {
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
    let hidden_channel = i64::try_from(
        mix64(seed ^ 0x4649_454C_4443_484E)
            % u64::try_from(CHANNEL_COUNT)
                .unwrap_or_else(|_| unreachable!("positive channel count fits u64")),
    )
    .unwrap_or_else(|_| unreachable!("fieldwork channel is bounded"));
    let hidden_slot = i64::try_from(
        mix64(seed ^ 0x4649_454C_4453_4C4F)
            % u64::try_from(channel_voxels)
                .unwrap_or_else(|_| unreachable!("positive channel span fits u64")),
    )
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
    assert!(!requested_mine_mass.is_zero());
    assert!(!deposit_mass.is_zero());
    let order_horizon = if requested_mine_mass <= mining_limits.base_quarry_batch {
        "short"
    } else {
        "long"
    };

    let mut state = AppState::new(WorldSeed::new(seed ^ 0x4649_454C_4457_524C));
    let (raw_opportunity, parts_capacity) = fieldwork_raw_opportunity(registries);
    let native_copper = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
    let starting_native_copper = raw_opportunity
        .get(&native_copper)
        .copied()
        .unwrap_or(Mass::ZERO);
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
    // Disclosed landing capacity supports the visible order, never signals hidden reserve.
    let destination = add_solid_stockpile(&mut state, requested_mine_mass);
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

    let episode_started_at = state.tick();
    let survival_before = *state
        .survival()
        .player()
        .unwrap_or_else(|| panic!("fieldwork initial survival record disappeared"));
    let (hammer, sampling_setup_ticks) =
        assemble_sampling_hammer(registries, &mut state, raw, parts);
    let search_started_at = state.tick();
    let (target, observed_hardness, transects, field_inspections, detailed_surveys) =
        localize_target(registries, &mut state, hammer, channel_voxels);
    let search_ticks = state.tick().value() - search_started_at.value();
    assert!(
        observed_hardness.lower() <= excavation_hardness
            && observed_hardness.upper() >= excavation_hardness,
        "actor-visible hardness band must conservatively contain diagnostic geological truth"
    );
    let estimate = choose_fieldwork_tool(
        registries,
        &state,
        raw,
        observed_hardness.upper(),
        requested_mine_mass,
    )
    .unwrap_or_else(|| {
        panic!("fieldwork bounded raw-tool family has no candidate for the acquired evidence")
    });
    reviewln!(
        "FIELDWORK DECISION seed=0x{seed:016X} tick={} selected={} policy=min-preparation-plus-wear-adjusted-order,then-native-copper,then-raw-mass,ties-light-first preparation={}t projected-order={}t total={}t authorization=not-yet",
        state.tick().value(),
        estimate.tool.label,
        estimate.preparation_ticks,
        estimate.order_ticks,
        estimate.total_ticks()
    );
    let raw_before: BTreeMap<_, _> = estimate
        .raw
        .keys()
        .map(|&commodity| {
            let mass = state
                .inventory()
                .get_stockpile(raw)
                .unwrap_or_else(|| panic!("fieldwork raw stockpile disappeared"))
                .get_mass(commodity);
            (commodity, mass)
        })
        .collect();
    let (mining_equipment, tool_prep_ticks) =
        assemble_fieldwork_tool(registries, &mut state, raw, parts, estimate.tool);
    for (&commodity, &expected) in &estimate.raw {
        let retained = state
            .inventory()
            .get_stockpile(raw)
            .unwrap_or_else(|| panic!("fieldwork raw stockpile disappeared"))
            .get_mass(commodity);
        assert_eq!(
            raw_before[&commodity].checked_sub(retained),
            Some(expected),
            "fieldwork executed raw bill must match the candidate's material cost"
        );
    }
    assert_eq!(
        tool_prep_ticks, estimate.preparation_ticks,
        "fieldwork executed preparation must agree with its pre-action craft resolutions"
    );
    let quarry_label = estimate.tool.label;
    let extraction = execute_fieldwork_extraction(
        registries,
        &mut state,
        FieldworkExtractionOrder {
            target,
            destination,
            equipment: mining_equipment,
            requested: requested_mine_mass,
            batch_limit: estimate.batch,
        },
    );
    let extracted_mass = extraction.extracted;
    let mining_ticks = extraction.ticks;
    let batches = extraction.batches;
    let condition_before = extraction.condition_before;
    let condition_after = extraction.condition_after;
    let adaptation = extraction.adaptation;
    let outcome = extraction.stop.outcome();
    let first_ore_ticks =
        sampling_setup_ticks + search_ticks + tool_prep_ticks + extraction.first_ore_ticks;
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
        .map(|stockpile| stockpile.get_mass(native_copper))
        .unwrap_or_else(|| panic!("fieldwork raw stockpile disappeared"));
    let survival_after = state
        .survival()
        .player()
        .unwrap_or_else(|| panic!("fieldwork final survival record disappeared"));
    // No intake occurs in this episode: reserve deltas include every canonical tick's
    // basal and work costs, from sampling-tool preparation through the completed extraction order.
    let metabolic_energy_spent = survival_before
        .metabolic_energy()
        .checked_sub(survival_after.metabolic_energy())
        .unwrap_or_else(|| panic!("fieldwork metabolic reserve increased without intake"));
    let hydration_spent = survival_before
        .hydration()
        .checked_sub(survival_after.hydration())
        .unwrap_or_else(|| panic!("fieldwork hydration reserve increased without intake"));
    assert!(metabolic_energy_spent > Energy::ZERO);
    assert!(hydration_spent > Volume::ZERO);
    assert!(
        survival_after.metabolic_energy() > Energy::ZERO
            && survival_after.hydration() > Volume::ZERO,
        "fieldwork cost evidence must not clip at exhausted survival reserves"
    );
    assert_eq!(
        survival_after
            .metabolic_energy()
            .checked_add(metabolic_energy_spent),
        Some(survival_before.metabolic_energy()),
        "fieldwork reported metabolic cost must reconcile with canonical player reserves"
    );
    assert_eq!(
        survival_after.hydration().checked_add(hydration_spent),
        Some(survival_before.hydration()),
        "fieldwork reported hydration cost must reconcile with canonical player reserves"
    );
    let output_grade_ppm = extraction.output_grade_ppm;
    let sampling_setup_time = format_physical_duration(registries, sampling_setup_ticks);
    let tool_prep_time = format_physical_duration(registries, tool_prep_ticks);
    let mining_time = format_physical_duration(registries, mining_ticks);
    let total_ticks = state.tick().value() - episode_started_at.value();
    let first_ore_time = format_physical_duration(registries, first_ore_ticks);
    assert_eq!(
        total_ticks,
        sampling_setup_ticks + search_ticks + tool_prep_ticks + mining_ticks,
        "fieldwork pacing must account for every elapsed tick, not only extraction"
    );
    let completed = extraction.stop == FieldworkStop::OrderComplete;
    let comparison = if completed {
        "full-order"
    } else {
        "partial-order-not-comparable"
    };
    let estimate_matched = if !completed {
        "not-applicable"
    } else if estimate.order_ticks == mining_ticks {
        "true"
    } else {
        "false"
    };
    let extraction_error = if completed {
        format!(
            "{:+}t",
            i128::from(mining_ticks) - i128::from(estimate.order_ticks)
        )
    } else {
        "not-applicable".to_owned()
    };
    let search_time = format_physical_duration(registries, search_ticks);
    let total_time = format_physical_duration(registries, total_ticks);
    reviewln!(
        "FIELDWORK ESTIMATE FEEDBACK seed=0x{seed:016X} selected={} order-horizon={order_horizon} outcome={outcome} requested={}mg output={}mg preparation-estimate={}t preparation-actual={}t wear-adjusted-order-estimate={}t extraction-actual={}t extraction-error={extraction_error} actual-build-plus-order={}t/{} condition={}ppm->{}ppm comparison={comparison} estimate-matched={estimate_matched} choice-frozen-before-action=true service=none",
        estimate.tool.label,
        requested_mine_mass.milligrams(),
        extracted_mass.milligrams(),
        estimate.preparation_ticks,
        tool_prep_ticks,
        estimate.order_ticks,
        mining_ticks,
        tool_prep_ticks + mining_ticks,
        format_physical_duration(registries, tool_prep_ticks + mining_ticks),
        condition_before.parts_per_million(),
        condition_after.parts_per_million(),
    );
    reviewln!(
        "FIELDWORK PACING seed=0x{seed:016X} search={search_ticks}t/{search_time} sampling-tool={sampling_setup_ticks}t/{sampling_setup_time} extraction-tool={tool_prep_ticks}t/{tool_prep_time} extraction={mining_ticks}t/{mining_time} batches={batches} first-ore={first_ore_ticks}t/{first_ore_time} episode-end={}t/{total_time} output={}mg outcome={outcome} requested={}mg scope=raw-tools-and-preowned-copper-to-first-ore repeat-extraction-excludes-discovery=true output-grade={output_grade_ppm}ppm",
        total_ticks,
        extracted_mass.milligrams(),
        requested_mine_mass.milligrams(),
    );

    reviewln!(
        "FIELDWORK EXPERIENCE seed=0x{seed:016X} sample={} outcome={outcome} order-horizon={order_horizon} demand=explicit-extraction-order search=compare-local-transects->cheap-inspection->targeted-survey channels={} transects={} selected-channel=observed-strongest field-inspections={} detailed-surveys={} target=acquired-evidence observed-hardness={}..{}Pa geology={geology_label} tool={quarry_label} adaptation={adaptation} sampling-setup={}t/{sampling_setup_time} tool-prep={}t/{tool_prep_time} starting-native-copper={}mg retained-native-copper={}mg requested={}mg mining={}mg duration={}t/{mining_time} condition={}ppm->{}ppm output-grade={output_grade_ppm}ppm matter=conserved survival=[energy:{}nJ hydration:{}uL]",
        focused_probe_role_label(case.role()),
        CHANNEL_COUNT,
        transects,
        field_inspections,
        detailed_surveys,
        observed_hardness.lower().pascals(),
        observed_hardness.upper().pascals(),
        sampling_setup_ticks,
        tool_prep_ticks,
        starting_native_copper.milligrams(),
        retained_native_copper.milligrams(),
        requested_mine_mass.milligrams(),
        extracted_mass.milligrams(),
        mining_ticks,
        condition_before.parts_per_million(),
        condition_after.parts_per_million(),
        metabolic_energy_spent.nanojoules(),
        hydration_spent.microliters(),
    );
    reviewln!(
        "FIELDWORK SUPPLY seed=0x{seed:016X} outcome={outcome} requested={}mg extracted={}mg shortfall={}mg stop={} effort={mining_ticks}t investment={}t",
        requested_mine_mass.milligrams(),
        extracted_mass.milligrams(),
        requested_mine_mass
            .checked_sub(extracted_mass)
            .unwrap_or_else(|| panic!("fieldwork output exceeded order"))
            .milligrams(),
        extraction.stop.label(),
        sampling_setup_ticks + tool_prep_ticks,
    );
    reviewln!(
        "FIELDWORK SUPPLY DIAGNOSTIC seed=0x{seed:016X} initial-reserve={}mg policy-input=false",
        deposit_mass.milligrams(),
    );
    FieldworkEpisode {
        tool: estimate.tool.target,
        preparation_ticks: tool_prep_ticks,
        projected_ticks: estimate.order_ticks,
        observed_hardness,
        extraction,
    }
}
