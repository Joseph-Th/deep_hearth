//! Matched primitive and settlement human-power comparisons through canonical craft and charging.

use deep_hearth::capability::CapabilityValue;
use deep_hearth::content::gameplay_fixture::{seed_composed_lot, seed_lot};
use deep_hearth::content::{
    ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE, ENERGY_STONE_FLYWHEEL_DRIVE, ENERGY_TIMBER_FLYWHEEL_DRIVE,
    ENERGY_TIMBER_FRAME_FLYWHEEL_BANK, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
    EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_TIMBER_TREADLE_DRIVE,
    EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE, FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL, FORM_ORE,
    MANUAL_POWER_FOOT_TREADLE, MANUAL_POWER_HAND_CRANK, MANUAL_POWER_WALKING_WHEEL,
    MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD, PROCESS_CRUSH_ORE,
    PROCESS_POWER_SAW_WOOD_BOARDS,
};
use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::core::state::AppState;
use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::fluid::calculate_fluid_volume_accounting;
use deep_hearth::labor::ManualPowerMethodId;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::registry::Registries;
use deep_hearth::survival::initialize_player_survival;

use super::environment::ROOM_TEMPERATURE;
use super::equipment_support::pristine_equipment_capability;
use super::focused_runner::focused_probe_role_label;
use super::focused_seeds::FocusedProbeCase;
use super::inventory_support::add_solid_stockpile;
use super::ore_fixture::copper_ore_composition;
use super::physical_time::format_physical_duration;
use super::seed::mix64;

#[path = "power_provider_build.rs"]
mod build;
#[path = "power_provider_consumers.rs"]
mod consumers;
#[path = "power_provider_execution.rs"]
mod execution;
#[path = "power_provider_planning.rs"]
mod planning;
#[path = "power_provider_provisioning.rs"]
mod provisioning;

use consumers::{build_primitive_power_consumer, build_settlement_power_consumer};
use execution::{
    PrimitiveComparison, ProjectExecutionResources, SettlementComparison,
    execute_primitive_comparison, execute_selected_primitive_project,
    execute_selected_settlement_project, execute_settlement_comparison,
};
use planning::{
    PrimitivePowerChoice, PrimitivePowerPlan, SettlementPowerChoice, SettlementPowerPlan,
    primitive_power_plan, settlement_power_plan,
};
use provisioning::seed_power_project_provisions;

fn declared_primitive_crushing_project(registries: &Registries, seed: u64) -> (Mass, Energy) {
    let kilograms = 5 + mix64(seed ^ 0x5052_494D_5F4F_5245) % 146;
    let mass = Mass::from_milligrams(
        kilograms
            .checked_mul(1_000_000)
            .unwrap_or_else(|| panic!("primitive power project mass overflowed")),
    );
    let definition = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("primitive power project crusher process disappeared"));
    (
        mass,
        deep_hearth::energy::calculate_mass_specific_energy(mass, definition.specific_energy()),
    )
}

