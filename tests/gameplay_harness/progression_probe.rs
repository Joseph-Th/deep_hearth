//! Canonical primitive-to-mechanized progression probe for the gameplay experience harness.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use super::environment::ROOM_TEMPERATURE;
use super::equipment_support::nominal_equipment_mass_capability;
use super::focused_runner::focused_probe_role_label;
use super::focused_seeds::{FocusedProbeCase, FocusedProbeRole};
use super::inventory_support::add_solid_stockpile;
use super::manual_craft_selection::select_manual_craft_request;
use super::manual_power_timing::finish_manual_power_work;
use super::material_selection::select_stockpile_mass;
use super::ore_fixture::copper_ore_composition;
use super::physical_time::format_physical_duration;
use super::production_timing::finish_uninterrupted_production_job;
use super::seed::mix64;
use deep_hearth::capability::{CapabilityId, CapabilityValue};
use deep_hearth::content::gameplay_fixture::{
    GeologicalDepositSeed, seed_geological_deposit, seed_lot,
};
use deep_hearth::content::{
    ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE, ENERGY_STONE_FLYWHEEL_DRIVE,
    EQUIPMENT_COPPER_REINFORCED_HAND_CRANK, EQUIPMENT_COPPER_REINFORCED_PICK,
    EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER, EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
    EQUIPMENT_STONE_CRUSHER, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_STONE_PICK,
    EQUIPMENT_STONE_SEPARATOR, FORM_NATIVE_METAL, FORM_ORE, FORM_REINFORCEMENT,
    MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER, MINING_METHOD_HAND_PICK,
    PROCESS_COLD_WORK_COPPER_REINFORCEMENT, PROCESS_CRUSH_ORE, PROCESS_HAND_BREAK_ORE,
    PROCESS_HAND_SORT_NATIVE_COPPER, PROCESS_KNAP_STONE_TOOL, PROCESS_SEPARATE_NATIVE_COPPER,
    PROCESS_SHAPE_STONE_FLYWHEEL, PROCESS_SHAPE_WOOD_HANDLE, PROSPECTING_DETAILED_FIELD_SURVEY,
    PROSPECTING_FIELD_INSPECTION, PROSPECTING_REGIONAL_RECONNAISSANCE,
};
use deep_hearth::core::quantity::{Energy, Mass, Power, Pressure};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::WorldSeed;
use deep_hearth::crafting::{ManualCraftStartRequest, validate_start_manual_craft};
use deep_hearth::energy::{
    calculate_mass_specific_energy, validate_assemble_energy_store, validate_upgrade_energy_store,
};
use deep_hearth::equipment::{
    EquipmentMaintenanceRequest, resolve_equipment_maintenance, validate_assemble_equipment,
    validate_equipment_maintenance, validate_upgrade_equipment,
};
use deep_hearth::geology::{
    FieldProspectingRequest, GeologicalEvidenceConsistency, assess_geological_knowledge,
    validate_start_field_prospecting,
};
use deep_hearth::labor::{
    ManualPowerError, ManualPowerRequest, ProspectingMethodId, validate_start_manual_power,
};
use deep_hearth::material::{
    COMPOSITION_PARTS_PER_MILLION, CommodityKey, MaterialAssemblyProfile, MaterialComposition,
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

const MAX_STEADY_STATE_CRUSH_CYCLES: u64 = 24;
const POST_PAYBACK_OBSERVATION_CYCLES: u64 = 2;
const MAINTAINED_EXTRACTION_GRADE_PREMIUM_PPM: u32 = 100_000;
const PROGRESSION_REGIONAL_ZONE_COUNT: usize = 2;

pub(super) fn extraction_grade_premium_ppm(case: FocusedProbeCase) -> u32 {
    match case.role() {
        FocusedProbeRole::MaintainedAnchor | FocusedProbeRole::MaintainedCoverage => {
            MAINTAINED_EXTRACTION_GRADE_PREMIUM_PPM
        }
        FocusedProbeRole::OrganicVariation | FocusedProbeRole::ExplicitReplay => {
            // Evaluation policy, not a production balance value. Organic actors vary how much
            // guaranteed grade improvement justifies delaying mechanization for better extraction.
            let behavior_seed = case.behavior_seed().unwrap_or_else(|| {
                panic!("progression actor-policy case is missing its behavior seed")
            });
            50_000 + (mix64(behavior_seed ^ 0x4752_4144_4550_5245) % 100_001) as u32
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum OreOpportunityDepth {
    Shallow,
    Deep,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OreOpportunity {
    #[cfg(test)]
    depth: OreOpportunityDepth,
    batch_budget: u64,
}

impl OreOpportunity {
    #[cfg(test)]
    pub(super) const fn depth(self) -> OreOpportunityDepth {
        self.depth
    }

    pub(super) const fn batch_budget(self) -> u64 {
        self.batch_budget
    }
}

pub(super) fn ore_opportunity(seed: u64, maintained_payback_required: bool) -> OreOpportunity {
    if maintained_payback_required {
        return OreOpportunity {
            #[cfg(test)]
            depth: OreOpportunityDepth::Deep,
            batch_budget: 512,
        };
    }
    let opportunity_roll = mix64(seed ^ 0x4F50_504F_5254_554E);
    if opportunity_roll.is_multiple_of(2) {
        OreOpportunity {
            #[cfg(test)]
            depth: OreOpportunityDepth::Shallow,
            batch_budget: 6 + (opportunity_roll >> 1) % 35,
        }
    } else {
        OreOpportunity {
            #[cfg(test)]
            depth: OreOpportunityDepth::Deep,
            batch_budget: 384 + (opportunity_roll >> 1) % 129,
        }
    }
}

fn reinforced_pick_mining_batch_limit(registries: &Registries) -> Mass {
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("primitive progression mining method disappeared"));
    nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        method.max_batch_mass_capability(),
    )
}

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

fn progression_regional_bounds(zone: usize) -> VoxelBounds {
    assert!(zone < PROGRESSION_REGIONAL_ZONE_COUNT);
    let x = i64::try_from(zone)
        .unwrap_or_else(|_| unreachable!("bounded progression regional zone fits i64"))
        .checked_mul(4)
        .unwrap_or_else(|| unreachable!("bounded regional clue coordinate cannot overflow"));
    VoxelBounds::new(VoxelCoord::new(x, -4, 0), VoxelCoord::new(x + 3, -3, 1))
        .unwrap_or_else(|error| panic!("primitive progression regional bounds failed: {error}"))
}

fn regional_zone_for_clue(region: VoxelBounds, zones: &[VoxelBounds]) -> usize {
    let mut matches = zones
        .iter()
        .enumerate()
        .filter(|(_, zone)| zone.has_intersection(region))
        .map(|(index, _)| index);
    let zone = matches
        .next()
        .unwrap_or_else(|| panic!("primitive progression clue lies outside regional search zones"));
    assert!(
        matches.next().is_none(),
        "primitive progression clue overlaps multiple regional search zones"
    );
    zone
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
/// The broader cold-agent catalog is discovered dynamically by the exploratory report. This
/// probe therefore does not freeze the whole playable catalog to an exact ID list just to protect its
/// own scenario. New routes may coexist without making this established primitive episode stale.
fn assert_progression_authored_dependencies(registries: &Registries) {
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
            definition.has_authored_acquisition_edge(),
            "primitive progression equipment {} lost its direct authored acquisition edge",
            equipment.value()
        );
        assert!(
            definition
                .maintenance_profile()
                .is_some_and(|profile| profile.is_component_replacement()),
            "primitive progression equipment {} lost its embodied-component service route",
            equipment.value()
        );
    }
    assert!(
        registries
            .ore_processing()
            .get_manual_comminution(PROCESS_HAND_BREAK_ORE)
            .is_some(),
        "primitive progression manual ore-breaking route disappeared"
    );
    assert!(
        registries
            .ore_processing()
            .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)
            .is_some(),
        "primitive progression manual native-copper sorting route disappeared"
    );
    assert!(
        registries
            .energy()
            .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
            .is_some_and(|definition| definition.has_authored_assembly_edge()),
        "primitive progression flywheel lost its direct authored assembly edge"
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
        PROSPECTING_REGIONAL_RECONNAISSANCE,
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
    let craft = select_manual_craft_request(
        registries,
        state,
        process,
        source,
        batches,
        "primitive progression repeated craft",
    );
    let job = validate_start_manual_craft(
        registries,
        state,
        ManualCraftStartRequest::new(craft, destination),
    )
    .unwrap_or_else(|error| panic!("primitive progression repeated craft failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("primitive progression repeated craft commit failed: {error}"));
    let duration = state
        .production()
        .get_job(job)
        .map(|record| record.active_duration())
        .unwrap_or_else(|| panic!("primitive progression craft job disappeared after start"));
    finish_uninterrupted_production_job(
        registries,
        state,
        job,
        duration,
        "primitive progression manual craft",
    );
}

fn finish_mining_work(
    registries: &Registries,
    state: &mut AppState,
    job: deep_hearth::mining::MiningJobId,
    concurrent_production: Option<ProductionJobId>,
    context: &'static str,
) -> u64 {
    let record = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("primitive progression {context} mining job disappeared"));
    let ticks = duration(record.started_at().value(), record.completes_at().value());
    for elapsed in 1..=ticks {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("primitive progression {context} tick failed: {error}"));
        assert!(
            outcome.production_availability_changes().is_empty(),
            "primitive progression {context} encountered an unexpected production availability change"
        );
        assert!(
            outcome.production_completions().iter().all(|completion| {
                concurrent_production.is_some_and(|expected| completion.job() == expected)
            }),
            "primitive progression {context} observed an unrelated production completion"
        );
        if elapsed < ticks {
            assert!(
                !outcome.ready_mining_jobs().contains(&job),
                "primitive progression {context} mining became ready before its validated completion"
            );
            assert_eq!(
                state.player_work().active(),
                Some(deep_hearth::labor::PlayerWork::Mining { job })
            );
        } else {
            assert_eq!(
                outcome.ready_mining_jobs(),
                &[job],
                "primitive progression {context} must expose the completed mining job exactly once"
            );
            assert_eq!(state.player_work().active(), None);
        }
    }
    ticks
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

fn manual_craft_plan_for_output(
    registries: &Registries,
    commodity: CommodityKey,
    required: Mass,
) -> (&deep_hearth::crafting::ManualCraftDefinition, u64) {
    assert!(!required.is_zero());
    let candidates = registries
        .crafting()
        .manual_producers(commodity)
        .map(|definition| {
            let batches =
                batches_for_output(required, output_mass_per_batch(definition, commodity));
            let total_ticks = definition
                .duration()
                .value()
                .checked_mul(batches)
                .unwrap_or_else(|| panic!("primitive manual-production attention cost overflowed"));
            let total_input_mg = definition
                .input_mass()
                .milligrams()
                .checked_mul(batches)
                .unwrap_or_else(|| panic!("primitive manual-production input cost overflowed"));
            let exertion = definition.exertion();
            let policy_key = (
                total_ticks,
                total_input_mg,
                exertion.energy_cost_per_tick().nanojoules(),
                exertion.hydration_loss_per_tick().microliters(),
            );
            (definition, batches, policy_key)
        })
        .collect::<Vec<_>>();
    let best_key = candidates
        .iter()
        .map(|(_, _, policy_key)| *policy_key)
        .min()
        .unwrap_or_else(|| {
            panic!(
                "primitive progression has no manual route to required component {}",
                commodity.value()
            )
        });
    let mut best = candidates
        .into_iter()
        .filter(|(_, _, policy_key)| *policy_key == best_key);
    let (definition, batches, _) = best
        .next()
        .unwrap_or_else(|| unreachable!("best manual-production policy key came from a candidate"));
    assert!(
        best.next().is_none(),
        "primitive progression has multiple equally efficient manual routes to component {}; add an explicit player-visible tie-break instead of using process identity",
        commodity.value()
    );
    (definition, batches)
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
                    "primitive progression equipment {} lost its authored assembly profile",
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
    let pick_service = registries
        .equipment()
        .get_equipment(EQUIPMENT_COPPER_REINFORCED_PICK)
        .and_then(|definition| definition.maintenance_profile())
        .unwrap_or_else(|| panic!("primitive reinforced pick lost its maintenance profile"));
    assert!(
        pick_service.is_component_replacement(),
        "primitive reinforced pick service must exchange an embodied component"
    );
    let service_entry = requirements
        .entry(pick_service.replacement())
        .or_insert(Mass::ZERO);
    add_mass(
        service_entry,
        pick_service.full_service_replacement_mass(),
        "primitive pick service reserve",
    );

    let mut process_batches: BTreeMap<deep_hearth::production::ProcessId, u64> = BTreeMap::new();
    for (commodity, required) in requirements {
        let (craft, batches) = manual_craft_plan_for_output(registries, commodity, required);
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

fn craft_requirement(
    registries: &Registries,
    state: &mut AppState,
    raw_source: deep_hearth::inventory::StockpileId,
    native_source: deep_hearth::inventory::StockpileId,
    destination: deep_hearth::inventory::StockpileId,
    commodity: CommodityKey,
    required: Mass,
) {
    let available = state
        .inventory()
        .get_stockpile(destination)
        .map(|stockpile| stockpile.get_mass(commodity))
        .unwrap_or_else(|| panic!("primitive progression shaped stockpile disappeared"));
    if available >= required {
        return;
    }
    let missing = required
        .checked_sub(available)
        .unwrap_or_else(|| unreachable!("available component mass was already checked"));
    let (craft, batches) = manual_craft_plan_for_output(registries, commodity, missing);
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
                commodity.value()
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

fn craft_for_profile(
    registries: &Registries,
    state: &mut AppState,
    raw_source: deep_hearth::inventory::StockpileId,
    native_source: deep_hearth::inventory::StockpileId,
    destination: deep_hearth::inventory::StockpileId,
    profile: &MaterialAssemblyProfile,
) {
    for input in profile.inputs() {
        craft_requirement(
            registries,
            state,
            raw_source,
            native_source,
            destination,
            input.commodity(),
            input.mass(),
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
                "primitive progression equipment {} has no authored assembly profile",
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
    try_mine_and_claim(registries, state, target, destination, equipment, mass)
        .unwrap_or_else(|error| panic!("primitive progression mining failed: {error}"))
}

fn try_mine_and_claim(
    registries: &Registries,
    state: &mut AppState,
    target: MiningTargetRequest,
    destination: deep_hearth::inventory::StockpileId,
    equipment: deep_hearth::equipment::EquipmentId,
    mass: Mass,
) -> Result<u64, MiningStartError> {
    let target = resolve_progression_mining_target(state, target);
    let mining = validate_start_mining(
        registries,
        state,
        MINING_METHOD_HAND_PICK,
        target,
        destination,
        equipment,
        mass,
    )?;
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
    assert_eq!(
        finish_mining_work(registries, state, mining_job, None, "mining"),
        mining_ticks
    );
    validate_claim_mining_output(registries, state, mining_job)
        .unwrap_or_else(|error| panic!("primitive progression mining claim failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| {
            panic!("primitive progression mining claim commit failed: {error}")
        });
    Ok(mining_ticks)
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
    try_mine_total_and_claim(
        registries,
        state,
        target,
        destination,
        equipment,
        total,
        maximum_batch,
    )
    .unwrap_or_else(|error| panic!("primitive progression mining failed: {error}"))
}

fn try_mine_total_and_claim(
    registries: &Registries,
    state: &mut AppState,
    target: MiningTargetRequest,
    destination: deep_hearth::inventory::StockpileId,
    equipment: deep_hearth::equipment::EquipmentId,
    total: Mass,
    maximum_batch: Mass,
) -> Result<u64, MiningStartError> {
    assert!(!total.is_zero());
    assert!(!maximum_batch.is_zero());
    let mut remaining = total;
    let mut elapsed = 0_u64;
    while !remaining.is_zero() {
        let batch = Mass::from_milligrams(remaining.milligrams().min(maximum_batch.milligrams()));
        elapsed = elapsed
            .checked_add(try_mine_and_claim(
                registries,
                state,
                target,
                destination,
                equipment,
                batch,
            )?)
            .unwrap_or_else(|| panic!("primitive progression mining duration overflowed"));
        remaining = remaining
            .checked_sub(batch)
            .unwrap_or_else(|| unreachable!("mining batch is bounded by remaining mass"));
    }
    Ok(elapsed)
}

fn resolve_progression_mining_target(
    state: &AppState,
    request: MiningTargetRequest,
) -> MiningTargetResolution {
    resolve_mining_target(state, request)
        .unwrap_or_else(|error| panic!("primitive progression mining evidence failed: {error}"))
}

fn acquire_copper_evidence(
    registries: &Registries,
    state: &mut AppState,
    method: ProspectingMethodId,
    region: VoxelBounds,
) -> u64 {
    let start = validate_start_field_prospecting(
        registries,
        state,
        FieldProspectingRequest::new(method, region, MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("primitive progression prospecting failed: {error}"));
    let work = start.work();
    let duration = work
        .completes_at()
        .value()
        .checked_sub(work.started_at().value())
        .unwrap_or_else(|| panic!("primitive progression prospecting schedule is inverted"));
    start
        .commit(state)
        .unwrap_or_else(|error| panic!("primitive progression prospecting commit failed: {error}"));
    let mut completion = None;
    for elapsed in 1..=duration {
        let outcome = advance_tick(registries, state).unwrap_or_else(|error| {
            panic!("primitive progression prospecting tick failed: {error}")
        });
        let acquired = outcome.field_prospecting();
        if elapsed < duration {
            assert_eq!(
                acquired, None,
                "primitive progression prospecting completed before its validated schedule"
            );
        } else {
            completion = acquired;
        }
    }
    let completion = completion.unwrap_or_else(|| {
        panic!("primitive progression prospecting produced no completion outcome")
    });
    assert_eq!(completion.method(), method);
    assert_eq!(completion.region(), region);
    assert_eq!(completion.material(), MATERIAL_COPPER);
    assert!(
        state
            .geological_knowledge()
            .get_observation(completion.observation())
            .is_some(),
        "prospecting completion receipt must identify the persisted observation"
    );
    duration
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
        other @ (GeologicalEvidenceConsistency::NoEvidence
        | GeologicalEvidenceConsistency::SpatiallyIncomparable
        | GeologicalEvidenceConsistency::Conflicting { .. }) => panic!(
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
    // Evidence quality is the actor's primary preference. Equal evidence uses the observable clue
    // region as an explicit stable policy rather than inheriting setup, registry, or iterator order.
    clues
        .into_iter()
        .max_by_key(|clue| {
            (
                clue.lower_ppm,
                clue.upper_ppm,
                Reverse(clue.request.region().min()),
                Reverse(clue.request.region().max_exclusive()),
            )
        })
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
    extraction_grade_premium_ppm: u32,
) -> PrimitivePriority {
    // The actor knows the exact grade of matter it already extracted, but only the evidence bounds
    // for the blocked hard seam. Prefer extraction only when the observed lower bound guarantees a
    // richer feed than the owned bulk sample; otherwise take the certain stored-work improvement.
    if hard_clue.lower_ppm.saturating_sub(bulk_sample_copper_ppm) >= extraction_grade_premium_ppm {
        PrimitivePriority::ExtractionFirst
    } else {
        PrimitivePriority::MechanizationFirst
    }
}

#[derive(Clone, Copy)]
struct PrimitiveProgressionExperience {
    natural_priority: PrimitivePriority,
    prospecting_ticks: u64,
    regional_recon_ticks: u64,
    regional_upper_bounds_ppm: [u32; PROGRESSION_REGIONAL_ZONE_COUNT],
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
    manual_bridge_feed_mass: Mass,
    manual_bridge_attention_ticks: u64,
    manual_bridge_recovery_ppm: u32,
    manual_bridge_metabolic_cost_nj: u128,
    manual_bridge_hydration_cost_ul: u64,
    selected_processing_feed_copper_ppm: u32,
    selected_processing_feed_is_hard: bool,
    processing_feed_selected_from_bulk: bool,
    post_convergence_mining_target_is_hard: bool,
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
    processing_line_preparation_metabolic_cost_nj: u128,
    processing_line_preparation_hydration_cost_ul: u64,
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
    flywheel_loss_before_reserve: Energy,
    reserve_recharge_ticks: u64,
    separation_ticks: u64,
    separation_completed_at: u64,
    processed_output_enabled_second_upgrade: bool,
    hard_ore_mined: Mass,
    hard_ore_before_convergence: Mass,
    total_ore_mined: Mass,
    direct_second_upgrade_blocked: bool,
    initial_crank_reinforced: bool,
    crank_reinforced: bool,
    maintenance_material_preparation_ticks: u64,
    component_service_mass: Mass,
    component_service_condition_before_ppm: u32,
    component_service_preserved_reinforcement: bool,
    final_pick_condition_ppm: u32,
    metabolic_energy_spent_nj: u128,
    hydration_spent_ul: u64,
    reinvestment: PrimitiveReinvestmentOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrimitiveReinvestmentOutcome {
    Completed(PrimitiveReinvestmentExperience),
    TargetSupplyLimited,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PrimitiveReinvestmentExperience {
    invested_copper_mass: Mass,
    base_crush_ticks: u64,
    reinforced_crush_ticks: u64,
    crusher_time_reduction_ppm: u32,
    base_separator_ticks: u64,
    reinforced_separator_ticks: u64,
    separator_time_reduction_ppm: u32,
    base_separator_target_mass: Mass,
    reinforced_separator_target_mass: Mass,
    base_separator_batch_capacity: Mass,
    upgraded_separator_batch_capacity: Mass,
    base_drive_capacity: Energy,
    upgraded_drive_capacity: Energy,
    expanded_batch_mass: Mass,
    expanded_batch_energy: Energy,
    expanded_charge_ticks: u64,
    expanded_crush_ticks: u64,
    expanded_separator_energy: Energy,
    expanded_separator_ticks: u64,
    expanded_separator_target_mass: Mass,
    survival_energy_spent_nj: u128,
    survival_hydration_spent_ul: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PrimitiveComponentService {
    preparation_ticks: u64,
    material_mass: Mass,
    condition_before_ppm: u32,
    preserved_reinforcement: bool,
}

fn service_reinforced_pick(
    registries: &Registries,
    state: &mut AppState,
    raw: deep_hearth::inventory::StockpileId,
    native_storage: deep_hearth::inventory::StockpileId,
    shaped: deep_hearth::inventory::StockpileId,
    pick: deep_hearth::equipment::EquipmentId,
) -> PrimitiveComponentService {
    let record = state
        .equipment()
        .get_equipment(pick)
        .unwrap_or_else(|| panic!("primitive progression pick disappeared before service"));
    assert_eq!(
        record.definition(),
        EQUIPMENT_COPPER_REINFORCED_PICK,
        "primitive service must preserve the converged reinforced pick rather than replace it"
    );
    let condition_before = record.condition();
    assert!(
        condition_before < deep_hearth::maintenance::Condition::PRISTINE,
        "primitive service demonstration requires real accumulated wear"
    );
    let profile = registries
        .equipment()
        .get_equipment(record.definition())
        .and_then(|definition| definition.maintenance_profile())
        .unwrap_or_else(|| panic!("primitive reinforced pick lost its service profile"));
    assert!(profile.is_component_replacement());
    let replacement = profile.replacement();
    let replacement_mass = profile.required_replacement_mass(condition_before);
    assert_eq!(replacement_mass, profile.full_service_replacement_mass());
    let reinforcement = CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT);
    let reinforcement_mass_before = record
        .embodied_material()
        .iter()
        .filter(|trace| trace.profile().commodity() == reinforcement)
        .fold(Mass::ZERO, |total, trace| {
            total
                .checked_add(trace.mass())
                .unwrap_or_else(|| panic!("primitive reinforcement mass overflowed"))
        });
    assert!(!reinforcement_mass_before.is_zero());

    let preparation_started_at = state.tick().value();
    craft_requirement(
        registries,
        state,
        raw,
        native_storage,
        shaped,
        replacement,
        replacement_mass,
    );
    let preparation_ticks = duration(preparation_started_at, state.tick().value());
    let resolution = resolve_equipment_maintenance(
        registries,
        state,
        EquipmentMaintenanceRequest::new(pick, shaped, raw),
    )
    .unwrap_or_else(|error| panic!("primitive pick service resolution failed: {error}"));
    assert!(resolution.replaces_embodied_component());
    assert_eq!(resolution.material_mass(), replacement_mass);
    let outcome = validate_equipment_maintenance(registries, state, resolution)
        .unwrap_or_else(|error| panic!("primitive pick service validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("primitive pick service commit failed: {error}"));
    assert_eq!(outcome.equipment(), pick);
    assert_eq!(outcome.material_mass(), replacement_mass);

    let serviced = state
        .equipment()
        .get_equipment(pick)
        .unwrap_or_else(|| panic!("primitive progression pick disappeared after service"));
    assert_eq!(serviced.definition(), EQUIPMENT_COPPER_REINFORCED_PICK);
    assert_eq!(
        serviced.condition(),
        deep_hearth::maintenance::Condition::PRISTINE
    );
    let reinforcement_mass_after = serviced
        .embodied_material()
        .iter()
        .filter(|trace| trace.profile().commodity() == reinforcement)
        .fold(Mass::ZERO, |total, trace| {
            total
                .checked_add(trace.mass())
                .unwrap_or_else(|| panic!("primitive serviced reinforcement mass overflowed"))
        });
    let preserved_reinforcement = reinforcement_mass_after == reinforcement_mass_before;
    assert!(
        preserved_reinforcement,
        "component service must retain the scarce copper reinforcement already invested in the pick"
    );

    PrimitiveComponentService {
        preparation_ticks,
        material_mass: replacement_mass,
        condition_before_ppm: condition_before.parts_per_million(),
        preserved_reinforcement,
    }
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
            let (craft, batches) =
                manual_craft_plan_for_output(registries, input.commodity(), input.mass());
            assert_eq!(
                craft.input(),
                native,
                "primitive copper upgrade component must remain directly cold-workable from native copper"
            );
            total.checked_add(multiply_mass(
                craft.input_mass(),
                batches,
                "upgrade native-copper input",
            ))
        })
        .unwrap_or_else(|| panic!("primitive upgrade native-copper requirement overflowed"))
}

fn feed_mass_for_exact_recovered_constituent(
    target: Mass,
    constituent_ppm: u32,
    target_recovery_ppm: u32,
) -> Mass {
    assert!(!target.is_zero());
    assert!(constituent_ppm > 0 && constituent_ppm < 1_000_000);
    assert!(target_recovery_ppm > 0 && target_recovery_ppm <= 1_000_000);
    let numerator = u128::from(target.milligrams())
        .checked_mul(1_000_000_000_000)
        .unwrap_or_else(|| panic!("primitive separation target scaling overflowed"));
    let recovery_factor = u128::from(constituent_ppm)
        .checked_mul(u128::from(target_recovery_ppm))
        .unwrap_or_else(|| panic!("primitive separation recovery factor overflowed"));
    let feed_mg = numerator.div_ceil(recovery_factor);
    let feed_mg = u64::try_from(feed_mg)
        .unwrap_or_else(|_| panic!("primitive separation feed mass exceeds authoritative range"));
    let feed = Mass::from_milligrams(feed_mg);
    let recovered = u128::from(feed.milligrams())
        .checked_mul(recovery_factor)
        .map(|scaled| scaled / 1_000_000_000_000)
        .unwrap_or_else(|| panic!("primitive recovered-target calculation overflowed"));
    assert_eq!(
        recovered,
        u128::from(target.milligrams()),
        "primitive separation feed selection must recover exactly one second-upgrade copper parcel after authored sorting loss"
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
    preparation_metabolic_cost_nj: u128,
    preparation_hydration_cost_ul: u64,
    crank_reinforced: bool,
}

#[derive(Clone, Copy)]
struct PrimitiveMachineBuildPlan {
    raw: deep_hearth::inventory::StockpileId,
    native_storage: deep_hearth::inventory::StockpileId,
    shaped: deep_hearth::inventory::StockpileId,
    mined_mass: Mass,
    separation_feed_mass: Mass,
    seed: u64,
}

fn build_primitive_machine(
    registries: &Registries,
    state: &mut AppState,
    plan: PrimitiveMachineBuildPlan,
) -> PrimitiveMachine {
    let PrimitiveMachineBuildPlan {
        raw,
        native_storage,
        shaped,
        mined_mass,
        separation_feed_mass,
        seed,
    } = plan;
    let preparation_started_at = state.tick().value();
    let survival_before = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("primitive processing-line builder lost player survival state"));
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
    let survival_after = assess_survival(registries, state).unwrap_or_else(|| {
        panic!("primitive processing-line builder lost player after construction")
    });
    let preparation_metabolic_cost_nj = survival_before
        .metabolic_energy()
        .checked_sub(survival_after.metabolic_energy())
        .unwrap_or_else(|| {
            unreachable!("manual processing-line construction cannot create metabolic reserve")
        })
        .nanojoules();
    let preparation_hydration_cost_ul = survival_before
        .hydration()
        .checked_sub(survival_after.hydration())
        .unwrap_or_else(|| {
            unreachable!("manual processing-line construction cannot create hydration reserve")
        })
        .microliters();

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
        preparation_metabolic_cost_nj,
        preparation_hydration_cost_ul,
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
    assert_eq!(
        finish_manual_power_work(
            registries,
            state,
            charge_work,
            "primitive accumulator charge"
        ),
        charge_ticks
    );
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
    required_energy: Energy,
) -> Result<u64, ManualPowerError> {
    assert!(
        required_energy <= machine.drive_capacity,
        "primitive accumulator cannot prepare work above its authored capacity"
    );
    let stored_before = state
        .energy()
        .get_store(machine.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("primitive progression flywheel disappeared before charging"));
    if stored_before >= required_energy {
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
    assert_eq!(
        finish_manual_power_work(registries, state, work, "primitive accumulator recharge"),
        ticks
    );
    let stored_after = state
        .energy()
        .get_store(machine.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("primitive progression flywheel disappeared after charging"));
    assert!(
        stored_after >= required_energy && stored_after <= machine.drive_capacity,
        "primitive accumulator recharge must leave enough work for the selected operation without exceeding capacity"
    );
    Ok(ticks)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum PrimitiveSteadyStop {
    #[default]
    CycleLimit,
    ProductivePaybackObserved,
    TargetSupply,
    ToolCondition,
    CrusherCondition,
    CrankCondition,
}

impl PrimitiveSteadyStop {
    const fn label(self) -> &'static str {
        match self {
            Self::CycleLimit => "probe-cycle-limit",
            Self::ProductivePaybackObserved => "productive-payback-observed",
            Self::TargetSupply => "known-target-supply",
            Self::ToolCondition => "player-tool-condition-lifetime",
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

#[derive(Clone, Copy)]
struct SteadyStateCrushingPlan {
    ore_storage: deep_hearth::inventory::StockpileId,
    crushed_storage: deep_hearth::inventory::StockpileId,
    machine: PrimitiveMachine,
    concurrent: ConcurrentMiningPlan,
    required_productive_ticks: u64,
}

fn run_steady_state_crushing(
    registries: &Registries,
    state: &mut AppState,
    plan: SteadyStateCrushingPlan,
) -> SteadyStateWork {
    let SteadyStateCrushingPlan {
        ore_storage,
        crushed_storage,
        machine,
        concurrent,
        required_productive_ticks,
    } = plan;
    let mut totals = SteadyStateWork::default();
    let mut productive_payback_cycle = None;
    for cycle in 1..=MAX_STEADY_STATE_CRUSH_CYCLES {
        let charge_ticks =
            match fill_primitive_accumulator(registries, state, machine, machine.required_energy) {
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
            CrushingBatch {
                mass: concurrent.mass,
                expected_energy: machine.required_energy,
            },
            concurrent,
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
        if work.mining_jobs == 0 {
            match work.autonomous_stop {
                AutonomousWorkStop::TargetSupply => {
                    totals.stop = PrimitiveSteadyStop::TargetSupply;
                    break;
                }
                AutonomousWorkStop::ToolCondition => {
                    totals.stop = PrimitiveSteadyStop::ToolCondition;
                    break;
                }
                AutonomousWorkStop::MachineCompleted | AutonomousWorkStop::FeedBufferCapacity => {}
            }
        }
        if productive_payback_cycle.is_some_and(|payback_cycle| {
            cycle >= payback_cycle.saturating_add(POST_PAYBACK_OBSERVATION_CYCLES)
        }) {
            totals.stop = PrimitiveSteadyStop::ProductivePaybackObserved;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ObservedMaterialSample {
    commodity: CommodityKey,
    copper_ppm: u32,
}

fn observe_material_sample(
    state: &AppState,
    stockpile: deep_hearth::inventory::StockpileId,
    context: &'static str,
) -> ObservedMaterialSample {
    let mut lots = state.inventory().lot_ids(stockpile);
    let first = lots
        .next()
        .unwrap_or_else(|| panic!("primitive progression {context} has no extracted material"));
    let first_record = state
        .inventory()
        .get_lot(first)
        .unwrap_or_else(|| panic!("primitive progression {context} sample disappeared"));
    let commodity = first_record.commodity();
    let composition = first_record.composition();
    for lot in lots {
        let record = state.inventory().get_lot(lot).unwrap_or_else(|| {
            panic!("primitive progression {context} sample fragment disappeared")
        });
        assert_eq!(
            record.commodity(),
            commodity,
            "primitive progression {context} contains physically different commodities and is not one observable sample"
        );
        assert_eq!(
            record.composition(),
            composition,
            "primitive progression {context} contains compositionally different lots and cannot be treated as one assay"
        );
    }
    ObservedMaterialSample {
        commodity,
        copper_ppm: composition.parts_per_million(MATERIAL_COPPER),
    }
}

#[derive(Clone, Copy)]
struct PrimitiveSeparationWork {
    feed_mass: Mass,
    target_mass: Mass,
    residue_mass: Mass,
    required_energy: Energy,
    charge_ticks: u64,
    ticks: u64,
}

#[derive(Clone, Copy)]
struct PrimitiveSeparationPlan {
    crushed_storage: deep_hearth::inventory::StockpileId,
    native_storage: deep_hearth::inventory::StockpileId,
    residue_storage: deep_hearth::inventory::StockpileId,
    machine: PrimitiveMachine,
    feed_mass: Mass,
    expected_target: Mass,
}

fn separate_native_copper(
    registries: &Registries,
    state: &mut AppState,
    plan: PrimitiveSeparationPlan,
) -> PrimitiveSeparationWork {
    let PrimitiveSeparationPlan {
        crushed_storage,
        native_storage,
        residue_storage,
        machine,
        feed_mass,
        expected_target,
    } = plan;
    let charge_ticks = fill_primitive_accumulator(
        registries,
        state,
        machine,
        machine.separation_required_energy,
    )
    .unwrap_or_else(|error| panic!("primitive separation recharge failed: {error}"));
    let selections = select_stockpile_mass(
        state,
        crushed_storage,
        feed_mass,
        "primitive separation feed",
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
    let job = validate_start_process_routed(
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
    finish_uninterrupted_production_job(
        registries,
        state,
        job,
        resolved.process_resolution().duration(),
        "primitive powered separation",
    );
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
        charge_ticks,
        ticks,
    }
}

#[derive(Clone, Copy)]
struct MatureReinvestmentPlan {
    raw: deep_hearth::inventory::StockpileId,
    shaped: deep_hearth::inventory::StockpileId,
    ore_storage: deep_hearth::inventory::StockpileId,
    crushed_storage: deep_hearth::inventory::StockpileId,
    native_storage: deep_hearth::inventory::StockpileId,
    residue_storage: deep_hearth::inventory::StockpileId,
    machine: PrimitiveMachine,
    pick: deep_hearth::equipment::EquipmentId,
    mining_target: MiningTargetRequest,
    primary_batch_mass: Mass,
    separation_feed_mass: Mass,
    reinforcement_mass: Mass,
}

fn crush_mass_for_exact_energy(registries: &Registries, energy: Energy) -> Mass {
    let process = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("primitive reinvestment crusher process disappeared"));
    let per_milligram = u128::from(process.specific_energy().nanojoules_per_milligram());
    assert!(per_milligram > 0);
    assert_eq!(
        energy.nanojoules() % per_milligram,
        0,
        "primitive reinvestment stored work must map to an exact crusher feed mass"
    );
    let milligrams = u64::try_from(energy.nanojoules() / per_milligram).unwrap_or_else(|_| {
        panic!("primitive reinvestment crusher mass exceeds authoritative range")
    });
    Mass::from_milligrams(milligrams)
}

fn resolve_crush_ticks(
    registries: &Registries,
    state: &AppState,
    source: deep_hearth::inventory::StockpileId,
    machine: PrimitiveMachine,
    mass: Mass,
    expected_energy: Energy,
    context: &'static str,
) -> u64 {
    let selection = select_stockpile_mass(state, source, mass, context);
    let resolved = resolve_comminution_process(
        registries,
        state,
        ComminutionRequest::new(
            PROCESS_CRUSH_ORE,
            source,
            selection.as_slice(),
            machine.crusher,
            machine.drive,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive reinvestment {context} resolution failed: {error}"));
    assert_eq!(resolved.required_energy(), expected_energy);
    resolved.process_resolution().duration().value()
}

fn run_uninterrupted_crush(
    registries: &Registries,
    state: &mut AppState,
    source: deep_hearth::inventory::StockpileId,
    destination: deep_hearth::inventory::StockpileId,
    machine: PrimitiveMachine,
    mass: Mass,
    expected_energy: Energy,
    context: &'static str,
) -> u64 {
    let selection = select_stockpile_mass(state, source, mass, context);
    let resolved = resolve_comminution_process(
        registries,
        state,
        ComminutionRequest::new(
            PROCESS_CRUSH_ORE,
            source,
            selection.as_slice(),
            machine.crusher,
            machine.drive,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive reinvestment {context} resolution failed: {error}"));
    assert_eq!(resolved.required_energy(), expected_energy);
    let ticks = resolved.process_resolution().duration().value();
    let job = validate_start_process(
        registries,
        state,
        resolved.process_resolution(),
        source,
        destination,
    )
    .unwrap_or_else(|error| panic!("primitive reinvestment {context} start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("primitive reinvestment {context} commit failed: {error}"));
    finish_uninterrupted_production_job(
        registries,
        state,
        job,
        resolved.process_resolution().duration(),
        context,
    );
    ticks
}

fn charge_exact_reinvestment_energy(
    registries: &Registries,
    state: &mut AppState,
    machine: PrimitiveMachine,
    energy: Energy,
) -> u64 {
    let start = validate_start_manual_power(
        registries,
        state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            machine.crank,
            machine.drive,
            energy,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive reinvestment accumulator charge failed: {error}"));
    let work = start.work();
    let ticks = duration(work.started_at().value(), work.completes_at().value());
    start
        .commit(state)
        .unwrap_or_else(|error| panic!("primitive reinvestment charge commit failed: {error}"));
    assert_eq!(
        finish_manual_power_work(registries, state, work, "primitive reinvestment charge"),
        ticks
    );
    ticks
}

#[derive(Clone, Copy)]
struct CrushingBatch {
    mass: Mass,
    expected_energy: Energy,
}

fn crush_while_mining(
    registries: &Registries,
    state: &mut AppState,
    ore_storage: deep_hearth::inventory::StockpileId,
    crushed_storage: deep_hearth::inventory::StockpileId,
    machine: PrimitiveMachine,
    batch: CrushingBatch,
    concurrent: ConcurrentMiningPlan,
) -> Result<ConcurrentMachineWork, ComminutionResolutionError> {
    let CrushingBatch {
        mass: crush_mass,
        expected_energy,
    } = batch;
    let machine_started_at = state.tick().value();
    let selection = select_stockpile_mass(state, ore_storage, crush_mass, "primitive crusher feed");
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
                    MiningStartError::TargetNoLongerResolved
                    | MiningStartError::InsufficientTargetMass { .. } => {
                        AutonomousWorkStop::TargetSupply
                    }
                    MiningStartError::ConditionDuration(_) | MiningStartError::ZeroThroughput => {
                        AutonomousWorkStop::ToolCondition
                    }
                    other @ (MiningStartError::UnknownMethod { .. }
                    | MiningStartError::ZeroMass
                    | MiningStartError::Equipment(_)
                    | MiningStartError::EquipmentMounted { .. }
                    | MiningStartError::EquipmentBusyProduction { .. }
                    | MiningStartError::EquipmentBusyMining { .. }
                    | MiningStartError::MissingCapability { .. }
                    | MiningStartError::CapabilityKindMismatch { .. }
                    | MiningStartError::BatchTooLarge { .. }
                    | MiningStartError::TargetTooHard { .. }
                    | MiningStartError::Duration(_)
                    | MiningStartError::CompletionTickOverflow
                    | MiningStartError::InvalidOutput(_)
                    | MiningStartError::UnknownDestination { .. }
                    | MiningStartError::DestinationStorage(_)
                    | MiningStartError::DestinationMassOverflow { .. }
                    | MiningStartError::InventoryRevisionExhausted
                    | MiningStartError::DestinationSupport(_)
                    | MiningStartError::MiningIdExhausted
                    | MiningStartError::MiningRevisionExhausted
                    | MiningStartError::Work(_)) => panic!(
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
        assert_eq!(
            finish_mining_work(
                registries,
                state,
                concurrent_mining_job,
                Some(crush_job),
                "concurrent mining",
            ),
            work_ticks
        );
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
    let mut completion_seen = false;
    for elapsed in 1..=player_free_ticks {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("primitive autonomous crusher tick failed: {error}"));
        assert!(
            !outcome
                .production_availability_changes()
                .iter()
                .any(|change| {
                    matches!(
                        change,
                        deep_hearth::production::ProductionAvailabilityChange::Suspended {
                            job: changed_job,
                            ..
                        } | deep_hearth::production::ProductionAvailabilityChange::Resumed {
                            job: changed_job,
                            ..
                        } if *changed_job == concurrent.job
                    )
                }),
            "primitive autonomous crusher unexpectedly changed availability"
        );
        if outcome
            .production_completions()
            .iter()
            .any(|completion| completion.job() == concurrent.job)
        {
            assert_eq!(
                elapsed, player_free_ticks,
                "primitive autonomous crusher completed before its authoritative schedule"
            );
            completion_seen = true;
        }
    }
    assert!(
        completion_seen,
        "primitive autonomous crusher produced no completion receipt at its authoritative schedule"
    );
    player_free_ticks
}

#[path = "progression_probe/reinvestment.rs"]
mod reinvestment;
use reinvestment::evaluate_mature_reinvestment;

#[path = "progression_probe/episode.rs"]
mod episode;
use episode::run_primitive_progression_case;

#[path = "progression_probe/manual_processing.rs"]
pub(super) mod manual_processing;
use manual_processing::{
    OwnedOreManualBridgePlan, evaluate_manual_processing_fallback, evaluate_owned_ore_manual_bridge,
};

#[path = "progression_probe/review.rs"]
mod review;

pub(super) use review::run_primitive_progression_probe;
