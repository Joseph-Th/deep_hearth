//! Raw-material opportunity and sampling-instrument upgrade planning for fieldwork.

use super::super::*;

pub(super) fn add_mass(
    totals: &mut BTreeMap<CommodityKey, Mass>,
    commodity: CommodityKey,
    mass: Mass,
    context: &'static str,
) {
    let next = totals
        .get(&commodity)
        .copied()
        .unwrap_or(Mass::ZERO)
        .checked_add(mass)
        .unwrap_or_else(|| panic!("fieldwork {context} mass overflowed"));
    totals.insert(commodity, next);
}

pub(in super::super) fn multiplied_mass(mass: Mass, batches: u64, context: &'static str) -> Mass {
    Mass::from_milligrams(
        mass.milligrams()
            .checked_mul(batches)
            .unwrap_or_else(|| panic!("fieldwork {context} mass overflowed")),
    )
}

pub(in super::super) fn equipment_component_requirements(
    registries: &Registries,
    equipment_definitions: &[EquipmentDefinitionId],
) -> BTreeMap<CommodityKey, Mass> {
    let mut requirements = BTreeMap::new();
    for &equipment in equipment_definitions {
        let assembly = registries
            .equipment()
            .get_equipment(equipment)
            .and_then(|definition| definition.assembly_profile())
            .unwrap_or_else(|| {
                panic!(
                    "fieldwork equipment {} lost its ordinary authored assembly",
                    equipment.value()
                )
            });
        for input in assembly.inputs() {
            add_mass(
                &mut requirements,
                input.commodity(),
                input.mass(),
                "tool-component requirement",
            );
        }
    }
    requirements
}

fn upgrade_raw_requirements(
    registries: &Registries,
    target: EquipmentDefinitionId,
    expected_base: EquipmentDefinitionId,
    context: &'static str,
) -> (BTreeMap<CommodityKey, Mass>, Mass) {
    let upgrade = registries
        .equipment()
        .get_equipment(target)
        .and_then(|definition| definition.upgrade_profile())
        .unwrap_or_else(|| {
            panic!(
                "fieldwork reinforced equipment {} lost its authored upgrade",
                target.value()
            )
        });
    assert_eq!(upgrade.from(), expected_base);
    let mut raw = BTreeMap::new();
    let mut total_raw = Mass::ZERO;
    for input in upgrade.additions().inputs() {
        let (craft, batches) = manual_craft_topology_plan_for_output(
            registries,
            input.commodity(),
            input.mass(),
            context,
        );
        let consumed = multiplied_mass(craft.input_mass(), batches, context);
        add_mass(&mut raw, craft.input(), consumed, context);
        total_raw = total_raw
            .checked_add(consumed)
            .unwrap_or_else(|| panic!("fieldwork {context} total raw mass overflowed"));
    }
    (raw, total_raw)
}

fn merge_maximum_requirements(
    target: &mut BTreeMap<CommodityKey, Mass>,
    candidate: &BTreeMap<CommodityKey, Mass>,
) {
    for (&commodity, &mass) in candidate {
        let entry = target.entry(commodity).or_insert(Mass::ZERO);
        *entry = (*entry).max(mass);
    }
}

