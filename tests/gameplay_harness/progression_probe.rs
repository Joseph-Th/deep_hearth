//! Canonical primitive-to-mechanized progression probe for the gameplay experience harness.

use std::collections::BTreeMap;
use std::num::NonZeroU64;

use super::seed::mix64;
use super::support::{ROOM_TEMPERATURE, add_solid_stockpile, nominal_equipment_mass_capability};
use deep_hearth::capability::{CapabilityId, CapabilityValue};
use deep_hearth::content::gameplay_fixture::{
    geological_deposit_spec, seed_geological_deposit, seed_lot,
};
use deep_hearth::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_STONE_CRUSHER, EQUIPMENT_STONE_HAND_CRANK,
    EQUIPMENT_STONE_PICK, EQUIPMENT_STONE_SEPARATOR, FORM_NATIVE_METAL, FORM_ORE,
    MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER, MATERIAL_STONE, MINING_METHOD_HAND_PICK,
    PROCESS_COLD_WORK_COPPER_REINFORCEMENT, PROCESS_CRUSH_ORE, PROCESS_KNAP_STONE_TOOL,
    PROCESS_SEPARATE_NATIVE_COPPER, PROCESS_SHAPE_STONE_FLYWHEEL, PROCESS_SHAPE_WOOD_HANDLE,
    PROSPECTING_DETAILED_FIELD_SURVEY, PROSPECTING_FIELD_INSPECTION,
};
use deep_hearth::core::quantity::{Energy, Mass, Power, Pressure};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::WorldSeed;
use deep_hearth::crafting::{
    ManualCraftRequest, ManualCraftStartRequest, validate_start_manual_craft,
};
use deep_hearth::energy::{calculate_mass_specific_energy, validate_assemble_energy_store};
use deep_hearth::equipment::{validate_assemble_equipment, validate_upgrade_equipment};
use deep_hearth::geology::{
    FieldProspectingRequest, GeologicalEvidenceConsistency, assess_geological_knowledge,
    validate_start_field_prospecting,
};
use deep_hearth::inventory::MaterialLotSelection;
use deep_hearth::labor::{
    ManualPowerError, ManualPowerRequest, ProspectingMethodId, validate_start_manual_power,
};
use deep_hearth::material::{
    COMPOSITION_PARTS_PER_MILLION, CommodityKey, CompositionComponent, MaterialAssemblyProfile,
    MaterialComposition,
};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::mining::{
    MiningStartError, MiningTargetRequest, MiningTargetResolution, MiningTargetResolutionError,
    resolve_mining_target, validate_claim_mining_output, validate_start_mining,
};
use deep_hearth::ore_processing::{
    ComminutionRequest, ComminutionResolutionError, ConstituentSeparationProcessDefinition,
    ConstituentSeparationRequest, resolve_comminution_process,
    resolve_constituent_separation_process,
};
use deep_hearth::production::{
    ProcessOutputRoute, ProductionJobId, validate_start_process, validate_start_process_routed,
};
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};
use deep_hearth::survival::{assess_survival, initialize_player_survival};

const MAX_STEADY_STATE_CRUSH_CYCLES: u64 = 12;
const EXTRACTION_GUARANTEED_GRADE_PREMIUM_PPM: u32 = 100_000;

pub(super) fn varied_four_way_order(seed: u64) -> [usize; 4] {
    let mut order = [0, 1, 2, 3];
    let mut random = seed;
    for upper in (1..order.len()).rev() {
        random = mix64(random ^ upper as u64);
        let selected = usize::try_from(random % (upper as u64 + 1))
            .unwrap_or_else(|_| unreachable!("four-way shuffle index fits usize"));
        order.swap(upper, selected);
    }
    order
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum AutonomousWorkStop {
    #[default]
    MachineCompleted,
    FeedBufferCapacity,
    TargetSupply,
    ToolCondition,
}

impl AutonomousWorkStop {
    const fn label(self) -> &'static str {
        match self {
            Self::MachineCompleted => "machine-completed",
            Self::FeedBufferCapacity => "feed-buffer-capacity",
            Self::TargetSupply => "target-supply",
            Self::ToolCondition => "tool-condition",
        }
    }
}

fn progression_clue_bounds(slot: usize) -> VoxelBounds {
    let x = i64::try_from(slot)
        .unwrap_or_else(|_| unreachable!("four-way clue slot fits i64"))
        .checked_mul(2)
        .unwrap_or_else(|| unreachable!("bounded clue coordinate cannot overflow"));
    VoxelBounds::new(VoxelCoord::new(x, -4, 0), VoxelCoord::new(x + 1, -3, 1))
        .unwrap_or_else(|error| panic!("primitive progression clue bounds failed: {error}"))
}

/// Verifies only the runtime dependencies this episode actually intends to use.
///
/// The broader cold-agent catalog is discovered dynamically by the aggregate harness report. This
/// probe therefore does not freeze the whole playable catalog to an exact ID list just to protect its
/// own scenario. New routes may coexist without making this established primitive episode stale.
fn assert_progression_runtime_dependencies(registries: &Registries) {
    for equipment in [
        EQUIPMENT_STONE_PICK,
        EQUIPMENT_STONE_HAND_CRANK,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        EQUIPMENT_STONE_CRUSHER,
        EQUIPMENT_STONE_SEPARATOR,
    ] {
        let definition = registries
            .equipment()
            .get_equipment(equipment)
            .unwrap_or_else(|| {
                panic!(
                    "primitive progression equipment {} disappeared",
                    equipment.value()
                )
            });
        assert!(
            definition.has_runtime_acquisition_route(),
            "primitive progression equipment {} lost its runtime acquisition route",
            equipment.value()
        );
    }
    assert!(
        registries
            .energy()
            .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
            .is_some_and(|definition| definition.has_runtime_assembly_route()),
        "primitive progression flywheel lost its runtime assembly route"
    );
    for process in [
        PROCESS_KNAP_STONE_TOOL,
        PROCESS_SHAPE_WOOD_HANDLE,
        PROCESS_SHAPE_STONE_FLYWHEEL,
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
    ] {
        assert!(
            registries.crafting().get_manual(process).is_some(),
            "primitive progression manual process {} disappeared",
            process.value()
        );
    }
    for method in [
        PROSPECTING_FIELD_INSPECTION,
        PROSPECTING_DETAILED_FIELD_SURVEY,
    ] {
        assert!(
            registries.labor().get_prospecting(method).is_some(),
            "primitive progression prospecting method {} disappeared",
            method.value()
        );
    }
    assert!(
        registries
            .mining()
            .get_method(MINING_METHOD_HAND_PICK)
            .is_some(),
        "primitive progression hand-mining method disappeared"
    );
}

fn advance_exact(registries: &Registries, state: &mut AppState, ticks: u64) {
    for _ in 0..ticks {
        advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("primitive progression tick failed: {error}"));
    }
}

fn duration(start: u64, end: u64) -> u64 {
    end.checked_sub(start)
        .unwrap_or_else(|| panic!("primitive progression work duration underflowed"))
}

