//! Ordinary first-foundry continuation from the already-proven native-copper stage.

use deep_hearth::content::gameplay_fixture::{seed_lot, seed_stockpile};
use deep_hearth::content::{
    ENERGY_COPPER_PLATE_ELECTRICAL_BUFFER, ENERGY_STONE_THERMAL_SINK,
    EQUIPMENT_FOUR_CAVITY_STONE_INGOT_MOLD, EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE,
    EQUIPMENT_STONE_INGOT_MOLD, EQUIPMENT_TIMBER_TREADLE_DRIVE, EQUIPMENT_TIMBER_TREADLE_DYNAMO,
    FORM_INGOT, FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL, FORM_REINFORCEMENT,
    MANUAL_POWER_TREADLE_DYNAMO, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
    PROCESS_CAST_PURE_COPPER, PROCESS_COLD_WORK_COPPER_REINFORCEMENT, PROCESS_MELT_PURE_COPPER,
};
use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::energy::validate_assemble_energy_store;
use deep_hearth::equipment::{validate_assemble_equipment, validate_upgrade_equipment};
use deep_hearth::inventory::StockpileStorageProfile;
use deep_hearth::labor::{ManualPowerRequest, validate_start_manual_power};
use deep_hearth::material::{CommodityKey, MaterialComposition};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::production::validate_start_process;
use deep_hearth::registry::Registries;
use deep_hearth::survival::{assess_survival, initialize_player_survival};
use deep_hearth::thermal::{
    MeltingRequest, calculate_fusion_heat, calculate_sensible_heat, resolve_melting_process,
};

use super::environment::ROOM_TEMPERATURE;
use super::equipment_support::nominal_equipment_mass_capability;
use super::focused_seeds::FocusedProbeCase;
use super::inventory_support::add_solid_stockpile;
use super::manual_craft_batches::execute_manual_craft_batches;
use super::manual_power_timing::finish_manual_power_work;
use super::physical_time::format_physical_duration;
use super::production_timing::finish_uninterrupted_production_job;
#[path = "first_foundry_probe/casting.rs"]
mod casting;
#[path = "first_foundry_probe/planning.rs"]
mod planning;