pub(in super::super) fn fieldwork_raw_opportunity(
    registries: &Registries,
) -> (BTreeMap<CommodityKey, Mass>, Mass) {
    let mut raw = BTreeMap::new();
    let mut parts_capacity = Mass::ZERO;
    for (commodity, required) in equipment_component_requirements(
        registries,
        &[
            EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
            EQUIPMENT_STONE_QUARRY_PICK,
            EQUIPMENT_STONE_PICK,
        ],
    ) {
        let (craft, batches) = manual_craft_topology_plan_for_output(
            registries,
            commodity,
            required,
            "field-tool component planning",
        );
        let consumed = multiplied_mass(craft.input_mass(), batches, "field-tool raw input");
        add_mass(
            &mut raw,
            craft.input(),
            consumed,
            "field-tool raw opportunity",
        );
        parts_capacity = parts_capacity
            .checked_add(consumed)
            .unwrap_or_else(|| panic!("fieldwork parts capacity overflowed"));
    }

    // The quarry and hard-pick reinforcements are mutually exclusive extraction choices. Reserve
    // the component-wise maximum raw bill for one of them, not the sum of both alternatives.
    let (quarry_upgrade, quarry_upgrade_mass) = upgrade_raw_requirements(
        registries,
        EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
        EQUIPMENT_STONE_QUARRY_PICK,
        "fieldwork quarry reinforcement planning",
    );
    let (hard_pick_upgrade, hard_pick_upgrade_mass) = upgrade_raw_requirements(
        registries,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        EQUIPMENT_STONE_PICK,
        "fieldwork hard-pick reinforcement planning",
    );
    let mut mining_upgrade = BTreeMap::new();
    merge_maximum_requirements(&mut mining_upgrade, &quarry_upgrade);
    merge_maximum_requirements(&mut mining_upgrade, &hard_pick_upgrade);
    for (&commodity, &mass) in &mining_upgrade {
        add_mass(
            &mut raw,
            commodity,
            mass,
            "fieldwork alternative mining reinforcement reserve",
        );
    }
    parts_capacity = parts_capacity
        .checked_add(quarry_upgrade_mass.max(hard_pick_upgrade_mass))
        .unwrap_or_else(|| panic!("fieldwork mining reinforcement parts capacity overflowed"));

    // A second reinforcement parcel is a distinct information investment: after the extraction
    // tool is chosen, it may upgrade the geological hammer for repeated-site indexed surveying.
    let (hammer_upgrade, hammer_upgrade_mass) = upgrade_raw_requirements(
        registries,
        EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER,
        EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
        "fieldwork sampling-hammer reinforcement planning",
    );
    for (commodity, mass) in hammer_upgrade {
        add_mass(
            &mut raw,
            commodity,
            mass,
            "fieldwork sampling-hammer reinforcement reserve",
        );
    }
    parts_capacity = parts_capacity
        .checked_add(hammer_upgrade_mass)
        .unwrap_or_else(|| panic!("fieldwork sampling reinforcement parts capacity overflowed"));
    (raw, parts_capacity)
}

pub(in super::super) fn project_sampling_hammer_upgrade_ticks(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    parts: StockpileId,
) -> Option<u64> {
    let upgrade = registries
        .equipment()
        .get_equipment(EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER)
        .and_then(|definition| definition.upgrade_profile())
        .unwrap_or_else(|| panic!("fieldwork reinforced sampling hammer lost authored upgrade"));
    assert_eq!(upgrade.from(), EQUIPMENT_STONE_GEOLOGICAL_HAMMER);

    let parts_record = state
        .inventory()
        .get_stockpile(parts)
        .unwrap_or_else(|| panic!("fieldwork parts stockpile disappeared"));
    let raw_record = state
        .inventory()
        .get_stockpile(raw)
        .unwrap_or_else(|| panic!("fieldwork raw stockpile disappeared"));
    let mut raw_required = BTreeMap::<CommodityKey, Mass>::new();
    for input in upgrade.additions().inputs() {
        let available = parts_record.get_mass(input.commodity());
        if available >= input.mass() {
            continue;
        }
        let missing = input.mass().checked_sub(available).unwrap_or_else(|| {
            unreachable!("fieldwork sampling upgrade checked parts availability")
        });
        let (craft, batches) = manual_craft_topology_plan_for_output(
            registries,
            input.commodity(),
            missing,
            "fieldwork sampling-hammer upgrade projection",
        );
        add_mass(
            &mut raw_required,
            craft.input(),
            multiplied_mass(
                craft.input_mass(),
                batches,
                "sampling-upgrade projection input",
            ),
            "sampling-upgrade projection input",
        );
    }
    if raw_required
        .iter()
        .any(|(&commodity, &required)| raw_record.get_mass(commodity) < required)
    {
        return None;
    }
    Some(
        project_manual_assembly_package(
            registries,
            state,
            &[raw],
            parts,
            &[upgrade.additions()],
            "fieldwork sampling-hammer upgrade projection",
        )
        .attention_ticks,
    )
}