fn craft_batches(
    registries: &Registries,
    state: &mut AppState,
    process: deep_hearth::production::ProcessId,
    source: deep_hearth::inventory::StockpileId,
    destination: deep_hearth::inventory::StockpileId,
    batches: u64,
) {
    let batches = NonZeroU64::new(batches)
        .unwrap_or_else(|| panic!("primitive progression craft batch count must be nonzero"));
    let job = validate_start_manual_craft(
        registries,
        state,
        ManualCraftStartRequest::new(
            ManualCraftRequest::new(process, source, batches),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive progression repeated craft failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("primitive progression repeated craft commit failed: {error}"));
    let duration = state
        .production()
        .get_job(job)
        .map(|record| record.active_duration())
        .unwrap_or_else(|| panic!("primitive progression craft job disappeared after start"));
    advance_exact(registries, state, duration.value());
}

fn multiply_mass(mass: Mass, count: u64, context: &'static str) -> Mass {
    let milligrams = mass
        .milligrams()
        .checked_mul(count)
        .unwrap_or_else(|| panic!("primitive progression {context} mass overflowed"));
    Mass::from_milligrams(milligrams)
}

fn add_mass(total: &mut Mass, amount: Mass, context: &'static str) {
    *total = total
        .checked_add(amount)
        .unwrap_or_else(|| panic!("primitive progression {context} mass overflowed"));
}

fn add_profile_requirements(
    requirements: &mut BTreeMap<CommodityKey, Mass>,
    profile: &MaterialAssemblyProfile,
) {
    for input in profile.inputs() {
        let entry = requirements.entry(input.commodity()).or_insert(Mass::ZERO);
        add_mass(entry, input.mass(), "assembly requirement");
    }
}

fn manual_craft_for_output(
    registries: &Registries,
    commodity: CommodityKey,
) -> &deep_hearth::crafting::ManualCraftDefinition {
    registries
        .crafting()
        .definitions()
        .filter(|definition| {
            definition
                .outputs()
                .iter()
                .any(|output| output.commodity() == commodity)
        })
        .min_by_key(|definition| definition.process())
        .unwrap_or_else(|| {
            panic!(
                "primitive progression has no manual route to required component {}",
                commodity.value()
            )
        })
}

fn output_mass_per_batch(
    definition: &deep_hearth::crafting::ManualCraftDefinition,
    commodity: CommodityKey,
) -> Mass {
    definition
        .outputs()
        .iter()
        .find(|output| output.commodity() == commodity)
        .map(|output| output.mass())
        .unwrap_or_else(|| {
            panic!(
                "primitive progression manual process {} no longer produces component {}",
                definition.process().value(),
                commodity.value()
            )
        })
}

fn batches_for_output(required: Mass, per_batch: Mass) -> u64 {
    assert!(!required.is_zero());
    assert!(!per_batch.is_zero());
    required.milligrams().div_ceil(per_batch.milligrams())
}

#[derive(Debug)]
struct PrimitiveMaterialPlan {
    raw_inputs: Vec<(CommodityKey, Mass)>,
    raw_capacity: Mass,
    shaped_capacity: Mass,
    native_copper: Mass,
}

fn primitive_material_plan(registries: &Registries) -> PrimitiveMaterialPlan {
    let mut requirements = BTreeMap::new();
    for equipment in [
        EQUIPMENT_STONE_PICK,
        EQUIPMENT_STONE_HAND_CRANK,
        EQUIPMENT_STONE_CRUSHER,
        EQUIPMENT_STONE_SEPARATOR,
    ] {
        let profile = registries
            .equipment()
            .get_equipment(equipment)
            .and_then(|definition| definition.assembly_profile())
            .unwrap_or_else(|| {
                panic!(
                    "primitive progression equipment {} lost its runtime assembly route",
                    equipment.value()
                )
            });
        add_profile_requirements(&mut requirements, profile);
    }
    let drive_profile = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("primitive progression flywheel drive lost its assembly route"));
    add_profile_requirements(&mut requirements, drive_profile);
    let pick_additions = equipment_upgrade_additions(registries, EQUIPMENT_COPPER_REINFORCED_PICK);
    let crank_additions =
        equipment_upgrade_additions(registries, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK);
    assert_eq!(
        pick_additions.inputs(),
        crank_additions.inputs(),
        "primitive progression competing copper upgrades must consume the same reinforcement parcel"
    );
    add_profile_requirements(&mut requirements, pick_additions);
    add_profile_requirements(&mut requirements, crank_additions);

    let mut process_batches: BTreeMap<deep_hearth::production::ProcessId, u64> = BTreeMap::new();
    for (commodity, required) in requirements {
        let craft = manual_craft_for_output(registries, commodity);
        let batches = batches_for_output(required, output_mass_per_batch(craft, commodity));
        process_batches
            .entry(craft.process())
            .and_modify(|existing| *existing = (*existing).max(batches))
            .or_insert(batches);
    }
    let native_key = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
    let mut raw_by_commodity = BTreeMap::new();
    let mut native_copper = Mass::ZERO;
    let mut shaped_capacity = Mass::ZERO;
    for (process, batches) in process_batches {
        let definition = registries
            .crafting()
            .get_manual(process)
            .unwrap_or_else(|| panic!("primitive progression craft definition disappeared"));
        let input_mass = multiply_mass(definition.input_mass(), batches, "craft input");
        add_mass(&mut shaped_capacity, input_mass, "shaped capacity");
        if definition.input() == native_key {
            add_mass(&mut native_copper, input_mass, "native copper requirement");
        } else {
            let entry = raw_by_commodity
                .entry(definition.input())
                .or_insert(Mass::ZERO);
            add_mass(entry, input_mass, "raw input requirement");
        }
    }
    assert!(
        !native_copper.is_zero(),
        "primitive progression upgrade path must consume mined native copper"
    );
    let mut raw_capacity = Mass::ZERO;
    for mass in raw_by_commodity.values().copied() {
        add_mass(&mut raw_capacity, mass, "raw stockpile capacity");
    }
    PrimitiveMaterialPlan {
        raw_inputs: raw_by_commodity.into_iter().collect(),
        raw_capacity,
        shaped_capacity,
        native_copper,
    }
}

fn craft_for_profile(
    registries: &Registries,
    state: &mut AppState,
    raw_source: deep_hearth::inventory::StockpileId,
    native_source: deep_hearth::inventory::StockpileId,
    destination: deep_hearth::inventory::StockpileId,
    profile: &MaterialAssemblyProfile,
) {
    for input in profile.inputs() {
        let available = state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.get_mass(input.commodity()))
            .unwrap_or_else(|| panic!("primitive progression shaped stockpile disappeared"));
        if available >= input.mass() {
            continue;
        }
        let missing = input
            .mass()
            .checked_sub(available)
            .unwrap_or_else(|| unreachable!("available component mass was already checked"));
        let craft = manual_craft_for_output(registries, input.commodity());
        let batches = batches_for_output(missing, output_mass_per_batch(craft, input.commodity()));
        let required_input = multiply_mass(craft.input_mass(), batches, "just-in-time craft input");
        let source = [raw_source, native_source]
            .into_iter()
            .find(|source| {
                state
                    .inventory()
                    .get_stockpile(*source)
                    .is_some_and(|stockpile| stockpile.get_mass(craft.input()) >= required_input)
            })
            .unwrap_or_else(|| {
                panic!(
                    "primitive progression lacks {}mg of manual-process input {} for component {}",
                    required_input.milligrams(),
                    craft.input().value(),
                    input.commodity().value()
                )
            });
        craft_batches(
            registries,
            state,
            craft.process(),
            source,
            destination,
            batches,
        );
    }
}

