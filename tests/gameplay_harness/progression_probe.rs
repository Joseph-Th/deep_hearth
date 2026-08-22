//! Canonical primitive-to-mechanized progression probe for the gameplay experience harness.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;

use super::seed::mix64;
use super::support::{ROOM_TEMPERATURE, add_solid_stockpile, nominal_equipment_mass_capability};
use deep_hearth::capability::{CapabilityId, CapabilityValue};
use deep_hearth::content::gameplay_fixture::{
    geological_deposit_spec, seed_geological_deposit, seed_lot,
};
use deep_hearth::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_STONE_CRUSHER, EQUIPMENT_STONE_HAND_CRANK,
    EQUIPMENT_STONE_PICK, FORM_CRUSHED, FORM_NATIVE_METAL, FORM_ORE, MANUAL_POWER_HAND_CRANK,
    MATERIAL_COPPER, MATERIAL_STONE, MINING_METHOD_HAND_PICK,
    PROCESS_COLD_WORK_COPPER_REINFORCEMENT, PROCESS_CRUSH_ORE, PROCESS_FORM_CLAY_VESSEL,
    PROCESS_KNAP_STONE_TOOL, PROCESS_SHAPE_STONE_FLYWHEEL, PROCESS_SHAPE_WOOD_HANDLE,
};
use deep_hearth::core::quantity::{Energy, Mass, Power, Pressure};
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
use deep_hearth::mining::{MiningStartError, validate_claim_mining_output, validate_start_mining};
use deep_hearth::ore_processing::{ComminutionRequest, resolve_comminution_process};
use deep_hearth::production::{ProductionJobId, validate_start_process};
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};
use deep_hearth::survival::{assess_survival, initialize_player_survival};

/// Fail closed when the authored player-facing acquisition/action catalog grows beyond what this
/// cold-agent progression episode either exercises naturally or explicitly classifies outside the
/// episode fantasy.
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
    let natural_manual_processes = BTreeSet::from([
        PROCESS_KNAP_STONE_TOOL.value(),
        PROCESS_SHAPE_WOOD_HANDLE.value(),
        PROCESS_SHAPE_STONE_FLYWHEEL.value(),
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT.value(),
    ]);
    let intentionally_out_of_episode = BTreeSet::from([PROCESS_FORM_CLAY_VESSEL.value()]);
    let accounted_manual_processes = natural_manual_processes
        .union(&intentionally_out_of_episode)
        .copied()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual_manual_processes, accounted_manual_processes,
        "cold-agent progression catalog partition is stale: classify new manual actions as natural progression work or explicitly out-of-episode instead of forcing unrelated actions into the player loop"
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
    let pick_additions = equipment_upgrade_additions(registries, EQUIPMENT_COPPER_REINFORCED_PICK);
    let crank_additions =
        equipment_upgrade_additions(registries, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK);
    assert_eq!(
        pick_additions.inputs(),
        crank_additions.inputs(),
        "primitive progression competing copper upgrades must consume the same reinforcement parcel"
    );
    add_profile_requirements(&mut requirements, pick_additions);

    let mut process_batches: BTreeMap<deep_hearth::production::ProcessId, u64> = BTreeMap::new();
    for (commodity, required) in requirements {
        let craft = manual_craft_for_output(registries, commodity);
        let batches = batches_for_output(required, output_mass_per_batch(craft, commodity));
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
    primary_batch_mass: Mass,
    pick_upgraded_at: Option<u64>,
    hard_seam_accessed_at: Option<u64>,
    machine_started_at: u64,
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
    hard_ore_mined: Mass,
    total_ore_mined: Mass,
    native_copper_remaining: Mass,
    crank_reinforced: bool,
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
    crank: deep_hearth::equipment::EquipmentId,
    crusher: deep_hearth::equipment::EquipmentId,
    drive: deep_hearth::energy::EnergyStoreId,
    drive_capacity: Energy,
    required_energy: Energy,
    charge_energy: Energy,
    reserve_mass: Mass,
    charge_fill_ppm: u32,
    charge_ticks: u64,
    crank_reinforced: bool,
}

fn build_and_charge_primitive_machine(
    registries: &Registries,
    state: &mut AppState,
    raw: deep_hearth::inventory::StockpileId,
    native_storage: deep_hearth::inventory::StockpileId,
    shaped: deep_hearth::inventory::StockpileId,
    mined_mass: Mass,
    seed: u64,
    reinforce_crank: bool,
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
    let maximum_follow_up_mass = mined_mass;
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
    if reinforce_crank {
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
    }

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
    let charge_ticks = duration(
        charge_work.started_at().value(),
        charge_work.completes_at().value(),
    );
    power
        .commit(state)
        .unwrap_or_else(|error| panic!("primitive progression charge commit failed: {error}"));
    advance_exact(registries, state, charge_ticks);
    assert_eq!(
        state.energy().get_store(drive).map(|store| store.stored()),
        Some(charge_energy),
        "primitive charging must deliver the requested finite stored work"
    );

    PrimitiveMachine {
        crank,
        crusher,
        drive,
        drive_capacity,
        required_energy,
        charge_energy,
        reserve_mass,
        charge_fill_ppm,
        charge_ticks,
        crank_reinforced: reinforce_crank,
    }
}

