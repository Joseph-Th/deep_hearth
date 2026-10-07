//! Ordinary first-foundry continuation from the already-proven native-copper stage.

use std::collections::BTreeMap;

use deep_hearth::content::gameplay_fixture::{
    seed_assembled_energy_store_at, seed_assembled_equipment_at, seed_composed_lot, seed_lot,
    seed_preused_assembled_equipment_at, seed_stockpile,
};
use deep_hearth::content::{
    ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE, ENERGY_COPPER_PLATE_ELECTRICAL_BUFFER,
    ENERGY_STONE_THERMAL_SINK, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
    EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER, EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
    EQUIPMENT_FOUR_CAVITY_STONE_INGOT_MOLD, EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE,
    EQUIPMENT_STONE_INGOT_MOLD, EQUIPMENT_TIMBER_FRAME_SAW_BENCH, EQUIPMENT_TIMBER_TREADLE_DRIVE,
    EQUIPMENT_TIMBER_TREADLE_DYNAMO, EQUIPMENT_TIMBER_TREADLE_HAMMER, FORM_INGOT, FORM_LOG,
    FORM_LUMP, FORM_NATIVE_METAL, FORM_ORE, FORM_REINFORCEMENT, MANUAL_POWER_HAND_CRANK,
    MANUAL_POWER_TREADLE_DYNAMO, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
    PROCESS_CAST_PURE_COPPER, PROCESS_HAND_SORT_NATIVE_COPPER, PROCESS_MELT_PURE_COPPER,
};
use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::energy::validate_assemble_energy_store;
use deep_hearth::equipment::{validate_assemble_equipment, validate_upgrade_equipment};
use deep_hearth::inventory::{StockpileId, StockpileStorageProfile};
use deep_hearth::labor::{ManualPowerRequest, validate_start_manual_power};
use deep_hearth::maintenance::Condition;
use deep_hearth::material::{CommodityKey, MaterialComposition};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::production::validate_start_player_process;
use deep_hearth::registry::Registries;
use deep_hearth::survival::assess_survival;
use deep_hearth::thermal::{
    MeltingRequest, calculate_fusion_heat, calculate_sensible_heat, resolve_melting_process,
};

use super::environment::ROOM_TEMPERATURE;
use super::equipment_support::nominal_equipment_mass_capability;
use super::exact_local_runtime::STATIONARY_PLAYER_ORIGIN;
use super::focused_case::FocusedProbeCase;
use super::inventory_support::add_solid_stockpile;
use super::manual_craft_execution::execute_manual_craft;
use super::manual_power_timing::finish_manual_power_work;
use super::material_selection::observable_material_cohorts;
use super::ore_fixture::copper_ore_composition;
use super::physical_time::format_physical_duration;
use super::production_timing::finish_uninterrupted_production_job;
use super::workshop_craft_planning::manual_craft_plan_with_available_equipment;
#[path = "first_foundry_probe/casting.rs"]
mod casting;
#[cfg(test)]
include_first_foundry_generation_contract_tests!();
#[path = "first_foundry_probe/planning.rs"]
mod planning;
#[path = "first_foundry_probe/recovery.rs"]
mod recovery;

use self::casting::resolve_full_cast_after_cooldown;
use self::planning::{
    craft_foundry_components, foundry_bootstrap_route_plan, foundry_resource_opportunity,
    inherited_equipment_condition, select_commodity_mass, settlement_mold_ingot_requirement,
    settlement_mold_stone_requirement,
};
use self::recovery::{
    InheritedProcessingLine, PoweredOreRecoveryPlan, current_processing_batch_limit,
    execute_powered_ore_recovery, minimum_current_powered_ore_feed_for_target_recovery,
};

#[derive(Clone, Copy)]
struct PriorSettlementWorkshop {
    component_source: StockpileId,
    frame_saw: deep_hearth::equipment::EquipmentId,
    treadle_hammer: deep_hearth::equipment::EquipmentId,
    processing_provider: deep_hearth::equipment::EquipmentId,
    crusher: deep_hearth::equipment::EquipmentId,
    separator: deep_hearth::equipment::EquipmentId,
    processing_drive: deep_hearth::energy::EnergyStoreId,
    minimum_start_condition_ppm: u32,
    maximum_start_condition_ppm: u32,
}