fn equipment_assembly_profile(
    registries: &Registries,
    equipment: deep_hearth::equipment::EquipmentDefinitionId,
) -> &MaterialAssemblyProfile {
    registries
        .equipment()
        .get_equipment(equipment)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| {
            panic!(
                "primitive progression equipment {} is not runtime-assemblable",
                equipment.value()
            )
        })
}

fn equipment_upgrade_additions(
    registries: &Registries,
    equipment: deep_hearth::equipment::EquipmentDefinitionId,
) -> &MaterialAssemblyProfile {
    registries
        .equipment()
        .get_equipment(equipment)
        .and_then(|definition| definition.upgrade_profile())
        .map(|profile| profile.additions())
        .unwrap_or_else(|| {
            panic!(
                "primitive progression equipment {} is not runtime-upgradeable",
                equipment.value()
            )
        })
}

fn stone_pick_mining_batch_limit(registries: &Registries) -> Mass {
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("primitive progression mining method disappeared"));
    nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_STONE_PICK,
        method.max_batch_mass_capability(),
    )
}

fn nominal_equipment_pressure_capability(
    registries: &Registries,
    equipment: deep_hearth::equipment::EquipmentDefinitionId,
    capability: CapabilityId,
) -> Pressure {
    let definition = registries
        .equipment()
        .get_equipment(equipment)
        .unwrap_or_else(|| panic!("primitive progression equipment definition disappeared"));
    match definition.capabilities().get_capability(capability) {
        Some(CapabilityValue::Pressure(pressure)) => pressure,
        Some(value) => panic!(
            "primitive progression expected pressure capability {} on equipment {} but found {:?}",
            capability.value(),
            equipment.value(),
            value.kind()
        ),
        None => panic!(
            "primitive progression equipment {} is missing authored pressure capability {}",
            equipment.value(),
            capability.value()
        ),
    }
}

fn mining_hardness_limits(registries: &Registries) -> (Pressure, Pressure, Pressure) {
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("primitive progression mining method disappeared"));
    let capability = method.max_hardness_capability();
    let stone_limit =
        nominal_equipment_pressure_capability(registries, EQUIPMENT_STONE_PICK, capability);
    let reinforced_limit = nominal_equipment_pressure_capability(
        registries,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        capability,
    );
    assert!(
        stone_limit < reinforced_limit,
        "primitive pick reinforcement must unlock a strictly harder excavation envelope"
    );
    let gap = reinforced_limit
        .pascals()
        .checked_sub(stone_limit.pascals())
        .unwrap_or_else(|| unreachable!("reinforced hardness was already checked above stone"));
    let hard_seam = Pressure::from_pascals(
        stone_limit
            .pascals()
            .checked_add(gap.div_ceil(2))
            .unwrap_or_else(|| panic!("primitive hard-seam hardness overflowed")),
    );
    assert!(hard_seam > stone_limit && hard_seam <= reinforced_limit);
    (stone_limit, reinforced_limit, hard_seam)
}

fn progression_mining_mass(registries: &Registries, seed: u64) -> Mass {
    let maximum = stone_pick_mining_batch_limit(registries).milligrams();
    assert!(
        maximum > 0,
        "primitive progression mining batch must be nonzero"
    );
    let minimum = maximum
        .checked_mul(3)
        .map(|scaled| scaled.div_ceil(4))
        .unwrap_or_else(|| panic!("primitive progression mining-range scaling overflowed"));
    Mass::from_milligrams(minimum + mix64(seed ^ 0x5052_4F47_4D49_4E45) % (maximum - minimum + 1))
}

fn mine_and_claim(
    registries: &Registries,
    state: &mut AppState,
    target: MiningTargetRequest,
    destination: deep_hearth::inventory::StockpileId,
    equipment: deep_hearth::equipment::EquipmentId,
    mass: Mass,
) -> u64 {
    let target = resolve_progression_mining_target(state, target);
    let mining = validate_start_mining(
        registries,
        state,
        MINING_METHOD_HAND_PICK,
        target,
        destination,
        equipment,
        mass,
    )
    .unwrap_or_else(|error| panic!("primitive progression mining failed: {error}"));
    let mining_job = mining
        .commit(state)
        .unwrap_or_else(|error| panic!("primitive progression mining commit failed: {error}"));
    let mining_record = state
        .mining()
        .get_job(mining_job)
        .unwrap_or_else(|| panic!("primitive progression mining job disappeared"));
    let mining_ticks = duration(
        mining_record.started_at().value(),
        mining_record.completes_at().value(),
    );
    advance_exact(registries, state, mining_ticks);
    validate_claim_mining_output(registries, state, mining_job)
        .unwrap_or_else(|error| panic!("primitive progression mining claim failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| {
            panic!("primitive progression mining claim commit failed: {error}")
        });
    mining_ticks
}

fn mine_total_and_claim(
    registries: &Registries,
    state: &mut AppState,
    target: MiningTargetRequest,
    destination: deep_hearth::inventory::StockpileId,
    equipment: deep_hearth::equipment::EquipmentId,
    total: Mass,
    maximum_batch: Mass,
) -> u64 {
    assert!(!total.is_zero());
    assert!(!maximum_batch.is_zero());
    let mut remaining = total;
    let mut elapsed = 0_u64;
    while !remaining.is_zero() {
        let batch = Mass::from_milligrams(remaining.milligrams().min(maximum_batch.milligrams()));
        elapsed = elapsed
            .checked_add(mine_and_claim(
                registries,
                state,
                target,
                destination,
                equipment,
                batch,
            ))
            .unwrap_or_else(|| panic!("primitive progression mining duration overflowed"));
        remaining = remaining
            .checked_sub(batch)
            .unwrap_or_else(|| unreachable!("mining batch is bounded by remaining mass"));
    }
    elapsed
}

fn resolve_progression_mining_target(
    state: &AppState,
    request: MiningTargetRequest,
) -> MiningTargetResolution {
    resolve_mining_target(state, request)
        .unwrap_or_else(|error| panic!("primitive progression mining evidence failed: {error}"))
}

