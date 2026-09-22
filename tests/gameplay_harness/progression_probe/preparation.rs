//! Primitive material preparation and shared early-work execution helpers.

use super::*;

pub(super) fn craft_batches(
    registries: &Registries,
    state: &mut AppState,
    process: deep_hearth::production::ProcessId,
    source: deep_hearth::inventory::StockpileId,
    destination: deep_hearth::inventory::StockpileId,
    batches: u64,
) {
    let craft = select_manual_craft_request(
        registries,
        state,
        process,
        source,
        batches,
        "primitive progression repeated craft",
    );
    let job = validate_start_manual_craft(
        registries,
        state,
        ManualCraftStartRequest::new(craft, destination),
    )
    .unwrap_or_else(|error| panic!("primitive progression repeated craft failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("primitive progression repeated craft commit failed: {error}"));
    finish_uninterrupted_production_job(
        registries,
        state,
        job,
        "primitive progression manual craft",
    );
}

pub(super) fn finish_mining_work(
    registries: &Registries,
    state: &mut AppState,
    job: deep_hearth::mining::MiningJobId,
    concurrent_production: Option<ProductionJobId>,
    context: &'static str,
) -> u64 {
    let record = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("primitive progression {context} mining job disappeared"));
    let ticks = duration(record.started_at().value(), record.completes_at().value());
    for elapsed in 1..=ticks {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("primitive progression {context} tick failed: {error}"));
        assert!(
            outcome.production_availability_changes().is_empty(),
            "primitive progression {context} encountered an unexpected production availability change"
        );
        assert!(
            outcome.production_completions().iter().all(|completion| {
                concurrent_production.is_some_and(|expected| completion.job() == expected)
            }),
            "primitive progression {context} observed an unrelated production completion"
        );
        if elapsed < ticks {
            assert!(
                !outcome.ready_mining_jobs().contains(&job),
                "primitive progression {context} mining became ready before its validated completion"
            );
            assert_eq!(
                state.player_work().active(),
                Some(deep_hearth::labor::PlayerWork::Mining { job })
            );
        } else {
            assert_eq!(
                outcome.ready_mining_jobs(),
                &[job],
                "primitive progression {context} must expose the completed mining job exactly once"
            );
            assert_eq!(state.player_work().active(), None);
        }
    }
    ticks
}

pub(super) fn multiply_mass(mass: Mass, count: u64, context: &'static str) -> Mass {
    let milligrams = mass
        .milligrams()
        .checked_mul(count)
        .unwrap_or_else(|| panic!("primitive progression {context} mass overflowed"));
    Mass::from_milligrams(milligrams)
}

pub(super) fn add_mass(total: &mut Mass, amount: Mass, context: &'static str) {
    *total = total
        .checked_add(amount)
        .unwrap_or_else(|| panic!("primitive progression {context} mass overflowed"));
}

pub(super) fn add_profile_requirements(
    requirements: &mut BTreeMap<CommodityKey, Mass>,
    profile: &MaterialAssemblyProfile,
) {
    for input in profile.inputs() {
        let entry = requirements.entry(input.commodity()).or_insert(Mass::ZERO);
        add_mass(entry, input.mass(), "assembly requirement");
    }
}

#[derive(Debug)]
pub(super) struct PrimitiveMaterialPlan {
    pub(super) raw_inputs: Vec<(CommodityKey, Mass)>,
    pub(super) raw_capacity: Mass,
    pub(super) shaped_capacity: Mass,
    pub(super) native_copper: Mass,
}

