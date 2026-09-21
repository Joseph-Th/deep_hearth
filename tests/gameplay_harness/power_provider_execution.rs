//! Canonical matched-arm execution for human-power gameplay evidence.

use deep_hearth::content::{
    ENERGY_TIMBER_FRAME_FLYWHEEL_BANK, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_TIMBER_TREADLE_DRIVE,
    EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE, MANUAL_POWER_FOOT_TREADLE, MANUAL_POWER_HAND_CRANK,
    MANUAL_POWER_WALKING_WHEEL,
};
use deep_hearth::core::quantity::{AggregateMass, Mass};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::energy::{EnergyStoreDefinitionId, EnergyStoreId, validate_assemble_energy_store};
use deep_hearth::equipment::{EquipmentId, validate_assemble_equipment};
use deep_hearth::inventory::StockpileId;
use deep_hearth::labor::{ManualPowerMethodId, ManualPowerRequest, validate_start_manual_power};
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::registry::Registries;
use deep_hearth::survival::assess_survival;

use super::super::manual_craft_execution::execute_manual_craft_batches;
use super::super::manual_craft_planning::manual_craft_plan_for_available_output;
use super::super::manual_power_timing::finish_manual_power_work;
use super::planning::{PrimitivePowerPlan, SettlementPowerPlan, ShapedBuild};

fn stockpile_mass(state: &AppState, raw: StockpileId) -> Mass {
    state
        .inventory()
        .get_stockpile(raw)
        .unwrap_or_else(|| panic!("power build stockpile disappeared"))
        .stored_mass()
}

fn shape_assembly_inputs(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    shaped: StockpileId,
    inputs: Vec<(CommodityKey, Mass)>,
    context: &'static str,
) -> u64 {
    let mut attention_ticks = 0_u64;
    for (commodity, required) in inputs {
        let available = state
            .inventory()
            .get_stockpile(shaped)
            .map(|stockpile| stockpile.get_mass(commodity))
            .unwrap_or_else(|| panic!("power provider {context} shaped stockpile disappeared"));
        if available >= required {
            continue;
        }
        let missing = required
            .checked_sub(available)
            .unwrap_or_else(|| unreachable!("power provider component availability was checked"));
        let (craft, batches, source) = manual_craft_plan_for_available_output(
            registries,
            state,
            &[raw],
            commodity,
            missing,
            context,
        );
        attention_ticks = attention_ticks
            .checked_add(
                execute_manual_craft_batches(
                    registries,
                    state,
                    craft.process(),
                    source,
                    shaped,
                    batches,
                    context,
                )
                .value(),
            )
            .unwrap_or_else(|| panic!("power provider {context} attention overflowed"));
    }
    attention_ticks
}