fn inspect_local_copper_evidence(
    registries: &Registries,
    state: &mut AppState,
    method: ProspectingMethodId,
    region: VoxelBounds,
) -> u64 {
    let before = state.geological_knowledge().observations().count();
    let definition = registries
        .labor()
        .get_prospecting(method)
        .copied()
        .unwrap_or_else(|| panic!("primitive progression prospecting definition disappeared"));
    validate_start_field_prospecting(
        registries,
        state,
        FieldProspectingRequest::new(method, region, MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("primitive progression prospecting failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("primitive progression prospecting commit failed: {error}"));
    advance_exact(registries, state, definition.duration().value());
    assert_eq!(
        state.geological_knowledge().observations().count(),
        before + 1,
        "completed prospecting work must persist exactly one acquired observation"
    );
    definition.duration().value()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ObservedCopperClue {
    request: MiningTargetRequest,
    lower_ppm: u32,
    upper_ppm: u32,
}

fn observed_copper_bounds(state: &AppState, request: MiningTargetRequest) -> (u32, u32) {
    match assess_geological_knowledge(
        state.geological_knowledge(),
        request.region(),
        request.material(),
    )
    .consistency()
    {
        GeologicalEvidenceConsistency::Compatible {
            lower_ppm,
            upper_ppm,
        } => (lower_ppm, upper_ppm),
        other => panic!(
            "primitive progression expected compatible copper evidence for {:?}, found {other:?}",
            request.region()
        ),
    }
}

fn observed_resolved_copper_clue(
    state: &AppState,
    request: MiningTargetRequest,
) -> ObservedCopperClue {
    let (lower_ppm, upper_ppm) = observed_copper_bounds(state, request);
    let _ = resolve_mining_target(state, request)
        .unwrap_or_else(|error| panic!("observable copper clue did not resolve: {error}"));
    ObservedCopperClue {
        request,
        lower_ppm,
        upper_ppm,
    }
}

fn strongest_observed_copper_clue(
    clues: impl IntoIterator<Item = ObservedCopperClue>,
) -> ObservedCopperClue {
    clues
        .into_iter()
        .max_by_key(|clue| (clue.lower_ppm, clue.upper_ppm))
        .unwrap_or_else(|| panic!("primitive progression has no eligible observed copper clue"))
}

fn preview_stone_pick_mining(
    registries: &Registries,
    state: &AppState,
    clue: ObservedCopperClue,
    destination: deep_hearth::inventory::StockpileId,
    pick: deep_hearth::equipment::EquipmentId,
    mass: Mass,
) -> Result<(), MiningStartError> {
    let target = resolve_progression_mining_target(state, clue.request);
    validate_start_mining(
        registries,
        state,
        MINING_METHOD_HAND_PICK,
        target,
        destination,
        pick,
        mass,
    )
    .map(|_| ())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrimitivePriority {
    ExtractionFirst,
    MechanizationFirst,
}

impl PrimitivePriority {
    const fn label(self) -> &'static str {
        match self {
            Self::ExtractionFirst => "extraction-first",
            Self::MechanizationFirst => "mechanization-first",
        }
    }
}

fn observed_primitive_priority(
    bulk_sample_copper_ppm: u32,
    hard_clue: ObservedCopperClue,
) -> PrimitivePriority {
    // The actor knows the exact grade of matter it already extracted, but only the evidence bounds
    // for the blocked hard seam. Prefer extraction only when the observed lower bound guarantees a
    // richer feed than the owned bulk sample; otherwise take the certain stored-work improvement.
    if hard_clue.lower_ppm.saturating_sub(bulk_sample_copper_ppm)
        >= EXTRACTION_GUARANTEED_GRADE_PREMIUM_PPM
    {
        PrimitivePriority::ExtractionFirst
    } else {
        PrimitivePriority::MechanizationFirst
    }
}

#[derive(Clone, Copy)]
struct PrimitiveProgressionExperience {
    natural_priority: PrimitivePriority,
    prospecting_ticks: u64,
    surface_prospecting_ticks: u64,
    detailed_survey_ticks: u64,
    surface_clue_count: u8,
    surface_resolved_clue_count: u8,
    information_refinement_required: bool,
    refinement_triggered_by_direct_shortage: bool,
    refined_coarse_lower_ppm: u32,
    refined_coarse_upper_ppm: u32,
    refined_detailed_lower_ppm: u32,
    refined_detailed_upper_ppm: u32,
    refined_sample_copper_ppm: u32,
    refined_sample_is_ore: bool,
    stone_mineable_clue_count: u8,
    hardness_blocked_clue_count: u8,
    direct_copper_evidence_lower_ppm: u32,
    direct_copper_evidence_upper_ppm: u32,
    bulk_ore_evidence_lower_ppm: u32,
    bulk_ore_evidence_upper_ppm: u32,
    hard_ore_evidence_lower_ppm: u32,
    hard_ore_evidence_upper_ppm: u32,
    bulk_sample_copper_ppm: u32,
    selected_processing_feed_copper_ppm: u32,
    selected_processing_feed_is_hard: bool,
    processing_feed_selected_from_bulk: bool,
    refined_clue_sample_mass: Mass,
    refined_clue_mining_ticks: u64,
    primary_batch_mass: Mass,
    first_upgrade_at: u64,
    second_upgrade_at: u64,
    pick_upgraded_at: Option<u64>,
    hard_seam_accessed_at: Option<u64>,
    machine_started_at: u64,
    automation_preparation_ticks: u64,
    separator_preparation_ticks: u64,
    processing_line_preparation_ticks: u64,
    productive_payback_cycles: Option<u64>,
    steady_state_cycles: u64,
    steady_state_stop: PrimitiveSteadyStop,
    final_crusher_condition_ppm: u32,
    initial_full_charge_ticks: u64,
    first_processed_output_at: u64,
    elapsed_ticks: u64,
    soft_ore_mining_ticks: u64,
    reinforced_mining_ticks: Option<u64>,
    charge_ticks: u64,
    machine_work_ticks: u64,
    reserve_machine_work_ticks: u64,
    overlap_ticks: u64,
    machine_useful_overlap_ticks: u64,
    reserve_useful_overlap_ticks: u64,
    machine_player_free_ticks: u64,
    primary_autonomous_stop: AutonomousWorkStop,
    reserve_autonomous_stop: AutonomousWorkStop,
    primary_mining_jobs: u64,
    reserve_mining_jobs: u64,
    steady_mining_jobs: u64,
    steady_feed_buffer_limited_cycles: u64,
    separation_feed_mass: Mass,
    recovered_copper_mass: Mass,
    separation_required_energy: Energy,
    separation_ticks: u64,
    separation_completed_at: u64,
    processed_output_enabled_second_upgrade: bool,
    hard_ore_mined: Mass,
    hard_ore_before_convergence: Mass,
    total_ore_mined: Mass,
    direct_second_upgrade_blocked: bool,
    initial_crank_reinforced: bool,
    crank_reinforced: bool,
    final_pick_condition_ppm: u32,
    metabolic_energy_spent_nj: u128,
    hydration_spent_ul: u64,
}

fn native_input_for_upgrade(
    registries: &Registries,
    equipment: deep_hearth::equipment::EquipmentDefinitionId,
) -> Mass {
    let native = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
    equipment_upgrade_additions(registries, equipment)
        .inputs()
        .iter()
        .try_fold(Mass::ZERO, |total, input| {
            let craft = manual_craft_for_output(registries, input.commodity());
            assert_eq!(
                craft.input(),
                native,
                "primitive copper upgrade component must remain directly cold-workable from native copper"
            );
            let batches = batches_for_output(
                input.mass(),
                output_mass_per_batch(craft, input.commodity()),
            );
            total.checked_add(multiply_mass(
                craft.input_mass(),
                batches,
                "upgrade native-copper input",
            ))
        })
        .unwrap_or_else(|| panic!("primitive upgrade native-copper requirement overflowed"))
}

fn feed_mass_for_exact_constituent(target: Mass, constituent_ppm: u32) -> Mass {
    assert!(!target.is_zero());
    assert!(constituent_ppm > 0 && constituent_ppm < 1_000_000);
    let numerator = u128::from(target.milligrams())
        .checked_mul(1_000_000)
        .unwrap_or_else(|| panic!("primitive separation target scaling overflowed"));
    let feed_mg = numerator.div_ceil(u128::from(constituent_ppm));
    let feed_mg = u64::try_from(feed_mg)
        .unwrap_or_else(|_| panic!("primitive separation feed mass exceeds authoritative range"));
    let feed = Mass::from_milligrams(feed_mg);
    let recovered = u128::from(feed.milligrams()) * u128::from(constituent_ppm) / 1_000_000;
    assert_eq!(
        recovered,
        u128::from(target.milligrams()),
        "primitive separation feed selection must recover exactly one second-upgrade copper parcel"
    );
    feed
}

fn reinforce_pick(
    registries: &Registries,
    state: &mut AppState,
    raw: deep_hearth::inventory::StockpileId,
    native_storage: deep_hearth::inventory::StockpileId,
    shaped: deep_hearth::inventory::StockpileId,
    pick: deep_hearth::equipment::EquipmentId,
) {
    let condition_before = state
        .equipment()
        .get_equipment(pick)
        .unwrap_or_else(|| panic!("primitive progression pick disappeared before reinforcement"))
        .condition();
    craft_for_profile(
        registries,
        state,
        raw,
        native_storage,
        shaped,
        equipment_upgrade_additions(registries, EQUIPMENT_COPPER_REINFORCED_PICK),
    );
    validate_upgrade_equipment(
        registries,
        state,
        pick,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        shaped,
    )
    .unwrap_or_else(|error| panic!("primitive progression pick reinforcement failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| {
        panic!("primitive progression pick reinforcement commit failed: {error}")
    });
    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .unwrap_or_else(|| panic!("primitive progression reinforced pick disappeared"))
            .condition(),
        condition_before,
        "reinforcement must not repair accumulated pick wear"
    );
}

fn reinforce_crank(
    registries: &Registries,
    state: &mut AppState,
    raw: deep_hearth::inventory::StockpileId,
    native_storage: deep_hearth::inventory::StockpileId,
    shaped: deep_hearth::inventory::StockpileId,
    crank: deep_hearth::equipment::EquipmentId,
) {
    let condition_before = state
        .equipment()
        .get_equipment(crank)
        .unwrap_or_else(|| panic!("primitive progression crank disappeared before reinforcement"))
        .condition();
    craft_for_profile(
        registries,
        state,
        raw,
        native_storage,
        shaped,
        equipment_upgrade_additions(registries, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK),
    );
    validate_upgrade_equipment(
        registries,
        state,
        crank,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        shaped,
    )
    .unwrap_or_else(|error| panic!("primitive progression crank reinforcement failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| {
        panic!("primitive progression crank reinforcement commit failed: {error}")
    });
    assert_eq!(
        state
            .equipment()
            .get_equipment(crank)
            .unwrap_or_else(|| panic!("primitive progression reinforced crank disappeared"))
            .condition(),
        condition_before,
        "reinforcement must not repair accumulated crank wear"
    );
}

#[derive(Clone, Copy)]
struct PrimitiveMachine {
    crank: deep_hearth::equipment::EquipmentId,
    crusher: deep_hearth::equipment::EquipmentId,
    separator: deep_hearth::equipment::EquipmentId,
    drive: deep_hearth::energy::EnergyStoreId,
    drive_capacity: Energy,
    required_energy: Energy,
    separation_required_energy: Energy,
    charge_energy: Energy,
    reserve_mass: Mass,
    charge_fill_ppm: u32,
    charge_ticks: u64,
    full_charge_ticks: u64,
    automation_preparation_ticks: u64,
    separator_preparation_ticks: u64,
    processing_line_preparation_ticks: u64,
    crank_reinforced: bool,
}

fn build_primitive_machine(
    registries: &Registries,
    state: &mut AppState,
    raw: deep_hearth::inventory::StockpileId,
    native_storage: deep_hearth::inventory::StockpileId,
    shaped: deep_hearth::inventory::StockpileId,
    mined_mass: Mass,
    separation_feed_mass: Mass,
    seed: u64,
) -> PrimitiveMachine {
    let preparation_started_at = state.tick().value();
    craft_for_profile(
        registries,
        state,
        raw,
        native_storage,
        shaped,
        equipment_assembly_profile(registries, EQUIPMENT_STONE_HAND_CRANK),
    );
    let crank = validate_assemble_equipment(registries, state, EQUIPMENT_STONE_HAND_CRANK, shaped)
        .unwrap_or_else(|error| panic!("primitive progression crank assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| {
            panic!("primitive progression crank assembly commit failed: {error}")
        });

    let drive_profile = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("primitive progression flywheel drive lost its assembly route"));
    craft_for_profile(
        registries,
        state,
        raw,
        native_storage,
        shaped,
        drive_profile,
    );
    let drive =
        validate_assemble_energy_store(registries, state, ENERGY_STONE_FLYWHEEL_DRIVE, shaped)
            .unwrap_or_else(|error| {
                panic!("primitive progression drive construction failed: {error}")
            })
            .commit(state)
            .unwrap_or_else(|error| {
                panic!("primitive progression drive construction commit failed: {error}")
            });

    let crusher_process = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("primitive progression crusher process disappeared"));
    let required_energy =
        calculate_mass_specific_energy(mined_mass, crusher_process.specific_energy());
    let separation_process = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("primitive progression separator process disappeared"));
    let separation_required_energy =
        calculate_mass_specific_energy(separation_feed_mass, separation_process.specific_energy());
    let primary_processing_energy = required_energy
        .checked_add(separation_required_energy)
        .unwrap_or_else(|| panic!("primitive progression primary processing energy overflowed"));
    let drive_capacity = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("primitive progression flywheel definition disappeared"));
    assert!(
        drive_capacity >= primary_processing_energy,
        "primitive progression constructed drive cannot hold one crusher batch plus its playable separation step"
    );
    let maximum_follow_up_mass = mined_mass;
    let maximum_follow_up_energy =
        calculate_mass_specific_energy(maximum_follow_up_mass, crusher_process.specific_energy());
    let maximum_useful_charge = primary_processing_energy
        .checked_add(maximum_follow_up_energy)
        .unwrap_or_else(|| panic!("primitive progression useful charge overflowed"));
    let charge_ceiling = std::cmp::min(drive_capacity, maximum_useful_charge);
    let charge_target_ppm = 850_000 + (mix64(seed ^ 0x4348_4152_4745_5253) % 150_001) as u32;
    let target_charge_nj = charge_ceiling
        .nanojoules()
        .checked_mul(u128::from(charge_target_ppm))
        .map(|scaled| scaled / 1_000_000)
        .unwrap_or_else(|| panic!("primitive progression charge target overflowed"));
    let reserve_energy_budget = target_charge_nj
        .checked_sub(primary_processing_energy.nanojoules())
        .unwrap_or_else(|| {
            panic!(
                "primitive progression charge target must fund crushing, separation, and useful follow-up work"
            )
        });
    let specific_energy = u128::from(crusher_process.specific_energy().nanojoules_per_milligram());
    let reserve_mass_mg =
        u64::try_from(reserve_energy_budget / specific_energy).unwrap_or_else(|_| {
            panic!("primitive progression reserve mass exceeds authoritative range")
        });
    assert!(
        reserve_mass_mg > 0,
        "primitive progression charge plan must bank a positive follow-up batch"
    );
    let reserve_mass = Mass::from_milligrams(reserve_mass_mg);
    let reserve_energy =
        calculate_mass_specific_energy(reserve_mass, crusher_process.specific_energy());
    let charge_energy = primary_processing_energy
        .checked_add(reserve_energy)
        .unwrap_or_else(|| panic!("primitive progression reserve charge overflowed"));
    assert!(
        charge_energy <= drive_capacity,
        "primitive progression selected reserve must fit the constructed flywheel"
    );
    let charge_fill_ppm = u32::try_from(
        charge_energy
            .nanojoules()
            .checked_mul(1_000_000)
            .map(|scaled| scaled / drive_capacity.nanojoules())
            .unwrap_or_else(|| panic!("primitive progression flywheel fill ratio overflowed")),
    )
    .unwrap_or_else(|_| panic!("primitive progression flywheel fill ratio exceeded u32"));
    craft_for_profile(
        registries,
        state,
        raw,
        native_storage,
        shaped,
        equipment_assembly_profile(registries, EQUIPMENT_STONE_CRUSHER),
    );
    let crusher = validate_assemble_equipment(registries, state, EQUIPMENT_STONE_CRUSHER, shaped)
        .unwrap_or_else(|error| {
            panic!("primitive progression crusher construction failed: {error}")
        })
        .commit(state)
        .unwrap_or_else(|error| {
            panic!("primitive progression crusher construction commit failed: {error}")
        });
    let automation_hardware_ready_at = state.tick().value();

    craft_for_profile(
        registries,
        state,
        raw,
        native_storage,
        shaped,
        equipment_assembly_profile(registries, EQUIPMENT_STONE_SEPARATOR),
    );
    let separator =
        validate_assemble_equipment(registries, state, EQUIPMENT_STONE_SEPARATOR, shaped)
            .unwrap_or_else(|error| {
                panic!("primitive progression separator construction failed: {error}")
            })
            .commit(state)
            .unwrap_or_else(|error| {
                panic!("primitive progression separator construction commit failed: {error}")
            });
    let separator_ready_at = state.tick().value();
    let separator_preparation_ticks = duration(automation_hardware_ready_at, separator_ready_at);
    let automation_preparation_ticks =
        duration(preparation_started_at, automation_hardware_ready_at);
    let processing_line_preparation_ticks = duration(preparation_started_at, separator_ready_at);

    PrimitiveMachine {
        crank,
        crusher,
        separator,
        drive,
        drive_capacity,
        required_energy,
        separation_required_energy,
        charge_energy,
        reserve_mass,
        charge_fill_ppm,
        charge_ticks: 0,
        full_charge_ticks: 0,
        automation_preparation_ticks,
        separator_preparation_ticks,
        processing_line_preparation_ticks,
        crank_reinforced: false,
    }
}