fn declared_settlement_lumber_project(registries: &Registries, seed: u64) -> (Mass, Energy) {
    let kilograms = 100 + mix64(seed ^ 0x5345_5454_5F4C_554D) % 1_901;
    let mass = Mass::from_milligrams(
        kilograms
            .checked_mul(1_000_000)
            .unwrap_or_else(|| panic!("settlement power project mass overflowed")),
    );
    let definition = registries
        .crafting()
        .get_powered(PROCESS_POWER_SAW_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("settlement power project saw process disappeared"));
    (
        mass,
        deep_hearth::energy::calculate_mass_specific_energy(mass, definition.specific_energy()),
    )
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
    let CapabilityValue::Power(power) =
        pristine_equipment_capability(registries, equipment, definition.power_capability())
    else {
        panic!("power provider {context} provider-power capability changed physical kind")
    };
    power
        .whole_microwatts()
        .unwrap_or_else(|| panic!("power provider {context} provider power is sub-microwatt"))
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
    let (primitive_project_mass, primitive_project_work) =
        declared_primitive_crushing_project(registries, seed);
    let mut state = AppState::new();
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
    let primitive_feed = add_solid_stockpile(&mut state, primitive_project_mass);
    let primitive_output = add_solid_stockpile(&mut state, primitive_project_mass);
    let primitive_service_replacement =
        add_solid_stockpile(&mut state, Mass::from_milligrams(15_000_000));
    let primitive_service_spent =
        // The generated crusher campaign reaches 120 kg and can cross ten component services.
        // Keep the spent sink finite but large enough for that declared horizon so this probe
        // measures provider/maintenance lifecycle rather than an unrelated waste-bin ceiling.
        add_solid_stockpile(&mut state, Mass::from_milligrams(20_000_000));
    let primitive_provisions = seed_power_project_provisions(registries, &mut state);
    seed_composed_lot(
        registries,
        &mut state,
        primitive_feed,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        primitive_project_mass,
        ROOM_TEMPERATURE,
        copper_ore_composition(350_000, 200_000),
    );
    let shaped = add_solid_stockpile(&mut state, Mass::from_milligrams(40_000_000));
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("power provider survival setup failed: {error}"));
    let primitive_consumer = build_primitive_power_consumer(
        registries,
        &mut state,
        raw,
        shaped,
        primitive_feed,
        primitive_output,
    );
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("power provider initial matter audit failed: {error}"))
        .total();
    let fluid_before = calculate_fluid_volume_accounting(&state)
        .unwrap_or_else(|error| panic!("power provider initial fluid audit failed: {error}"))
        .total();
    let capacity_nj = registries
        .energy()
        .get_store(store_definition)
        .map(|definition| definition.capacity().nanojoules())
        .unwrap_or_else(|| panic!("power provider flywheel definition disappeared"));
    // Freeze the actor's investment choice from current authored topology and canonical physical
    // projections before any matched branch is executed.
    let plan = primitive_power_plan(
        registries,
        &state,
        raw,
        shaped,
        store_definition,
        capacity_nj,
        primitive_project_work.nanojoules(),
    );
    let (settlement_project_mass, settlement_project_work) =
        declared_settlement_lumber_project(registries, seed);
    let mut settlement_state = AppState::new();
    let settlement_raw =
        add_solid_stockpile(&mut settlement_state, Mass::from_milligrams(61_000_000));
    seed_lot(
        registries,
        &mut settlement_state,
        settlement_raw,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(30_000_000),
        ROOM_TEMPERATURE,
    );
    seed_lot(
        registries,
        &mut settlement_state,
        settlement_raw,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(30_000_000),
        ROOM_TEMPERATURE,
    );
    seed_lot(
        registries,
        &mut settlement_state,
        settlement_raw,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        // The generated lumber campaign reaches 2,000 kg. Keep its sawmill blade-service copper
        // finite but sufficiently funded so this provider probe reaches the declared workload
        // instead of turning into a separate copper-depletion episode.
        Mass::from_milligrams(1_000_000),
        ROOM_TEMPERATURE,
    );
    let settlement_feed = add_solid_stockpile(&mut settlement_state, settlement_project_mass);
    seed_lot(
        registries,
        &mut settlement_state,
        settlement_feed,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        settlement_project_mass,
        ROOM_TEMPERATURE,
    );
    let settlement_shaped =
        add_solid_stockpile(&mut settlement_state, Mass::from_milligrams(50_000_000));
    let settlement_output = add_solid_stockpile(&mut settlement_state, settlement_project_mass);
    let settlement_service_replacement =
        add_solid_stockpile(&mut settlement_state, Mass::from_milligrams(1_000_000));
    let settlement_service_spent =
        add_solid_stockpile(&mut settlement_state, Mass::from_milligrams(1_000_000));
    let settlement_provisions = seed_power_project_provisions(registries, &mut settlement_state);
    initialize_player_survival(registries, &mut settlement_state)
        .unwrap_or_else(|error| panic!("settlement power survival setup failed: {error}"));
    let settlement_consumer = build_settlement_power_consumer(
        registries,
        &mut settlement_state,
        settlement_raw,
        settlement_shaped,
        settlement_feed,
        settlement_output,
    );
    let settlement_matter_before = calculate_matter_accounting(&settlement_state)
        .unwrap_or_else(|error| panic!("settlement power initial matter audit failed: {error}"))
        .total();
    let settlement_fluid_before = calculate_fluid_volume_accounting(&settlement_state)
        .unwrap_or_else(|error| panic!("settlement power initial fluid audit failed: {error}"))
        .total();
    let settlement_capacity_nj = registries
        .energy()
        .get_store(ENERGY_TIMBER_FRAME_FLYWHEEL_BANK)
        .map(|definition| definition.capacity().nanojoules())
        .unwrap_or_else(|| panic!("settlement flywheel bank definition disappeared"));
    let settlement_plan = settlement_power_plan(
        registries,
        &settlement_state,
        settlement_raw,
        settlement_shaped,
        settlement_capacity_nj,
        settlement_project_work.nanojoules(),
    );

    // Experience the complete project through the actor-selected provider. The matched branches
    // below still isolate provider deltas over one productive cycle, while these selected runs pay
    // every repeated charge and any consumer maintenance that the declared project actually reaches.
    let primitive_resources = ProjectExecutionResources::new(
        raw,
        shaped,
        primitive_service_replacement,
        primitive_service_spent,
        primitive_provisions,
        matter_before,
        fluid_before,
    );
    let primitive_crank_project = execute_selected_primitive_project(
        registries,
        &state,
        primitive_resources,
        PrimitivePowerPlan {
            choice: PrimitivePowerChoice::Crank,
            ..plan
        },
        primitive_consumer,
    );
    let primitive_treadle_project = execute_selected_primitive_project(
        registries,
        &state,
        primitive_resources,
        PrimitivePowerPlan {
            choice: PrimitivePowerChoice::Treadle,
            ..plan
        },
        primitive_consumer,
    );
    let primitive_selected = match plan.choice {
        PrimitivePowerChoice::Crank => primitive_crank_project,
        PrimitivePowerChoice::Treadle => primitive_treadle_project,
    };
    let settlement_resources = ProjectExecutionResources::new(
        settlement_raw,
        settlement_shaped,
        settlement_service_replacement,
        settlement_service_spent,
        settlement_provisions,
        settlement_matter_before,
        settlement_fluid_before,
    );
    let settlement_treadle_project = execute_selected_settlement_project(
        registries,
        &settlement_state,
        settlement_resources,
        SettlementPowerPlan {
            choice: SettlementPowerChoice::Treadle,
            ..settlement_plan
        },
        settlement_consumer,
    );
    let settlement_walking_project = execute_selected_settlement_project(
        registries,
        &settlement_state,
        settlement_resources,
        SettlementPowerPlan {
            choice: SettlementPowerChoice::WalkingWheel,
            ..settlement_plan
        },
        settlement_consumer,
    );
    let settlement_selected = match settlement_plan.choice {
        SettlementPowerChoice::Treadle => settlement_treadle_project,
        SettlementPowerChoice::WalkingWheel => settlement_walking_project,
    };
    let primitive_actual_attention_best = match primitive_crank_project
        .active_attention_ticks()
        .cmp(&primitive_treadle_project.active_attention_ticks())
    {
        std::cmp::Ordering::Less => "crank",
        std::cmp::Ordering::Equal => "tie",
        std::cmp::Ordering::Greater => "treadle",
    };
    let settlement_actual_attention_best = match settlement_treadle_project
        .active_attention_ticks()
        .cmp(&settlement_walking_project.active_attention_ticks())
    {
        std::cmp::Ordering::Less => "treadle",
        std::cmp::Ordering::Equal => "tie",
        std::cmp::Ordering::Greater => "walking-wheel",
    };
    reviewln!(
        "POWER PROJECT EXPERIENCE seed=0x{seed:016X} sample={} era=primitive selected={} declared=[work:{}nJ pristine-charge-events:{} project-cache=[food:{}mg preservation:{}ppm water:{}uL]] executed=[charge-events:{} survival-limited-batches:{} active-attention:{}t provider-attention:{}t consumer-runtime:{}t maintenance=[services:{} preparation:{}t service:{}t replacement:{}mg] provisioning=[stops:{} attention:{}t drinks:{} volume:{}uL meals:{} mass:{}mg] elapsed:{}t reserve-delta:{}nJ/{}uL] end=[provider-condition:{}ppm consumer-condition:{}ppm metabolic:{}nJ hydration:{}uL] full-counterfactual=[crank-active-attention:{}t treadle-active-attention:{}t attention-best:{} selected-agrees:{}] evidence=complete-selected-project-canonical",
        focused_probe_role_label(case.role()),
        plan.choice.label(),
        plan.declared_work_nj,
        plan.charge_events,
        primitive_provisions.food_supply_mg,
        primitive_provisions.food_preservation_ppm,
        primitive_provisions.water_supply_ul,
        primitive_selected.charge_events,
        primitive_selected.survival_limited_batches,
        primitive_selected.active_attention_ticks(),
        primitive_selected.provider_attention_ticks,
        primitive_selected.consumer_ticks,
        primitive_selected.consumer_services,
        primitive_selected.service_preparation_ticks,
        primitive_selected.service_ticks,
        primitive_selected.replacement_mass_mg,
        primitive_selected.provisioning_stops,
        primitive_selected.provisioning_attention_ticks,
        primitive_selected.drink_actions,
        primitive_selected.drink_volume_ul,
        primitive_selected.meal_actions,
        primitive_selected.meal_mass_mg,
        primitive_selected.elapsed_ticks,
        primitive_selected.metabolic_nj,
        primitive_selected.hydration_ul,
        primitive_selected.provider_condition_ppm,
        primitive_selected.consumer_condition_ppm,
        primitive_selected.final_metabolic_nj,
        primitive_selected.final_hydration_ul,
        primitive_crank_project.active_attention_ticks(),
        primitive_treadle_project.active_attention_ticks(),
        primitive_actual_attention_best,
        plan.choice.label() == primitive_actual_attention_best
            || primitive_actual_attention_best == "tie",
    );
    reviewln!(
        "POWER PROJECT EXPERIENCE seed=0x{seed:016X} sample={} era=settlement selected={} declared=[work:{}nJ pristine-charge-events:{} project-cache=[food:{}mg preservation:{}ppm water:{}uL]] executed=[charge-events:{} survival-limited-batches:{} active-attention:{}t provider-attention:{}t consumer-runtime:{}t maintenance=[services:{} preparation:{}t service:{}t replacement:{}mg] provisioning=[stops:{} attention:{}t drinks:{} volume:{}uL meals:{} mass:{}mg] elapsed:{}t reserve-delta:{}nJ/{}uL] end=[provider-condition:{}ppm consumer-condition:{}ppm metabolic:{}nJ hydration:{}uL] full-counterfactual=[treadle-active-attention:{}t walking-active-attention:{}t attention-best:{} selected-agrees:{}] evidence=complete-selected-project-canonical",
        focused_probe_role_label(case.role()),
        settlement_plan.choice.label(),
        settlement_plan.declared_work_nj,
        settlement_plan.charge_events,
        settlement_provisions.food_supply_mg,
        settlement_provisions.food_preservation_ppm,
        settlement_provisions.water_supply_ul,
        settlement_selected.charge_events,
        settlement_selected.survival_limited_batches,
        settlement_selected.active_attention_ticks(),
        settlement_selected.provider_attention_ticks,
        settlement_selected.consumer_ticks,
        settlement_selected.consumer_services,
        settlement_selected.service_preparation_ticks,
        settlement_selected.service_ticks,
        settlement_selected.replacement_mass_mg,
        settlement_selected.provisioning_stops,
        settlement_selected.provisioning_attention_ticks,
        settlement_selected.drink_actions,
        settlement_selected.drink_volume_ul,
        settlement_selected.meal_actions,
        settlement_selected.meal_mass_mg,
        settlement_selected.elapsed_ticks,
        settlement_selected.metabolic_nj,
        settlement_selected.hydration_ul,
        settlement_selected.provider_condition_ppm,
        settlement_selected.consumer_condition_ppm,
        settlement_selected.final_metabolic_nj,
        settlement_selected.final_hydration_ul,
        settlement_treadle_project.active_attention_ticks(),
        settlement_walking_project.active_attention_ticks(),
        settlement_actual_attention_best,
        settlement_plan.choice.label() == settlement_actual_attention_best
            || settlement_actual_attention_best == "tie",
    );

    // Matched arms inherit the same actor-visible state. Execution owns projection agreement,
    // conservation, and trusted-load validity before reporting compares outcomes.
    let PrimitiveComparison {
        crank_build,
        crank_drive_build,
        crank_charge,
        treadle_build,
        treadle_drive_build,
        treadle_charge,
        crank_second_charge,
        treadle_second_charge,
        crank_consumer_ticks,
        treadle_consumer_ticks,
        crank_residual_mg,
        treadle_residual_mg,
    } = execute_primitive_comparison(
        registries,
        &state,
        raw,
        shaped,
        matter_before,
        plan,
        primitive_consumer,
    );
    let SettlementComparison {
        settlement_treadle_build,
        settlement_treadle_drive_build,
        settlement_treadle_charge,
        walking_build,
        walking_drive_build,
        walking_charge,
        settlement_treadle_second_charge,
        walking_second_charge,
        settlement_treadle_consumer_ticks,
        walking_consumer_ticks,
    } = execute_settlement_comparison(
        registries,
        &settlement_state,
        settlement_raw,
        settlement_shaped,
        settlement_matter_before,
        settlement_plan,
        settlement_consumer,
    );

    let settlement_treadle_build_attention = settlement_treadle_build
        .attention_ticks
        .checked_add(settlement_treadle_drive_build.attention_ticks)
        .unwrap_or_else(|| panic!("settlement treadle build attention overflowed"));
    let walking_build_attention = walking_build
        .attention_ticks
        .checked_add(walking_drive_build.attention_ticks)
        .unwrap_or_else(|| panic!("walking-wheel build attention overflowed"));
    let settlement_treadle_build_mass = settlement_treadle_build
        .input_mass_mg
        .checked_add(settlement_treadle_drive_build.input_mass_mg)
        .unwrap_or_else(|| panic!("settlement treadle build mass overflowed"));
    let walking_build_mass = walking_build
        .input_mass_mg
        .checked_add(walking_drive_build.input_mass_mg)
        .unwrap_or_else(|| panic!("walking-wheel build mass overflowed"));
    assert!(
        walking_build_mass > settlement_treadle_build_mass,
        "walking-wheel route must retain a larger timber investment than the treadle route"
    );
    let settlement_charge_saving = settlement_treadle_charge
        .attention_ticks
        .checked_sub(walking_charge.attention_ticks)
        .unwrap_or_else(|| panic!("walking wheel must save settlement charge attention"));
    let settlement_build_attention_delta = walking_build_attention
        .checked_sub(settlement_treadle_build_attention)
        .unwrap_or_else(|| panic!("walking wheel must cost more build attention than the treadle"));
    let settlement_break_even_charges =
        settlement_build_attention_delta.div_ceil(settlement_charge_saving);
    let settlement_decision_crossover = settlement_plan
        .decision_crossover_charges
        .map_or_else(|| "none".to_owned(), |charges| charges.to_string());
    assert_eq!(
        settlement_plan.declared_work_nj,
        settlement_project_work.nanojoules()
    );
    reviewln!(
        "POWER SETTLEMENT seed=0x{seed:016X} sample={} workload-source=declared-consumer-project project=[consumer:powered-saw feed:{}mg work:{}nJ charge-events:{}] buffer:{}nJ decision=[selected:{} policy=minimize-workload-attention-then-metabolic-then-hydration-then-material projected-attention-treadle:{}t projected-attention-walking:{}t choice-frozen-before-action:true] treadle=[build:{}mg attention:{}t build-body:{}nJ/{}uL first-charge:{}t second-charge:{}t metabolic:{}nJ hydration:{}uL condition:{}ppm] walking-wheel=[build:{}mg attention:{}t build-body:{}nJ/{}uL first-charge:{}t second-charge:{}t metabolic:{}nJ hydration:{}uL condition:{}ppm] productive-cycle=[consumer:powered-saw treadle:{}t walking:{}t] projected-provider-lifecycle=[treadle:body:{}nJ/{}uL condition:{}ppm walking-wheel:body:{}nJ/{}uL condition:{}ppm] comparison=[charge-saving:{}t metabolic-saving:{}nJ pristine-rate-break-even:{}charges wear-aware-decision-crossover:{}charges provider-lifecycle=condition-carried-no-service] selected-project=[charge-events:{} provider-attention:{}t consumer:{}t maintenance=[services:{} preparation:{}t service:{}t replacement:{}mg policy:service-at-critical] elapsed:{}t body:{}nJ/{}uL provider-condition:{}ppm consumer-condition:{}ppm final-reserve:{}nJ/{}uL] evidence=[build+charge+productive-discharge+recharge:executed selected-project:executed comparator-lifecycle:projected-canonical consumer:powered-saw] matter=conserved",
        focused_probe_role_label(case.role()),
        settlement_project_mass.milligrams(),
        settlement_project_work.nanojoules(),
        settlement_plan.charge_events,
        settlement_capacity_nj,
        settlement_plan.choice.label(),
        settlement_plan.treadle_lifecycle_attention,
        settlement_plan.walking_lifecycle_attention,
        settlement_treadle_build_mass,
        settlement_treadle_build_attention,
        settlement_plan.treadle_build.metabolic_nj,
        settlement_plan.treadle_build.hydration_ul,
        settlement_treadle_charge.attention_ticks,
        settlement_treadle_second_charge.attention_ticks,
        settlement_treadle_charge.metabolic_nj,
        settlement_treadle_charge.hydration_ul,
        settlement_treadle_charge.condition_after_ppm,
        walking_build_mass,
        walking_build_attention,
        settlement_plan.walking_build.metabolic_nj,
        settlement_plan.walking_build.hydration_ul,
        walking_charge.attention_ticks,
        walking_second_charge.attention_ticks,
        walking_charge.metabolic_nj,
        walking_charge.hydration_ul,
        walking_charge.condition_after_ppm,
        settlement_treadle_consumer_ticks,
        walking_consumer_ticks,
        settlement_plan.treadle_lifecycle_metabolic_nj,
        settlement_plan.treadle_lifecycle_hydration_ul,
        settlement_plan
            .treadle_lifecycle_condition
            .parts_per_million(),
        settlement_plan.walking_lifecycle_metabolic_nj,
        settlement_plan.walking_lifecycle_hydration_ul,
        settlement_plan
            .walking_lifecycle_condition
            .parts_per_million(),
        settlement_charge_saving,
        settlement_treadle_charge.metabolic_nj - walking_charge.metabolic_nj,
        settlement_break_even_charges,
        settlement_decision_crossover,
        settlement_selected.charge_events,
        settlement_selected.provider_attention_ticks,
        settlement_selected.consumer_ticks,
        settlement_selected.consumer_services,
        settlement_selected.service_preparation_ticks,
        settlement_selected.service_ticks,
        settlement_selected.replacement_mass_mg,
        settlement_selected.elapsed_ticks,
        settlement_selected.metabolic_nj,
        settlement_selected.hydration_ul,
        settlement_selected.provider_condition_ppm,
        settlement_selected.consumer_condition_ppm,
        settlement_selected.final_metabolic_nj,
        settlement_selected.final_hydration_ul,
    );

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
    let decision_crossover = plan
        .decision_crossover_charges
        .map_or_else(|| "none".to_owned(), |charges| charges.to_string());
    assert_eq!(plan.declared_work_nj, primitive_project_work.nanojoules());
    let crank_embodied = crank_build.embodied_mass_mg + crank_drive_build.embodied_mass_mg;
    let treadle_embodied = treadle_build.embodied_mass_mg + treadle_drive_build.embodied_mass_mg;
    let crank_residual = crank_residual_mg;
    let treadle_residual = treadle_residual_mg;
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
        "POWER PROVIDER EXPERIENCE seed=0x{seed:016X} sample={} workload-source=declared-consumer-project project=[consumer:stone-crusher feed:{}mg work:{}nJ charge-events:{}] buffer:{}nJ decision=[selected:{} policy=attention-first-with-minimum-investment-return minimum-attention-return:{}t projected-attention-crank:{}t projected-attention-treadle:{}t choice-frozen-before-action:true] crank=[build:{}mg attention:{}t build-body:{}nJ/{}uL first-charge:{}t second-charge:{}t metabolic:{}nJ hydration:{}uL condition:{}ppm] treadle=[build:{}mg attention:{}t build-body:{}nJ/{}uL first-charge:{}t second-charge:{}t metabolic:{}nJ hydration:{}uL condition:{}ppm] productive-cycle=[consumer:stone-crusher crank:{}t treadle:{}t] projected-provider-lifecycle=[crank:body:{}nJ/{}uL condition:{}ppm treadle:body:{}nJ/{}uL condition:{}ppm] comparison=[basis:matched-starting-state charge-attention-reduction:{}ppm build-mass-crank:{}mg build-mass-treadle:{}mg metabolic-crank:{}nJ metabolic-treadle:{}nJ build-attention-crank:{}t build-attention-treadle:{}t charge-crank:{}t charge-treadle:{}t charge-saving:{}t pristine-rate-break-even:{} wear-aware-decision-crossover:{} provider-lifecycle=condition-carried-no-service] selected-project=[charge-events:{} provider-attention:{}t consumer:{}t maintenance=[services:{} preparation:{}t service:{}t replacement:{}mg policy:service-at-critical] elapsed:{}t body:{}nJ/{}uL provider-condition:{}ppm consumer-condition:{}ppm final-reserve:{}nJ/{}uL] evidence=[build+charge+productive-discharge+recharge:executed selected-project:executed comparator-lifecycle:projected-canonical consumer:stone-crusher] matter=conserved",
        focused_probe_role_label(case.role()),
        primitive_project_mass.milligrams(),
        primitive_project_work.nanojoules(),
        plan.charge_events,
        capacity_nj,
        plan.choice.label(),
        plan.minimum_attention_return_ticks,
        plan.crank_lifecycle_attention,
        plan.treadle_lifecycle_attention,
        crank_build_mass_mg,
        crank_build_attention,
        plan.crank_build.metabolic_nj,
        plan.crank_build.hydration_ul,
        crank_charge.attention_ticks,
        crank_second_charge.attention_ticks,
        crank_charge.metabolic_nj,
        crank_charge.hydration_ul,
        crank_charge.condition_after_ppm,
        treadle_build_mass_mg,
        treadle_build_attention,
        plan.treadle_build.metabolic_nj,
        plan.treadle_build.hydration_ul,
        treadle_charge.attention_ticks,
        treadle_second_charge.attention_ticks,
        treadle_charge.metabolic_nj,
        treadle_charge.hydration_ul,
        treadle_charge.condition_after_ppm,
        crank_consumer_ticks,
        treadle_consumer_ticks,
        plan.crank_lifecycle_metabolic_nj,
        plan.crank_lifecycle_hydration_ul,
        plan.crank_lifecycle_condition.parts_per_million(),
        plan.treadle_lifecycle_metabolic_nj,
        plan.treadle_lifecycle_hydration_ul,
        plan.treadle_lifecycle_condition.parts_per_million(),
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
        decision_crossover,
        primitive_selected.charge_events,
        primitive_selected.provider_attention_ticks,
        primitive_selected.consumer_ticks,
        primitive_selected.consumer_services,
        primitive_selected.service_preparation_ticks,
        primitive_selected.service_ticks,
        primitive_selected.replacement_mass_mg,
        primitive_selected.elapsed_ticks,
        primitive_selected.metabolic_nj,
        primitive_selected.hydration_ul,
        primitive_selected.provider_condition_ppm,
        primitive_selected.consumer_condition_ppm,
        primitive_selected.final_metabolic_nj,
        primitive_selected.final_hydration_ul,
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
    let walking_power_uw = provider_power_microwatts(
        registries,
        MANUAL_POWER_WALKING_WHEEL,
        EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE,
        "copper-context walking wheel",
    );
    let walking_method = registries
        .labor()
        .get_manual_power(MANUAL_POWER_WALKING_WHEEL)
        .unwrap_or_else(|| panic!("power provider context lost the walking-wheel method"));
    reviewln!(
        "POWER COPPER-CONTEXT seed=0x{seed:016X} sample={} job=[flywheel:{}nJ] provider-power=[stone-crank:{}uW copper-crank:{}uW treadle:{}uW walking-wheel:{}uW] labor=[crank-efficiency:{}ppm wear:{}ppm/t treadle-efficiency:{}ppm wear:{}ppm/t walking-efficiency:{}ppm wear:{}ppm/t] observed=[crank-charge:{}t treadle-charge:{}t] catalog-note=copper-crank-needs-mined-native-copper-not-in-copper-free-start reachability-authority=STATUS.md",
        focused_probe_role_label(case.role()),
        capacity_nj,
        stone_crank_power_uw,
        copper_crank_power_uw,
        treadle_power_uw,
        walking_power_uw,
        crank_method.metabolic_efficiency_ppm(),
        crank_method.condition_wear_ppm_per_active_tick(),
        treadle_method.metabolic_efficiency_ppm(),
        treadle_method.condition_wear_ppm_per_active_tick(),
        walking_method.metabolic_efficiency_ppm(),
        walking_method.condition_wear_ppm_per_active_tick(),
        crank_charge.attention_ticks,
        treadle_charge.attention_ticks,
    );
}