fn seed_prior_settlement_workshop(
    registries: &Registries,
    state: &mut AppState,
    case: FocusedProbeCase,
) -> PriorSettlementWorkshop {
    let definitions = [
        EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
        EQUIPMENT_TIMBER_TREADLE_HAMMER,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
        EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
    ];
    let mut requirements = BTreeMap::<CommodityKey, Mass>::new();
    for definition in definitions {
        let assembly = registries
            .equipment()
            .get_equipment(definition)
            .and_then(|record| record.assembly_profile())
            .unwrap_or_else(|| panic!("first foundry prior settlement equipment lost assembly"));
        for input in assembly.inputs() {
            let total = requirements.entry(input.commodity()).or_insert(Mass::ZERO);
            *total = total
                .checked_add(input.mass())
                .unwrap_or_else(|| panic!("first foundry prior workshop material overflowed"));
        }
    }
    let processing_drive_profile = registries
        .energy()
        .get_store(ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("first foundry inherited processing drive lost assembly"));
    for input in processing_drive_profile.inputs() {
        let total = requirements.entry(input.commodity()).or_insert(Mass::ZERO);
        *total = total
            .checked_add(input.mass())
            .unwrap_or_else(|| panic!("first foundry processing-drive material overflowed"));
    }
    let capacity = requirements
        .values()
        .copied()
        .try_fold(Mass::ZERO, Mass::checked_add)
        .unwrap_or_else(|| panic!("first foundry prior workshop capacity overflowed"));
    let source = seed_stockpile(
        state,
        capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    for (commodity, mass) in requirements {
        let _ = seed_lot(registries, state, source, commodity, mass, ROOM_TEMPERATURE);
    }
    let mut start_conditions = Vec::with_capacity(definitions.len());
    let mut assemble = |definition| {
        let condition = inherited_equipment_condition(registries, definition, case);
        start_conditions.push(condition.parts_per_million());
        if condition == Condition::PRISTINE {
            seed_assembled_equipment_at(
                registries,
                state,
                definition,
                source,
                STATIONARY_PLAYER_ORIGIN,
            )
        } else {
            seed_preused_assembled_equipment_at(
                registries,
                state,
                definition,
                source,
                STATIONARY_PLAYER_ORIGIN,
                condition,
            )
        }
    };
    let frame_saw = assemble(EQUIPMENT_TIMBER_FRAME_SAW_BENCH);
    let treadle_hammer = assemble(EQUIPMENT_TIMBER_TREADLE_HAMMER);
    let processing_provider = assemble(EQUIPMENT_COPPER_REINFORCED_HAND_CRANK);
    let crusher = assemble(EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER);
    let separator = assemble(EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR);
    let processing_drive = seed_assembled_energy_store_at(
        registries,
        state,
        ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
        source,
        STATIONARY_PLAYER_ORIGIN,
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO),
        "first foundry inherited workshop must embody its complete disclosed component stock"
    );
    PriorSettlementWorkshop {
        component_source: source,
        frame_saw,
        treadle_hammer,
        processing_provider,
        crusher,
        separator,
        processing_drive,
        minimum_start_condition_ppm: *start_conditions
            .iter()
            .min()
            .unwrap_or_else(|| unreachable!("inherited workshop has equipment")),
        maximum_start_condition_ppm: *start_conditions
            .iter()
            .max()
            .unwrap_or_else(|| unreachable!("inherited workshop has equipment")),
    }
}

