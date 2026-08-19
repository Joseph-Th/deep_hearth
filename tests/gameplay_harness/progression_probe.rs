//! Canonical primitive-to-mechanized progression probe for the gameplay experience harness.

use std::collections::BTreeMap;
use std::num::NonZeroU64;

use super::seed::mix64;
use super::{ROOM_TEMPERATURE, add_solid_stockpile, nominal_equipment_mass_capability, seed_lot};
use deep_hearth::content::gameplay_fixture::seed_geological_deposit;
use deep_hearth::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_STONE_CRUSHER, EQUIPMENT_STONE_HAND_CRANK,
    EQUIPMENT_STONE_PICK, FORM_NATIVE_METAL, FORM_ORE, MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER,
    MATERIAL_STONE, MINING_METHOD_HAND_PICK, PROCESS_CRUSH_ORE,
};
use deep_hearth::core::quantity::{Mass, Temperature};
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
use deep_hearth::production::validate_start_process;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};
use deep_hearth::survival::{assess_survival, initialize_player_survival};

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
    let minimum = maximum.div_ceil(2);
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

pub(super) fn run_primitive_progression_probe(registries: &Registries, seed: u64) {
    let mined_mass = progression_mining_mass(registries, seed);
    let two_mining_batches = mined_mass
        .checked_add(mined_mass)
        .unwrap_or_else(|| panic!("primitive progression ore fixture mass overflowed"));
    let ore_total = two_mining_batches
        .checked_add(mined_mass)
        .unwrap_or_else(|| panic!("primitive progression ore fixture mass overflowed"));
    let ore_copper_ppm = 450_000 + (mix64(seed ^ 0x5052_4F47_4752_4144) % 300_001) as u32;
    let PrimitiveMaterialPlan {
        raw_inputs,
        raw_capacity,
        shaped_capacity,
        native_copper,
    } = primitive_material_plan(registries);
    let stone_pick_batch_limit = stone_pick_mining_batch_limit(registries);

    let mut state = AppState::new(WorldSeed::new(seed));
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("primitive progression survival setup failed: {error}"));
    let raw = add_solid_stockpile(&mut state, raw_capacity, "primitive raw materials");
    let shaped = add_solid_stockpile(&mut state, shaped_capacity, "primitive shaped materials");
    let ore_storage = add_solid_stockpile(&mut state, ore_total, "primitive mined ore");
    let native_storage = add_solid_stockpile(&mut state, native_copper, "primitive native copper");
    let crushed_storage = add_solid_stockpile(&mut state, mined_mass, "primitive crushed ore");
    for (commodity, mass) in raw_inputs {
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
        ore_bounds,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        ore_total,
        Temperature::from_millikelvin(293_150),
        ore_composition,
    );
    let native_bounds = VoxelBounds::new(VoxelCoord::new(2, -4, 0), VoxelCoord::new(3, -3, 1))
        .unwrap_or_else(|error| panic!("primitive native-copper bounds failed: {error}"));
    let native_deposit = seed_geological_deposit(
        registries,
        &mut state,
        native_bounds,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        native_copper,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_COPPER),
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
        ore_deposit,
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
    let worn_stone_condition = state
        .equipment()
        .get_equipment(pick)
        .unwrap_or_else(|| panic!("primitive progression worn pick disappeared"))
        .condition();

    craft_for_profile(
        registries,
        &mut state,
        raw,
        native_storage,
        shaped,
        equipment_upgrade_additions(registries, EQUIPMENT_COPPER_REINFORCED_PICK),
    );
    validate_upgrade_equipment(
        registries,
        &state,
        pick,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        shaped,
    )
    .unwrap_or_else(|error| panic!("primitive progression pick reinforcement failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| {
        panic!("primitive progression pick reinforcement commit failed: {error}")
    });
    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .unwrap_or_else(|| panic!("primitive progression reinforced pick disappeared"))
            .condition(),
        worn_stone_condition,
        "reinforcement must not repair accumulated pick wear"
    );
    let reinforced_mining_ticks = mine_and_claim(
        registries,
        &mut state,
        ore_deposit,
        ore_storage,
        pick,
        mined_mass,
    );
    assert!(
        reinforced_mining_ticks < stone_mining_ticks,
        "copper reinforcement should reduce active extraction time for the same mass"
    );

    craft_for_profile(
        registries,
        &mut state,
        raw,
        native_storage,
        shaped,
        equipment_assembly_profile(registries, EQUIPMENT_STONE_HAND_CRANK),
    );
    let crank = validate_assemble_equipment(registries, &state, EQUIPMENT_STONE_HAND_CRANK, shaped)
        .unwrap_or_else(|error| panic!("primitive progression crank assembly failed: {error}"))
        .commit(&mut state)
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
        &mut state,
        raw,
        native_storage,
        shaped,
        drive_profile,
    );
    let drive =
        validate_assemble_energy_store(registries, &state, ENERGY_STONE_FLYWHEEL_DRIVE, shaped)
            .unwrap_or_else(|error| {
                panic!("primitive progression drive construction failed: {error}")
            })
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("primitive progression drive construction commit failed: {error}")
            });

    let crusher_process = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("primitive progression crusher process disappeared"));
    let required_energy =
        calculate_mass_specific_energy(mined_mass, crusher_process.specific_energy());
    let stone_power = validate_start_manual_power(
        registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, required_energy),
    )
    .unwrap_or_else(|error| panic!("primitive progression stone-crank projection failed: {error}"));
    let stone_charge_ticks = duration(
        stone_power.work().started_at().value(),
        stone_power.work().completes_at().value(),
    );
    craft_for_profile(
        registries,
        &mut state,
        raw,
        native_storage,
        shaped,
        equipment_upgrade_additions(registries, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK),
    );
    validate_upgrade_equipment(
        registries,
        &state,
        crank,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        shaped,
    )
    .unwrap_or_else(|error| panic!("primitive progression crank reinforcement failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| {
        panic!("primitive progression crank reinforcement commit failed: {error}")
    });

    craft_for_profile(
        registries,
        &mut state,
        raw,
        native_storage,
        shaped,
        equipment_assembly_profile(registries, EQUIPMENT_STONE_CRUSHER),
    );
    let crusher = validate_assemble_equipment(registries, &state, EQUIPMENT_STONE_CRUSHER, shaped)
        .unwrap_or_else(|error| {
            panic!("primitive progression crusher construction failed: {error}")
        })
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("primitive progression crusher construction commit failed: {error}")
        });
    let power = validate_start_manual_power(
        registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, required_energy),
    )
    .unwrap_or_else(|error| panic!("primitive progression manual charging failed: {error}"));
    let charge_work = power.work();
    let charge_ticks = duration(
        charge_work.started_at().value(),
        charge_work.completes_at().value(),
    );
    power
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("primitive progression charge commit failed: {error}"));
    advance_exact(registries, &mut state, charge_ticks);
    assert!(
        charge_ticks < stone_charge_ticks,
        "copper reinforcement should reduce primitive charging time for the same stored work"
    );
    assert_eq!(
        state.energy().get_store(drive).map(|store| store.stored()),
        Some(required_energy),
        "reinforcement may change charging rate but not the requested stored work"
    );

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
        &state,
        ComminutionRequest::new(PROCESS_CRUSH_ORE, ore_storage, &selection, crusher, drive),
    )
    .unwrap_or_else(|error| panic!("primitive progression crushing resolution failed: {error}"));
    assert_eq!(resolved.required_energy(), required_energy);
    let crush_ticks = resolved.process_resolution().duration().value();
    let crush_job = validate_start_process(
        registries,
        &state,
        resolved.process_resolution(),
        ore_storage,
        crushed_storage,
    )
    .unwrap_or_else(|error| panic!("primitive progression crushing start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("primitive progression crushing commit failed: {error}"));

    let concurrent_mining = validate_start_mining(
        registries,
        &state,
        MINING_METHOD_HAND_PICK,
        ore_deposit,
        ore_storage,
        pick,
        mined_mass,
    )
    .unwrap_or_else(|error| {
        panic!("primitive progression concurrent mining admission failed: {error}")
    });
    let concurrent_mining_job = concurrent_mining
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("primitive progression concurrent mining commit failed: {error}")
        });
    let concurrent_mining_ticks = state
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
    let overlap_ticks = crush_ticks.min(concurrent_mining_ticks);
    let overlap_witness_ticks = overlap_ticks.saturating_sub(1);
    if overlap_witness_ticks > 0 {
        advance_exact(registries, &mut state, overlap_witness_ticks);
        assert!(
            state.production().get_job(crush_job).is_some()
                && state.mining().get_job(concurrent_mining_job).is_some()
                && state.player_work().active().is_some(),
            "machine production and player mining must remain independently active during their shared interval"
        );
    }
    let concurrent_span = crush_ticks.max(concurrent_mining_ticks);
    advance_exact(
        registries,
        &mut state,
        concurrent_span - overlap_witness_ticks,
    );
    assert!(
        state.production().get_job(crush_job).is_none(),
        "primitive crusher should complete without consuming player labor"
    );
    assert!(
        state.mining().get_job(concurrent_mining_job).is_some(),
        "completed mining output must remain claimable after concurrent machine work"
    );
    validate_claim_mining_output(registries, &state, concurrent_mining_job)
        .unwrap_or_else(|error| {
            panic!("primitive progression concurrent mining claim failed: {error}")
        })
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("primitive progression concurrent mining claim commit failed: {error}")
        });

    let survival_after = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("primitive progression final survival state disappeared"));
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
    assert_eq!(
        state
            .inventory()
            .get_stockpile(crushed_storage)
            .unwrap_or_else(|| panic!("primitive progression crushed storage disappeared"))
            .stored_mass(),
        mined_mass
    );
    assert_eq!(state.player_work().active(), None);
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("primitive progression persistence audit failed: {error}"));

    let drive_mass = state
        .energy()
        .get_store(drive)
        .unwrap_or_else(|| panic!("primitive progression constructed drive disappeared"))
        .embodied_mass();
    let crusher_mass = state
        .equipment()
        .get_equipment(crusher)
        .unwrap_or_else(|| panic!("primitive progression constructed crusher disappeared"))
        .embodied_mass();

    std::println!(
        "PROGRESSION seed=0x{seed:016X} fantasy=survive->craft-tools->extract-ore->find-native-metal->reinforce-tools->extract-better->build-power->reinforce-power->build-machine->mechanize ore=[grade:{}ppm batch:{}mg comparison:x2 concurrent:x1] native={}mg mining=[stone-ore:{}t native:{}t reinforced-ore:{}t concurrent:{}t] infrastructure=[drive:{}mg crusher:{}mg] stored_work={}nJ charge=[stone:{}t reinforced:{}t] mechanization=[crush:{}t overlap:{}t] survival=[energy:-{}nJ hydration:-{}uL] matter=conserved",
        ore_copper_ppm,
        mined_mass.milligrams(),
        native_copper.milligrams(),
        stone_mining_ticks,
        native_mining_ticks,
        reinforced_mining_ticks,
        concurrent_mining_ticks,
        drive_mass.milligrams(),
        crusher_mass.milligrams(),
        required_energy.nanojoules(),
        stone_charge_ticks,
        charge_ticks,
        crush_ticks,
        overlap_ticks,
        survival_before.metabolic_energy().nanojoules()
            - survival_after.metabolic_energy().nanojoules(),
        survival_before.hydration().microliters() - survival_after.hydration().microliters(),
    );
}