pub(super) fn primitive_material_plan(registries: &Registries) -> PrimitiveMaterialPlan {
    let mut requirements = BTreeMap::new();
    for equipment in [
        EQUIPMENT_STONE_PICK,
        EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
        EQUIPMENT_STONE_HAND_CRANK,
        EQUIPMENT_STONE_CRUSHER,
        EQUIPMENT_STONE_SEPARATOR,
    ] {
        let profile = registries
            .equipment()
            .get_equipment(equipment)
            .and_then(|definition| definition.assembly_profile())
            .unwrap_or_else(|| {
                panic!(
                    "primitive progression equipment {} lost its authored assembly profile",
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
    add_profile_requirements(&mut requirements, crank_additions);
    let pick_service = registries
        .equipment()
        .get_equipment(EQUIPMENT_COPPER_REINFORCED_PICK)
        .and_then(|definition| definition.maintenance_profile())
        .unwrap_or_else(|| panic!("primitive reinforced pick lost its maintenance profile"));
    assert!(
        pick_service.is_component_replacement(),
        "primitive reinforced pick service must exchange an embodied component"
    );
    let service_entry = requirements
        .entry(pick_service.replacement())
        .or_insert(Mass::ZERO);
    add_mass(
        service_entry,
        pick_service.full_service_replacement_mass(),
        "primitive pick service reserve",
    );

    let mut process_batches: BTreeMap<deep_hearth::production::ProcessId, u64> = BTreeMap::new();
    for (commodity, required) in requirements {
        let (craft, batches) = manual_craft_topology_plan_for_output(
            registries,
            commodity,
            required,
            "primitive progression component planning",
        );
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

pub(super) fn craft_requirement(
    registries: &Registries,
    state: &mut AppState,
    raw_source: deep_hearth::inventory::StockpileId,
    native_source: deep_hearth::inventory::StockpileId,
    destination: deep_hearth::inventory::StockpileId,
    commodity: CommodityKey,
    required: Mass,
) {
    let available = state
        .inventory()
        .get_stockpile(destination)
        .map(|stockpile| stockpile.get_mass(commodity))
        .unwrap_or_else(|| panic!("primitive progression shaped stockpile disappeared"));
    if available >= required {
        return;
    }
    let missing = required
        .checked_sub(available)
        .unwrap_or_else(|| unreachable!("available component mass was already checked"));
    let (craft, batches, source) = manual_craft_plan_for_available_output(
        registries,
        state,
        &[raw_source, native_source],
        commodity,
        missing,
        "primitive just-in-time component planning",
    );
    craft_batches(
        registries,
        state,
        craft.process(),
        source,
        destination,
        batches,
    );
}

pub(super) fn craft_for_profile(
    registries: &Registries,
    state: &mut AppState,
    raw_source: deep_hearth::inventory::StockpileId,
    native_source: deep_hearth::inventory::StockpileId,
    destination: deep_hearth::inventory::StockpileId,
    profile: &MaterialAssemblyProfile,
) {
    for input in profile.inputs() {
        craft_requirement(
            registries,
            state,
            raw_source,
            native_source,
            destination,
            input.commodity(),
            input.mass(),
        );
    }
}

pub(super) fn equipment_assembly_profile(
    registries: &Registries,
    equipment: deep_hearth::equipment::EquipmentDefinitionId,
) -> &MaterialAssemblyProfile {
    registries
        .equipment()
        .get_equipment(equipment)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| {
            panic!(
                "primitive progression equipment {} has no authored assembly profile",
                equipment.value()
            )
        })
}

pub(super) fn equipment_upgrade_additions(
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

pub(super) fn stone_pick_mining_batch_limit(registries: &Registries) -> Mass {
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

pub(super) fn nominal_equipment_pressure_capability(
    registries: &Registries,
    equipment: deep_hearth::equipment::EquipmentDefinitionId,
    capability: CapabilityId,
) -> Pressure {
    match pristine_equipment_capability(registries, equipment, capability) {
        CapabilityValue::Pressure(pressure) => pressure,
        value @ (CapabilityValue::Mass(_)
        | CapabilityValue::Temperature(_)
        | CapabilityValue::Power(_)
        | CapabilityValue::MassFlow(_)) => panic!(
            "primitive progression expected pressure capability {} on equipment {} but found {:?}",
            capability.value(),
            equipment.value(),
            value.kind()
        ),
    }
}

pub(super) fn mining_hardness_limits(registries: &Registries) -> (Pressure, Pressure, Pressure) {
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
