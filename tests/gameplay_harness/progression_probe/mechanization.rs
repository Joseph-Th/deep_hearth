//! Copper reinforcement, machine investment, assembly, and charging for primitive progression.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PrimitiveComponentService {
    pub(super) preparation_ticks: u64,
    pub(super) service_ticks: u64,
    pub(super) material_mass: Mass,
    pub(super) condition_before_ppm: u32,
    pub(super) preserved_reinforcement: bool,
}

pub(super) fn service_reinforced_pick(
    registries: &Registries,
    state: &mut AppState,
    raw: deep_hearth::inventory::StockpileId,
    native_storage: deep_hearth::inventory::StockpileId,
    shaped: deep_hearth::inventory::StockpileId,
    pick: deep_hearth::equipment::EquipmentId,
    staged_preparation_ticks: u64,
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
    let serial_preparation_ticks = duration(preparation_started_at, state.tick().value());
    if staged_preparation_ticks > 0 {
        assert_eq!(
            serial_preparation_ticks, 0,
            "staged maintenance material must eliminate serial component preparation"
        );
    }
    let preparation_ticks = staged_preparation_ticks
        .checked_add(serial_preparation_ticks)
        .unwrap_or_else(|| panic!("primitive maintenance preparation duration overflowed"));
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
    let (service_ticks, completion) =
        finish_active_equipment_maintenance(registries, state, "primitive reinforced-pick service");
    assert_eq!(completion.equipment(), pick);

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
        service_ticks,
        material_mass: replacement_mass,
        condition_before_ppm: condition_before.parts_per_million(),
        preserved_reinforcement,
    }
}

