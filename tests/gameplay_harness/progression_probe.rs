//! Canonical primitive-to-mechanized progression probe for the gameplay experience harness.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;

use super::seed::mix64;
use super::support::{ROOM_TEMPERATURE, add_solid_stockpile, nominal_equipment_mass_capability};
use deep_hearth::content::gameplay_fixture::{
    geological_deposit_spec, seed_geological_deposit, seed_lot,
};
use deep_hearth::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_STONE_CRUSHER, EQUIPMENT_STONE_HAND_CRANK,
    EQUIPMENT_STONE_PICK, FORM_NATIVE_METAL, FORM_ORE, FORM_UNFIRED_POTTERY,
    MANUAL_POWER_HAND_CRANK, MATERIAL_CLAY, MATERIAL_COPPER, MATERIAL_STONE,
    MINING_METHOD_HAND_PICK, PROCESS_COLD_WORK_COPPER_REINFORCEMENT, PROCESS_CRUSH_ORE,
    PROCESS_FORM_CLAY_VESSEL, PROCESS_KNAP_STONE_TOOL, PROCESS_SHAPE_STONE_FLYWHEEL,
    PROCESS_SHAPE_WOOD_HANDLE,
};
use deep_hearth::core::quantity::{Energy, Mass, Pressure, Temperature};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::WorldSeed;
use deep_hearth::crafting::{
    ManualCraftRequest, ManualCraftStartRequest, validate_start_manual_craft,
};
use deep_hearth::energy::{calculate_mass_specific_energy, validate_assemble_energy_store};
use deep_hearth::equipment::{validate_assemble_equipment, validate_upgrade_equipment};
use deep_hearth::inventory::MaterialLotSelection;
use deep_hearth::labor::{ManualPowerRequest, validate_start_manual_power};
use deep_hearth::material::{
    CommodityKey, CompositionComponent, MaterialAssemblyProfile, MaterialComposition,
};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::mining::{validate_claim_mining_output, validate_start_mining};
use deep_hearth::ore_processing::{ComminutionRequest, resolve_comminution_process};
use deep_hearth::production::{ProductionJobId, validate_start_process};
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};
use deep_hearth::survival::{assess_survival, initialize_player_survival};