fn build_provider(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    shaped: StockpileId,
    definition: deep_hearth::equipment::EquipmentDefinitionId,
    context: &'static str,
) -> (EquipmentId, ShapedBuild) {
    let inputs = registries
        .equipment()
        .get_equipment(definition)
        .and_then(|equipment| equipment.assembly_profile())
        .map(|profile| {
            (
                profile.input_mass(),
                profile
                    .inputs()
                    .iter()
                    .map(|input| (input.commodity(), input.mass()))
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_else(|| {
            panic!(
                "power provider equipment {} lost authored assembly",
                definition.value()
            )
        });
    let raw_before = stockpile_mass(state, raw);
    let attention_ticks = shape_assembly_inputs(registries, state, raw, shaped, inputs.1, context);
    let input_mass_mg = raw_before
        .checked_sub(stockpile_mass(state, raw))
        .unwrap_or_else(|| panic!("power build must withdraw raw matter"))
        .milligrams();
    assert!(
        input_mass_mg >= inputs.0.milligrams(),
        "raw bill must cover embodied matter"
    );
    let equipment = validate_assemble_equipment(registries, state, definition, shaped)
        .unwrap_or_else(|error| panic!("power provider equipment assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("power provider equipment commit failed: {error}"));
    (
        equipment,
        ShapedBuild {
            attention_ticks,
            input_mass_mg,
            embodied_mass_mg: inputs.0.milligrams(),
        },
    )
}

fn build_flywheel(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    shaped: StockpileId,
    definition: EnergyStoreDefinitionId,
    context: &'static str,
) -> (EnergyStoreId, ShapedBuild) {
    let inputs = registries
        .energy()
        .get_store(definition)
        .and_then(|store| store.assembly_profile())
        .map(|profile| {
            (
                profile.input_mass(),
                profile
                    .inputs()
                    .iter()
                    .map(|input| (input.commodity(), input.mass()))
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_else(|| panic!("power provider flywheel lost authored assembly"));
    let raw_before = stockpile_mass(state, raw);
    let attention_ticks = shape_assembly_inputs(registries, state, raw, shaped, inputs.1, context);
    let input_mass_mg = raw_before
        .checked_sub(stockpile_mass(state, raw))
        .unwrap_or_else(|| panic!("power build must withdraw raw matter"))
        .milligrams();
    assert!(
        input_mass_mg >= inputs.0.milligrams(),
        "raw bill must cover embodied matter"
    );
    let store = validate_assemble_energy_store(registries, state, definition, shaped)
        .unwrap_or_else(|error| panic!("power provider flywheel assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("power provider flywheel commit failed: {error}"));
    (
        store,
        ShapedBuild {
            attention_ticks,
            input_mass_mg,
            embodied_mass_mg: inputs.0.milligrams(),
        },
    )
}

pub(super) struct ChargeOutcome {
    pub(super) attention_ticks: u64,
    pub(super) metabolic_nj: u128,
    pub(super) hydration_ul: u128,
    pub(super) condition_after_ppm: u32,
}

fn charge_to_full(
    registries: &Registries,
    state: &mut AppState,
    method: ManualPowerMethodId,
    equipment: EquipmentId,
    store: EnergyStoreId,
    capacity_nj: u128,
    context: &'static str,
) -> ChargeOutcome {
    let capacity = deep_hearth::core::quantity::Energy::from_nanojoules(capacity_nj);
    let before = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("power provider {context} lost the player before charging"));
    let charge = validate_start_manual_power(
        registries,
        state,
        ManualPowerRequest::new(method, equipment, store, capacity),
    )
    .unwrap_or_else(|error| panic!("power provider {context} charge failed: {error}"));
    let work = charge.work();
    charge
        .commit(state)
        .unwrap_or_else(|error| panic!("power provider {context} charge commit failed: {error}"));
    let attention_ticks = finish_manual_power_work(registries, state, work, context);
    assert_eq!(
        state
            .energy()
            .get_store(store)
            .map(|record| record.stored().nanojoules()),
        Some(capacity_nj),
        "power provider {context} must deliver the full requested flywheel charge"
    );
    let after = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("power provider {context} lost the player after charging"));
    let condition_after_ppm = state
        .equipment()
        .get_equipment(equipment)
        .map(|record| record.condition().parts_per_million())
        .unwrap_or_else(|| panic!("power provider {context} equipment disappeared"));
    ChargeOutcome {
        attention_ticks,
        metabolic_nj: before
            .metabolic_energy()
            .nanojoules()
            .checked_sub(after.metabolic_energy().nanojoules())
            .unwrap_or_else(|| panic!("power provider {context} metabolic audit underflowed")),
        hydration_ul: u128::from(before.hydration().microliters())
            .checked_sub(u128::from(after.hydration().microliters()))
            .unwrap_or_else(|| panic!("power provider {context} hydration audit underflowed")),
        condition_after_ppm,
    }
}

pub(super) struct PrimitiveComparison {
    pub(super) crank_build: ShapedBuild,
    pub(super) crank_drive_build: ShapedBuild,
    pub(super) crank_charge: ChargeOutcome,
    pub(super) treadle_build: ShapedBuild,
    pub(super) treadle_drive_build: ShapedBuild,
    pub(super) treadle_charge: ChargeOutcome,
    pub(super) crank_residual_mg: u64,
    pub(super) treadle_residual_mg: u64,
}

pub(super) fn execute_primitive_comparison(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    shaped: StockpileId,
    matter_before: AggregateMass,
    plan: PrimitivePowerPlan,
) -> PrimitiveComparison {
    let mut crank_state = state.clone();
    let mut treadle_state = state.clone();
    let (crank, crank_build) = build_provider(
        registries,
        &mut crank_state,
        raw,
        shaped,
        EQUIPMENT_STONE_HAND_CRANK,
        "power provider crank build",
    );
    let (crank_drive, crank_drive_build) = build_flywheel(
        registries,
        &mut crank_state,
        raw,
        shaped,
        plan.store_definition,
        "power provider crank flywheel",
    );
    let crank_charge = charge_to_full(
        registries,
        &mut crank_state,
        MANUAL_POWER_HAND_CRANK,
        crank,
        crank_drive,
        plan.capacity_nj,
        "power provider crank charge",
    );
    let (treadle, treadle_build) = build_provider(
        registries,
        &mut treadle_state,
        raw,
        shaped,
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
        "power provider treadle build",
    );
    let (treadle_drive, treadle_drive_build) = build_flywheel(
        registries,
        &mut treadle_state,
        raw,
        shaped,
        plan.store_definition,
        "power provider treadle flywheel",
    );
    let treadle_charge = charge_to_full(
        registries,
        &mut treadle_state,
        MANUAL_POWER_FOOT_TREADLE,
        treadle,
        treadle_drive,
        plan.capacity_nj,
        "power provider treadle charge",
    );
    assert_eq!(
        plan.crank_build.attention_ticks,
        crank_build
            .attention_ticks
            .checked_add(crank_drive_build.attention_ticks)
            .unwrap_or_else(|| panic!("crank package attention overflowed"))
    );
    assert_eq!(
        plan.crank_build.input_mass_mg,
        crank_build
            .input_mass_mg
            .checked_add(crank_drive_build.input_mass_mg)
            .unwrap_or_else(|| panic!("crank package mass overflowed"))
    );
    assert_eq!(
        plan.treadle_build.attention_ticks,
        treadle_build
            .attention_ticks
            .checked_add(treadle_drive_build.attention_ticks)
            .unwrap_or_else(|| panic!("treadle package attention overflowed"))
    );
    assert_eq!(
        plan.treadle_build.input_mass_mg,
        treadle_build
            .input_mass_mg
            .checked_add(treadle_drive_build.input_mass_mg)
            .unwrap_or_else(|| panic!("treadle package mass overflowed"))
    );
    assert_eq!(
        plan.crank_charge.duration().value(),
        crank_charge.attention_ticks
    );
    assert_eq!(
        plan.treadle_charge.duration().value(),
        treadle_charge.attention_ticks
    );
    assert_eq!(
        plan.crank_charge
            .resource_budget()
            .metabolic_energy()
            .nanojoules(),
        crank_charge.metabolic_nj
    );
    assert_eq!(
        u128::from(
            plan.crank_charge
                .resource_budget()
                .hydration()
                .microliters()
        ),
        crank_charge.hydration_ul
    );
    assert_eq!(
        plan.treadle_charge
            .resource_budget()
            .metabolic_energy()
            .nanojoules(),
        treadle_charge.metabolic_nj
    );
    assert_eq!(
        u128::from(
            plan.treadle_charge
                .resource_budget()
                .hydration()
                .microliters()
        ),
        treadle_charge.hydration_ul
    );
    assert!(treadle_charge.attention_ticks < crank_charge.attention_ticks);
    for (label, arm) in [("crank", &crank_state), ("treadle", &treadle_state)] {
        assert_eq!(
            calculate_matter_accounting(arm)
                .unwrap_or_else(|error| panic!(
                    "power provider {label} matter audit failed: {error}"
                ))
                .total(),
            matter_before,
            "power provider {label} arm must conserve matter"
        );
        validate_loaded_state(registries, arm)
            .unwrap_or_else(|error| panic!("power provider {label} state invalid: {error}"));
    }
    PrimitiveComparison {
        crank_build,
        crank_drive_build,
        crank_charge,
        treadle_build,
        treadle_drive_build,
        treadle_charge,
        crank_residual_mg: stockpile_mass(&crank_state, shaped).milligrams(),
        treadle_residual_mg: stockpile_mass(&treadle_state, shaped).milligrams(),
    }
}

pub(super) struct SettlementComparison {
    pub(super) settlement_treadle_build: ShapedBuild,
    pub(super) settlement_treadle_drive_build: ShapedBuild,
    pub(super) settlement_treadle_charge: ChargeOutcome,
    pub(super) walking_build: ShapedBuild,
    pub(super) walking_drive_build: ShapedBuild,
    pub(super) walking_charge: ChargeOutcome,
}

pub(super) fn execute_settlement_comparison(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    shaped: StockpileId,
    matter_before: AggregateMass,
    plan: SettlementPowerPlan,
) -> SettlementComparison {
    let mut treadle_state = state.clone();
    let mut walking_state = state.clone();
    let (settlement_treadle, settlement_treadle_build) = build_provider(
        registries,
        &mut treadle_state,
        raw,
        shaped,
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
        "settlement treadle build",
    );
    let (settlement_treadle_drive, settlement_treadle_drive_build) = build_flywheel(
        registries,
        &mut treadle_state,
        raw,
        shaped,
        ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
        "settlement treadle flywheel bank",
    );
    let settlement_treadle_charge = charge_to_full(
        registries,
        &mut treadle_state,
        MANUAL_POWER_FOOT_TREADLE,
        settlement_treadle,
        settlement_treadle_drive,
        plan.capacity_nj,
        "settlement treadle charge",
    );
    let (walking, walking_build) = build_provider(
        registries,
        &mut walking_state,
        raw,
        shaped,
        EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE,
        "walking-wheel build",
    );
    let (walking_drive, walking_drive_build) = build_flywheel(
        registries,
        &mut walking_state,
        raw,
        shaped,
        ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
        "walking-wheel flywheel bank",
    );
    let walking_charge = charge_to_full(
        registries,
        &mut walking_state,
        MANUAL_POWER_WALKING_WHEEL,
        walking,
        walking_drive,
        plan.capacity_nj,
        "walking-wheel charge",
    );
    assert_eq!(
        plan.treadle_build.attention_ticks,
        settlement_treadle_build
            .attention_ticks
            .checked_add(settlement_treadle_drive_build.attention_ticks)
            .unwrap_or_else(|| panic!("settlement treadle package attention overflowed"))
    );
    assert_eq!(
        plan.treadle_build.input_mass_mg,
        settlement_treadle_build
            .input_mass_mg
            .checked_add(settlement_treadle_drive_build.input_mass_mg)
            .unwrap_or_else(|| panic!("settlement treadle package mass overflowed"))
    );
    assert_eq!(
        plan.walking_build.attention_ticks,
        walking_build
            .attention_ticks
            .checked_add(walking_drive_build.attention_ticks)
            .unwrap_or_else(|| panic!("walking-wheel package attention overflowed"))
    );
    assert_eq!(
        plan.walking_build.input_mass_mg,
        walking_build
            .input_mass_mg
            .checked_add(walking_drive_build.input_mass_mg)
            .unwrap_or_else(|| panic!("walking-wheel package mass overflowed"))
    );
    assert_eq!(
        plan.treadle_charge.duration().value(),
        settlement_treadle_charge.attention_ticks
    );
    assert_eq!(
        plan.walking_charge.duration().value(),
        walking_charge.attention_ticks
    );
    assert_eq!(
        plan.treadle_charge
            .resource_budget()
            .metabolic_energy()
            .nanojoules(),
        settlement_treadle_charge.metabolic_nj
    );
    assert_eq!(
        plan.walking_charge
            .resource_budget()
            .metabolic_energy()
            .nanojoules(),
        walking_charge.metabolic_nj
    );
    assert!(walking_charge.attention_ticks < settlement_treadle_charge.attention_ticks);
    assert!(walking_charge.metabolic_nj < settlement_treadle_charge.metabolic_nj);
    for (label, arm) in [
        ("settlement treadle", &treadle_state),
        ("walking-wheel", &walking_state),
    ] {
        assert_eq!(
            calculate_matter_accounting(arm)
                .unwrap_or_else(|error| panic!("{label} matter audit failed: {error}"))
                .total(),
            matter_before,
            "{label} arm must conserve matter"
        );
        validate_loaded_state(registries, arm)
            .unwrap_or_else(|error| panic!("{label} state invalid: {error}"));
    }
    SettlementComparison {
        settlement_treadle_build,
        settlement_treadle_drive_build,
        settlement_treadle_charge,
        walking_build,
        walking_drive_build,
        walking_charge,
    }
}