fn charge_primitive_machine(
    registries: &Registries,
    state: &mut AppState,
    machine: PrimitiveMachine,
) -> PrimitiveMachine {
    let full_charge = validate_start_manual_power(
        registries,
        state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            machine.crank,
            machine.drive,
            machine.drive_capacity,
        ),
    )
    .unwrap_or_else(|error| {
        panic!("primitive progression full-accumulator charge projection failed: {error}")
    });
    let full_charge_work = full_charge.work();
    let full_charge_ticks = duration(
        full_charge_work.started_at().value(),
        full_charge_work.completes_at().value(),
    );

    let power = validate_start_manual_power(
        registries,
        state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            machine.crank,
            machine.drive,
            machine.charge_energy,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive progression manual charging failed: {error}"));
    let charge_work = power.work();
    let charge_ticks = duration(
        charge_work.started_at().value(),
        charge_work.completes_at().value(),
    );
    power
        .commit(state)
        .unwrap_or_else(|error| panic!("primitive progression charge commit failed: {error}"));
    advance_exact(registries, state, charge_ticks);
    assert_eq!(
        state
            .energy()
            .get_store(machine.drive)
            .map(|store| store.stored()),
        Some(machine.charge_energy),
        "primitive charging must deliver the requested finite stored work"
    );
    let automation_preparation_ticks = machine
        .automation_preparation_ticks
        .checked_add(charge_ticks)
        .unwrap_or_else(|| panic!("primitive automation preparation duration overflowed"));
    let processing_line_preparation_ticks = machine
        .processing_line_preparation_ticks
        .checked_add(charge_ticks)
        .unwrap_or_else(|| panic!("primitive processing-line preparation duration overflowed"));

    PrimitiveMachine {
        charge_ticks,
        full_charge_ticks,
        automation_preparation_ticks,
        processing_line_preparation_ticks,
        ..machine
    }
}