pub(super) fn native_input_for_upgrade(
    registries: &Registries,
    equipment: deep_hearth::equipment::EquipmentDefinitionId,
) -> Mass {
    let native = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
    equipment_upgrade_additions(registries, equipment)
        .inputs()
        .iter()
        .try_fold(Mass::ZERO, |total, input| {
            let (craft, batches) =
                manual_craft_topology_plan_for_output(
                    registries,
                    input.commodity(),
                    input.mass(),
                    "primitive copper upgrade planning",
                );
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

pub(super) fn reinforce_pick(
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

pub(super) fn reinforce_crank(
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
pub(super) struct PrimitiveMachine {
    pub(super) crank: deep_hearth::equipment::EquipmentId,
    pub(super) crusher: deep_hearth::equipment::EquipmentId,
    pub(super) separator: deep_hearth::equipment::EquipmentId,
    pub(super) drive: deep_hearth::energy::EnergyStoreId,
    pub(super) drive_capacity: Energy,
    pub(super) required_energy: Energy,
    pub(super) separation_required_energy: Energy,
    pub(super) charge_energy: Energy,
    pub(super) reserve_mass: Mass,
    pub(super) charge_fill_ppm: u32,
    pub(super) charge_ticks: u64,
    pub(super) full_charge_ticks: u64,
    pub(super) automation_preparation_ticks: u64,
    pub(super) separator_preparation_ticks: u64,
    pub(super) processing_line_preparation_ticks: u64,
    pub(super) preparation_metabolic_cost_nj: u128,
    pub(super) preparation_hydration_cost_ul: u64,
    pub(super) crank_reinforced: bool,
}

#[derive(Clone, Copy)]
pub(super) struct PrimitiveMachineBuildPlan {
    pub(super) raw: deep_hearth::inventory::StockpileId,
    pub(super) native_storage: deep_hearth::inventory::StockpileId,
    pub(super) shaped: deep_hearth::inventory::StockpileId,
    pub(super) mined_mass: Mass,
    pub(super) separation_feed_mass: Mass,
    pub(super) seed: u64,
}

#[derive(Clone, Copy)]
pub(super) struct PrimitiveMachineEnergyPlan {
    pub(super) drive_capacity: Energy,
    pub(super) required_energy: Energy,
    pub(super) separation_required_energy: Energy,
    pub(super) charge_energy: Energy,
    pub(super) reserve_mass: Mass,
    pub(super) charge_fill_ppm: u32,
}

pub(super) fn primitive_machine_energy_plan(
    registries: &Registries,
    mined_mass: Mass,
    separation_feed_mass: Mass,
    seed: u64,
) -> PrimitiveMachineEnergyPlan {
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
    let maximum_follow_up_energy =
        calculate_mass_specific_energy(mined_mass, crusher_process.specific_energy());
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
    PrimitiveMachineEnergyPlan {
        drive_capacity,
        required_energy,
        separation_required_energy,
        charge_energy,
        reserve_mass,
        charge_fill_ppm,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PrimitiveProcessingInvestmentProjection {
    pub(super) assembly_attention_ticks: u64,
    pub(super) initial_charge_ticks: u64,
    pub(super) repeated_charge_ticks: u64,
    pub(super) conservative_attention_ticks: u64,
}

#[derive(Clone, Copy)]
pub(super) struct PrimitiveProcessingInvestmentPlan {
    pub(super) raw: deep_hearth::inventory::StockpileId,
    pub(super) native_storage: deep_hearth::inventory::StockpileId,
    pub(super) shaped: deep_hearth::inventory::StockpileId,
    pub(super) mined_mass: Mass,
    pub(super) separation_feed_mass: Mass,
    pub(super) seed: u64,
    pub(super) crank_reinforced_before_charge: bool,
}

pub(super) fn project_primitive_processing_investment(
    registries: &Registries,
    state: &AppState,
    plan: PrimitiveProcessingInvestmentPlan,
) -> PrimitiveProcessingInvestmentProjection {
    let PrimitiveProcessingInvestmentPlan {
        raw,
        native_storage,
        shaped,
        mined_mass,
        separation_feed_mass,
        seed,
        crank_reinforced_before_charge,
    } = plan;
    let drive_profile = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("primitive progression flywheel drive lost its assembly route"));
    let assembly = project_manual_assembly_package(
        registries,
        state,
        &[raw, native_storage],
        shaped,
        &[
            equipment_assembly_profile(registries, EQUIPMENT_STONE_HAND_CRANK),
            drive_profile,
            equipment_assembly_profile(registries, EQUIPMENT_STONE_CRUSHER),
            equipment_assembly_profile(registries, EQUIPMENT_STONE_SEPARATOR),
        ],
        "primitive processing-line pre-action build",
    );
    let energy = primitive_machine_energy_plan(registries, mined_mass, separation_feed_mass, seed);
    let initial_definition = if crank_reinforced_before_charge {
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK
    } else {
        EQUIPMENT_STONE_HAND_CRANK
    };
    let initial_charge = project_manual_power(
        registries,
        MANUAL_POWER_HAND_CRANK,
        initial_definition,
        Condition::PRISTINE,
        ENERGY_STONE_FLYWHEEL_DRIVE,
        energy.charge_energy,
    )
    .unwrap_or_else(|error| {
        panic!("primitive pre-action initial charge projection failed: {error}")
    });
    let mut condition = initial_charge.condition_after();
    let mut repeated_charge_ticks = 0_u64;
    // Full-capacity top-ups are deliberately conservative. The actual line reuses residual stored
    // work; two extra top-ups cover separation/reserve uncertainty beyond the disclosed cycles.
    for _ in 0..STOCKPILE_WORK_ORDER_CYCLES + 2 {
        let charge = project_manual_power(
            registries,
            MANUAL_POWER_HAND_CRANK,
            EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
            condition,
            ENERGY_STONE_FLYWHEEL_DRIVE,
            energy.drive_capacity,
        )
        .unwrap_or_else(|error| {
            panic!("primitive pre-action repeated charge projection failed: {error}")
        });
        repeated_charge_ticks = repeated_charge_ticks
            .checked_add(charge.duration().value())
            .unwrap_or_else(|| panic!("primitive projected repeated charging overflowed"));
        condition = charge.condition_after();
    }
    let reinforcement_allowance_ticks = registries
        .crafting()
        .get_manual(PROCESS_COLD_WORK_COPPER_REINFORCEMENT)
        .map(|definition| definition.duration().value())
        .unwrap_or_else(|| panic!("primitive copper reinforcement route disappeared"));
    let conservative_attention_ticks = assembly
        .attention_ticks
        .checked_add(initial_charge.duration().value())
        .and_then(|ticks| ticks.checked_add(repeated_charge_ticks))
        .and_then(|ticks| ticks.checked_add(reinforcement_allowance_ticks))
        .unwrap_or_else(|| panic!("primitive processing investment projection overflowed"));
    PrimitiveProcessingInvestmentProjection {
        assembly_attention_ticks: assembly.attention_ticks,
        initial_charge_ticks: initial_charge.duration().value(),
        repeated_charge_ticks,
        conservative_attention_ticks,
    }
}

pub(super) fn build_primitive_machine(
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

    let PrimitiveMachineEnergyPlan {
        drive_capacity,
        required_energy,
        separation_required_energy,
        charge_energy,
        reserve_mass,
        charge_fill_ppm,
    } = primitive_machine_energy_plan(registries, mined_mass, separation_feed_mass, seed);
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

pub(super) fn charge_primitive_machine(
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

pub(super) fn fill_primitive_accumulator(
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
