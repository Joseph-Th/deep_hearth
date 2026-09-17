//! Copper-free crank-vs-treadle provider comparison through canonical manual craft.

use deep_hearth::capability::CapabilityValue;
use deep_hearth::content::gameplay_fixture::seed_lot;
use deep_hearth::content::{
    ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE, ENERGY_STONE_FLYWHEEL_DRIVE, ENERGY_TIMBER_FLYWHEEL_DRIVE,
    EQUIPMENT_COPPER_REINFORCED_HAND_CRANK, EQUIPMENT_STONE_HAND_CRANK,
    EQUIPMENT_TIMBER_TREADLE_DRIVE, FORM_LOG, FORM_LUMP, MANUAL_POWER_FOOT_TREADLE,
    MANUAL_POWER_HAND_CRANK, MATERIAL_STONE, MATERIAL_WOOD,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::WorldSeed;
use deep_hearth::energy::{EnergyStoreDefinitionId, EnergyStoreId, validate_assemble_energy_store};
use deep_hearth::equipment::{EquipmentDefinitionId, EquipmentId, validate_assemble_equipment};
use deep_hearth::inventory::StockpileId;
use deep_hearth::labor::{ManualPowerMethodId, ManualPowerRequest, validate_start_manual_power};
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::registry::Registries;
use deep_hearth::survival::{assess_survival, initialize_player_survival};

use super::environment::ROOM_TEMPERATURE;
use super::focused_runner::focused_probe_role_label;
use super::focused_seeds::FocusedProbeCase;
use super::inventory_support::add_solid_stockpile;
use super::manual_craft_execution::execute_manual_craft_batches;
use super::manual_craft_planning::manual_craft_plan_for_available_output;
use super::manual_power_timing::finish_manual_power_work;
use super::physical_time::format_physical_duration;
use super::seed::mix64;

struct ShapedBuild {
    attention_ticks: u64,
    input_mass_mg: u64,
    embodied_mass_mg: u64,
}

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
        let (craft, batches, source) = manual_craft_plan_for_available_output(
            registries,
            state,
            &[raw],
            commodity,
            required,
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

struct ChargeOutcome {
    attention_ticks: u64,
    metabolic_nj: u128,
    hydration_ul: u128,
    condition_after_ppm: u32,
}

fn provider_power_microwatts(
    registries: &Registries,
    method: ManualPowerMethodId,
    equipment: EquipmentDefinitionId,
    context: &'static str,
) -> u128 {
    let definition = registries
        .labor()
        .get_manual_power(method)
        .unwrap_or_else(|| panic!("power provider {context} lost its manual-power method"));
    let record = registries
        .equipment()
        .get_equipment(equipment)
        .unwrap_or_else(|| {
            panic!(
                "power provider {context} equipment {} disappeared",
                equipment.value()
            )
        });
    let CapabilityValue::Power(power) = record
        .capabilities()
        .get_capability(definition.power_capability())
        .unwrap_or_else(|| panic!("power provider {context} lost its provider-power capability"))
    else {
        panic!("power provider {context} provider-power capability changed physical kind")
    };
    power
        .whole_microwatts()
        .unwrap_or_else(|| panic!("power provider {context} provider power is sub-microwatt"))
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

pub(super) fn run_power_provider_probe(registries: &Registries, case: FocusedProbeCase) {
    let seed = case.seed();
    // Vary the matched charge job across all three copper-free ordinary accumulators. Timber
    // substitutes bulk woodworking for stone and lower capacity; paired stone maximizes work
    // buffering. All remain large enough for the treadle's higher throughput to save attention.
    let store_definition = match mix64(seed ^ 0x0504_F575_24A4_F421) % 3 {
        0 => ENERGY_TIMBER_FLYWHEEL_DRIVE,
        1 => ENERGY_STONE_FLYWHEEL_DRIVE,
        _ => ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE,
    };
    let mut state = AppState::new(WorldSeed::new(seed ^ 0x504F_5752_5052_0001));
    // Raw gathered nature only: every shaped component below is player-crafted through
    // canonical manual production, so build attention and material costs are earned.
    let raw = add_solid_stockpile(&mut state, Mass::from_milligrams(60_000_000));
    seed_lot(
        registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(30_000_000),
        ROOM_TEMPERATURE,
    );
    seed_lot(
        registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(30_000_000),
        ROOM_TEMPERATURE,
    );
    let shaped = add_solid_stockpile(&mut state, Mass::from_milligrams(20_000_000));
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("power provider survival setup failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("power provider initial matter audit failed: {error}"))
        .total();
    let capacity_nj = registries
        .energy()
        .get_store(store_definition)
        .map(|definition| definition.capacity().nanojoules())
        .unwrap_or_else(|| panic!("power provider flywheel definition disappeared"));

    // Comparative evidence must not let one provider arm consume survival reserve or material
    // before the other starts. Both arms therefore inherit the exact same actor-visible state.
    let mut crank_state = state.clone();
    let mut treadle_state = state;

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
        store_definition,
        "power provider crank flywheel",
    );
    let crank_charge = charge_to_full(
        registries,
        &mut crank_state,
        MANUAL_POWER_HAND_CRANK,
        crank,
        crank_drive,
        capacity_nj,
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
        store_definition,
        "power provider treadle flywheel",
    );
    let treadle_charge = charge_to_full(
        registries,
        &mut treadle_state,
        MANUAL_POWER_FOOT_TREADLE,
        treadle,
        treadle_drive,
        capacity_nj,
        "power provider treadle charge",
    );

    assert!(
        treadle_charge.attention_ticks < crank_charge.attention_ticks,
        "the treadle's higher charging throughput must repay attention on the same flywheel job"
    );
    assert_eq!(
        calculate_matter_accounting(&crank_state)
            .unwrap_or_else(|error| panic!("power provider crank matter audit failed: {error}"))
            .total(),
        matter_before,
        "crank comparison arm must conserve matter across build and charge"
    );
    assert_eq!(
        calculate_matter_accounting(&treadle_state)
            .unwrap_or_else(|error| panic!("power provider treadle matter audit failed: {error}"))
            .total(),
        matter_before,
        "treadle comparison arm must conserve matter across build and charge"
    );
    validate_loaded_state(registries, &crank_state)
        .unwrap_or_else(|error| panic!("power provider crank state invalid: {error}"));
    validate_loaded_state(registries, &treadle_state)
        .unwrap_or_else(|error| panic!("power provider treadle state invalid: {error}"));

    let charge_attention_reduction_ppm = u64::try_from(
        u128::from(crank_charge.attention_ticks - treadle_charge.attention_ticks)
            .checked_mul(1_000_000)
            .unwrap_or_else(|| panic!("power provider attention reduction overflowed"))
            / u128::from(crank_charge.attention_ticks),
    )
    .unwrap_or_else(|_| panic!("power provider attention reduction exceeds u64"));
    let crank_build_attention = crank_build
        .attention_ticks
        .checked_add(crank_drive_build.attention_ticks)
        .unwrap_or_else(|| panic!("power provider crank build attention overflowed"));
    let crank_build_mass_mg = crank_build
        .input_mass_mg
        .checked_add(crank_drive_build.input_mass_mg)
        .unwrap_or_else(|| panic!("power provider crank build mass overflowed"));
    let treadle_build_attention = treadle_build
        .attention_ticks
        .checked_add(treadle_drive_build.attention_ticks)
        .unwrap_or_else(|| panic!("power provider treadle build attention overflowed"));
    let treadle_build_mass_mg = treadle_build
        .input_mass_mg
        .checked_add(treadle_drive_build.input_mass_mg)
        .unwrap_or_else(|| panic!("power provider treadle build mass overflowed"));
    assert!(
        treadle_build_mass_mg > crank_build_mass_mg,
        "the treadle route must remain a heavier material investment than the crank route"
    );
    // Initial-rate estimate, not executed lifetime payback: repeat charges incur wear and
    // eventually service. Keep that uncertainty visible rather than projecting one fresh
    // charge into a guaranteed lifetime result.
    let charge_saving_per_job_ticks = crank_charge
        .attention_ticks
        .checked_sub(treadle_charge.attention_ticks)
        .unwrap_or_else(|| {
            panic!("power provider treadle must save charge attention on the same job")
        });
    assert!(
        charge_saving_per_job_ticks > 0,
        "the treadle's higher charging throughput must repay attention on the same flywheel job"
    );
    let build_attention_delta_ticks = treadle_build_attention.saturating_sub(crank_build_attention);
    let break_even_charges = build_attention_delta_ticks.div_ceil(charge_saving_per_job_ticks);
    let crank_embodied = crank_build.embodied_mass_mg + crank_drive_build.embodied_mass_mg;
    let treadle_embodied = treadle_build.embodied_mass_mg + treadle_drive_build.embodied_mass_mg;
    let crank_residual = stockpile_mass(&crank_state, shaped).milligrams();
    let treadle_residual = stockpile_mass(&treadle_state, shaped).milligrams();
    assert_eq!(
        crank_build_mass_mg,
        crank_embodied + crank_residual,
        "crank raw bill must reconcile equipment, flywheel, surplus and shaping residue"
    );
    assert_eq!(
        treadle_build_mass_mg,
        treadle_embodied + treadle_residual,
        "treadle raw bill must reconcile equipment, flywheel, surplus and shaping residue"
    );
    assert!(
        crank_residual > 0 && treadle_residual > 0,
        "these shaped builds must expose their real surplus/residue rather than just assembly mass"
    );
    reviewln!(
        "POWER BUILD BILL seed=0x{seed:016X} basis=executed-raw-withdrawal crank=[raw:{:.3}kg embodied:{:.3}kg residual:{:.3}kg build:{} charge:{} charge-body:{:.2}kJ] treadle=[raw:{:.3}kg embodied:{:.3}kg residual:{:.3}kg build:{} charge:{} charge-body:{:.2}kJ] buffer:{:.0}J break-even:{}charges estimate=initial-charge-rate-excludes-future-wear-and-service",
        crank_build_mass_mg as f64 / 1_000_000.0,
        crank_embodied as f64 / 1_000_000.0,
        crank_residual as f64 / 1_000_000.0,
        format_physical_duration(registries, crank_build_attention),
        format_physical_duration(registries, crank_charge.attention_ticks),
        crank_charge.metabolic_nj as f64 / 1_000_000_000_000.0,
        treadle_build_mass_mg as f64 / 1_000_000.0,
        treadle_embodied as f64 / 1_000_000.0,
        treadle_residual as f64 / 1_000_000.0,
        format_physical_duration(registries, treadle_build_attention),
        format_physical_duration(registries, treadle_charge.attention_ticks),
        treadle_charge.metabolic_nj as f64 / 1_000_000_000_000.0,
        capacity_nj as f64 / 1_000_000_000.0,
        break_even_charges,
    );
    reviewln!(
        "POWER PROVIDER EXPERIENCE seed=0x{seed:016X} sample={} job=[flywheel:{}nJ] crank=[build:{}mg attention:{}t charge:{}t metabolic:{}nJ hydration:{}uL condition:{}ppm] treadle=[build:{}mg attention:{}t charge:{}t metabolic:{}nJ hydration:{}uL condition:{}ppm] comparison=[basis:matched-starting-state charge-attention-reduction:{}ppm build-mass-crank:{}mg build-mass-treadle:{}mg metabolic-crank:{}nJ metabolic-treadle:{}nJ build-attention-crank:{}t build-attention-treadle:{}t charge-crank:{}t charge-treadle:{}t charge-saving:{}t break-even-charges:{}] matter=conserved",
        focused_probe_role_label(case.role()),
        capacity_nj,
        crank_build_mass_mg,
        crank_build_attention,
        crank_charge.attention_ticks,
        crank_charge.metabolic_nj,
        crank_charge.hydration_ul,
        crank_charge.condition_after_ppm,
        treadle_build_mass_mg,
        treadle_build_attention,
        treadle_charge.attention_ticks,
        treadle_charge.metabolic_nj,
        treadle_charge.hydration_ul,
        treadle_charge.condition_after_ppm,
        charge_attention_reduction_ppm,
        crank_build_mass_mg,
        treadle_build_mass_mg,
        crank_charge.metabolic_nj,
        treadle_charge.metabolic_nj,
        crank_build_attention,
        treadle_build_attention,
        crank_charge.attention_ticks,
        treadle_charge.attention_ticks,
        charge_saving_per_job_ticks,
        break_even_charges,
    );
    // Post-copper catalog context without disturbing the copper-free matched comparison above.
    // The reinforced crank needs mined native copper, so it cannot join the copper-free arms;
    // these canonical registry reads plus the observed copper-free charges frame the later
    // speed-versus-efficiency choice instead of leaving the best provider invisible.
    let crank_method = registries
        .labor()
        .get_manual_power(MANUAL_POWER_HAND_CRANK)
        .unwrap_or_else(|| panic!("power provider copper context lost the crank method"));
    let treadle_method = registries
        .labor()
        .get_manual_power(MANUAL_POWER_FOOT_TREADLE)
        .unwrap_or_else(|| panic!("power provider copper context lost the treadle method"));
    let stone_crank_power_uw = provider_power_microwatts(
        registries,
        MANUAL_POWER_HAND_CRANK,
        EQUIPMENT_STONE_HAND_CRANK,
        "copper-context stone crank",
    );
    let copper_crank_power_uw = provider_power_microwatts(
        registries,
        MANUAL_POWER_HAND_CRANK,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        "copper-context reinforced crank",
    );
    let treadle_power_uw = provider_power_microwatts(
        registries,
        MANUAL_POWER_FOOT_TREADLE,
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
        "copper-context treadle",
    );
    reviewln!(
        "POWER COPPER-CONTEXT seed=0x{seed:016X} sample={} job=[flywheel:{}nJ] provider-power=[stone-crank:{}uW copper-crank:{}uW treadle:{}uW] labor=[crank-efficiency:{}ppm wear:{}ppm/t treadle-efficiency:{}ppm wear:{}ppm/t] observed=[crank-charge:{}t treadle-charge:{}t] catalog-note=copper-crank-needs-mined-native-copper-not-in-copper-free-start reachability-authority=STATUS.md",
        focused_probe_role_label(case.role()),
        capacity_nj,
        stone_crank_power_uw,
        copper_crank_power_uw,
        treadle_power_uw,
        crank_method.metabolic_efficiency_ppm(),
        crank_method.condition_wear_ppm_per_active_tick(),
        treadle_method.metabolic_efficiency_ppm(),
        treadle_method.condition_wear_ppm_per_active_tick(),
        crank_charge.attention_ticks,
        treadle_charge.attention_ticks,
    );
}