pub(super) fn run_first_foundry_probe(registries: &Registries, case: FocusedProbeCase) {
    let melting_definition = registries
        .thermal()
        .get_melting(PROCESS_MELT_PURE_COPPER)
        .unwrap_or_else(|| panic!("first foundry melting definition disappeared"));
    let casting_definition = registries
        .thermal()
        .get_casting(PROCESS_CAST_PURE_COPPER)
        .unwrap_or_else(|| panic!("first foundry casting definition disappeared"));
    let first_cast_mass = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE,
        melting_definition.max_batch_mass_capability(),
    )
    .min(nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_STONE_INGOT_MOLD,
        casting_definition.max_batch_mass_capability(),
    ));
    assert!(
        !first_cast_mass.is_zero(),
        "first foundry authored furnace/mold batch must remain nonzero"
    );
    let settlement_cast_mass = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_FOUR_CAVITY_STONE_INGOT_MOLD,
        casting_definition.max_batch_mass_capability(),
    );
    assert!(
        settlement_cast_mass > first_cast_mass,
        "cast-ingot reinvestment must increase the player's available casting batch"
    );
    assert!(
        settlement_cast_mass
            .milligrams()
            .is_multiple_of(first_cast_mass.milligrams()),
        "settlement cast must be an integral number of first-foundry melts"
    );

    let route_plan = foundry_bootstrap_route_plan(registries, case, first_cast_mass);
    let capital_copper = route_plan.capital_native_copper();
    let settlement_ingots = settlement_mold_ingot_requirement(registries);
    let settlement_stone = settlement_mold_stone_requirement(registries);
    let component_stone = route_plan
        .capital_raw
        .get(&CommodityKey::new(MATERIAL_STONE, FORM_LUMP))
        .copied()
        .unwrap_or(Mass::ZERO);
    let component_wood = route_plan
        .capital_raw
        .get(&CommodityKey::new(MATERIAL_WOOD, FORM_LOG))
        .copied()
        .unwrap_or(Mass::ZERO);
    let disclosed_stone_opportunity = component_stone
        .checked_add(settlement_stone)
        .unwrap_or_else(|| panic!("first foundry disclosed stone opportunity overflowed"));
    // Machinery should be justified by disclosed work, not merely by enough matter to assemble
    // the next unlock. Require enough copper to build the first foundry, cast the mold-upgrade
    // stock, and then actually run one settlement-size batch through that upgraded capability.
    let resource_opportunity = foundry_resource_opportunity(
        registries,
        case,
        &route_plan,
        settlement_ingots,
        settlement_cast_mass,
    );
    let native_opportunity = resource_opportunity.native;
    let inherited_ore = resource_opportunity.owned_ore;
    let required_after_current = resource_opportunity.required_after_current;

    let settlement_mold_upgrade = registries
        .equipment()
        .get_equipment(EQUIPMENT_FOUR_CAVITY_STONE_INGOT_MOLD)
        .and_then(|definition| definition.upgrade_profile())
        .unwrap_or_else(|| panic!("settlement foundry mold lost its upgrade profile"));
    assert_eq!(
        settlement_mold_upgrade.from(),
        EQUIPMENT_STONE_INGOT_MOLD,
        "settlement casting mold must remain an additive upgrade of the first foundry mold"
    );

    let mut state = AppState::new();
    let prior_workshop = seed_prior_settlement_workshop(registries, &mut state, case);
    let frame_saw = prior_workshop.frame_saw;
    let treadle_hammer = prior_workshop.treadle_hammer;
    let processing_line = InheritedProcessingLine {
        crusher: prior_workshop.crusher,
        separator: prior_workshop.separator,
        drive: prior_workshop.processing_drive,
        power_method: MANUAL_POWER_HAND_CRANK,
        provider: prior_workshop.processing_provider,
    };
    assert_eq!(
        current_processing_batch_limit(registries, &state, processing_line),
        Some(resource_opportunity.recovery_batch_limit),
        "first foundry generated recovery scale must match the live inherited processing line"
    );
    let workshop_tools = [frame_saw, treadle_hammer];
    let non_copper_component_raw = route_plan
        .capital_raw_mass()
        .checked_sub(capital_copper)
        .unwrap_or_else(|| unreachable!("component copper is part of total component raw mass"));
    let raw_capacity = non_copper_component_raw
        .checked_add(native_opportunity)
        .and_then(|mass| mass.checked_add(inherited_ore.mass))
        .unwrap_or_else(|| panic!("first foundry disclosed raw opportunity overflowed"));
    let raw = add_solid_stockpile(&mut state, raw_capacity);
    // Component fabrication only redistributes finite disclosed raw matter. The extra ore-sized
    // capacity is inbound headroom for native copper recovered from the separate owned-ore
    // stockpile; no additional matter is seeded into the raw store.
    let parts = add_solid_stockpile(&mut state, raw_capacity);
    let owned_ore = add_solid_stockpile(&mut state, inherited_ore.mass);
    let recovery_crushed = add_solid_stockpile(&mut state, inherited_ore.mass);
    let recovery_residue = add_solid_stockpile(&mut state, inherited_ore.mass);
    let current_output = add_solid_stockpile(&mut state, first_cast_mass);
    let upgrade_source =
        add_solid_stockpile(&mut state, settlement_mold_upgrade.additions().input_mass());
    let settlement_output = add_solid_stockpile(&mut state, settlement_cast_mass);
    let melting_point = registries
        .materials()
        .get_material(MATERIAL_COPPER)
        .and_then(|material| material.properties().thermal().melting_point())
        .unwrap_or_else(|| panic!("first foundry copper melting point disappeared"));
    let molten_profile = StockpileStorageProfile::new(false, true, melting_point)
        .unwrap_or_else(|error| panic!("first foundry molten storage profile failed: {error}"));
    let molten = seed_stockpile(&mut state, settlement_cast_mass, molten_profile);

    for (&commodity, &mass) in &route_plan.capital_raw {
        if commodity == CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL) {
            continue;
        }
        let _ = seed_lot(
            registries,
            &mut state,
            raw,
            commodity,
            mass,
            ROOM_TEMPERATURE,
        );
    }
    let _ = seed_lot(
        registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        native_opportunity,
        ROOM_TEMPERATURE,
    );
    seed_composed_lot(
        registries,
        &mut state,
        owned_ore,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        inherited_ore.mass,
        ROOM_TEMPERATURE,
        copper_ore_composition(inherited_ore.copper_ppm, 200_000),
    );
    for input in settlement_mold_upgrade
        .additions()
        .inputs()
        .iter()
        .filter(|input| input.commodity() != CommodityKey::new(MATERIAL_COPPER, FORM_INGOT))
    {
        let _ = seed_lot(
            registries,
            &mut state,
            upgrade_source,
            input.commodity(),
            input.mass(),
            ROOM_TEMPERATURE,
        );
    }
    super::world_admission::admit_stationary_player(
        registries,
        &mut state,
        &[
            prior_workshop.component_source,
            raw,
            parts,
            owned_ore,
            recovery_crushed,
            recovery_residue,
            current_output,
            upgrade_source,
            molten,
            settlement_output,
        ],
        &[],
        "first foundry",
    );
    let survival_before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("first foundry player survival disappeared"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("first foundry matter setup failed: {error}"))
        .total();
    let started_at = state.tick().value();

    // Fulfill the immediate reinforcement order through the cheapest canonically available manual
    // route. The foundry is considered only for the next capability step, after the player has paid
    // the current-order cost and can see exactly how much copper remains.
    let reinforcement = CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT);
    let (current_request, _current_batches) = manual_craft_plan_with_available_equipment(
        registries,
        &state,
        &[raw],
        &workshop_tools,
        reinforcement,
        first_cast_mass,
        "first foundry current reinforcement order",
    );
    let current_input = current_request
        .selections()
        .iter()
        .map(|selection| selection.mass())
        .try_fold(Mass::ZERO, Mass::checked_add)
        .unwrap_or_else(|| panic!("first foundry current-order input mass overflowed"));
    let current_tool_definition = current_request.equipment().and_then(|equipment| {
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.definition())
    });
    assert_eq!(
        (
            current_request.process(),
            current_input,
            current_tool_definition
        ),
        (
            route_plan.immediate.process,
            route_plan.immediate.input_mass,
            route_plan.immediate.equipment
        ),
        "first foundry pre-action route projection must match the live current-order planner"
    );
    let current_tool = current_request.equipment();
    let direct_native_ticks = execute_manual_craft(
        registries,
        &mut state,
        current_request,
        current_output,
        "first foundry current-order native working",
    )
    .value();
    let direct_native_reinforcement = state
        .inventory()
        .get_stockpile(current_output)
        .unwrap_or_else(|| panic!("first foundry current-order output disappeared"))
        .get_mass(reinforcement);
    assert!(
        direct_native_reinforcement >= first_cast_mass,
        "direct native-copper route must satisfy the immediate reinforcement order"
    );

    let remaining_native_before_recovery = state
        .inventory()
        .get_stockpile(raw)
        .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL)))
        .unwrap_or_else(|| panic!("first foundry raw stockpile disappeared"));
    let shortfall_before_recovery = required_after_current
        .checked_sub(remaining_native_before_recovery)
        .unwrap_or(Mass::ZERO);
    let ore_commodity = CommodityKey::new(MATERIAL_COPPER, FORM_ORE);
    let mut live_ore_cohorts = observable_material_cohorts(&state, owned_ore)
        .into_iter()
        .filter(|cohort| cohort.profile().commodity() == ore_commodity);
    let live_ore = live_ore_cohorts
        .next()
        .unwrap_or_else(|| panic!("first foundry admitted owned ore disappeared"));
    assert!(
        live_ore_cohorts.next().is_none(),
        "first foundry owned ore has multiple observable material profiles; actor policy must choose a cohort explicitly"
    );
    let live_owned_ore_mass = live_ore.mass();
    let live_owned_ore_copper_ppm = live_ore
        .profile()
        .composition()
        .parts_per_million(MATERIAL_COPPER);
    assert!(
        !live_owned_ore_mass.is_zero() && live_owned_ore_copper_ppm > 0,
        "first foundry admitted owned ore must remain a visible positive copper opportunity"
    );
    let sorting = registries
        .ore_processing()
        .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("first foundry manual recovery fallback disappeared"));
    let recovery_planned_feed = (!shortfall_before_recovery.is_zero())
        .then(|| {
            minimum_current_powered_ore_feed_for_target_recovery(
                registries,
                &state,
                processing_line,
                shortfall_before_recovery,
                live_owned_ore_copper_ppm,
            )
        })
        .flatten();
    let recovery_available =
        recovery_planned_feed.is_some_and(|required| required <= live_owned_ore_mass);
    let mut recovery_feed = Mass::ZERO;
    let mut recovery_attention = 0_u64;
    let mut recovery_autonomous = 0_u64;
    let mut recovery_elapsed = 0_u64;
    let mut recovery_batches = 0_u64;
    let mut recovered_native = Mass::ZERO;
    let mut recovery_stop = if shortfall_before_recovery.is_zero() {
        "not-needed"
    } else if recovery_planned_feed.is_none() {
        "processing-line-unavailable"
    } else if recovery_available {
        "not-executed"
    } else {
        "owned-ore-insufficient"
    };
    let manual_recovery_ppm = sorting.target_recovery_ppm();
    let powered_recovery_ppm = registries
        .ore_processing()
        .get_constituent_separation(deep_hearth::content::PROCESS_SEPARATE_NATIVE_COPPER)
        .map(|definition| definition.target_recovery_ppm())
        .unwrap_or_else(|| panic!("first foundry powered copper recovery reference disappeared"));
    if recovery_available {
        let recovery = execute_powered_ore_recovery(
            registries,
            &mut state,
            PoweredOreRecoveryPlan {
                ore_source: owned_ore,
                crushed_destination: recovery_crushed,
                native_destination: raw,
                residue_destination: recovery_residue,
                target: shortfall_before_recovery,
                copper_ppm: live_owned_ore_copper_ppm,
                line: processing_line,
            },
        );
        recovery_feed = recovery.feed_mass;
        recovery_attention = recovery.attention_ticks;
        recovery_autonomous = recovery.autonomous_ticks;
        recovery_elapsed = recovery.elapsed_ticks;
        recovery_batches = recovery.batches;
        recovered_native = recovery.recovered_native;
        recovery_stop = recovery.stop.label();
        assert_eq!(recovery.recovery_ppm, powered_recovery_ppm);
    }
    let remaining_native = state
        .inventory()
        .get_stockpile(raw)
        .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL)))
        .unwrap_or_else(|| {
            panic!("first foundry raw stockpile disappeared after recovery decision")
        });
    let shortfall_after_recovery = required_after_current
        .checked_sub(remaining_native)
        .unwrap_or(Mass::ZERO);
    if !shortfall_after_recovery.is_zero() {
        validate_loaded_state(registries, &state)
            .unwrap_or_else(|error| panic!("first foundry deferred state invalid: {error}"));
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!(
                    "first foundry deferred matter audit failed: {error}"
                ))
                .total(),
            matter_before,
        );
        let survival_after = assess_survival(registries, &state)
            .unwrap_or_else(|| panic!("first foundry deferred player survival disappeared"));
        reviewln!(
            "FIRST FOUNDRY EXPERIENCE seed=0x{:016X} sample={} scope=ordinary-foundry-bootstrap-decision upstream=post-settlement-mechanization-disclosed-opportunity continuity=separate-episode inherited-workshop=[frame-saw,treadle-hammer,copper-reinforced-hand-crank,copper-reinforced-crusher,copper-reinforced-separator,copper-banded-flywheel] inherited-condition={}..{}ppm resource-opportunity=[stone:{}mg wood:{}mg native:{}mg owned-ore:{}mg@{}ppm] immediate-choice=[order:{}mg attention:{}t reinforcement:{}mg tool:{} reason=cheapest-live-route] bootstrap-choice=[remaining-native-before:{}mg foundry-capital:{}mg cast-ingots:{}mg disclosed-followup:{}mg required:{}mg shortfall-before:{}mg remaining-native:{}mg shortfall:{}mg selection:continue-acquisition foundry-deferred:true reason=owned-copper-cannot-fund-foundry-plus-first-settlement-batch] owned-ore-recovery=[route:powered-inherited-line provider:copper-reinforced-hand-crank planned-feed:{}mg available:{}mg sufficient:{} executed:{} feed:{}mg recovered:{}mg attention:{}t autonomous:{}t elapsed:{}t batches:{} stop:{} powered-recovery:{}ppm manual-fallback:{}ppm] foundry-build=false electrical-prime-mover=not-built episode-attention:{}t survival=[energy:{}nJ hydration:{}uL] matter=conserved continuation=acquire-more-copper",
            case.seed(),
            case.role().label(),
            prior_workshop.minimum_start_condition_ppm,
            prior_workshop.maximum_start_condition_ppm,
            disclosed_stone_opportunity.milligrams(),
            component_wood.milligrams(),
            native_opportunity.milligrams(),
            live_owned_ore_mass.milligrams(),
            live_owned_ore_copper_ppm,
            first_cast_mass.milligrams(),
            direct_native_ticks,
            direct_native_reinforcement.milligrams(),
            if current_tool == Some(treadle_hammer) {
                "treadle-hammer"
            } else if current_tool == Some(frame_saw) {
                "frame-saw"
            } else {
                "hand"
            },
            remaining_native_before_recovery.milligrams(),
            capital_copper.milligrams(),
            settlement_ingots.milligrams(),
            settlement_cast_mass.milligrams(),
            required_after_current.milligrams(),
            shortfall_before_recovery.milligrams(),
            remaining_native.milligrams(),
            shortfall_after_recovery.milligrams(),
            recovery_planned_feed.unwrap_or(Mass::ZERO).milligrams(),
            live_owned_ore_mass.milligrams(),
            recovery_available,
            recovery_feed > Mass::ZERO,
            recovery_feed.milligrams(),
            recovered_native.milligrams(),
            recovery_attention,
            recovery_autonomous,
            recovery_elapsed,
            recovery_batches,
            recovery_stop,
            powered_recovery_ppm,
            manual_recovery_ppm,
            direct_native_ticks
                .checked_add(recovery_attention)
                .unwrap_or_else(|| panic!("first foundry deferred attention overflowed")),
            survival_before
                .metabolic_energy()
                .checked_sub(survival_after.metabolic_energy())
                .unwrap_or_else(|| panic!("first foundry deferred metabolic reserve increased"))
                .nanojoules(),
            survival_before
                .hydration()
                .checked_sub(survival_after.hydration())
                .unwrap_or_else(|| panic!("first foundry deferred hydration reserve increased"))
                .microliters(),
        );
        return;
    }

    let component_raw_before = route_plan
        .fabrication_raw
        .iter()
        .map(|(&commodity, &required)| {
            let available = state
                .inventory()
                .get_stockpile(raw)
                .map(|stockpile| stockpile.get_mass(commodity))
                .unwrap_or_else(|| panic!("first foundry raw stockpile disappeared"));
            (commodity, required, available)
        })
        .collect::<Vec<_>>();
    let mut unassisted_fabrication_state = state.clone();
    let unassisted_fabrication = craft_foundry_components(
        registries,
        &mut unassisted_fabrication_state,
        raw,
        parts,
        &[],
    );
    let fabrication = craft_foundry_components(registries, &mut state, raw, parts, &workshop_tools);
    for (commodity, expected, before) in component_raw_before {
        let after = state
            .inventory()
            .get_stockpile(raw)
            .map(|stockpile| stockpile.get_mass(commodity))
            .unwrap_or_else(|| panic!("first foundry raw stockpile disappeared after fabrication"));
        let consumed = before
            .checked_sub(after)
            .unwrap_or_else(|| panic!("first foundry component fabrication created raw material"));
        assert_eq!(
            consumed,
            expected,
            "first foundry projected raw component cost diverged from live production for commodity {}",
            commodity.value()
        );
    }
    assert!(
        fabrication.total_ticks < unassisted_fabrication.total_ticks,
        "inherited settlement workshop must reduce the first-foundry component workload"
    );
    let workshop_attention_saved = unassisted_fabrication
        .total_ticks
        .checked_sub(fabrication.total_ticks)
        .unwrap_or_else(|| unreachable!("assisted fabrication was established as faster"));
    let assemble_equipment = |state: &mut AppState, definition| {
        validate_assemble_equipment(registries, state, definition, parts)
            .unwrap_or_else(|error| panic!("first foundry equipment assembly failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("first foundry equipment commit failed: {error}"))
    };
    let treadle_drive = assemble_equipment(&mut state, EQUIPMENT_TIMBER_TREADLE_DRIVE);
    let dynamo = validate_upgrade_equipment(
        registries,
        &state,
        treadle_drive,
        EQUIPMENT_TIMBER_TREADLE_DYNAMO,
        parts,
    )
    .unwrap_or_else(|error| panic!("first foundry treadle-dynamo upgrade failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("first foundry treadle-dynamo upgrade commit failed: {error}"));
    let furnace = assemble_equipment(&mut state, EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE);
    let mold = assemble_equipment(&mut state, EQUIPMENT_STONE_INGOT_MOLD);
    let electrical = validate_assemble_energy_store(
        registries,
        &state,
        ENERGY_COPPER_PLATE_ELECTRICAL_BUFFER,
        parts,
    )
    .unwrap_or_else(|error| panic!("first foundry electrical-buffer assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("first foundry electrical-buffer commit failed: {error}"));
    let heat_sink =
        validate_assemble_energy_store(registries, &state, ENERGY_STONE_THERMAL_SINK, raw)
            .unwrap_or_else(|error| panic!("first foundry thermal-sink assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("first foundry thermal-sink commit failed: {error}"));

    let composition = MaterialComposition::pure(MATERIAL_COPPER);
    let sensible = calculate_sensible_heat(
        registries.materials(),
        first_cast_mass,
        &composition,
        ROOM_TEMPERATURE,
        melting_point,
    )
    .unwrap_or_else(|error| panic!("first foundry sensible heat failed: {error}"))
    .energy();
    let fusion = calculate_fusion_heat(registries.materials(), first_cast_mass, MATERIAL_COPPER)
        .unwrap_or_else(|error| panic!("first foundry fusion heat failed: {error}"))
        .energy();
    let required_electrical = sensible
        .checked_add(fusion)
        .unwrap_or_else(|| panic!("first foundry melt energy overflowed"));

    assert!(
        settlement_ingots
            .milligrams()
            .is_multiple_of(first_cast_mass.milligrams()),
        "settlement mold ingot requirement must be an exact number of first-foundry casts"
    );
    let cast_batches = settlement_ingots.milligrams() / first_cast_mass.milligrams();
    let mut charge_ticks = 0_u64;
    let mut melt_ticks = 0_u64;
    let mut cast_ticks = 0_u64;
    let mut cooldown_ticks = 0_u64;
    let mut released_heat = Energy::ZERO;

    for batch_index in 0..cast_batches {
        let charge = validate_start_manual_power(
            registries,
            &state,
            ManualPowerRequest::new(
                MANUAL_POWER_TREADLE_DYNAMO,
                dynamo,
                electrical,
                required_electrical,
            ),
        )
        .unwrap_or_else(|error| {
            panic!(
                "first foundry bootstrap charge {} failed: {error}",
                batch_index + 1
            )
        });
        let charge_work = charge.work();
        charge.commit(&mut state).unwrap_or_else(|error| {
            panic!(
                "first foundry bootstrap charge {} commit failed: {error}",
                batch_index + 1
            )
        });
        charge_ticks = charge_ticks
            .checked_add(finish_manual_power_work(
                registries,
                &mut state,
                charge_work,
                "first foundry bootstrap electrical charging",
            ))
            .unwrap_or_else(|| panic!("first foundry bootstrap charge time overflowed"));

        let native_selection = select_commodity_mass(
            &state,
            raw,
            CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
            first_cast_mass,
            "first foundry bootstrap native-copper feed",
        );
        let melting = resolve_melting_process(
            registries,
            &state,
            MeltingRequest::new(
                PROCESS_MELT_PURE_COPPER,
                raw,
                native_selection.as_slice(),
                furnace,
                electrical,
            ),
        )
        .unwrap_or_else(|error| {
            panic!(
                "first foundry bootstrap melt {} resolution failed: {error}",
                batch_index + 1
            )
        });
        assert_eq!(
            melting.required_energy(),
            required_electrical,
            "homogeneous first-foundry native batches must have the same exact melt energy"
        );
        melt_ticks = melt_ticks
            .checked_add(melting.process_resolution().duration().value())
            .unwrap_or_else(|| panic!("first foundry bootstrap melt time overflowed"));
        let melt_job = validate_start_player_process(
            registries,
            &state,
            melting.process_resolution(),
            raw,
            molten,
        )
        .unwrap_or_else(|error| {
            panic!(
                "first foundry bootstrap melt {} start failed: {error}",
                batch_index + 1
            )
        })
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!(
                "first foundry bootstrap melt {} commit failed: {error}",
                batch_index + 1
            )
        });
        finish_uninterrupted_production_job(
            registries,
            &mut state,
            melt_job,
            "first foundry bootstrap melt",
        );

        let (casting, waited) = resolve_full_cast_after_cooldown(
            registries,
            &mut state,
            molten,
            mold,
            heat_sink,
            first_cast_mass,
        );
        cooldown_ticks = cooldown_ticks
            .checked_add(waited)
            .unwrap_or_else(|| panic!("first foundry bootstrap cooldown time overflowed"));
        cast_ticks = cast_ticks
            .checked_add(casting.process_resolution().duration().value())
            .unwrap_or_else(|| panic!("first foundry bootstrap cast time overflowed"));
        released_heat = released_heat
            .checked_add(casting.released_energy())
            .unwrap_or_else(|| panic!("first foundry bootstrap released heat overflowed"));
        let cast_job = validate_start_player_process(
            registries,
            &state,
            casting.process_resolution(),
            molten,
            upgrade_source,
        )
        .unwrap_or_else(|error| {
            panic!(
                "first foundry bootstrap cast {} start failed: {error}",
                batch_index + 1
            )
        })
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!(
                "first foundry bootstrap cast {} commit failed: {error}",
                batch_index + 1
            )
        });
        finish_uninterrupted_production_job(
            registries,
            &mut state,
            cast_job,
            "first foundry bootstrap cast",
        );
    }

    let cast_ingots = state
        .inventory()
        .get_stockpile(upgrade_source)
        .unwrap_or_else(|| panic!("settlement mold upgrade stockpile disappeared"))
        .get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_INGOT));
    assert_eq!(
        cast_ingots, settlement_ingots,
        "first foundry must cast the exact copper stock required by the settlement mold upgrade"
    );
    let upgraded_mold = validate_upgrade_equipment(
        registries,
        &state,
        mold,
        EQUIPMENT_FOUR_CAVITY_STONE_INGOT_MOLD,
        upgrade_source,
    )
    .unwrap_or_else(|error| panic!("settlement casting mold upgrade failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("settlement casting mold upgrade commit failed: {error}"));
    assert_eq!(
        upgraded_mold, mold,
        "settlement casting upgrade must preserve the existing mold identity"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(upgrade_source)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO),
        "settlement mold upgrade must consume the disclosed stone and cast-ingot stock"
    );

    // A nominal capability increase is not enough evidence. If this lived world still owns enough
    // copper, accumulate four primitive melts and prove that the upgraded mold plus the unchanged
    // first-foundry heat sink can execute one real settlement-size cast. Lower-supply worlds stop
    // here honestly and expose the local-resource frontier instead of receiving fixture matter.
    let remaining_before_settlement_cast = state
        .inventory()
        .get_stockpile(raw)
        .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL)))
        .unwrap_or_else(|| panic!("first foundry raw stockpile disappeared after mold upgrade"));
    let settlement_supply_shortfall = settlement_cast_mass
        .checked_sub(remaining_before_settlement_cast)
        .unwrap_or(Mass::ZERO);
    let mut settlement_charge_ticks = 0_u64;
    let mut settlement_melt_ticks = 0_u64;
    let mut settlement_cast_ticks = 0_u64;
    let mut settlement_cooldown_ticks = 0_u64;
    let mut settlement_released_heat = Energy::ZERO;
    let settlement_batch_executed = remaining_before_settlement_cast >= settlement_cast_mass;
    if settlement_batch_executed {
        let melt_batches = settlement_cast_mass.milligrams() / first_cast_mass.milligrams();
        for batch_index in 0..melt_batches {
            let charge = validate_start_manual_power(
                registries,
                &state,
                ManualPowerRequest::new(
                    MANUAL_POWER_TREADLE_DYNAMO,
                    dynamo,
                    electrical,
                    required_electrical,
                ),
            )
            .unwrap_or_else(|error| {
                panic!(
                    "first foundry settlement-batch charge {} failed: {error}",
                    batch_index + 1
                )
            });
            let charge_work = charge.work();
            charge.commit(&mut state).unwrap_or_else(|error| {
                panic!(
                    "first foundry settlement-batch charge {} commit failed: {error}",
                    batch_index + 1
                )
            });
            settlement_charge_ticks = settlement_charge_ticks
                .checked_add(finish_manual_power_work(
                    registries,
                    &mut state,
                    charge_work,
                    "first foundry settlement-batch electrical charging",
                ))
                .unwrap_or_else(|| panic!("first foundry settlement charge time overflowed"));

            let native_selection = select_commodity_mass(
                &state,
                raw,
                CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
                first_cast_mass,
                "first foundry settlement-batch native-copper feed",
            );
            let melting = resolve_melting_process(
                registries,
                &state,
                MeltingRequest::new(
                    PROCESS_MELT_PURE_COPPER,
                    raw,
                    native_selection.as_slice(),
                    furnace,
                    electrical,
                ),
            )
            .unwrap_or_else(|error| {
                panic!(
                    "first foundry settlement-batch melt {} resolution failed: {error}",
                    batch_index + 1
                )
            });
            settlement_melt_ticks = settlement_melt_ticks
                .checked_add(melting.process_resolution().duration().value())
                .unwrap_or_else(|| panic!("first foundry settlement melt time overflowed"));
            let job = validate_start_player_process(
                registries,
                &state,
                melting.process_resolution(),
                raw,
                molten,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "first foundry settlement-batch melt {} start failed: {error}",
                    batch_index + 1
                )
            })
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!(
                    "first foundry settlement-batch melt {} commit failed: {error}",
                    batch_index + 1
                )
            });
            finish_uninterrupted_production_job(
                registries,
                &mut state,
                job,
                "first foundry settlement-batch melt",
            );
        }
        assert_eq!(
            state
                .inventory()
                .get_stockpile(molten)
                .map(|stockpile| stockpile.stored_mass()),
            Some(settlement_cast_mass),
            "four primitive melts must accumulate the exact upgraded-mold batch"
        );
        let (casting, waited) = resolve_full_cast_after_cooldown(
            registries,
            &mut state,
            molten,
            upgraded_mold,
            heat_sink,
            settlement_cast_mass,
        );
        settlement_cooldown_ticks = waited;
        settlement_cast_ticks = casting.process_resolution().duration().value();
        settlement_released_heat = casting.released_energy();
        let job = validate_start_player_process(
            registries,
            &state,
            casting.process_resolution(),
            molten,
            settlement_output,
        )
        .unwrap_or_else(|error| panic!("first foundry settlement-size cast start failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("first foundry settlement-size cast commit failed: {error}")
        });
        finish_uninterrupted_production_job(
            registries,
            &mut state,
            job,
            "first foundry settlement-size cast",
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(settlement_output)
                .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_INGOT))),
            Some(settlement_cast_mass),
            "mold reinvestment must produce one real settlement-size copper ingot batch"
        );
    } else {
        assert_eq!(
            state
                .inventory()
                .get_stockpile(settlement_output)
                .map(|stockpile| stockpile.stored_mass()),
            Some(Mass::ZERO),
            "supply-limited follow-up must not invent settlement cast output"
        );
    }

    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("first foundry final state invalid: {error}"));
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("first foundry matter audit failed: {error}"))
            .total(),
        matter_before,
    );
    let survival_after = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("first foundry player survival disappeared after execution"));
    let elapsed = state.tick().value() - started_at;
    let active_foundry_ticks = fabrication
        .total_ticks
        .checked_add(charge_ticks)
        .and_then(|ticks| ticks.checked_add(settlement_charge_ticks))
        .unwrap_or_else(|| panic!("first foundry active attention overflowed"));
    let total_player_attention = direct_native_ticks
        .checked_add(recovery_attention)
        .and_then(|ticks| ticks.checked_add(active_foundry_ticks))
        .unwrap_or_else(|| panic!("first foundry total player attention overflowed"));
    let bootstrap_autonomous_ticks = melt_ticks
        .checked_add(cast_ticks)
        .and_then(|ticks| ticks.checked_add(cooldown_ticks))
        .unwrap_or_else(|| panic!("first foundry bootstrap autonomous window overflowed"));
    let settlement_autonomous_ticks = settlement_melt_ticks
        .checked_add(settlement_cast_ticks)
        .and_then(|ticks| ticks.checked_add(settlement_cooldown_ticks))
        .unwrap_or_else(|| panic!("first foundry settlement autonomous window overflowed"));
    let autonomous_ticks = recovery_autonomous
        .checked_add(bootstrap_autonomous_ticks)
        .and_then(|ticks| ticks.checked_add(settlement_autonomous_ticks))
        .unwrap_or_else(|| panic!("first foundry autonomous window overflowed"));
    let remaining_native_after = state
        .inventory()
        .get_stockpile(raw)
        .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL)))
        .unwrap_or_else(|| panic!("first foundry raw stockpile disappeared after bootstrap"));

    reviewln!(
        "FIRST FOUNDRY EXPERIENCE seed=0x{:016X} sample={} scope=ordinary-foundry-bootstrap-decision upstream=post-settlement-mechanization-disclosed-opportunity continuity=separate-episode inherited-workshop=[frame-saw,treadle-hammer,copper-reinforced-hand-crank,copper-reinforced-crusher,copper-reinforced-separator,copper-banded-flywheel] inherited-condition={}..{}ppm resource-opportunity=[stone:{}mg wood:{}mg native:{}mg owned-ore:{}mg@{}ppm] immediate-choice=[order:{}mg attention:{}t reinforcement:{}mg tool:{} reason=cheapest-live-route] bootstrap-choice=[remaining-native-before:{}mg foundry-capital:{}mg cast-ingots:{}mg disclosed-followup:{}mg required:{}mg shortfall-before:{}mg remaining-native:{}mg shortfall:0mg selection=foundry reason=disclosed-followup-work-justifies-bootstrap] owned-ore-recovery=[route:powered-inherited-line provider:copper-reinforced-hand-crank planned-feed:{}mg available:{}mg sufficient:{} executed:{} feed:{}mg recovered:{}mg attention:{}t autonomous:{}t elapsed:{}t batches:{} stop:{} powered-recovery:{}ppm manual-fallback:{}ppm] foundry-build=true electrical-prime-mover=[base:treadle-built conversion:treadle-dynamo] fabrication=[total:{}t/{} material=[stone:{}t wood:{}t copper:{}t] route=[hand:{}t frame-saw:{}t treadle-hammer:{}t]] workshop-reuse=[hand-only:{}t saved:{}t] campaign=[batches:{} charge:{}t melt:{}t cast:{}t cooldown:{}t autonomous:{}t released-heat:{}nJ] mold-upgrade=[{}mg->{}mg] settlement-cast=[executed:{} batch:{}mg supply-shortfall:{}mg charge:{}t melt:{}t cast:{}t cooldown:{}t autonomous:{}t released-heat:{}nJ] total-autonomous:{}t copper-after-episode:{}mg total-player-attention:{}t total-elapsed:{}t/{} survival=[energy:{}nJ hydration:{}uL] matter=conserved continuation={}",
        case.seed(),
        case.role().label(),
        prior_workshop.minimum_start_condition_ppm,
        prior_workshop.maximum_start_condition_ppm,
        disclosed_stone_opportunity.milligrams(),
        component_wood.milligrams(),
        native_opportunity.milligrams(),
        live_owned_ore_mass.milligrams(),
        live_owned_ore_copper_ppm,
        first_cast_mass.milligrams(),
        direct_native_ticks,
        direct_native_reinforcement.milligrams(),
        if current_tool == Some(treadle_hammer) {
            "treadle-hammer"
        } else if current_tool == Some(frame_saw) {
            "frame-saw"
        } else {
            "hand"
        },
        remaining_native_before_recovery.milligrams(),
        capital_copper.milligrams(),
        settlement_ingots.milligrams(),
        settlement_cast_mass.milligrams(),
        required_after_current.milligrams(),
        shortfall_before_recovery.milligrams(),
        remaining_native.milligrams(),
        recovery_planned_feed.unwrap_or(Mass::ZERO).milligrams(),
        live_owned_ore_mass.milligrams(),
        recovery_available,
        recovery_feed > Mass::ZERO,
        recovery_feed.milligrams(),
        recovered_native.milligrams(),
        recovery_attention,
        recovery_autonomous,
        recovery_elapsed,
        recovery_batches,
        recovery_stop,
        powered_recovery_ppm,
        manual_recovery_ppm,
        fabrication.total_ticks,
        format_physical_duration(registries, fabrication.total_ticks),
        fabrication.stone_ticks,
        fabrication.wood_ticks,
        fabrication.copper_ticks,
        fabrication.hand_ticks,
        fabrication.frame_saw_ticks,
        fabrication.treadle_hammer_ticks,
        unassisted_fabrication.total_ticks,
        workshop_attention_saved,
        cast_batches,
        charge_ticks,
        melt_ticks,
        cast_ticks,
        cooldown_ticks,
        bootstrap_autonomous_ticks,
        released_heat.nanojoules(),
        first_cast_mass.milligrams(),
        settlement_cast_mass.milligrams(),
        settlement_batch_executed,
        settlement_cast_mass.milligrams(),
        settlement_supply_shortfall.milligrams(),
        settlement_charge_ticks,
        settlement_melt_ticks,
        settlement_cast_ticks,
        settlement_cooldown_ticks,
        settlement_autonomous_ticks,
        settlement_released_heat.nanojoules(),
        autonomous_ticks,
        remaining_native_after.milligrams(),
        total_player_attention,
        elapsed,
        format_physical_duration(registries, elapsed),
        survival_before
            .metabolic_energy()
            .checked_sub(survival_after.metabolic_energy())
            .unwrap_or_else(|| panic!("first foundry metabolic reserve increased"))
            .nanojoules(),
        survival_before
            .hydration()
            .checked_sub(survival_after.hydration())
            .unwrap_or_else(|| panic!("first foundry hydration reserve increased"))
            .microliters(),
        if settlement_batch_executed {
            "settlement-batch-proven"
        } else {
            "settlement-batch-supply"
        },
    );
}