use self::casting::resolve_full_cast_after_cooldown;
use self::planning::{
    FOUNDRY_STONE_OPPORTUNITY, FOUNDRY_WOOD_OPPORTUNITY, craft_foundry_components,
    foundry_capital_copper, manual_batches_for_output, native_copper_opportunity,
    select_commodity_mass, settlement_mold_ingot_requirement, settlement_mold_stone_requirement,
};

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

    let native_opportunity = native_copper_opportunity(case);
    let capital_copper = foundry_capital_copper(registries);
    let settlement_ingots = settlement_mold_ingot_requirement(registries);
    let settlement_stone = settlement_mold_stone_requirement(registries);
    let disclosed_stone_opportunity = FOUNDRY_STONE_OPPORTUNITY
        .checked_add(settlement_stone)
        .unwrap_or_else(|| panic!("first foundry disclosed stone opportunity overflowed"));
    let required_after_current = capital_copper
        .checked_add(settlement_ingots)
        .unwrap_or_else(|| panic!("first foundry bootstrap copper requirement overflowed"));

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
    let raw = add_solid_stockpile(&mut state, Mass::from_milligrams(25_000_000));
    let parts = add_solid_stockpile(&mut state, Mass::from_milligrams(25_000_000));
    let current_output = add_solid_stockpile(&mut state, first_cast_mass);
    let upgrade_source =
        add_solid_stockpile(&mut state, settlement_mold_upgrade.additions().input_mass());
    let melting_point = registries
        .materials()
        .get_material(MATERIAL_COPPER)
        .and_then(|material| material.properties().thermal().melting_point())
        .unwrap_or_else(|| panic!("first foundry copper melting point disappeared"));
    let molten_profile = StockpileStorageProfile::new(false, true, melting_point)
        .unwrap_or_else(|error| panic!("first foundry molten storage profile failed: {error}"));
    let molten = seed_stockpile(&mut state, first_cast_mass, molten_profile);

    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            FOUNDRY_STONE_OPPORTUNITY,
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            FOUNDRY_WOOD_OPPORTUNITY,
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
            native_opportunity,
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
    super::world_admission::locate_stationary_endpoints(
        &mut state,
        &[raw, parts, current_output, upgrade_source, molten],
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

    // Fulfill the immediate 20 g reinforcement order through the cheap direct route first. The
    // foundry is considered only for the next capability step, after the player has paid the
    // current-order cost and can see exactly how much copper remains.
    let native_rework = registries
        .crafting()
        .get_manual(PROCESS_COLD_WORK_COPPER_REINFORCEMENT)
        .unwrap_or_else(|| panic!("first foundry native-copper rework definition disappeared"));
    let direct_native_batches = manual_batches_for_output(
        native_rework,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        first_cast_mass,
        "current reinforcement order",
    );
    let direct_native_ticks = execute_manual_craft_batches(
        registries,
        &mut state,
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
        raw,
        current_output,
        direct_native_batches,
        "first foundry current-order native working",
    )
    .value();
    let direct_native_reinforcement = state
        .inventory()
        .get_stockpile(current_output)
        .unwrap_or_else(|| panic!("first foundry current-order output disappeared"))
        .get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT));
    assert!(
        direct_native_reinforcement >= first_cast_mass,
        "direct native-copper route must satisfy the immediate reinforcement order"
    );

    let remaining_native = state
        .inventory()
        .get_stockpile(raw)
        .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL)))
        .unwrap_or_else(|| panic!("first foundry raw stockpile disappeared"));
    if remaining_native < required_after_current {
        let shortfall = required_after_current
            .checked_sub(remaining_native)
            .unwrap_or_else(|| unreachable!("remaining copper is below the bootstrap requirement"));
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
            "FIRST FOUNDRY EXPERIENCE seed=0x{:016X} sample={} scope=ordinary-foundry-bootstrap-decision upstream=post-settlement-mechanization-disclosed-opportunity continuity=separate-episode resource-opportunity=[stone:{}mg wood:{}mg native:{}mg] immediate-choice=[order:{}mg direct-native:{}t reinforcement:{}mg selection:direct-native reason=cheap-current-order] bootstrap-choice=[remaining-native:{}mg foundry-capital:{}mg cast-ingots:{}mg required:{}mg shortfall:{}mg selection:continue-acquisition foundry-deferred:true reason=insufficient-copper-to-complete-bootstrap] foundry-build=false episode-attention:{}t survival=[energy:{}nJ hydration:{}uL] matter=conserved continuation=acquire-more-copper",
            case.seed(),
            case.role().label(),
            disclosed_stone_opportunity.milligrams(),
            FOUNDRY_WOOD_OPPORTUNITY.milligrams(),
            native_opportunity.milligrams(),
            first_cast_mass.milligrams(),
            direct_native_ticks,
            direct_native_reinforcement.milligrams(),
            remaining_native.milligrams(),
            capital_copper.milligrams(),
            settlement_ingots.milligrams(),
            required_after_current.milligrams(),
            shortfall.milligrams(),
            direct_native_ticks,
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
        let melt_job = validate_start_process(
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
        let cast_job = validate_start_process(
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
    let settlement_cast_mass = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_FOUR_CAVITY_STONE_INGOT_MOLD,
        casting_definition.max_batch_mass_capability(),
    );
    assert!(
        settlement_cast_mass > first_cast_mass,
        "cast-ingot reinvestment must increase the player's available casting batch"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(upgrade_source)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO),
        "settlement mold upgrade must consume the disclosed stone and cast-ingot stock"
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
    let active_foundry_ticks = fabrication_ticks
        .checked_add(charge_ticks)
        .unwrap_or_else(|| panic!("first foundry active attention overflowed"));
    let total_player_attention = direct_native_ticks
        .checked_add(active_foundry_ticks)
        .unwrap_or_else(|| panic!("first foundry total player attention overflowed"));
    let autonomous_ticks = melt_ticks
        .checked_add(cast_ticks)
        .and_then(|ticks| ticks.checked_add(cooldown_ticks))
        .unwrap_or_else(|| panic!("first foundry autonomous window overflowed"));
    let remaining_native_after = state
        .inventory()
        .get_stockpile(raw)
        .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL)))
        .unwrap_or_else(|| panic!("first foundry raw stockpile disappeared after bootstrap"));

    reviewln!(
        "FIRST FOUNDRY EXPERIENCE seed=0x{:016X} sample={} scope=ordinary-foundry-bootstrap-decision upstream=post-settlement-mechanization-disclosed-opportunity continuity=separate-episode resource-opportunity=[stone:{}mg wood:{}mg native:{}mg] immediate-choice=[order:{}mg direct-native:{}t reinforcement:{}mg selection:direct-native reason=cheap-current-order] bootstrap-choice=[remaining-native:{}mg foundry-capital:{}mg cast-ingots:{}mg required:{}mg shortfall:0mg selection:foundry reason=cast-ingot-stock-required-for-next-stage-mold] foundry-build=true fabrication={}t/{} campaign=[batches:{} charge:{}t melt:{}t cast:{}t cooldown:{}t autonomous:{}t released-heat:{}nJ] mold-upgrade=[{}mg->{}mg] copper-after-bootstrap:{}mg total-player-attention:{}t total-elapsed:{}t/{} survival=[energy:{}nJ hydration:{}uL] matter=conserved continuation=settlement-batch-foundry",
        case.seed(),
        case.role().label(),
        disclosed_stone_opportunity.milligrams(),
        FOUNDRY_WOOD_OPPORTUNITY.milligrams(),
        native_opportunity.milligrams(),
        first_cast_mass.milligrams(),
        direct_native_ticks,
        direct_native_reinforcement.milligrams(),
        remaining_native.milligrams(),
        capital_copper.milligrams(),
        settlement_ingots.milligrams(),
        required_after_current.milligrams(),
        fabrication_ticks,
        format_physical_duration(registries, fabrication_ticks),
        cast_batches,
        charge_ticks,
        melt_ticks,
        cast_ticks,
        cooldown_ticks,
        autonomous_ticks,
        released_heat.nanojoules(),
        first_cast_mass.milligrams(),
        settlement_cast_mass.milligrams(),
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
    );
}
