//! Matched primitive and settlement human-power comparisons through canonical craft and charging.

use deep_hearth::capability::CapabilityValue;
use deep_hearth::content::gameplay_fixture::seed_lot;
use deep_hearth::content::{
    ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE, ENERGY_STONE_FLYWHEEL_DRIVE, ENERGY_TIMBER_FLYWHEEL_DRIVE,
    ENERGY_TIMBER_FRAME_FLYWHEEL_BANK, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
    EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_TIMBER_TREADLE_DRIVE,
    EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE, FORM_LOG, FORM_LUMP, MANUAL_POWER_FOOT_TREADLE,
    MANUAL_POWER_HAND_CRANK, MANUAL_POWER_WALKING_WHEEL, MATERIAL_STONE, MATERIAL_WOOD,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::core::time::WorldSeed;
use deep_hearth::equipment::EquipmentDefinitionId;
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
use super::physical_time::format_physical_duration;
use super::seed::mix64;

#[path = "power_provider_execution.rs"]
mod execution;
#[path = "power_provider_planning.rs"]
mod planning;

use execution::{
    PrimitiveComparison, SettlementComparison, execute_primitive_comparison,
    execute_settlement_comparison,
};
use planning::{primitive_power_plan, settlement_power_plan};

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
    let shaped = add_solid_stockpile(&mut state, Mass::from_milligrams(40_000_000));
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
    // Freeze the actor's investment choice from current authored topology and canonical physical
    // projections before any matched branch is executed.
    let plan = primitive_power_plan(
        registries,
        &state,
        raw,
        shaped,
        store_definition,
        capacity_nj,
        seed,
    );
    let settlement_capacity_nj = registries
        .energy()
        .get_store(ENERGY_TIMBER_FRAME_FLYWHEEL_BANK)
        .map(|definition| definition.capacity().nanojoules())
        .unwrap_or_else(|| panic!("settlement flywheel bank definition disappeared"));
    let settlement_plan = settlement_power_plan(
        registries,
        &state,
        raw,
        shaped,
        settlement_capacity_nj,
        seed,
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
        crank_residual_mg,
        treadle_residual_mg,
    } = execute_primitive_comparison(registries, &state, raw, shaped, matter_before, plan);
    let SettlementComparison {
        settlement_treadle_build,
        settlement_treadle_drive_build,
        settlement_treadle_charge,
        walking_build,
        walking_drive_build,
        walking_charge,
    } = execute_settlement_comparison(
        registries,
        &state,
        raw,
        shaped,
        matter_before,
        settlement_plan,
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
    reviewln!(
        "POWER SETTLEMENT seed=0x{seed:016X} sample={} workload-source=declared-charge-horizon buffer:{}nJ planned-charges={} decision=[selected:{} policy:minimize-workload-attention-then-metabolic-then-hydration-then-material projected-attention-treadle:{}t projected-attention-walking:{}t choice-frozen-before-action:true] treadle=[build:{}mg attention:{}t build-body:{}nJ/{}uL charge:{}t metabolic:{}nJ hydration:{}uL condition:{}ppm] walking-wheel=[build:{}mg attention:{}t build-body:{}nJ/{}uL charge:{}t metabolic:{}nJ hydration:{}uL condition:{}ppm] projected-lifecycle=[treadle:body:{}nJ/{}uL condition:{}ppm walking-wheel:body:{}nJ/{}uL condition:{}ppm] comparison=[charge-saving:{}t metabolic-saving:{}nJ pristine-rate-break-even:{}charges wear-aware-decision-crossover:{}charges lifecycle=condition-carried-no-service] evidence=[build+first-charge:executed lifecycle:projected-canonical consumer:not-instantiated] matter=conserved",
        focused_probe_role_label(case.role()),
        settlement_capacity_nj,
        settlement_plan.planned_charges,
        settlement_plan.choice.label(),
        settlement_plan.treadle_lifecycle_attention,
        settlement_plan.walking_lifecycle_attention,
        settlement_treadle_build_mass,
        settlement_treadle_build_attention,
        settlement_plan.treadle_build.metabolic_nj,
        settlement_plan.treadle_build.hydration_ul,
        settlement_treadle_charge.attention_ticks,
        settlement_treadle_charge.metabolic_nj,
        settlement_treadle_charge.hydration_ul,
        settlement_treadle_charge.condition_after_ppm,
        walking_build_mass,
        walking_build_attention,
        settlement_plan.walking_build.metabolic_nj,
        settlement_plan.walking_build.hydration_ul,
        walking_charge.attention_ticks,
        walking_charge.metabolic_nj,
        walking_charge.hydration_ul,
        walking_charge.condition_after_ppm,
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
        "POWER PROVIDER EXPERIENCE seed=0x{seed:016X} sample={} workload-source=declared-charge-horizon job=[flywheel:{}nJ planned-charges:{}] decision=[selected:{} policy:minimize-workload-attention-then-metabolic-then-hydration-then-material projected-attention-crank:{}t projected-attention-treadle:{}t choice-frozen-before-action:true] crank=[build:{}mg attention:{}t build-body:{}nJ/{}uL charge:{}t metabolic:{}nJ hydration:{}uL condition:{}ppm] treadle=[build:{}mg attention:{}t build-body:{}nJ/{}uL charge:{}t metabolic:{}nJ hydration:{}uL condition:{}ppm] projected-lifecycle=[crank:body:{}nJ/{}uL condition:{}ppm treadle:body:{}nJ/{}uL condition:{}ppm] comparison=[basis:matched-starting-state charge-attention-reduction:{}ppm build-mass-crank:{}mg build-mass-treadle:{}mg metabolic-crank:{}nJ metabolic-treadle:{}nJ build-attention-crank:{}t build-attention-treadle:{}t charge-crank:{}t charge-treadle:{}t charge-saving:{}t pristine-rate-break-even:{} wear-aware-decision-crossover:{} lifecycle=condition-carried-no-service] evidence=[build+first-charge:executed lifecycle:projected-canonical consumer:not-instantiated] matter=conserved",
        focused_probe_role_label(case.role()),
        capacity_nj,
        plan.planned_charges,
        plan.choice.label(),
        plan.crank_lifecycle_attention,
        plan.treadle_lifecycle_attention,
        crank_build_mass_mg,
        crank_build_attention,
        plan.crank_build.metabolic_nj,
        plan.crank_build.hydration_ul,
        crank_charge.attention_ticks,
        crank_charge.metabolic_nj,
        crank_charge.hydration_ul,
        crank_charge.condition_after_ppm,
        treadle_build_mass_mg,
        treadle_build_attention,
        plan.treadle_build.metabolic_nj,
        plan.treadle_build.hydration_ul,
        treadle_charge.attention_ticks,
        treadle_charge.metabolic_nj,
        treadle_charge.hydration_ul,
        treadle_charge.condition_after_ppm,
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
