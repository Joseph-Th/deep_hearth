//! Ordinary first-foundry continuation from the already-proven native-copper stage.

use std::collections::BTreeMap;

use deep_hearth::content::gameplay_fixture::{seed_lot, seed_stockpile};
use deep_hearth::content::{
    ENERGY_COPPER_PLATE_ELECTRICAL_BUFFER, ENERGY_STONE_THERMAL_SINK,
    EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE, EQUIPMENT_STONE_INGOT_MOLD,
    EQUIPMENT_TIMBER_TREADLE_DRIVE, EQUIPMENT_TIMBER_TREADLE_DYNAMO, FORM_CHIP, FORM_INGOT,
    FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL, FORM_REINFORCEMENT, FORM_SCRAP,
    MANUAL_POWER_TREADLE_DYNAMO, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
    PROCESS_CAST_PURE_COPPER, PROCESS_COLD_WORK_COPPER_INGOT_REINFORCEMENT,
    PROCESS_COLD_WORK_COPPER_REINFORCEMENT, PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT,
    PROCESS_MELT_PURE_COPPER,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::energy::validate_assemble_energy_store;
use deep_hearth::equipment::{validate_assemble_equipment, validate_upgrade_equipment};
use deep_hearth::inventory::{MaterialLotSelection, StockpileId, StockpileStorageProfile};
use deep_hearth::labor::{ManualPowerRequest, validate_start_manual_power};
use deep_hearth::material::{CommodityKey, MaterialComposition};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::production::validate_start_process;
use deep_hearth::registry::Registries;
use deep_hearth::survival::{assess_survival, initialize_player_survival};
use deep_hearth::thermal::{
    CastingRequest, MeltingRequest, calculate_fusion_heat, calculate_sensible_heat,
    resolve_casting_process, resolve_melting_process,
};

use super::environment::ROOM_TEMPERATURE;
use super::focused_runner::focused_probe_role_label;
use super::focused_seeds::{FocusedProbeCase, FocusedProbeRole};
use super::inventory_support::add_solid_stockpile;
use super::manual_craft_execution::execute_manual_craft_batches;
use super::manual_craft_planning::manual_craft_plan_for_available_output;
use super::manual_power_timing::finish_manual_power_work;
use super::material_selection::select_stockpile_mass;
use super::physical_time::format_physical_duration;
use super::production_timing::finish_uninterrupted_production_job;

const FIRST_CAST_MASS: Mass = Mass::from_milligrams(20_000);

fn select_commodity_mass(
    state: &AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
    context: &'static str,
) -> Vec<MaterialLotSelection> {
    let mut remaining = mass;
    let mut selections = Vec::new();
    for lot in state.inventory().lot_ids(stockpile) {
        if remaining.is_zero() {
            break;
        }
        let record = state
            .inventory()
            .get_lot(lot)
            .unwrap_or_else(|| panic!("first foundry {context} lot disappeared"));
        if record.commodity() != commodity {
            continue;
        }
        let selected =
            Mass::from_milligrams(record.mass().milligrams().min(remaining.milligrams()));
        if selected.is_zero() {
            continue;
        }
        selections.push(MaterialLotSelection::new(lot, selected));
        remaining = remaining
            .checked_sub(selected)
            .unwrap_or_else(|| unreachable!("selected commodity mass is bounded by demand"));
    }
    assert!(
        remaining.is_zero(),
        "first foundry {context} is missing {}mg of commodity {}",
        remaining.milligrams(),
        commodity.value(),
    );
    selections
}

fn craft_foundry_components(
    registries: &Registries,
    state: &mut AppState,
    raw: deep_hearth::inventory::StockpileId,
    parts: deep_hearth::inventory::StockpileId,
) -> u64 {
    let mut requirements = BTreeMap::<CommodityKey, Mass>::new();
    for equipment in [
        EQUIPMENT_TIMBER_TREADLE_DYNAMO,
        EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE,
        EQUIPMENT_STONE_INGOT_MOLD,
    ] {
        let profile = registries
            .equipment()
            .get_equipment(equipment)
            .and_then(|definition| definition.assembly_profile())
            .unwrap_or_else(|| {
                panic!("first foundry equipment lost its ordinary assembly profile")
            });
        for input in profile.inputs() {
            let entry = requirements.entry(input.commodity()).or_insert(Mass::ZERO);
            *entry = entry
                .checked_add(input.mass())
                .unwrap_or_else(|| panic!("first foundry component demand overflowed"));
        }
    }
    let electrical = registries
        .energy()
        .get_store(ENERGY_COPPER_PLATE_ELECTRICAL_BUFFER)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("first foundry electrical buffer lost assembly"));
    for input in electrical.inputs() {
        let entry = requirements.entry(input.commodity()).or_insert(Mass::ZERO);
        *entry = entry
            .checked_add(input.mass())
            .unwrap_or_else(|| panic!("first foundry electrical component demand overflowed"));
    }

    let mut ticks = 0_u64;
    for (commodity, required) in requirements {
        let available = state
            .inventory()
            .get_stockpile(parts)
            .map(|stockpile| stockpile.get_mass(commodity))
            .unwrap_or_else(|| panic!("first foundry parts stockpile disappeared"));
        if available >= required {
            continue;
        }
        let missing = required
            .checked_sub(available)
            .unwrap_or_else(|| unreachable!("component shortfall was established"));
        let (definition, batches, source) = manual_craft_plan_for_available_output(
            registries,
            state,
            &[raw],
            commodity,
            missing,
            "first foundry component shaping",
        );
        ticks = ticks
            .checked_add(
                execute_manual_craft_batches(
                    registries,
                    state,
                    definition.process(),
                    source,
                    parts,
                    batches,
                    "first foundry component shaping",
                )
                .value(),
            )
            .unwrap_or_else(|| panic!("first foundry fabrication attention overflowed"));
    }
    ticks
}