fn fill_primitive_accumulator(
    registries: &Registries,
    state: &mut AppState,
    machine: PrimitiveMachine,
) -> Result<u64, ManualPowerError> {
    let stored_before = state
        .energy()
        .get_store(machine.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("primitive progression flywheel disappeared before charging"));
    if stored_before >= machine.required_energy {
        return Ok(0);
    }
    let energy = machine
        .drive_capacity
        .checked_sub(stored_before)
        .unwrap_or_else(|| panic!("primitive accumulator exceeds its authored capacity"));
    assert!(!energy.is_zero());
    let power = validate_start_manual_power(
        registries,
        state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            machine.crank,
            machine.drive,
            energy,
        ),
    )?;
    let work = power.work();
    let ticks = duration(work.started_at().value(), work.completes_at().value());
    power
        .commit(state)
        .unwrap_or_else(|error| panic!("primitive progression recharge commit failed: {error}"));
    advance_exact(registries, state, ticks);
    assert_eq!(
        state
            .energy()
            .get_store(machine.drive)
            .map(|store| store.stored()),
        Some(machine.drive_capacity),
        "primitive accumulator fill must reach its authored capacity exactly"
    );
    Ok(ticks)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum PrimitiveSteadyStop {
    #[default]
    CycleLimit,
    NoConcurrentWork,
    CrusherCondition,
    CrankCondition,
}