/// Fail closed when the authored player-facing acquisition/action catalog grows beyond what this
/// cold-agent progression episode actually exercises.
///
/// Runtime registries remain the authority for legality. These IDs are only an evidence inventory so
/// newly playable content cannot appear without forcing the gameplay harness to learn it.
fn assert_playable_catalog_coverage(registries: &Registries) {
    let actual_equipment = registries
        .equipment()
        .definitions()
        .filter(|definition| {
            definition.assembly_profile().is_some() || definition.upgrade_profile().is_some()
        })
        .map(|definition| definition.id().value())
        .collect::<BTreeSet<_>>();
    let exercised_equipment = BTreeSet::from([
        EQUIPMENT_STONE_PICK.value(),
        EQUIPMENT_STONE_HAND_CRANK.value(),
        EQUIPMENT_COPPER_REINFORCED_PICK.value(),
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK.value(),
        EQUIPMENT_STONE_CRUSHER.value(),
    ]);
    assert_eq!(
        actual_equipment, exercised_equipment,
        "cold-agent progression coverage is stale: update the probe so every equipment definition with a runtime assembly/upgrade route is exercised"
    );

    let actual_energy = registries
        .energy()
        .definitions()
        .filter(|definition| definition.assembly_profile().is_some())
        .map(|definition| definition.id().value())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual_energy,
        BTreeSet::from([ENERGY_STONE_FLYWHEEL_DRIVE.value()]),
        "cold-agent progression coverage is stale: update the probe so every runtime-assemblable energy store is exercised"
    );

    let actual_manual_processes = registries
        .crafting()
        .definitions()
        .map(|definition| definition.process().value())
        .collect::<BTreeSet<_>>();
    let exercised_manual_processes = BTreeSet::from([
        PROCESS_KNAP_STONE_TOOL.value(),
        PROCESS_FORM_CLAY_VESSEL.value(),
        PROCESS_SHAPE_WOOD_HANDLE.value(),
        PROCESS_SHAPE_STONE_FLYWHEEL.value(),
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT.value(),
    ]);
    assert_eq!(
        actual_manual_processes, exercised_manual_processes,
        "cold-agent progression coverage is stale: update the probe so every authored manual crafting action is exercised"
    );

    let actual_mining_methods = registries
        .mining()
        .definitions()
        .map(|definition| definition.id().value())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual_mining_methods,
        BTreeSet::from([MINING_METHOD_HAND_PICK.value()]),
        "cold-agent progression coverage is stale: update the probe so every authored mining method is exercised"
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
    for equipment in [
        EQUIPMENT_COPPER_REINFORCED_PICK,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
    ] {
        let additions = registries
            .equipment()
            .get_equipment(equipment)
            .and_then(|definition| definition.upgrade_profile())
            .map(|profile| profile.additions())
            .unwrap_or_else(|| {
                panic!(
                    "primitive progression equipment {} lost its additive upgrade route",
                    equipment.value()
                )
            });
        add_profile_requirements(&mut requirements, additions);
    }

    let mut process_batches: BTreeMap<deep_hearth::production::ProcessId, u64> = BTreeMap::new();
    for (commodity, required) in requirements {
        let craft = manual_craft_for_output(registries, commodity);
        let batches = batches_for_output(required, output_mass_per_batch(craft, commodity));
        process_batches
            .entry(craft.process())
            .and_modify(|existing| *existing = (*existing).max(batches))
            .or_insert(batches);
    }
    process_batches.entry(PROCESS_FORM_CLAY_VESSEL).or_insert(1);

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
    deposit: deep_hearth::geology::GeologicalDepositId,
    destination: deep_hearth::inventory::StockpileId,
    equipment: deep_hearth::equipment::EquipmentId,
    mass: Mass,
) -> u64 {
    let mining = validate_start_mining(
        registries,
        state,
        MINING_METHOD_HAND_PICK,
        deposit,
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
    deposit: deep_hearth::geology::GeologicalDepositId,
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
                deposit,
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

    const fn opposite(self) -> Self {
        match self {
            Self::ExtractionFirst => Self::MechanizationFirst,
            Self::MechanizationFirst => Self::ExtractionFirst,
        }
    }
}

fn primitive_priority(seed: u64) -> PrimitivePriority {
    if mix64(seed ^ 0x5052_494F_5249_5459).is_multiple_of(2) {
        PrimitivePriority::ExtractionFirst
    } else {
        PrimitivePriority::MechanizationFirst
    }
}

#[derive(Clone, Copy)]
struct PrimitiveProgressionExperience {
    priority: PrimitivePriority,
    pick_upgraded_at: u64,
    reinforced_ore_acquired_at: u64,
    machine_started_at: u64,
    first_processed_output_at: u64,
    elapsed_ticks: u64,
    stone_ore_mining_ticks: u64,
    first_native_mining_ticks: u64,
    second_native_mining_ticks: u64,
    reinforced_ore_mining_ticks: u64,
    final_ore_mining_ticks: u64,
    stone_charge_ticks: u64,
    reinforced_charge_ticks: u64,
    machine_work_ticks: u64,
    overlap_ticks: u64,
    machine_useful_overlap_ticks: u64,
    machine_idle_wait_ticks: u64,
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

#[derive(Clone, Copy)]
struct PrimitiveMachine {
    crusher: deep_hearth::equipment::EquipmentId,
    drive: deep_hearth::energy::EnergyStoreId,
    required_energy: Energy,
    charge_energy: Energy,
    reserve_mass: Mass,
    charge_fill_ppm: u32,
    stone_charge_ticks: u64,
    reinforced_charge_ticks: u64,
}

fn build_and_charge_primitive_machine(
    registries: &Registries,
    state: &mut AppState,
    raw: deep_hearth::inventory::StockpileId,
    native_storage: deep_hearth::inventory::StockpileId,
    shaped: deep_hearth::inventory::StockpileId,
    mined_mass: Mass,
    seed: u64,
) -> PrimitiveMachine {
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
    let drive_capacity = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("primitive progression flywheel definition disappeared"));
    assert!(
        drive_capacity >= required_energy,
        "primitive progression constructed drive cannot hold one legal crusher batch"
    );
    let maximum_follow_up_mass = mined_mass
        .checked_add(mined_mass)
        .unwrap_or_else(|| panic!("primitive progression follow-up ore mass overflowed"));
    let maximum_follow_up_energy =
        calculate_mass_specific_energy(maximum_follow_up_mass, crusher_process.specific_energy());
    let maximum_useful_charge = required_energy
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
        .checked_sub(required_energy.nanojoules())
        .unwrap_or_else(|| {
            panic!(
                "primitive progression charge target must leave useful work beyond the primary batch"
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
    let charge_energy = required_energy
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
    let stone_power = validate_start_manual_power(
        registries,
        state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, charge_energy),
    )
    .unwrap_or_else(|error| panic!("primitive progression stone-crank projection failed: {error}"));
    let stone_charge_ticks = duration(
        stone_power.work().started_at().value(),
        stone_power.work().completes_at().value(),
    );

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

    let power = validate_start_manual_power(
        registries,
        state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, charge_energy),
    )
    .unwrap_or_else(|error| panic!("primitive progression manual charging failed: {error}"));
    let charge_work = power.work();
    let reinforced_charge_ticks = duration(
        charge_work.started_at().value(),
        charge_work.completes_at().value(),
    );
    power
        .commit(state)
        .unwrap_or_else(|error| panic!("primitive progression charge commit failed: {error}"));
    advance_exact(registries, state, reinforced_charge_ticks);
    assert!(
        reinforced_charge_ticks < stone_charge_ticks,
        "the maintained primitive charge must be large enough for copper crank reinforcement to save player-attention time"
    );
    assert_eq!(
        state.energy().get_store(drive).map(|store| store.stored()),
        Some(charge_energy),
        "reinforcement may change charging rate but not the requested stored work"
    );

    PrimitiveMachine {
        crusher,
        drive,
        required_energy,
        charge_energy,
        reserve_mass,
        charge_fill_ppm,
        stone_charge_ticks,
        reinforced_charge_ticks,
    }
}

#[derive(Clone, Copy)]
struct ConcurrentMachineWork {
    job: ProductionJobId,
    machine_started_at: u64,
    crush_ticks: u64,
    player_work_ticks: u64,
    overlap_ticks: u64,
}

#[derive(Clone, Copy)]
struct ConcurrentMiningPlan {
    deposit: deep_hearth::geology::GeologicalDepositId,
    destination: deep_hearth::inventory::StockpileId,
    pick: deep_hearth::equipment::EquipmentId,
    mass: Mass,
}

fn crush_while_mining(
    registries: &Registries,
    state: &mut AppState,
    ore_storage: deep_hearth::inventory::StockpileId,
    crushed_storage: deep_hearth::inventory::StockpileId,
    machine: PrimitiveMachine,
    mined_mass: Mass,
    concurrent: ConcurrentMiningPlan,
) -> ConcurrentMachineWork {
    let machine_started_at = state.tick().value();
    let ore_lot = state
        .inventory()
        .lot_ids(ore_storage)
        .find(|lot| {
            state
                .inventory()
                .get_lot(*lot)
                .is_some_and(|record| record.mass() >= mined_mass)
        })
        .unwrap_or_else(|| panic!("primitive progression claimed ore lot disappeared"));
    let selection = [MaterialLotSelection::new(ore_lot, mined_mass)];
    let resolved = resolve_comminution_process(
        registries,
        state,
        ComminutionRequest::new(
            PROCESS_CRUSH_ORE,
            ore_storage,
            &selection,
            machine.crusher,
            machine.drive,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive progression crushing resolution failed: {error}"));
    assert_eq!(resolved.required_energy(), machine.required_energy);
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

    let concurrent_mining = validate_start_mining(
        registries,
        state,
        MINING_METHOD_HAND_PICK,
        concurrent.deposit,
        concurrent.destination,
        concurrent.pick,
        concurrent.mass,
    )
    .unwrap_or_else(|error| {
        panic!("primitive progression concurrent mining admission failed: {error}")
    });
    let concurrent_mining_job = concurrent_mining.commit(state).unwrap_or_else(|error| {
        panic!("primitive progression concurrent mining commit failed: {error}")
    });
    let player_work_ticks = state
        .mining()
        .get_job(concurrent_mining_job)
        .map(|record| duration(record.started_at().value(), record.completes_at().value()))
        .unwrap_or_else(|| panic!("primitive progression concurrent mining job disappeared"));
    assert!(
        state.production().get_job(crush_job).is_some()
            && state.mining().get_job(concurrent_mining_job).is_some()
            && state.player_work().active().is_some(),
        "autonomous crushing and player mining must coexist after both canonical starts"
    );
    let overlap_ticks = crush_ticks.min(player_work_ticks);
    let overlap_witness_ticks = overlap_ticks.saturating_sub(1);
    if overlap_witness_ticks > 0 {
        advance_exact(registries, state, overlap_witness_ticks);
        assert!(
            state.production().get_job(crush_job).is_some()
                && state.mining().get_job(concurrent_mining_job).is_some()
                && state.player_work().active().is_some(),
            "machine production and player mining must remain independently active during their shared interval"
        );
    }
    advance_exact(registries, state, player_work_ticks - overlap_witness_ticks);
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

    ConcurrentMachineWork {
        job: crush_job,
        machine_started_at,
        crush_ticks,
        player_work_ticks,
        overlap_ticks,
    }
}

fn finish_primary_crush(
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
    let idle_wait_ticks = job
        .completes_at()
        .value()
        .checked_sub(state.tick().value())
        .unwrap_or_else(|| panic!("primitive crusher completion fell behind authoritative time"));
    advance_exact(registries, state, idle_wait_ticks);
    assert!(
        state.production().get_job(concurrent.job).is_none(),
        "primitive crusher should complete after its remaining autonomous work"
    );
    idle_wait_ticks
}

fn spend_reserved_crusher_work(
    registries: &Registries,
    state: &mut AppState,
    ore_storage: deep_hearth::inventory::StockpileId,
    crushed_storage: deep_hearth::inventory::StockpileId,
    machine: PrimitiveMachine,
) -> u64 {
    if machine.reserve_mass.is_zero() {
        return 0;
    }
    let ore_lot = state
        .inventory()
        .lot_ids(ore_storage)
        .find(|lot| {
            state
                .inventory()
                .get_lot(*lot)
                .is_some_and(|record| record.mass() >= machine.reserve_mass)
        })
        .unwrap_or_else(|| {
            panic!("primitive progression banked work has no remaining ore batch to process")
        });
    let selection = [MaterialLotSelection::new(ore_lot, machine.reserve_mass)];
    let resolved = resolve_comminution_process(
        registries,
        state,
        ComminutionRequest::new(
            PROCESS_CRUSH_ORE,
            ore_storage,
            &selection,
            machine.crusher,
            machine.drive,
        ),
    )
    .unwrap_or_else(|error| {
        panic!("primitive progression reserve crushing resolution failed: {error}")
    });
    let remaining = state
        .energy()
        .get_store(machine.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("primitive progression flywheel disappeared before reserve use"));
    assert_eq!(
        resolved.required_energy(),
        remaining,
        "player-banked primitive work should map exactly to the selected follow-up batch"
    );
    let ticks = resolved.process_resolution().duration().value();
    validate_start_process(
        registries,
        state,
        resolved.process_resolution(),
        ore_storage,
        crushed_storage,
    )
    .unwrap_or_else(|error| panic!("primitive progression reserve crushing start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| {
        panic!("primitive progression reserve crushing commit failed: {error}")
    });
    advance_exact(registries, state, ticks);
    assert_eq!(
        state
            .energy()
            .get_store(machine.drive)
            .map(|store| store.stored()),
        Some(Energy::ZERO),
        "using the intentionally banked follow-up batch should leave no stranded flywheel work"
    );
    ticks
}

fn run_primitive_progression_case(
    registries: &Registries,
    seed: u64,
    priority: PrimitivePriority,
    emit_detail: bool,
) -> PrimitiveProgressionExperience {
    let mined_mass = progression_mining_mass(registries, seed);
    let two_mining_batches = mined_mass
        .checked_add(mined_mass)
        .unwrap_or_else(|| panic!("primitive progression ore fixture mass overflowed"));
    let ore_total = two_mining_batches
        .checked_add(mined_mass)
        .unwrap_or_else(|| panic!("primitive progression ore fixture mass overflowed"));
    let ore_surplus = Mass::from_milligrams(
        mined_mass.milligrams().div_ceil(2)
            + mix64(seed ^ 0x4F52_455F_5355_5250) % (mined_mass.milligrams() + 1),
    );
    let ore_deposit_mass = ore_total
        .checked_add(ore_surplus)
        .unwrap_or_else(|| panic!("primitive progression ore reserve mass overflowed"));
    let ore_copper_ppm = 450_000 + (mix64(seed ^ 0x5052_4F47_4752_4144) % 300_001) as u32;
    let PrimitiveMaterialPlan {
        raw_inputs,
        raw_capacity,
        shaped_capacity,
        native_copper,
    } = primitive_material_plan(registries);
    let raw_seed_inputs = raw_inputs
        .into_iter()
        .enumerate()
        .map(|(index, (commodity, required))| {
            let maximum_extra = required.milligrams().div_ceil(2).max(1);
            let extra = Mass::from_milligrams(
                1 + mix64(seed ^ 0x5241_575F_5355_5250 ^ index as u64) % maximum_extra,
            );
            let seeded = required
                .checked_add(extra)
                .unwrap_or_else(|| panic!("primitive progression raw-material surplus overflowed"));
            (commodity, seeded)
        })
        .collect::<Vec<_>>();
    let raw_seed_capacity = raw_seed_inputs
        .iter()
        .try_fold(Mass::ZERO, |total, (_, mass)| total.checked_add(*mass))
        .unwrap_or_else(|| panic!("primitive progression raw-material capacity overflowed"));
    let raw_surplus = raw_seed_capacity
        .checked_sub(raw_capacity)
        .unwrap_or_else(|| unreachable!("seeded raw material includes every required input"));
    let stone_pick_batch_limit = stone_pick_mining_batch_limit(registries);
    let pick_upgrade_native =
        native_input_for_upgrade(registries, EQUIPMENT_COPPER_REINFORCED_PICK);
    let crank_upgrade_native =
        native_input_for_upgrade(registries, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK);
    assert_eq!(
        pick_upgrade_native
            .checked_add(crank_upgrade_native)
            .unwrap_or_else(|| panic!("primitive native-copper upgrade requirement overflowed")),
        native_copper,
        "primitive material plan must expose exactly the native copper consumed by both upgrade choices"
    );
    let native_surplus = Mass::from_milligrams(
        1 + mix64(seed ^ 0x4E41_5449_5645_5355) % native_copper.milligrams().max(1),
    );
    let native_deposit_mass = native_copper
        .checked_add(native_surplus)
        .unwrap_or_else(|| panic!("primitive progression native-copper reserve overflowed"));

    let mut state = AppState::new(WorldSeed::new(seed));
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("primitive progression survival setup failed: {error}"));
    let raw = add_solid_stockpile(&mut state, raw_seed_capacity, "primitive raw materials");
    let shaped = add_solid_stockpile(&mut state, shaped_capacity, "primitive shaped materials");
    let ore_storage = add_solid_stockpile(&mut state, ore_total, "primitive mined ore");
    let native_storage = add_solid_stockpile(&mut state, native_copper, "primitive native copper");
    let crushed_storage = add_solid_stockpile(&mut state, ore_total, "primitive crushed ore");
    for (commodity, mass) in raw_seed_inputs {
        seed_lot(
            registries,
            &mut state,
            raw,
            commodity,
            mass,
            ROOM_TEMPERATURE,
        );
    }
    let ore_bounds = VoxelBounds::new(VoxelCoord::new(0, -4, 0), VoxelCoord::new(1, -3, 1))
        .unwrap_or_else(|error| panic!("primitive progression deposit bounds failed: {error}"));
    let ore_composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, ore_copper_ppm),
        CompositionComponent::new(MATERIAL_STONE, 1_000_000 - ore_copper_ppm),
    ])
    .unwrap_or_else(|error| panic!("primitive progression ore composition failed: {error}"));
    let ore_deposit = seed_geological_deposit(
        registries,
        &mut state,
        geological_deposit_spec(
            ore_bounds,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            ore_deposit_mass,
            Temperature::from_millikelvin(293_150),
            Pressure::from_pascals(450_000_000),
            ore_composition,
        ),
    );
    let native_bounds = VoxelBounds::new(VoxelCoord::new(2, -4, 0), VoxelCoord::new(3, -3, 1))
        .unwrap_or_else(|error| panic!("primitive native-copper bounds failed: {error}"));
    let native_deposit = seed_geological_deposit(
        registries,
        &mut state,
        geological_deposit_spec(
            native_bounds,
            CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
            native_deposit_mass,
            Temperature::from_millikelvin(293_150),
            Pressure::from_pascals(350_000_000),
            MaterialComposition::pure(MATERIAL_COPPER),
        ),
    );
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| {
            panic!("primitive progression initial matter audit failed: {error}")
        })
        .total();
    let survival_before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("primitive progression survival state disappeared"));

    let pottery = CommodityKey::new(MATERIAL_CLAY, FORM_UNFIRED_POTTERY);
    let pottery_definition = registries
        .crafting()
        .get_manual(PROCESS_FORM_CLAY_VESSEL)
        .unwrap_or_else(|| panic!("primitive progression clay-vessel action disappeared"));
    let pottery_mass = output_mass_per_batch(pottery_definition, pottery);
    craft_batches(
        registries,
        &mut state,
        PROCESS_FORM_CLAY_VESSEL,
        raw,
        shaped,
        1,
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(shaped)
            .map(|stockpile| stockpile.get_mass(pottery)),
        Some(pottery_mass),
        "the authored clay-vessel side craft must be reachable through the normal manual-crafting transaction"
    );

    craft_for_profile(
        registries,
        &mut state,
        raw,
        native_storage,
        shaped,
        equipment_assembly_profile(registries, EQUIPMENT_STONE_PICK),
    );
    let pick = validate_assemble_equipment(registries, &state, EQUIPMENT_STONE_PICK, shaped)
        .unwrap_or_else(|error| panic!("primitive progression pick assembly failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("primitive progression pick assembly commit failed: {error}")
        });

    let stone_mining_ticks = mine_and_claim(
        registries,
        &mut state,
        ore_deposit,
        ore_storage,
        pick,
        mined_mass,
    );
    let (first_native_mass, second_native_mass) = match priority {
        PrimitivePriority::ExtractionFirst => (pick_upgrade_native, crank_upgrade_native),
        PrimitivePriority::MechanizationFirst => (crank_upgrade_native, pick_upgrade_native),
    };
    let first_native_mining_ticks = mine_total_and_claim(
        registries,
        &mut state,
        native_deposit,
        native_storage,
        pick,
        first_native_mass,
        stone_pick_batch_limit,
    );
    let (
        machine,
        second_native_mining_ticks,
        reinforced_mining_ticks,
        third_ore_mining_ticks,
        concurrent_work,
        concurrent_task,
        pick_upgraded_at,
        reinforced_ore_acquired_at,
    ) = match priority {
        PrimitivePriority::ExtractionFirst => {
            reinforce_pick(registries, &mut state, raw, native_storage, shaped, pick);
            let pick_upgraded_at = state.tick().value();
            let reinforced_mining_ticks = mine_and_claim(
                registries,
                &mut state,
                ore_deposit,
                ore_storage,
                pick,
                mined_mass,
            );
            let reinforced_ore_acquired_at = state.tick().value();
            let second_native_mining_ticks = mine_total_and_claim(
                registries,
                &mut state,
                native_deposit,
                native_storage,
                pick,
                second_native_mass,
                stone_pick_batch_limit,
            );
            let machine = build_and_charge_primitive_machine(
                registries,
                &mut state,
                raw,
                native_storage,
                shaped,
                mined_mass,
                seed,
            );
            let concurrent_work = crush_while_mining(
                registries,
                &mut state,
                ore_storage,
                crushed_storage,
                machine,
                mined_mass,
                ConcurrentMiningPlan {
                    deposit: ore_deposit,
                    destination: ore_storage,
                    pick,
                    mass: mined_mass,
                },
            );
            (
                machine,
                second_native_mining_ticks,
                reinforced_mining_ticks,
                concurrent_work.player_work_ticks,
                concurrent_work,
                "ore",
                pick_upgraded_at,
                reinforced_ore_acquired_at,
            )
        }
        PrimitivePriority::MechanizationFirst => {
            let machine = build_and_charge_primitive_machine(
                registries,
                &mut state,
                raw,
                native_storage,
                shaped,
                mined_mass,
                seed,
            );
            let concurrent_work = crush_while_mining(
                registries,
                &mut state,
                ore_storage,
                crushed_storage,
                machine,
                mined_mass,
                ConcurrentMiningPlan {
                    deposit: native_deposit,
                    destination: native_storage,
                    pick,
                    mass: second_native_mass,
                },
            );
            let second_native_mining_ticks = concurrent_work.player_work_ticks;
            reinforce_pick(registries, &mut state, raw, native_storage, shaped, pick);
            let pick_upgraded_at = state.tick().value();
            let reinforced_mining_ticks = mine_and_claim(
                registries,
                &mut state,
                ore_deposit,
                ore_storage,
                pick,
                mined_mass,
            );
            let reinforced_ore_acquired_at = state.tick().value();
            let third_ore_mining_ticks = mine_and_claim(
                registries,
                &mut state,
                ore_deposit,
                ore_storage,
                pick,
                mined_mass,
            );
            (
                machine,
                second_native_mining_ticks,
                reinforced_mining_ticks,
                third_ore_mining_ticks,
                concurrent_work,
                "native-copper",
                pick_upgraded_at,
                reinforced_ore_acquired_at,
            )
        }
    };
    let primary_idle_wait_ticks = finish_primary_crush(registries, &mut state, concurrent_work);
    let machine_useful_overlap_ticks = concurrent_work
        .crush_ticks
        .checked_sub(primary_idle_wait_ticks)
        .unwrap_or_else(|| panic!("primitive machine idle wait exceeded its active process time"));
    assert!(
        reinforced_mining_ticks < stone_mining_ticks,
        "the maintained primitive mining batch must be large enough for copper pick reinforcement to save player-attention time"
    );
    match priority {
        PrimitivePriority::ExtractionFirst => assert!(
            pick_upgraded_at < concurrent_work.machine_started_at,
            "extraction-first priority must improve extraction before starting autonomous work"
        ),
        PrimitivePriority::MechanizationFirst => assert!(
            concurrent_work.machine_started_at < pick_upgraded_at,
            "mechanization-first priority must start autonomous work before improving extraction"
        ),
    }

    let banked_energy = state
        .energy()
        .get_store(machine.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| {
            panic!("primitive progression flywheel disappeared after primary crushing")
        });
    assert_eq!(
        banked_energy,
        machine
            .charge_energy
            .checked_sub(machine.required_energy)
            .unwrap_or_else(|| unreachable!("charge is bounded below by required crusher energy")),
        "primary crushing must preserve the player's intentionally banked follow-up work"
    );
    let reserve_crush_ticks = spend_reserved_crusher_work(
        registries,
        &mut state,
        ore_storage,
        crushed_storage,
        machine,
    );
    let drive_remaining = state
        .energy()
        .get_store(machine.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| {
            panic!("primitive progression flywheel disappeared after reserve crushing")
        });
    assert_eq!(drive_remaining, Energy::ZERO);
    let survival_after = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("primitive progression final survival state disappeared"));
    let unmined_ore_reserve = ore_deposit_mass.checked_sub(ore_total).unwrap_or_else(|| {
        unreachable!("ore world fixture exceeds the actor's planned extraction")
    });
    let unmined_native_reserve = native_deposit_mass
        .checked_sub(native_copper)
        .unwrap_or_else(|| {
            unreachable!("native-copper world fixture exceeds the actor's required extraction")
        });
    assert!(!unmined_ore_reserve.is_zero() && !unmined_native_reserve.is_zero());
    assert!(survival_after.metabolic_energy() < survival_before.metabolic_energy());
    assert!(survival_after.hydration() < survival_before.hydration());
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!(
                "primitive progression final matter audit failed: {error}"
            ))
            .total(),
        matter_before
    );
    let processed_mass = mined_mass
        .checked_add(machine.reserve_mass)
        .unwrap_or_else(|| panic!("primitive progression processed mass overflowed"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(crushed_storage)
            .unwrap_or_else(|| panic!("primitive progression crushed storage disappeared"))
            .stored_mass(),
        processed_mass
    );
    assert_eq!(state.player_work().active(), None);
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("primitive progression persistence audit failed: {error}"));

    let drive_mass = state
        .energy()
        .get_store(machine.drive)
        .unwrap_or_else(|| panic!("primitive progression constructed drive disappeared"))
        .embodied_mass();
    let crusher_mass = state
        .equipment()
        .get_equipment(machine.crusher)
        .unwrap_or_else(|| panic!("primitive progression constructed crusher disappeared"))
        .embodied_mass();
    let final_pick_condition_ppm = state
        .equipment()
        .get_equipment(pick)
        .unwrap_or_else(|| panic!("primitive progression final pick disappeared"))
        .condition()
        .parts_per_million();
    let metabolic_energy_spent_nj = survival_before.metabolic_energy().nanojoules()
        - survival_after.metabolic_energy().nanojoules();
    let hydration_spent_ul =
        survival_before.hydration().microliters() - survival_after.hydration().microliters();
    let physiology = registries.survival().physiology();
    let experience = PrimitiveProgressionExperience {
        priority,
        pick_upgraded_at,
        reinforced_ore_acquired_at,
        machine_started_at: concurrent_work.machine_started_at,
        first_processed_output_at: concurrent_work
            .machine_started_at
            .checked_add(concurrent_work.crush_ticks)
            .unwrap_or_else(|| panic!("primitive processed-output milestone overflowed")),
        elapsed_ticks: state.tick().value(),
        stone_ore_mining_ticks: stone_mining_ticks,
        first_native_mining_ticks,
        second_native_mining_ticks,
        reinforced_ore_mining_ticks: reinforced_mining_ticks,
        final_ore_mining_ticks: third_ore_mining_ticks,
        stone_charge_ticks: machine.stone_charge_ticks,
        reinforced_charge_ticks: machine.reinforced_charge_ticks,
        machine_work_ticks: concurrent_work.crush_ticks,
        overlap_ticks: concurrent_work.overlap_ticks,
        machine_useful_overlap_ticks,
        machine_idle_wait_ticks: primary_idle_wait_ticks,
        final_pick_condition_ppm,
        metabolic_energy_spent_nj,
        hydration_spent_ul,
    };
    let (first_upgrade, second_upgrade) = match priority {
        PrimitivePriority::ExtractionFirst => ("pick", "hand-crank"),
        PrimitivePriority::MechanizationFirst => ("hand-crank", "pick"),
    };

    if emit_detail {
        std::println!(
            "PLAYABLE PROGRESSION seed=0x{seed:016X} priority={} world-bootstrap=[raw-gathered-matter-surplus:{}mg,preauthorized-ore-site-identity,preauthorized-native-copper-site-identity,empty-storage] discovery=not-modeled manual-sidecraft=[unfired-pottery:{}mg] canonical=shape->assemble->mine->choose-upgrade->store-work->manual-power->assemble-machine->autonomous-crush fantasy=survive->craft-tools->extract->choose-where-scarce-copper-matters->mechanize->work-in-parallel choice=[first:{}:{}mg second:{}:{}mg] milestones=[pick-upgrade:{}t machine-start:{}t] ore=[grade:{}ppm:composition-only batch:{}mg initial-stone:{}t post-upgrade:{}t final:{}t remaining:{}mg] native=[first:{}t second:{}t total:{}mg remaining:{}mg] infrastructure=[drive:{}mg crusher:{}mg] stored-work=[fill:{}ppm charge:{}nJ primary:{}nJ banked:{}nJ follow-up:{}mg:{}t final:{}nJ] charge=[stone:{}t reinforced:{}t saved:{}t] mechanization=[primary-crush:{}t initial-concurrent:{}:{}t initial-overlap:{}t useful-overlap:{}t idle-wait:{}t processed:{}mg] durability=[pick:{}ppm] survival=[spent:{}nJ/{}uL remaining:{}nJ/{}uL warning:{}nJ/{}uL state:{:?}/{:?} elapsed:{}t] matter=conserved",
            priority.label(),
            raw_surplus.milligrams(),
            pottery_mass.milligrams(),
            first_upgrade,
            first_native_mass.milligrams(),
            second_upgrade,
            second_native_mass.milligrams(),
            pick_upgraded_at,
            concurrent_work.machine_started_at,
            ore_copper_ppm,
            mined_mass.milligrams(),
            stone_mining_ticks,
            reinforced_mining_ticks,
            third_ore_mining_ticks,
            unmined_ore_reserve.milligrams(),
            first_native_mining_ticks,
            second_native_mining_ticks,
            native_copper.milligrams(),
            unmined_native_reserve.milligrams(),
            drive_mass.milligrams(),
            crusher_mass.milligrams(),
            machine.charge_fill_ppm,
            machine.charge_energy.nanojoules(),
            machine.required_energy.nanojoules(),
            banked_energy.nanojoules(),
            machine.reserve_mass.milligrams(),
            reserve_crush_ticks,
            drive_remaining.nanojoules(),
            machine.stone_charge_ticks,
            machine.reinforced_charge_ticks,
            machine.stone_charge_ticks - machine.reinforced_charge_ticks,
            concurrent_work.crush_ticks,
            concurrent_task,
            concurrent_work.player_work_ticks,
            concurrent_work.overlap_ticks,
            machine_useful_overlap_ticks,
            primary_idle_wait_ticks,
            processed_mass.milligrams(),
            final_pick_condition_ppm,
            metabolic_energy_spent_nj,
            hydration_spent_ul,
            survival_after.metabolic_energy().nanojoules(),
            survival_after.hydration().microliters(),
            physiology.hungry_below().nanojoules(),
            physiology.thirsty_below().microliters(),
            survival_after.hunger(),
            survival_after.hydration_state(),
            state.tick().value(),
        );
    }
    experience
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PrimitiveProgressionReview {
    pub(super) tool_attention_saved_ticks: u64,
    pub(super) crank_attention_saved_ticks: u64,
    pub(super) extraction_ore_lead_ticks: u64,
    pub(super) mechanization_output_lead_ticks: u64,
    pub(super) machine_work_ticks: u64,
    pub(super) mechanization_useful_overlap_ticks: u64,
    pub(super) mechanization_idle_wait_saved_ticks: u64,
    pub(super) mechanization_elapsed_saved_ticks: u64,
}