pub(super) fn run_first_foundry_probe(registries: &Registries, case: FocusedProbeCase) {
    if !matches!(
        case.role(),
        FocusedProbeRole::MaintainedAnchor | FocusedProbeRole::ExplicitReplay
    ) {
        return;
    }

    let mut state = AppState::new();
    let raw = add_solid_stockpile(&mut state, Mass::from_milligrams(25_000_000));
    let parts = add_solid_stockpile(&mut state, Mass::from_milligrams(25_000_000));
    let cast_storage = add_solid_stockpile(&mut state, FIRST_CAST_MASS);
    let melting_point = registries
        .materials()
        .get_material(MATERIAL_COPPER)
        .and_then(|material| material.properties().thermal().melting_point())
        .unwrap_or_else(|| panic!("first foundry copper melting point disappeared"));
    let molten_profile = StockpileStorageProfile::new(false, true, melting_point)
        .unwrap_or_else(|error| panic!("first foundry molten storage profile failed: {error}"));
    let molten = seed_stockpile(&mut state, FIRST_CAST_MASS, molten_profile);

    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(12_000_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(12_000_000),
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
            Mass::from_milligrams(160_000),
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
            FIRST_CAST_MASS,
        ),
    ] {
        let _ = seed_lot(
            registries,
            &mut state,
            raw,
            commodity,
            mass,
            ROOM_TEMPERATURE,
        );
    }
    super::world_admission::locate_stationary_endpoints(
        &mut state,
        &[raw, parts, cast_storage, molten],
        &[],
    );
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("first foundry survival setup failed: {error}"));
    super::world_admission::initialize_stationary_player_logistics(&mut state);
    let survival_before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("first foundry player survival disappeared"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("first foundry matter setup failed: {error}"))
        .total();
    let started_at = state.tick().value();

    // Freeze the actual current-order decision before constructing foundry infrastructure. The
    // disclosed native copper is already a legal 20 g reinforcement source, so a player solving
    // only this order should use it rather than paying the foundry setup cost. Foundry execution
    // below remains ordinary acquisition/execution coverage and quantifies its distinct recovery
    // value once installed.
    let mut direct_native_state = state.clone();
    let direct_native_ticks = execute_manual_craft_batches(
        registries,
        &mut direct_native_state,
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
        raw,
        parts,
        1,
        "first foundry pre-investment native-copper alternative",
    )
    .value();
    let direct_native_reinforcement = direct_native_state
        .inventory()
        .get_stockpile(parts)
        .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT)))
        .unwrap_or(Mass::ZERO);
    assert_eq!(direct_native_reinforcement, FIRST_CAST_MASS);
    validate_loaded_state(registries, &direct_native_state)
        .unwrap_or_else(|error| panic!("first foundry direct-native branch invalid: {error}"));

    let fabrication_ticks = craft_foundry_components(registries, &mut state, raw, parts);
    let assemble_equipment = |state: &mut AppState, definition| {
        validate_assemble_equipment(registries, state, definition, parts)
            .unwrap_or_else(|error| panic!("first foundry equipment assembly failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("first foundry equipment commit failed: {error}"))
    };
    let treadle = assemble_equipment(&mut state, EQUIPMENT_TIMBER_TREADLE_DRIVE);
    let dynamo = validate_upgrade_equipment(
        registries,
        &state,
        treadle,
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

    // Compare the two legitimate scrap-recovery choices from the same post-setup observable state.
    // Cold rework is faster but leaves fine copper chips that cannot be cold-consolidated again;
    // remelting spends more active attention to recover the whole batch into standardized stock.
    let mut direct_rework_state = state.clone();
    let direct_rework_ticks = execute_manual_craft_batches(
        registries,
        &mut direct_rework_state,
        PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT,
        raw,
        parts,
        1,
        "first foundry direct scrap-rework counterfactual",
    )
    .value();
    let direct_reinforcement = direct_rework_state
        .inventory()
        .get_stockpile(parts)
        .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT)))
        .unwrap_or(Mass::ZERO);
    let direct_residual = direct_rework_state
        .inventory()
        .get_stockpile(parts)
        .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_CHIP)))
        .unwrap_or(Mass::ZERO);
    assert_eq!(direct_reinforcement, Mass::from_milligrams(18_000));
    assert_eq!(direct_residual, Mass::from_milligrams(2_000));
    assert!(
        direct_reinforcement < FIRST_CAST_MASS,
        "direct scrap rework must remain short of the declared 20 g reinforcement order"
    );
    let direct_fulfillment_ppm = direct_reinforcement
        .milligrams()
        .checked_mul(1_000_000)
        .unwrap_or_else(|| panic!("first foundry direct fulfillment overflowed"))
        / FIRST_CAST_MASS.milligrams();

    let composition = MaterialComposition::pure(MATERIAL_COPPER);
    let sensible = calculate_sensible_heat(
        registries.materials(),
        FIRST_CAST_MASS,
        &composition,
        ROOM_TEMPERATURE,
        melting_point,
    )
    .unwrap_or_else(|error| panic!("first foundry sensible heat failed: {error}"))
    .energy();
    let fusion = calculate_fusion_heat(registries.materials(), FIRST_CAST_MASS, MATERIAL_COPPER)
        .unwrap_or_else(|error| panic!("first foundry fusion heat failed: {error}"))
        .energy();
    let required_electrical = sensible
        .checked_add(fusion)
        .unwrap_or_else(|| panic!("first foundry melt energy overflowed"));
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
    .unwrap_or_else(|error| panic!("first foundry electrical charge failed: {error}"));
    let charge_budget = charge.resource_budget();
    let charge_work = charge.work();
    charge
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("first foundry electrical charge commit failed: {error}"));
    let charge_ticks = finish_manual_power_work(
        registries,
        &mut state,
        charge_work,
        "first foundry electrical charging",
    );

    let scrap_selection = select_commodity_mass(
        &state,
        raw,
        CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
        FIRST_CAST_MASS,
        "recoverable copper scrap feed",
    );
    let melting = resolve_melting_process(
        registries,
        &state,
        MeltingRequest::new(
            PROCESS_MELT_PURE_COPPER,
            raw,
            scrap_selection.as_slice(),
            furnace,
            electrical,
        ),
    )
    .unwrap_or_else(|error| panic!("first foundry melt resolution failed: {error}"));
    assert_eq!(melting.required_energy(), required_electrical);
    let melt_ticks = melting.process_resolution().duration().value();
    let melt_job = validate_start_process(
        registries,
        &state,
        melting.process_resolution(),
        raw,
        molten,
    )
    .unwrap_or_else(|error| panic!("first foundry melt start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("first foundry melt commit failed: {error}"));
    finish_uninterrupted_production_job(registries, &mut state, melt_job, "first foundry melt");

    let molten_selection = select_stockpile_mass(
        &state,
        molten,
        FIRST_CAST_MASS,
        "first foundry molten copper feed",
    );
    let casting = resolve_casting_process(
        registries,
        &state,
        CastingRequest::new(
            PROCESS_CAST_PURE_COPPER,
            molten,
            molten_selection.as_slice(),
            mold,
            heat_sink,
        ),
    )
    .unwrap_or_else(|error| panic!("first foundry casting resolution failed: {error}"));
    let cast_ticks = casting.process_resolution().duration().value();
    let released_heat = casting.released_energy();
    let cast_job = validate_start_process(
        registries,
        &state,
        casting.process_resolution(),
        molten,
        cast_storage,
    )
    .unwrap_or_else(|error| panic!("first foundry cast start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("first foundry cast commit failed: {error}"));
    finish_uninterrupted_production_job(registries, &mut state, cast_job, "first foundry cast");

    let downstream_ticks = execute_manual_craft_batches(
        registries,
        &mut state,
        PROCESS_COLD_WORK_COPPER_INGOT_REINFORCEMENT,
        cast_storage,
        parts,
        1,
        "first foundry cast-output reinforcement",
    )
    .value();
    let foundry_reinforcement = state
        .inventory()
        .get_stockpile(parts)
        .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT)))
        .unwrap_or(Mass::ZERO);
    assert_eq!(
        foundry_reinforcement, FIRST_CAST_MASS,
        "first foundry recovery must complete the declared 20 g reinforcement order"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(cast_storage)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_INGOT))),
        Some(Mass::ZERO),
    );
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
    let foundry_recovery_attention = charge_ticks
        .checked_add(downstream_ticks)
        .unwrap_or_else(|| panic!("first foundry recovery attention overflowed"));
    let recovery_attention_delta =
        i128::from(foundry_recovery_attention) - i128::from(direct_rework_ticks);
    reviewln!(
        "FIRST FOUNDRY EXPERIENCE seed=0x{:016X} sample={} scope=ordinary-copper-recovery-coverage upstream=primitive-liberation-capability-proved state-continuity=separate-disclosed-opportunity raw-opportunity=[stone:12000000mg wood:12000000mg native:160000mg scrap:20000mg] build-choice=[order:20000mg direct-native:{}t reinforcement:{}mg fulfillment:1000000ppm selection:direct-native foundry-deferred:true reason=current-order-does-not-repay-setup] scarcity-choice=[order:20000mg source:scrap-only cold-rework:{}mg/{}ppm shortfall:{}mg foundry:20000mg/1000000ppm selection:foundry reason:cold-rework-underfills-order] fabrication={}t/{} dynamo-path=treadle-additive-upgrade electrical-charge=[{}t {}nJ body:{}nJ/{}uL] melt=[{}t {} feed:scrap] cast=[{}t {} heat:{}nJ] downstream=[ingot:20000mg reinforcement:20000mg cold-work:{}t] installed-recovery=[cold-rework:{}t reinforcement:{}mg chips:{}mg fulfillment:{}ppm foundry-active:{}t reinforcement:20000mg chips:0mg fulfillment:1000000ppm useful-gain:+{}mg attention-delta:{:+}t] total={}t/{} survival=[energy:{}nJ hydration:{}uL] matter=conserved continuation=full-scrap-recovery",
        case.seed(),
        focused_probe_role_label(case.role()),
        direct_native_ticks,
        direct_native_reinforcement.milligrams(),
        direct_reinforcement.milligrams(),
        direct_fulfillment_ppm,
        FIRST_CAST_MASS
            .checked_sub(direct_reinforcement)
            .unwrap_or_else(|| panic!("direct scrap rework exceeded scarcity order"))
            .milligrams(),
        fabrication_ticks,
        format_physical_duration(registries, fabrication_ticks),
        charge_ticks,
        required_electrical.nanojoules(),
        charge_budget.metabolic_energy().nanojoules(),
        charge_budget.hydration().microliters(),
        melt_ticks,
        format_physical_duration(registries, melt_ticks),
        cast_ticks,
        format_physical_duration(registries, cast_ticks),
        released_heat.nanojoules(),
        downstream_ticks,
        direct_rework_ticks,
        direct_reinforcement.milligrams(),
        direct_residual.milligrams(),
        direct_fulfillment_ppm,
        foundry_recovery_attention,
        FIRST_CAST_MASS
            .checked_sub(direct_reinforcement)
            .unwrap_or_else(|| panic!("direct scrap rework exceeded foundry recovery"))
            .milligrams(),
        recovery_attention_delta,
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
    );
}