fn recharge_primitive_machine(
    registries: &Registries,
    state: &mut AppState,
    machine: PrimitiveMachine,
    energy: Energy,
) -> u64 {
    assert_eq!(
        state
            .energy()
            .get_store(machine.drive)
            .map(|store| store.stored()),
        Some(Energy::ZERO),
        "primitive recharge begins only after the prior stored-work cycle is exhausted"
    );
    let power = validate_start_manual_power(
        registries,
        state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            machine.crank,
            machine.drive,
            energy,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive progression recharge failed: {error}"));
    let work = power.work();
    let ticks = duration(work.started_at().value(), work.completes_at().value());
    power
        .commit(state)
        .unwrap_or_else(|error| panic!("primitive progression recharge commit failed: {error}"));
    advance_exact(registries, state, ticks);
    assert_eq!(
        state
            .energy()
            .get_store(machine.drive)
            .map(|store| store.stored()),
        Some(energy),
        "primitive recharge must restore exactly the requested stored work"
    );
    ticks
}

fn crush_autonomous_batch(
    registries: &Registries,
    state: &mut AppState,
    ore_storage: deep_hearth::inventory::StockpileId,
    crushed_storage: deep_hearth::inventory::StockpileId,
    machine: PrimitiveMachine,
    mass: Mass,
    expected_energy: Energy,
) -> u64 {
    assert_eq!(state.player_work().active(), None);
    let selection = select_stockpile_mass(state, ore_storage, mass);
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
    )
    .unwrap_or_else(|error| panic!("primitive autonomous repeat resolution failed: {error}"));
    assert_eq!(resolved.required_energy(), expected_energy);
    let ticks = resolved.process_resolution().duration().value();
    let job = validate_start_process(
        registries,
        state,
        resolved.process_resolution(),
        ore_storage,
        crushed_storage,
    )
    .unwrap_or_else(|error| panic!("primitive autonomous repeat start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("primitive autonomous repeat commit failed: {error}"));
    assert!(state.production().get_job(job).is_some());
    advance_exact(registries, state, ticks);
    assert!(
        state.production().get_job(job).is_none(),
        "primitive autonomous repeat must complete without direct player labor"
    );
    ticks
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

fn select_stockpile_mass(
    state: &AppState,
    stockpile: deep_hearth::inventory::StockpileId,
    mass: Mass,
) -> Vec<MaterialLotSelection> {
    assert!(!mass.is_zero());
    let mut remaining = mass;
    let mut selections = Vec::new();
    for lot in state.inventory().lot_ids(stockpile) {
        if remaining.is_zero() {
            break;
        }
        let available = state
            .inventory()
            .get_lot(lot)
            .unwrap_or_else(|| panic!("primitive progression selected ore lot disappeared"))
            .mass();
        let selected = Mass::from_milligrams(available.milligrams().min(remaining.milligrams()));
        if selected.is_zero() {
            continue;
        }
        selections.push(MaterialLotSelection::new(lot, selected));
        remaining = remaining
            .checked_sub(selected)
            .unwrap_or_else(|| unreachable!("selected ore mass is bounded by remaining demand"));
    }
    assert!(
        remaining.is_zero(),
        "primitive progression lacks {}mg of claimed ore for the selected crusher work order",
        remaining.milligrams()
    );
    selections
}

fn crush_while_mining(
    registries: &Registries,
    state: &mut AppState,
    ore_storage: deep_hearth::inventory::StockpileId,
    crushed_storage: deep_hearth::inventory::StockpileId,
    machine: PrimitiveMachine,
    crush_mass: Mass,
    expected_energy: Energy,
    concurrent: ConcurrentMiningPlan,
) -> ConcurrentMachineWork {
    let machine_started_at = state.tick().value();
    let selection = select_stockpile_mass(state, ore_storage, crush_mass);
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
    )
    .unwrap_or_else(|error| panic!("primitive progression crushing resolution failed: {error}"));
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
    advance_exact(registries, state, player_free_ticks);
    assert!(
        state.production().get_job(concurrent.job).is_none(),
        "primitive crusher should complete after its remaining autonomous work"
    );
    player_free_ticks
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
    let three_mining_batches = two_mining_batches
        .checked_add(mined_mass)
        .unwrap_or_else(|| panic!("primitive progression ore fixture mass overflowed"));
    let ore_total = three_mining_batches
        .checked_add(mined_mass)
        .unwrap_or_else(|| panic!("primitive progression ore fixture mass overflowed"));
    let soft_ore_surplus = Mass::from_milligrams(
        1 + mix64(seed ^ 0x534F_4654_5F4F_5245) % mined_mass.milligrams().max(1),
    );
    let hard_ore_surplus = Mass::from_milligrams(
        mined_mass.milligrams().div_ceil(2)
            + mix64(seed ^ 0x4841_5244_5F4F_5245) % (mined_mass.milligrams() + 1),
    );
    let soft_ore_deposit_mass = three_mining_batches
        .checked_add(soft_ore_surplus)
        .unwrap_or_else(|| panic!("primitive progression soft-ore reserve mass overflowed"));
    let hard_ore_deposit_mass = three_mining_batches
        .checked_add(hard_ore_surplus)
        .unwrap_or_else(|| panic!("primitive progression hard-ore reserve mass overflowed"));
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
    let (stone_hardness_limit, reinforced_hardness_limit, hard_seam_hardness) =
        mining_hardness_limits(registries);
    let native_seam_hardness =
        Pressure::from_pascals(stone_hardness_limit.pascals().div_ceil(2).max(1));
    let pick_upgrade_native =
        native_input_for_upgrade(registries, EQUIPMENT_COPPER_REINFORCED_PICK);
    let crank_upgrade_native =
        native_input_for_upgrade(registries, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK);
    assert_eq!(
        pick_upgrade_native, crank_upgrade_native,
        "primitive competing copper upgrades must require the same scarce native-copper investment"
    );
    assert_eq!(
        pick_upgrade_native, native_copper,
        "primitive material plan must provision exactly one competing copper upgrade"
    );
    assert!(
        native_copper.milligrams() > 1,
        "primitive scarce-copper episode requires a nontrivial upgrade parcel"
    );
    let native_surplus = Mass::from_milligrams(
        1 + mix64(seed ^ 0x4E41_5449_5645_5355) % (native_copper.milligrams() - 1),
    );
    let native_deposit_mass = native_copper
        .checked_add(native_surplus)
        .unwrap_or_else(|| panic!("primitive progression native-copper reserve overflowed"));

    let mut state = AppState::new(WorldSeed::new(seed));
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("primitive progression survival setup failed: {error}"));
    let raw = add_solid_stockpile(&mut state, raw_seed_capacity);
    let shaped = add_solid_stockpile(&mut state, shaped_capacity);
    let ore_storage = add_solid_stockpile(&mut state, ore_total);
    let native_storage = add_solid_stockpile(&mut state, native_copper);
    let crushed_storage = add_solid_stockpile(&mut state, ore_total);
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
    let soft_ore_bounds = VoxelBounds::new(VoxelCoord::new(0, -4, 0), VoxelCoord::new(1, -3, 1))
        .unwrap_or_else(|error| panic!("primitive progression soft-ore bounds failed: {error}"));
    let ore_composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, ore_copper_ppm),
        CompositionComponent::new(MATERIAL_STONE, 1_000_000 - ore_copper_ppm),
    ])
    .unwrap_or_else(|error| panic!("primitive progression ore composition failed: {error}"));
    let soft_ore_deposit = seed_geological_deposit(
        registries,
        &mut state,
        geological_deposit_spec(
            soft_ore_bounds,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            soft_ore_deposit_mass,
            ROOM_TEMPERATURE,
            stone_hardness_limit,
            ore_composition.clone(),
        ),
    );
    let hard_ore_bounds = VoxelBounds::new(VoxelCoord::new(2, -4, 0), VoxelCoord::new(3, -3, 1))
        .unwrap_or_else(|error| panic!("primitive progression hard-ore bounds failed: {error}"));
    let hard_ore_deposit = seed_geological_deposit(
        registries,
        &mut state,
        geological_deposit_spec(
            hard_ore_bounds,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            hard_ore_deposit_mass,
            ROOM_TEMPERATURE,
            hard_seam_hardness,
            ore_composition,
        ),
    );
    let native_bounds = VoxelBounds::new(VoxelCoord::new(4, -4, 0), VoxelCoord::new(5, -3, 1))
        .unwrap_or_else(|error| panic!("primitive native-copper bounds failed: {error}"));
    let native_deposit = seed_geological_deposit(
        registries,
        &mut state,
        geological_deposit_spec(
            native_bounds,
            CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
            native_deposit_mass,
            ROOM_TEMPERATURE,
            native_seam_hardness,
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
        soft_ore_deposit,
        ore_storage,
        pick,
        mined_mass,
    );
    let native_mining_ticks = mine_total_and_claim(
        registries,
        &mut state,
        native_deposit,
        native_storage,
        pick,
        native_copper,
        stone_pick_batch_limit,
    );
    let native_copper_remaining = native_surplus;
    assert!(
        native_copper_remaining < native_copper,
        "primitive priority episode must leave too little native copper for the competing second upgrade"
    );
    assert_eq!(
        validate_start_mining(
            registries,
            &state,
            MINING_METHOD_HAND_PICK,
            hard_ore_deposit,
            ore_storage,
            pick,
            mined_mass,
        )
        .err(),
        Some(MiningStartError::DepositTooHard {
            hardness: hard_seam_hardness,
            maximum: stone_hardness_limit,
        }),
        "the known hard seam must be a real blocked affordance before pick reinforcement"
    );
    let (
        machine,
        reinforced_mining_ticks,
        concurrent_work,
        concurrent_task,
        pick_upgraded_at,
        hard_seam_accessed_at,
    ) = match priority {
        PrimitivePriority::ExtractionFirst => {
            reinforce_pick(registries, &mut state, raw, native_storage, shaped, pick);
            let pick_upgraded_at = state.tick().value();
            let reinforced_mining_ticks = mine_and_claim(
                registries,
                &mut state,
                hard_ore_deposit,
                ore_storage,
                pick,
                mined_mass,
            );
            let hard_seam_accessed_at = state.tick().value();
            let machine = build_and_charge_primitive_machine(
                registries,
                &mut state,
                raw,
                native_storage,
                shaped,
                mined_mass,
                seed,
                false,
            );
            let concurrent_work = crush_while_mining(
                registries,
                &mut state,
                ore_storage,
                crushed_storage,
                machine,
                mined_mass,
                machine.required_energy,
                ConcurrentMiningPlan {
                    deposit: hard_ore_deposit,
                    destination: ore_storage,
                    pick,
                    mass: mined_mass,
                },
            );
            (
                machine,
                Some(reinforced_mining_ticks),
                concurrent_work,
                "hard-ore",
                Some(pick_upgraded_at),
                Some(hard_seam_accessed_at),
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
                true,
            );
            let concurrent_work = crush_while_mining(
                registries,
                &mut state,
                ore_storage,
                crushed_storage,
                machine,
                mined_mass,
                machine.required_energy,
                ConcurrentMiningPlan {
                    deposit: soft_ore_deposit,
                    destination: ore_storage,
                    pick,
                    mass: mined_mass,
                },
            );
            (machine, None, concurrent_work, "soft-ore", None, None)
        }
    };
    let primary_player_free_ticks =
        finish_autonomous_crush(registries, &mut state, concurrent_work);
    let machine_useful_overlap_ticks = concurrent_work
        .crush_ticks
        .checked_sub(primary_player_free_ticks)
        .unwrap_or_else(|| {
            panic!("primitive machine player-free time exceeded active process time")
        });
    match priority {
        PrimitivePriority::ExtractionFirst => {
            let pick_upgraded_at = pick_upgraded_at
                .unwrap_or_else(|| unreachable!("extraction-first upgrades the pick"));
            let reinforced_mining_ticks = reinforced_mining_ticks
                .unwrap_or_else(|| unreachable!("extraction-first mines the hard seam"));
            assert!(
                reinforced_mining_ticks < stone_mining_ticks,
                "copper pick reinforcement must save player-attention time on the maintained mining batch"
            );
            assert!(
                pick_upgraded_at < concurrent_work.machine_started_at,
                "extraction-first priority must improve extraction before starting autonomous work"
            );
            assert!(!machine.crank_reinforced);
        }
        PrimitivePriority::MechanizationFirst => {
            assert!(pick_upgraded_at.is_none() && hard_seam_accessed_at.is_none());
            assert!(machine.crank_reinforced);
        }
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
    let reserve_deposit = match priority {
        PrimitivePriority::ExtractionFirst => hard_ore_deposit,
        PrimitivePriority::MechanizationFirst => soft_ore_deposit,
    };
    let reserve_work = crush_while_mining(
        registries,
        &mut state,
        ore_storage,
        crushed_storage,
        machine,
        machine.reserve_mass,
        banked_energy,
        ConcurrentMiningPlan {
            deposit: reserve_deposit,
            destination: ore_storage,
            pick,
            mass: mined_mass,
        },
    );
    let reserve_player_free_ticks = finish_autonomous_crush(registries, &mut state, reserve_work);
    let reserve_useful_overlap_ticks = reserve_work
        .crush_ticks
        .checked_sub(reserve_player_free_ticks)
        .unwrap_or_else(|| panic!("reserve crusher player-free time exceeded active process time"));
    assert!(
        reserve_useful_overlap_ticks > 0,
        "banked mechanical work must support another cycle while the player performs useful extraction"
    );
    let drive_after_reserve = state
        .energy()
        .get_store(machine.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| {
            panic!("primitive progression flywheel disappeared after reserve crushing")
        });
    assert_eq!(drive_after_reserve, Energy::ZERO);
    let recharge_ticks =
        recharge_primitive_machine(registries, &mut state, machine, machine.drive_capacity);
    let repeated_crush_ticks = crush_autonomous_batch(
        registries,
        &mut state,
        ore_storage,
        crushed_storage,
        machine,
        mined_mass,
        machine.required_energy,
    );
    let drive_remaining = state
        .energy()
        .get_store(machine.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| {
            panic!("primitive progression flywheel disappeared after repeated crushing")
        });
    let expected_remaining = machine
        .drive_capacity
        .checked_sub(machine.required_energy)
        .unwrap_or_else(|| unreachable!("one crusher batch fits the primitive flywheel"));
    assert_eq!(
        drive_remaining, expected_remaining,
        "full recharge should leave unused stored work after one repeated crusher batch"
    );
    if priority == PrimitivePriority::MechanizationFirst {
        assert_eq!(
            validate_start_mining(
                registries,
                &state,
                MINING_METHOD_HAND_PICK,
                hard_ore_deposit,
                ore_storage,
                pick,
                mined_mass,
            )
            .err(),
            Some(MiningStartError::DepositTooHard {
                hardness: hard_seam_hardness,
                maximum: stone_hardness_limit,
            }),
            "mechanization-first must still own the consequences of spending scarce copper on the crank"
        );
    }
    let survival_after = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("primitive progression final survival state disappeared"));
    let total_ore_reserve = soft_ore_deposit_mass
        .checked_add(hard_ore_deposit_mass)
        .unwrap_or_else(|| panic!("primitive progression combined ore reserve overflowed"));
    let hard_ore_mined = match priority {
        PrimitivePriority::ExtractionFirst => three_mining_batches,
        PrimitivePriority::MechanizationFirst => Mass::ZERO,
    };
    let total_ore_mined = match priority {
        PrimitivePriority::ExtractionFirst => ore_total,
        PrimitivePriority::MechanizationFirst => three_mining_batches,
    };
    let unmined_ore_reserve = total_ore_reserve
        .checked_sub(total_ore_mined)
        .unwrap_or_else(|| unreachable!("ore world fixture exceeds the actor's actual extraction"));
    assert!(!unmined_ore_reserve.is_zero() && !native_copper_remaining.is_zero());
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
        .and_then(|mass| mass.checked_add(mined_mass))
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
    let total_machine_work_ticks = concurrent_work
        .crush_ticks
        .checked_add(reserve_work.crush_ticks)
        .and_then(|ticks| ticks.checked_add(repeated_crush_ticks))
        .unwrap_or_else(|| panic!("primitive autonomous-work duration overflowed"));
    let total_useful_overlap_ticks = machine_useful_overlap_ticks
        .checked_add(reserve_useful_overlap_ticks)
        .unwrap_or_else(|| panic!("primitive useful-overlap duration overflowed"));
    let total_player_free_ticks = primary_player_free_ticks
        .checked_add(reserve_player_free_ticks)
        .and_then(|ticks| ticks.checked_add(repeated_crush_ticks))
        .unwrap_or_else(|| panic!("primitive player-free autonomous duration overflowed"));
    let total_charge_ticks = machine
        .charge_ticks
        .checked_add(recharge_ticks)
        .unwrap_or_else(|| panic!("primitive charging attention overflowed"));
    let experience = PrimitiveProgressionExperience {
        priority,
        primary_batch_mass: mined_mass,
        pick_upgraded_at,
        hard_seam_accessed_at,
        machine_started_at: concurrent_work.machine_started_at,
        first_processed_output_at: concurrent_work
            .machine_started_at
            .checked_add(concurrent_work.crush_ticks)
            .unwrap_or_else(|| panic!("primitive processed-output milestone overflowed")),
        elapsed_ticks: state.tick().value(),
        soft_ore_mining_ticks: stone_mining_ticks,
        reinforced_mining_ticks,
        charge_ticks: total_charge_ticks,
        machine_work_ticks: total_machine_work_ticks,
        reserve_machine_work_ticks: reserve_work.crush_ticks,
        overlap_ticks: concurrent_work.overlap_ticks,
        machine_useful_overlap_ticks: total_useful_overlap_ticks,
        reserve_useful_overlap_ticks,
        machine_player_free_ticks: total_player_free_ticks,
        hard_ore_mined,
        total_ore_mined,
        native_copper_remaining,
        crank_reinforced: machine.crank_reinforced,
        final_pick_condition_ppm,
        metabolic_energy_spent_nj,
        hydration_spent_ul,
    };
    let (chosen_upgrade, foregone_upgrade) = match priority {
        PrimitivePriority::ExtractionFirst => ("pick", "hand-crank"),
        PrimitivePriority::MechanizationFirst => ("hand-crank", "pick"),
    };
    let pick_milestone = pick_upgraded_at
        .map(|tick| format!("{tick}t"))
        .unwrap_or_else(|| "not-acquired".to_string());
    let hard_seam_milestone = hard_seam_accessed_at
        .map(|tick| format!("{tick}t"))
        .unwrap_or_else(|| "locked".to_string());

    if emit_detail && std::env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some() {
        std::println!(
            "PLAYABLE PROGRESSION seed=0x{seed:016X} priority={} world-bootstrap=[raw-gathered-matter-surplus:{}mg,preauthorized-soft+hard-ore-site-identities,preauthorized-native-copper-site-identity,empty-storage] discovery=not-modeled episode-scope=[natural-actions-only catalog-outside-episode:clay-vessel] canonical=shape->assemble->mine-soft->encounter-hardness-gate->spend-one-scarce-upgrade->pursue-extraction-or-mechanization->store-work->autonomous-crush fantasy=survive->craft-tools->make-competing-material-investments->turn-investment-into-affordance->store-work->delegate-repetition->work-in-parallel",
            priority.label(),
            raw_surplus.milligrams(),
        );
        std::println!(
            "PROGRESSION DECISION choice=[spent:{}:{}mg foregone:{} second-affordable:false native-remaining:{}mg] milestones=[pick-upgrade:{} hard-seam-access:{} machine-start:{}t] hardness=[hard-seam:{}Pa stone-limit:{}Pa reinforced-limit:{}Pa blocked-before-choice:true]",
            chosen_upgrade,
            native_copper.milligrams(),
            foregone_upgrade,
            native_copper_remaining.milligrams(),
            pick_milestone,
            hard_seam_milestone,
            concurrent_work.machine_started_at,
            hard_seam_hardness.pascals(),
            stone_hardness_limit.pascals(),
            reinforced_hardness_limit.pascals(),
        );
        std::println!(
            "PROGRESSION SYSTEMS ore=[grade:{}ppm:composition-only batch:{}mg stone-mining:{}t reinforced-mining:{:?} total-mined:{}mg hard-mined:{}mg remaining:{}mg] native=[mining:{}t invested:{}mg remaining:{}mg] infrastructure=[drive:{}mg crusher:{}mg] stored-work=[fill:{}ppm initial-charge:{}nJ primary:{}nJ banked:{}nJ follow-up:{}mg:{}t recharge:{}nJ:{}t repeat:{}mg:{}t final:{}nJ] charge=[crank-reinforced:{} initial:{}t total:{}t] mechanization=[primary:{}t initial-concurrent:{}:{}t initial-overlap:{}t primary-productive-overlap:{}t primary-player-free:{}t reserve:{}t reserve-mining:{}t reserve-productive-overlap:{}t reserve-player-free:{}t repeated-autonomous:{}t total-productive-overlap:{}t total-player-free:{}t processed:{}mg] durability=[pick:{}ppm] survival=[spent:{}nJ/{}uL remaining:{}nJ/{}uL warning:{}nJ/{}uL state:{:?}/{:?} elapsed:{}t] matter=conserved",
            ore_copper_ppm,
            mined_mass.milligrams(),
            stone_mining_ticks,
            reinforced_mining_ticks,
            total_ore_mined.milligrams(),
            hard_ore_mined.milligrams(),
            unmined_ore_reserve.milligrams(),
            native_mining_ticks,
            native_copper.milligrams(),
            native_copper_remaining.milligrams(),
            drive_mass.milligrams(),
            crusher_mass.milligrams(),
            machine.charge_fill_ppm,
            machine.charge_energy.nanojoules(),
            machine.required_energy.nanojoules(),
            banked_energy.nanojoules(),
            machine.reserve_mass.milligrams(),
            reserve_work.crush_ticks,
            machine.drive_capacity.nanojoules(),
            recharge_ticks,
            mined_mass.milligrams(),
            repeated_crush_ticks,
            drive_remaining.nanojoules(),
            machine.crank_reinforced,
            machine.charge_ticks,
            total_charge_ticks,
            concurrent_work.crush_ticks,
            concurrent_task,
            concurrent_work.player_work_ticks,
            concurrent_work.overlap_ticks,
            machine_useful_overlap_ticks,
            primary_player_free_ticks,
            reserve_work.crush_ticks,
            reserve_work.player_work_ticks,
            reserve_useful_overlap_ticks,
            reserve_player_free_ticks,
            repeated_crush_ticks,
            total_useful_overlap_ticks,
            total_player_free_ticks,
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
    pub(super) tool_attention_reduction_ppm: u32,
    pub(super) processed_output_has_playable_acquisition_use: bool,
    pub(super) crank_power_gain_ppm: u32,
    pub(super) crank_attention_reduction_ppm: u32,
    pub(super) mechanization_autonomy_lead_ticks: u64,
    pub(super) mechanization_output_delta_ticks: i128,
    pub(super) extraction_hard_ore_mass_mg: u64,
    pub(super) extraction_extra_ore_mass_mg: u64,
    pub(super) scarce_copper_remaining_mg: u64,
    pub(super) persistent_upgrade_tradeoff: bool,
    pub(super) mechanization_processed_without_pick_upgrade: bool,
    pub(super) machine_work_ticks: u64,
    pub(super) reserve_machine_work_ticks: u64,
    pub(super) mechanization_useful_overlap_ticks: u64,
    pub(super) reserve_useful_overlap_ticks: u64,
    pub(super) mechanization_player_free_delta_ticks: i128,
    pub(super) mechanization_elapsed_delta_ticks: i128,
}

fn tick_delta(left: u64, right: u64) -> i128 {
    i128::from(left) - i128::from(right)
}

fn attention_reduction_ppm(baseline_ticks: u64, improved_ticks: u64) -> u32 {
    assert!(baseline_ticks > 0 && improved_ticks <= baseline_ticks);
    u32::try_from(
        u128::from(baseline_ticks - improved_ticks) * 1_000_000 / u128::from(baseline_ticks),
    )
    .unwrap_or_else(|_| unreachable!("bounded attention reduction ratio fits u32"))
}

fn nominal_manual_power(
    registries: &Registries,
    equipment: deep_hearth::equipment::EquipmentDefinitionId,
) -> Power {
    let capability = registries
        .labor()
        .get_manual_power(MANUAL_POWER_HAND_CRANK)
        .map(|definition| definition.power_capability())
        .unwrap_or_else(|| panic!("primitive progression manual-power definition disappeared"));
    let value = registries
        .equipment()
        .get_equipment(equipment)
        .and_then(|definition| definition.capabilities().get_capability(capability))
        .unwrap_or_else(|| {
            panic!(
                "primitive progression equipment {} lost manual-power capability {}",
                equipment.value(),
                capability.value()
            )
        });
    match value {
        CapabilityValue::Power(power) => power,
        other => panic!(
            "primitive progression equipment {} manual-power capability has wrong kind {:?}",
            equipment.value(),
            other.kind()
        ),
    }
}

fn relative_power_gain_ppm(base: Power, upgraded: Power) -> u32 {
    let base = base.whole_microwatts();
    let upgraded = upgraded.whole_microwatts();
    assert!(base > 0 && upgraded > base);
    u32::try_from((upgraded - base) * 1_000_000 / base)
        .unwrap_or_else(|_| panic!("primitive manual-power gain exceeds report range"))
}

fn playable_acquisition_consumes_crushed_material(registries: &Registries) -> bool {
    registries
        .crafting()
        .definitions()
        .any(|definition| definition.input().form() == FORM_CRUSHED)
        || registries.equipment().definitions().any(|definition| {
            definition.assembly_profile().is_some_and(|profile| {
                profile
                    .inputs()
                    .iter()
                    .any(|input| input.commodity().form() == FORM_CRUSHED)
            }) || definition.upgrade_profile().is_some_and(|profile| {
                profile
                    .additions()
                    .inputs()
                    .iter()
                    .any(|input| input.commodity().form() == FORM_CRUSHED)
            })
        })
        || registries.energy().definitions().any(|definition| {
            definition.assembly_profile().is_some_and(|profile| {
                profile
                    .inputs()
                    .iter()
                    .any(|input| input.commodity().form() == FORM_CRUSHED)
            })
        })
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

    let extraction_pick_at = extraction
        .pick_upgraded_at
        .unwrap_or_else(|| panic!("extraction-first failed to spend scarce copper on the pick"));
    let extraction_hard_at = extraction.hard_seam_accessed_at.unwrap_or_else(|| {
        panic!("extraction-first pick upgrade failed to unlock the known hard seam")
    });
    let extraction_reinforced_mining_ticks = extraction
        .reinforced_mining_ticks
        .unwrap_or_else(|| panic!("extraction-first never exercised its reinforced pick"));
    assert!(mechanization.pick_upgraded_at.is_none());
    assert!(mechanization.hard_seam_accessed_at.is_none());
    assert!(!extraction.crank_reinforced && mechanization.crank_reinforced);
    assert_eq!(
        extraction.native_copper_remaining, mechanization.native_copper_remaining,
        "matched-world scarce copper residue must not depend on the chosen upgrade"
    );
    assert!(
        mechanization.machine_started_at < extraction.machine_started_at,
        "mechanization-first must deliver autonomous work earlier on the same world"
    );
    assert!(
        extraction_reinforced_mining_ticks < extraction.soft_ore_mining_ticks,
        "spending scarce copper on the pick must reduce actual mining attention"
    );
    assert!(extraction.hard_ore_mined > Mass::ZERO);
    assert_eq!(mechanization.hard_ore_mined, Mass::ZERO);
    assert!(extraction.total_ore_mined > mechanization.total_ore_mined);
    assert_eq!(
        extraction.machine_work_ticks, mechanization.machine_work_ticks,
        "matched-world priorities must compare the same autonomous crusher workload"
    );
    assert_eq!(
        extraction.reserve_machine_work_ticks, mechanization.reserve_machine_work_ticks,
        "matched-world priorities must compare the same banked follow-up crusher workload"
    );
    assert_eq!(
        extraction.primary_batch_mass, mechanization.primary_batch_mass,
        "matched-world priorities must compare the same primary crusher batch"
    );

    let mechanization_autonomy_lead_ticks = extraction
        .machine_started_at
        .checked_sub(mechanization.machine_started_at)
        .unwrap_or_else(|| unreachable!("mechanization-first already wins autonomous-work access"));
    let mechanization_processed_without_pick_upgrade = mechanization.pick_upgraded_at.is_none()
        && mechanization.hard_seam_accessed_at.is_none()
        && mechanization.first_processed_output_at > mechanization.machine_started_at;
    let persistent_upgrade_tradeoff = extraction.pick_upgraded_at.is_some()
        && extraction.hard_seam_accessed_at.is_some()
        && !extraction.crank_reinforced
        && mechanization.pick_upgraded_at.is_none()
        && mechanization.hard_seam_accessed_at.is_none()
        && mechanization.crank_reinforced;
    let extraction_extra_ore_mass_mg = extraction
        .total_ore_mined
        .checked_sub(mechanization.total_ore_mined)
        .unwrap_or_else(|| {
            unreachable!("extraction-first mines more ore in the maintained episode")
        })
        .milligrams();
    let tool_attention_reduction_ppm = attention_reduction_ppm(
        extraction.soft_ore_mining_ticks,
        extraction_reinforced_mining_ticks,
    );
    let crank_power_gain_ppm = relative_power_gain_ppm(
        nominal_manual_power(registries, EQUIPMENT_STONE_HAND_CRANK),
        nominal_manual_power(registries, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK),
    );
    assert!(
        mechanization.charge_ticks < extraction.charge_ticks,
        "full flywheel recharge must make the reinforced crank's higher work rate observable"
    );
    let crank_attention_reduction_ppm =
        attention_reduction_ppm(extraction.charge_ticks, mechanization.charge_ticks);
    let processed_output_has_playable_acquisition_use =
        playable_acquisition_consumes_crushed_material(registries);

    std::println!(
        "PROGRESSION STRATEGIC CHOICE seed=0x{seed:016X} scarce-copper={}mg residue={}mg second-upgrade-affordable:false persistent=[extraction-first:pick-upgraded@{}t hard-seam@{}t crank=stone hard-ore={}mg; mechanization-first:pick=stone hard-seam=locked crank=reinforced machine@{}t] autonomy-lead:{}t extraction-extra-ore:{}mg convergence=not-within-episode",
        native_input_for_upgrade(registries, EQUIPMENT_COPPER_REINFORCED_PICK).milligrams(),
        extraction.native_copper_remaining.milligrams(),
        extraction_pick_at,
        extraction_hard_at,
        extraction.hard_ore_mined.milligrams(),
        mechanization.machine_started_at,
        mechanization_autonomy_lead_ticks,
        extraction_extra_ore_mass_mg,
    );
    std::println!(
        "PROGRESSION AGENCY seed=0x{seed:016X} matched-world choices=[extraction-first,mechanization-first] milestones=[machine-start:{}vs{}t first-processed-output:{}vs{}t] attention=[mining:stone:{}t reinforced:{}t reduction:{}ppm charge:stone:{}t reinforced:{}t reduction:{}ppm] resources=[ore-mined:{}vs{}mg hard-ore:{}vs{}mg native-residue:{}vs{}mg] autonomy=[machine-total:{}t reserve-cycle:{}t initial-overlap:{}vs{}t productive-overlap:{}vs{}t reserve-productive:{}vs{}t player-free-during-machine:{}vs{}t] final-pick=[{}vs{}ppm] survival=[energy:{}vs{}nJ hydration:{}vs{}uL] elapsed=[{}vs{}t] tradeoff=hard-material-access+greater-extraction-vs-earlier-autonomy+faster-stored-work",
        extraction.machine_started_at,
        mechanization.machine_started_at,
        extraction.first_processed_output_at,
        mechanization.first_processed_output_at,
        extraction.soft_ore_mining_ticks,
        extraction_reinforced_mining_ticks,
        tool_attention_reduction_ppm,
        extraction.charge_ticks,
        mechanization.charge_ticks,
        crank_attention_reduction_ppm,
        extraction.total_ore_mined.milligrams(),
        mechanization.total_ore_mined.milligrams(),
        extraction.hard_ore_mined.milligrams(),
        mechanization.hard_ore_mined.milligrams(),
        extraction.native_copper_remaining.milligrams(),
        mechanization.native_copper_remaining.milligrams(),
        extraction.machine_work_ticks,
        extraction.reserve_machine_work_ticks,
        extraction.overlap_ticks,
        mechanization.overlap_ticks,
        extraction.machine_useful_overlap_ticks,
        mechanization.machine_useful_overlap_ticks,
        extraction.reserve_useful_overlap_ticks,
        mechanization.reserve_useful_overlap_ticks,
        extraction.machine_player_free_ticks,
        mechanization.machine_player_free_ticks,
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
        tool_attention_reduction_ppm,
        processed_output_has_playable_acquisition_use,
        crank_power_gain_ppm,
        crank_attention_reduction_ppm,
        mechanization_autonomy_lead_ticks,
        mechanization_output_delta_ticks: tick_delta(
            extraction.first_processed_output_at,
            mechanization.first_processed_output_at,
        ),
        extraction_hard_ore_mass_mg: extraction.hard_ore_mined.milligrams(),
        extraction_extra_ore_mass_mg,
        scarce_copper_remaining_mg: extraction.native_copper_remaining.milligrams(),
        persistent_upgrade_tradeoff,
        mechanization_processed_without_pick_upgrade,
        machine_work_ticks: mechanization.machine_work_ticks,
        reserve_machine_work_ticks: mechanization.reserve_machine_work_ticks,
        mechanization_useful_overlap_ticks: mechanization.machine_useful_overlap_ticks,
        reserve_useful_overlap_ticks: mechanization.reserve_useful_overlap_ticks,
        mechanization_player_free_delta_ticks: tick_delta(
            extraction.machine_player_free_ticks,
            mechanization.machine_player_free_ticks,
        ),
        mechanization_elapsed_delta_ticks: tick_delta(
            extraction.elapsed_ticks,
            mechanization.elapsed_ticks,
        ),
    };
    let fantasy_captured = review.persistent_upgrade_tradeoff
        && review.tool_attention_reduction_ppm > 0
        && review.crank_power_gain_ppm > 0
        && review.crank_attention_reduction_ppm > 0
        && review.mechanization_autonomy_lead_ticks > 0
        && review.mechanization_output_delta_ticks > 0
        && review.extraction_hard_ore_mass_mg > 0
        && review.extraction_extra_ore_mass_mg > 0
        && review.mechanization_processed_without_pick_upgrade
        && review.mechanization_useful_overlap_ticks > 0
        && review.reserve_useful_overlap_ticks > 0;
    assert!(
        fantasy_captured,
        "primitive progression must turn one genuinely competing material investment into distinct persistent affordances"
    );
    let output_material_utility = if review.processed_output_has_playable_acquisition_use {
        "playable-acquisition"
    } else {
        "capability-only-downstream"
    };
    std::println!(
        "PROGRESSION REVIEW fantasy=bootstrap-by-hand->invest-scarce-copper->choose-affordance->store-work->delegate-repetition captured:{fantasy_captured} agency=persistent-competing-upgrade observations=[tool-attention-reduction:{}ppm crank-power-gain:{}ppm observed-crank-attention-reduction:{}ppm extraction-hard-ore:{}mg extraction-extra-ore:{}mg scarce-copper-residue:{}mg autonomy-lead:{}t output-lead:{:+}t processed-without-pick:{} autonomous-work:{}t reserve-cycle:{}t mechanization-productive-overlap:{}t reserve-productive-overlap:{}t player-free-during-machine-delta:{:+}t mechanization-elapsed-delta:{:+}t] output-utility=[attention:direct material-progression:{output_material_utility}] interpretation=[copper-pick=hard-material-access+faster-extraction copper-crank=doubled-work-rate-leveraged-by-full-accumulator-recharge productive-overlap=useful-player-work-while-machine-runs crushed-ore-material-advancement-remains-outside-current-playable-slice] sign=[output/elapsed:+ means mechanization-first was earlier/lower; player-free:+ means extraction-first had more autonomous free-attention time]",
        review.tool_attention_reduction_ppm,
        review.crank_power_gain_ppm,
        review.crank_attention_reduction_ppm,
        review.extraction_hard_ore_mass_mg,
        review.extraction_extra_ore_mass_mg,
        review.scarce_copper_remaining_mg,
        review.mechanization_autonomy_lead_ticks,
        review.mechanization_output_delta_ticks,
        review.mechanization_processed_without_pick_upgrade,
        review.machine_work_ticks,
        review.reserve_machine_work_ticks,
        review.mechanization_useful_overlap_ticks,
        review.reserve_useful_overlap_ticks,
        review.mechanization_player_free_delta_ticks,
        review.mechanization_elapsed_delta_ticks,
    );
    review
}

pub(super) fn run_primitive_progression_probe(registries: &Registries, seed: u64) {
    let _ = evaluate_primitive_progression_probe(registries, seed);
}