pub(super) fn evaluate_primitive_progression_probe(
    registries: &Registries,
    seed: u64,
) -> PrimitiveProgressionReview {
    assert_playable_catalog_coverage(registries);
    let selected_priority = primitive_priority(seed);
    let selected = run_primitive_progression_case(registries, seed, selected_priority, true);
    let alternative =
        run_primitive_progression_case(registries, seed, selected_priority.opposite(), false);
    let (extraction, mechanization) = match selected.priority {
        PrimitivePriority::ExtractionFirst => (selected, alternative),
        PrimitivePriority::MechanizationFirst => (alternative, selected),
    };
    assert!(
        extraction.pick_upgraded_at < mechanization.pick_upgraded_at,
        "extraction-first must deliver the pick upgrade earlier on the same world"
    );
    assert!(
        extraction.reinforced_ore_acquired_at < mechanization.reinforced_ore_acquired_at,
        "extraction-first must turn its early tool upgrade into useful mined ore sooner"
    );
    assert!(
        mechanization.machine_started_at < extraction.machine_started_at,
        "mechanization-first must deliver autonomous work earlier on the same world"
    );
    for experience in [extraction, mechanization] {
        assert!(
            experience.stone_ore_mining_ticks > experience.reinforced_ore_mining_ticks,
            "primitive copper pick reinforcement must save direct extraction attention in the maintained gameplay episode"
        );
        assert!(
            experience.stone_charge_ticks > experience.reinforced_charge_ticks,
            "primitive copper crank reinforcement must save direct charging attention in the maintained gameplay episode"
        );
    }
    assert_eq!(
        extraction.machine_work_ticks, mechanization.machine_work_ticks,
        "matched-world upgrade priorities must compare the same autonomous crusher workload"
    );
    assert!(
        mechanization.machine_useful_overlap_ticks > extraction.machine_useful_overlap_ticks,
        "mechanization-first must hide more autonomous crusher time behind useful player work"
    );
    assert!(
        mechanization.machine_idle_wait_ticks < extraction.machine_idle_wait_ticks,
        "mechanization-first must leave less primary-machine idle waiting after useful player work"
    );
    assert!(
        mechanization.elapsed_ticks < extraction.elapsed_ticks,
        "earlier mechanization must create a measurable elapsed-time payoff by freeing player attention"
    );
    std::println!(
        "PROGRESSION AGENCY seed=0x{seed:016X} matched-world choices=[extraction-first,mechanization-first] milestones=[pick-upgrade:{}vs{}t reinforced-ore:{}vs{}t machine-start:{}vs{}t first-processed-output:{}vs{}t] tool=[stone:{}t reinforced:{}vs{}t] charge=[stone:{}vs{}t reinforced:{}vs{}t] native=[first:{}vs{}t second:{}vs{}t] ore-after-upgrade=[{}+{}vs{}+{}t] attention=[machine:{}t initial-overlap:{}vs{}t useful-overlap:{}vs{}t idle-wait:{}vs{}t] final-pick=[{}vs{}ppm] survival=[energy:{}vs{}nJ hydration:{}vs{}uL] elapsed=[{}vs{}t] tradeoff=earlier-useful-extraction-vs-earlier-autonomy+attention-recovery",
        extraction.pick_upgraded_at,
        mechanization.pick_upgraded_at,
        extraction.reinforced_ore_acquired_at,
        mechanization.reinforced_ore_acquired_at,
        extraction.machine_started_at,
        mechanization.machine_started_at,
        extraction.first_processed_output_at,
        mechanization.first_processed_output_at,
        extraction.stone_ore_mining_ticks,
        extraction.reinforced_ore_mining_ticks,
        mechanization.reinforced_ore_mining_ticks,
        extraction.stone_charge_ticks,
        mechanization.stone_charge_ticks,
        extraction.reinforced_charge_ticks,
        mechanization.reinforced_charge_ticks,
        extraction.first_native_mining_ticks,
        mechanization.first_native_mining_ticks,
        extraction.second_native_mining_ticks,
        mechanization.second_native_mining_ticks,
        extraction.reinforced_ore_mining_ticks,
        extraction.final_ore_mining_ticks,
        mechanization.reinforced_ore_mining_ticks,
        mechanization.final_ore_mining_ticks,
        extraction.machine_work_ticks,
        extraction.overlap_ticks,
        mechanization.overlap_ticks,
        extraction.machine_useful_overlap_ticks,
        mechanization.machine_useful_overlap_ticks,
        extraction.machine_idle_wait_ticks,
        mechanization.machine_idle_wait_ticks,
        extraction.final_pick_condition_ppm,
        mechanization.final_pick_condition_ppm,
        extraction.metabolic_energy_spent_nj,
        mechanization.metabolic_energy_spent_nj,
        extraction.hydration_spent_ul,
        mechanization.hydration_spent_ul,
        extraction.elapsed_ticks,
        mechanization.elapsed_ticks,
    );
    let review = PrimitiveProgressionReview {
        tool_attention_saved_ticks: extraction
            .stone_ore_mining_ticks
            .checked_sub(extraction.reinforced_ore_mining_ticks)
            .unwrap_or_else(|| unreachable!("reinforced pick is already required to be faster")),
        crank_attention_saved_ticks: extraction
            .stone_charge_ticks
            .checked_sub(extraction.reinforced_charge_ticks)
            .unwrap_or_else(|| unreachable!("reinforced crank is already required to be faster")),
        extraction_ore_lead_ticks: mechanization
            .reinforced_ore_acquired_at
            .checked_sub(extraction.reinforced_ore_acquired_at)
            .unwrap_or_else(|| unreachable!("extraction-first already wins the ore milestone")),
        mechanization_output_lead_ticks: extraction
            .first_processed_output_at
            .checked_sub(mechanization.first_processed_output_at)
            .unwrap_or_else(|| unreachable!("mechanization-first already wins processed output")),
        machine_work_ticks: mechanization.machine_work_ticks,
        mechanization_useful_overlap_ticks: mechanization.machine_useful_overlap_ticks,
        mechanization_idle_wait_saved_ticks: extraction
            .machine_idle_wait_ticks
            .checked_sub(mechanization.machine_idle_wait_ticks)
            .unwrap_or_else(|| unreachable!("mechanization-first already waits less")),
        mechanization_elapsed_saved_ticks: extraction
            .elapsed_ticks
            .checked_sub(mechanization.elapsed_ticks)
            .unwrap_or_else(|| unreachable!("mechanization-first already finishes sooner")),
    };
    std::println!(
        "PROGRESSION REVIEW evidence=[tool-attention-saved:{}t crank-attention-saved:{}t extraction-ore-lead:{}t mechanization-output-lead:{}t autonomous-work:{}t useful-overlap:{}t idle-wait-saved:{}t elapsed-saved:{}t]",
        review.tool_attention_saved_ticks,
        review.crank_attention_saved_ticks,
        review.extraction_ore_lead_ticks,
        review.mechanization_output_lead_ticks,
        review.machine_work_ticks,
        review.mechanization_useful_overlap_ticks,
        review.mechanization_idle_wait_saved_ticks,
        review.mechanization_elapsed_saved_ticks,
    );
    review
}

pub(super) fn run_primitive_progression_probe(registries: &Registries, seed: u64) {
    let _ = evaluate_primitive_progression_probe(registries, seed);
}