impl PrimitiveSteadyStop {
    const fn label(self) -> &'static str {
        match self {
            Self::CycleLimit => "probe-cycle-limit",
            Self::NoConcurrentWork => "no-concurrent-player-work",
            Self::CrusherCondition => "crusher-condition-lifetime",
            Self::CrankCondition => "crank-condition-lifetime",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SteadyStateWork {
    cycles: u64,
    productive_payback_cycle: Option<u64>,
    charge_ticks: u64,
    machine_ticks: u64,
    useful_overlap_ticks: u64,
    player_free_ticks: u64,
    mined_mass: Mass,
    mining_jobs: u64,
    feed_buffer_limited_cycles: u64,
    stop: PrimitiveSteadyStop,
    terminal_crusher_condition_ppm: u32,
}

fn run_steady_state_crushing(
    registries: &Registries,
    state: &mut AppState,
    ore_storage: deep_hearth::inventory::StockpileId,
    crushed_storage: deep_hearth::inventory::StockpileId,
    machine: PrimitiveMachine,
    hard_ore_target: MiningTargetRequest,
    pick: deep_hearth::equipment::EquipmentId,
    mass: Mass,
    required_productive_ticks: u64,
) -> SteadyStateWork {
    let mut totals = SteadyStateWork::default();
    let mut productive_payback_cycle = None;
    for cycle in 1..=MAX_STEADY_STATE_CRUSH_CYCLES {
        let charge_ticks = match fill_primitive_accumulator(registries, state, machine) {
            Ok(ticks) => ticks,
            Err(
                ManualPowerError::ConditionDuration(_)
                | ManualPowerError::ZeroEquipmentPower { .. },
            ) => {
                totals.stop = PrimitiveSteadyStop::CrankCondition;
                break;
            }
            Err(error) => panic!("primitive progression steady recharge failed: {error}"),
        };
        totals.charge_ticks = totals
            .charge_ticks
            .checked_add(charge_ticks)
            .unwrap_or_else(|| panic!("primitive steady-state charge duration overflowed"));
        let work = match crush_while_mining(
            registries,
            state,
            ore_storage,
            crushed_storage,
            machine,
            mass,
            machine.required_energy,
            ConcurrentMiningPlan {
                target: hard_ore_target,
                destination: ore_storage,
                pick,
                mass,
            },
        ) {
            Ok(work) => work,
            Err(ComminutionResolutionError::ConditionDuration(_)) => {
                totals.stop = PrimitiveSteadyStop::CrusherCondition;
                break;
            }
            Err(error) => {
                panic!("primitive progression steady crushing resolution failed: {error}")
            }
        };
        totals.cycles = cycle;
        let player_free = finish_autonomous_crush(registries, state, work);
        let useful_overlap = work
            .crush_ticks
            .checked_sub(player_free)
            .unwrap_or_else(|| panic!("steady-state free time exceeded machine duration"));
        totals.machine_ticks = totals
            .machine_ticks
            .checked_add(work.crush_ticks)
            .unwrap_or_else(|| panic!("primitive steady-state machine duration overflowed"));
        totals.useful_overlap_ticks = totals
            .useful_overlap_ticks
            .checked_add(useful_overlap)
            .unwrap_or_else(|| panic!("primitive steady-state overlap duration overflowed"));
        totals.player_free_ticks = totals
            .player_free_ticks
            .checked_add(player_free)
            .unwrap_or_else(|| panic!("primitive steady-state free duration overflowed"));
        totals.mined_mass = totals
            .mined_mass
            .checked_add(work.mined_mass)
            .unwrap_or_else(|| panic!("primitive steady-state mined mass overflowed"));
        totals.mining_jobs = totals
            .mining_jobs
            .checked_add(work.mining_jobs)
            .unwrap_or_else(|| panic!("primitive steady-state mining-job count overflowed"));
        if work.autonomous_stop == AutonomousWorkStop::FeedBufferCapacity {
            totals.feed_buffer_limited_cycles = totals
                .feed_buffer_limited_cycles
                .checked_add(1)
                .unwrap_or_else(|| panic!("primitive steady-state buffer-limit count overflowed"));
        }
        if productive_payback_cycle.is_none()
            && totals.useful_overlap_ticks >= required_productive_ticks
        {
            productive_payback_cycle = Some(cycle);
        }
        if work.mining_jobs == 0
            && matches!(
                work.autonomous_stop,
                AutonomousWorkStop::TargetSupply | AutonomousWorkStop::ToolCondition
            )
        {
            totals.stop = PrimitiveSteadyStop::NoConcurrentWork;
            break;
        }
    }
    totals.productive_payback_cycle = productive_payback_cycle;
    totals.terminal_crusher_condition_ppm = state
        .equipment()
        .get_equipment(machine.crusher)
        .unwrap_or_else(|| panic!("primitive crusher disappeared at steady-state endpoint"))
        .condition()
        .parts_per_million();
    assert!(
        totals.cycles > 0,
        "primitive automation must complete useful work before its lifecycle endpoint"
    );
    totals
}

#[derive(Clone, Copy)]
struct ConcurrentMachineWork {
    job: ProductionJobId,
    machine_started_at: u64,
    crush_ticks: u64,
    player_work_ticks: u64,
    overlap_ticks: u64,
    mined_mass: Mass,
    mining_jobs: u64,
    autonomous_stop: AutonomousWorkStop,
}

#[derive(Clone, Copy)]
struct ConcurrentMiningPlan {
    target: MiningTargetRequest,
    destination: deep_hearth::inventory::StockpileId,
    pick: deep_hearth::equipment::EquipmentId,
    mass: Mass,
}

fn select_stockpile_mass(
    state: &AppState,
    stockpile: deep_hearth::inventory::StockpileId,
    mass: Mass,
) -> Vec<MaterialLotSelection> {
    assert!(!mass.is_zero());
    let mut remaining = mass;
    let mut selections = Vec::new();
    for lot in state.inventory().lot_ids(stockpile) {
        if remaining.is_zero() {
            break;
        }
        let available = state
            .inventory()
            .get_lot(lot)
            .unwrap_or_else(|| panic!("primitive progression selected ore lot disappeared"))
            .mass();
        let selected = Mass::from_milligrams(available.milligrams().min(remaining.milligrams()));
        if selected.is_zero() {
            continue;
        }
        selections.push(MaterialLotSelection::new(lot, selected));
        remaining = remaining
            .checked_sub(selected)
            .unwrap_or_else(|| unreachable!("selected ore mass is bounded by remaining demand"));
    }
    assert!(
        remaining.is_zero(),
        "primitive progression lacks {}mg of claimed ore for the selected crusher work order",
        remaining.milligrams()
    );
    selections
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ObservedMaterialSample {
    commodity: CommodityKey,
    copper_ppm: u32,
}

fn observe_single_material_sample(
    state: &AppState,
    stockpile: deep_hearth::inventory::StockpileId,
    context: &'static str,
) -> ObservedMaterialSample {
    let mut lots = state.inventory().lot_ids(stockpile);
    let lot = lots
        .next()
        .unwrap_or_else(|| panic!("primitive progression {context} has no extracted material"));
    assert!(
        lots.next().is_none(),
        "primitive progression {context} should remain one observable material sample"
    );
    let record = state
        .inventory()
        .get_lot(lot)
        .unwrap_or_else(|| panic!("primitive progression {context} sample disappeared"));
    ObservedMaterialSample {
        commodity: record.commodity(),
        copper_ppm: record.composition().parts_per_million(MATERIAL_COPPER),
    }
}

#[derive(Clone, Copy)]
struct PrimitiveSeparationWork {
    feed_mass: Mass,
    target_mass: Mass,
    residue_mass: Mass,
    required_energy: Energy,
    ticks: u64,
}

fn separate_native_copper(
    registries: &Registries,
    state: &mut AppState,
    crushed_storage: deep_hearth::inventory::StockpileId,
    native_storage: deep_hearth::inventory::StockpileId,
    residue_storage: deep_hearth::inventory::StockpileId,
    machine: PrimitiveMachine,
    feed_mass: Mass,
    expected_target: Mass,
) -> PrimitiveSeparationWork {
    let selections = select_stockpile_mass(state, crushed_storage, feed_mass);
    assert_eq!(
        selections.len(),
        1,
        "primary crusher output should remain one homogeneous lot before the progression separation step"
    );
    let native = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
    let target_before = state
        .inventory()
        .get_stockpile(native_storage)
        .map(|stockpile| stockpile.get_mass(native))
        .unwrap_or_else(|| panic!("primitive native-copper storage disappeared before separation"));
    let resolved = resolve_constituent_separation_process(
        registries,
        state,
        ConstituentSeparationRequest::new(
            PROCESS_SEPARATE_NATIVE_COPPER,
            crushed_storage,
            selections.as_slice(),
            machine.separator,
            machine.drive,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive progression separation resolution failed: {error}"));
    assert!(
        resolved.required_energy() <= machine.separation_required_energy,
        "selected progression feed must not exceed the conservative separation-energy allowance used to charge the primitive flywheel"
    );
    assert_eq!(resolved.target_mass(), expected_target);
    let ticks = resolved.process_resolution().duration().value();
    validate_start_process_routed(
        registries,
        state,
        resolved.process_resolution(),
        crushed_storage,
        &[
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::TARGET_STREAM,
                native_storage,
            ),
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::RESIDUE_STREAM,
                residue_storage,
            ),
        ],
    )
    .unwrap_or_else(|error| panic!("primitive progression separation start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("primitive progression separation commit failed: {error}"));
    advance_exact(registries, state, ticks);
    let target_after = state
        .inventory()
        .get_stockpile(native_storage)
        .map(|stockpile| stockpile.get_mass(native))
        .unwrap_or_else(|| panic!("primitive native-copper storage disappeared after separation"));
    assert_eq!(
        target_after.checked_sub(target_before),
        Some(expected_target),
        "processed ore must provide the exact copper parcel used for the second upgrade"
    );
    PrimitiveSeparationWork {
        feed_mass,
        target_mass: resolved.target_mass(),
        residue_mass: resolved.residue_mass(),
        required_energy: resolved.required_energy(),
        ticks,
    }
}

fn crush_while_mining(
    registries: &Registries,
    state: &mut AppState,
    ore_storage: deep_hearth::inventory::StockpileId,
    crushed_storage: deep_hearth::inventory::StockpileId,
    machine: PrimitiveMachine,
    crush_mass: Mass,
    expected_energy: Energy,
    concurrent: ConcurrentMiningPlan,
) -> Result<ConcurrentMachineWork, ComminutionResolutionError> {
    let machine_started_at = state.tick().value();
    let selection = select_stockpile_mass(state, ore_storage, crush_mass);
    let resolved = resolve_comminution_process(
        registries,
        state,
        ComminutionRequest::new(
            PROCESS_CRUSH_ORE,
            ore_storage,
            selection.as_slice(),
            machine.crusher,
            machine.drive,
        ),
    )?;
    assert_eq!(resolved.required_energy(), expected_energy);
    let crush_ticks = resolved.process_resolution().duration().value();
    let crush_job = validate_start_process(
        registries,
        state,
        resolved.process_resolution(),
        ore_storage,
        crushed_storage,
    )
    .unwrap_or_else(|error| panic!("primitive progression crushing start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("primitive progression crushing commit failed: {error}"));

    let mut player_work_ticks = 0_u64;
    let mut overlap_ticks = 0_u64;
    let mut mined_mass = Mass::ZERO;
    let mut mining_jobs = 0_u64;
    let autonomous_stop = loop {
        let Some(machine_job) = state.production().get_job(crush_job) else {
            break AutonomousWorkStop::MachineCompleted;
        };
        let machine_ticks_remaining = machine_job
            .completes_at()
            .value()
            .checked_sub(state.tick().value())
            .unwrap_or_else(|| {
                panic!("primitive crusher completion fell behind authoritative time")
            });
        let concurrent_target = resolve_progression_mining_target(state, concurrent.target);
        let concurrent_mining = match validate_start_mining(
            registries,
            state,
            MINING_METHOD_HAND_PICK,
            concurrent_target,
            concurrent.destination,
            concurrent.pick,
            concurrent.mass,
        ) {
            Ok(start) => start,
            Err(error) => {
                break match error {
                    MiningStartError::DestinationCapacityExceeded { .. } => {
                        AutonomousWorkStop::FeedBufferCapacity
                    }
                    MiningStartError::TargetDepleted
                    | MiningStartError::InsufficientTargetMass { .. } => {
                        AutonomousWorkStop::TargetSupply
                    }
                    MiningStartError::ConditionDuration(_) | MiningStartError::ZeroThroughput => {
                        AutonomousWorkStop::ToolCondition
                    }
                    other => panic!(
                        "primitive progression autonomous-window mining hit unexpected blocker: {other}"
                    ),
                };
            }
        };
        let concurrent_mining_job = concurrent_mining.commit(state).unwrap_or_else(|error| {
            panic!("primitive progression concurrent mining commit failed: {error}")
        });
        let work_ticks = state
            .mining()
            .get_job(concurrent_mining_job)
            .map(|record| duration(record.started_at().value(), record.completes_at().value()))
            .unwrap_or_else(|| panic!("primitive progression concurrent mining job disappeared"));
        if mining_jobs == 0 {
            assert!(
                state.production().get_job(crush_job).is_some()
                    && state.mining().get_job(concurrent_mining_job).is_some()
                    && state.player_work().active().is_some(),
                "autonomous crushing and player mining must coexist after both canonical starts"
            );
        }
        overlap_ticks = overlap_ticks
            .checked_add(machine_ticks_remaining.min(work_ticks))
            .unwrap_or_else(|| panic!("primitive concurrent overlap duration overflowed"));
        player_work_ticks = player_work_ticks
            .checked_add(work_ticks)
            .unwrap_or_else(|| panic!("primitive concurrent player-work duration overflowed"));
        mined_mass = mined_mass
            .checked_add(concurrent.mass)
            .unwrap_or_else(|| panic!("primitive concurrent mined mass overflowed"));
        mining_jobs = mining_jobs
            .checked_add(1)
            .unwrap_or_else(|| panic!("primitive concurrent mining-job count overflowed"));
        advance_exact(registries, state, work_ticks);
        assert!(
            state.mining().get_job(concurrent_mining_job).is_some(),
            "completed mining output must remain claimable after concurrent machine work"
        );
        validate_claim_mining_output(registries, state, concurrent_mining_job)
            .unwrap_or_else(|error| {
                panic!("primitive progression concurrent mining claim failed: {error}")
            })
            .commit(state)
            .unwrap_or_else(|error| {
                panic!("primitive progression concurrent mining claim commit failed: {error}")
            });
    };
    Ok(ConcurrentMachineWork {
        job: crush_job,
        machine_started_at,
        crush_ticks,
        player_work_ticks,
        overlap_ticks,
        mined_mass,
        mining_jobs,
        autonomous_stop,
    })
}

fn finish_autonomous_crush(
    registries: &Registries,
    state: &mut AppState,
    concurrent: ConcurrentMachineWork,
) -> u64 {
    let Some(job) = state.production().get_job(concurrent.job) else {
        return 0;
    };
    assert!(
        !job.is_suspended(),
        "primitive progression has no world mutation that should suspend its autonomous crusher"
    );
    let player_free_ticks = job
        .completes_at()
        .value()
        .checked_sub(state.tick().value())
        .unwrap_or_else(|| panic!("primitive crusher completion fell behind authoritative time"));
    advance_exact(registries, state, player_free_ticks);
    assert!(
        state.production().get_job(concurrent.job).is_none(),
        "primitive crusher should complete after its remaining autonomous work"
    );
    player_free_ticks
}

#[path = "progression_probe/episode.rs"]
mod episode;
use episode::run_primitive_progression_case;

#[path = "progression_probe/review.rs"]
mod review;

pub(super) use review::run_primitive_progression_probe;
