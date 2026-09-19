//! Fieldwork tool opportunity, feasibility, and pre-action cost planning.

use super::*;

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

pub(super) fn multiplied_mass(mass: Mass, batches: u64, context: &'static str) -> Mass {
    Mass::from_milligrams(
        mass.milligrams()
            .checked_mul(batches)
            .unwrap_or_else(|| panic!("fieldwork {context} mass overflowed")),
    )
}

pub(super) fn equipment_component_requirements(
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

pub(super) fn fieldwork_raw_opportunity(
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

    for (target, expected_base) in [
        (
            EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
            EQUIPMENT_STONE_QUARRY_PICK,
        ),
        (EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_STONE_PICK),
    ] {
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
        for input in upgrade.additions().inputs() {
            let (craft, batches) = manual_craft_topology_plan_for_output(
                registries,
                input.commodity(),
                input.mass(),
                "fieldwork reinforcement planning",
            );
            let upgrade_raw =
                multiplied_mass(craft.input_mass(), batches, "reinforcement raw input");
            add_mass(
                &mut raw,
                craft.input(),
                upgrade_raw,
                "reinforcement raw opportunity",
            );
            parts_capacity = parts_capacity
                .checked_add(upgrade_raw)
                .unwrap_or_else(|| panic!("fieldwork reinforcement parts capacity overflowed"));
        }
    }
    (raw, parts_capacity)
}

#[derive(Clone, Copy)]
pub(super) struct FieldworkMiningLimits {
    pub(super) base_quarry_hardness: Pressure,
    pub(super) reinforced_quarry_hardness: Pressure,
    pub(super) reinforced_pick_hardness: Pressure,
    pub(super) base_quarry_batch: Mass,
}

pub(super) fn fieldwork_mining_limits(registries: &Registries) -> FieldworkMiningLimits {
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("fieldwork hand-pick mining method disappeared"));
    let resolve = |equipment| {
        registries
            .equipment()
            .get_equipment(equipment)
            .unwrap_or_else(|| {
                panic!(
                    "fieldwork quarry equipment {} disappeared",
                    equipment.value()
                )
            })
    };
    let base = resolve(EQUIPMENT_STONE_QUARRY_PICK);
    let reinforced = resolve(EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK);
    let hard_pick = resolve(EQUIPMENT_COPPER_REINFORCED_PICK);
    let CapabilityValue::Pressure(base_hardness) = base
        .capabilities()
        .get_capability(method.max_hardness_capability())
        .unwrap_or_else(|| panic!("fieldwork stone quarry pick lost mining-hardness capability"))
    else {
        panic!("fieldwork stone quarry hardness capability changed physical kind")
    };
    let CapabilityValue::Pressure(reinforced_hardness) = reinforced
        .capabilities()
        .get_capability(method.max_hardness_capability())
        .unwrap_or_else(|| {
            panic!("fieldwork reinforced quarry pick lost mining-hardness capability")
        })
    else {
        panic!("fieldwork reinforced quarry hardness capability changed physical kind")
    };
    let CapabilityValue::Mass(base_batch) = base
        .capabilities()
        .get_capability(method.max_batch_mass_capability())
        .unwrap_or_else(|| panic!("fieldwork stone quarry pick lost mining-batch capability"))
    else {
        panic!("fieldwork stone quarry batch capability changed physical kind")
    };
    let CapabilityValue::Pressure(hard_pick_hardness) = hard_pick
        .capabilities()
        .get_capability(method.max_hardness_capability())
        .unwrap_or_else(|| panic!("fieldwork reinforced pick lost mining-hardness capability"))
    else {
        panic!("fieldwork reinforced pick hardness capability changed physical kind")
    };
    assert!(
        reinforced_hardness > base_hardness,
        "fieldwork requires quarry reinforcement to open a harder geological opportunity"
    );
    assert!(
        hard_pick_hardness > reinforced_hardness,
        "fieldwork requires the light reinforced pick to retain a distinct hard-rock niche"
    );
    FieldworkMiningLimits {
        base_quarry_hardness: base_hardness,
        reinforced_quarry_hardness: reinforced_hardness,
        reinforced_pick_hardness: hard_pick_hardness,
        base_quarry_batch: base_batch,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FieldworkTool {
    pub(super) base: EquipmentDefinitionId,
    pub(super) target: EquipmentDefinitionId,
    pub(super) label: &'static str,
}

// A bounded actor family, not an exhaustive equipment catalog. Equal observable costs prefer
// the light stone pick, then its reinforcement, then the corresponding heavy quarry tools.
pub(super) const FIELDWORK_TOOLS: [FieldworkTool; 4] = [
    FieldworkTool {
        base: EQUIPMENT_STONE_PICK,
        target: EQUIPMENT_STONE_PICK,
        label: "stone-pick",
    },
    FieldworkTool {
        base: EQUIPMENT_STONE_PICK,
        target: EQUIPMENT_COPPER_REINFORCED_PICK,
        label: "copper-reinforced-hard-pick",
    },
    FieldworkTool {
        base: EQUIPMENT_STONE_QUARRY_PICK,
        target: EQUIPMENT_STONE_QUARRY_PICK,
        label: "stone-quarry",
    },
    FieldworkTool {
        base: EQUIPMENT_STONE_QUARRY_PICK,
        target: EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
        label: "copper-reinforced-quarry",
    },
];

#[derive(Clone, Debug)]
pub(super) struct FieldworkToolEstimate {
    pub(super) tool: FieldworkTool,
    pub(super) preparation_ticks: u64,
    pub(super) order_ticks: u64,
    pub(super) batch: Mass,
    pub(super) raw: BTreeMap<CommodityKey, Mass>,
}

impl FieldworkToolEstimate {
    pub(super) fn total_ticks(&self) -> u64 {
        self.preparation_ticks
            .checked_add(self.order_ticks)
            .unwrap_or_else(|| panic!("fieldwork estimated attention overflowed"))
    }

    fn policy_key(&self) -> (u64, Mass, Mass) {
        let copper = self
            .raw
            .get(&CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL))
            .copied()
            .unwrap_or(Mass::ZERO);
        let raw = self
            .raw
            .values()
            .copied()
            .try_fold(Mass::ZERO, Mass::checked_add)
            .unwrap_or_else(|| panic!("fieldwork raw estimate overflowed"));
        (self.total_ticks(), copper, raw)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum FieldworkToolBlocker {
    AcquiredHardness {
        upper: Pressure,
        maximum: Pressure,
    },
    RawInput {
        commodity: CommodityKey,
        required: Mass,
    },
    Order(deep_hearth::mining::MiningOrderError),
}

fn estimate_tool_preparation(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    tool: FieldworkTool,
) -> Result<(u64, BTreeMap<CommodityKey, Mass>), FieldworkToolBlocker> {
    // Assembly and upgrade execute as separate crafts. Preserve their batch rounding rather
    // than merging a shared component into one cheaper hypothetical preparation step.
    let mut requirements: Vec<_> = equipment_component_requirements(registries, &[tool.base])
        .into_iter()
        .collect();
    if tool.target != tool.base {
        let upgrade = registries
            .equipment()
            .get_equipment(tool.target)
            .and_then(|definition| definition.upgrade_profile())
            .unwrap_or_else(|| panic!("fieldwork upgrade disappeared"));
        assert_eq!(upgrade.from(), tool.base);
        for input in upgrade.additions().inputs() {
            requirements.push((input.commodity(), input.mass()));
        }
    }
    let mut raw_required = BTreeMap::new();
    let mut ticks = 0_u64;
    for (commodity, required) in requirements {
        // The declared raw-tool family uses its equipment-free topology route. Missing raw
        // inputs exclude this route; they do not prove that every possible salvage route fails.
        let (craft, batches) = manual_craft_topology_plan_for_output(
            registries,
            commodity,
            required,
            "fieldwork pre-action components",
        );
        let consumed = multiplied_mass(craft.input_mass(), batches, "planned raw input");
        add_mass(
            &mut raw_required,
            craft.input(),
            consumed,
            "planned cumulative raw input",
        );
        let cumulative = raw_required[&craft.input()];
        if first_sufficient_pure_temperature(
            state,
            raw,
            craft.input(),
            cumulative,
            "fieldwork pre-action raw availability",
        )
        .is_none()
        {
            return Err(FieldworkToolBlocker::RawInput {
                commodity: craft.input(),
                required: cumulative,
            });
        }
        let request = select_manual_craft_request(
            registries,
            state,
            craft.process(),
            raw,
            batches,
            "fieldwork pre-action craft",
        );
        let resolution =
            resolve_manual_craft(registries, state, &request).unwrap_or_else(|error| {
                panic!("fieldwork pre-action craft resolution failed: {error}")
            });
        ticks = ticks
            .checked_add(resolution.duration().value())
            .unwrap_or_else(|| panic!("fieldwork preparation estimate overflowed"));
    }
    Ok((ticks, raw_required))
}

pub(super) fn estimate_fieldwork_tool(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    tool: FieldworkTool,
    observed_upper: Pressure,
    order: Mass,
) -> Result<FieldworkToolEstimate, FieldworkToolBlocker> {
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("fieldwork mining method disappeared"));
    let definition = registries
        .equipment()
        .get_equipment(tool.target)
        .unwrap_or_else(|| panic!("fieldwork candidate disappeared"));
    let capabilities = definition.capabilities();
    let CapabilityValue::Pressure(maximum) = capabilities
        .get_capability(method.max_hardness_capability())
        .unwrap_or_else(|| panic!("fieldwork candidate hardness disappeared"))
    else {
        panic!("fieldwork hardness kind changed")
    };
    if observed_upper > maximum {
        return Err(FieldworkToolBlocker::AcquiredHardness {
            upper: observed_upper,
            maximum,
        });
    }
    let CapabilityValue::Mass(batch) = capabilities
        .get_capability(method.max_batch_mass_capability())
        .unwrap_or_else(|| panic!("fieldwork candidate batch disappeared"))
    else {
        panic!("fieldwork batch kind changed")
    };
    // Planning uses the same sequential wear and per-batch rounding as mining admission.
    // The bounded projection promises effort, not hidden supply or current authorization.
    let projection = resolve_mining_order(
        registries.core().physical_tick_duration(),
        method,
        definition,
        MiningOrderRequest::new(
            deep_hearth::maintenance::Condition::PRISTINE,
            observed_upper,
            order,
            batch,
            256,
        ),
    )
    .map_err(FieldworkToolBlocker::Order)?;
    let (preparation_ticks, raw) = estimate_tool_preparation(registries, state, raw, tool)?;
    Ok(FieldworkToolEstimate {
        tool,
        preparation_ticks,
        order_ticks: projection.duration().value(),
        batch,
        raw,
    })
}

pub(super) fn choose_fieldwork_tool(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    observed_upper: Pressure,
    order: Mass,
) -> Option<FieldworkToolEstimate> {
    let mut selected: Option<FieldworkToolEstimate> = None;
    for tool in FIELDWORK_TOOLS {
        let estimate = estimate_fieldwork_tool(registries, state, raw, tool, observed_upper, order);
        reviewln!(
            "FIELDWORK CANDIDATE tick={} tool={} observed-upper={}Pa order={}mg estimate={estimate:?} scope=four-raw-build-tools authorization=not-yet assumptions=no-service,unknown-deposit-reserve",
            state.tick().value(),
            tool.label,
            observed_upper.pascals(),
            order.milligrams()
        );
        let Ok(estimate) = estimate else { continue };
        if selected
            .as_ref()
            .is_none_or(|best| estimate.policy_key() < best.policy_key())
        {
            selected = Some(estimate);
        }
    }
    selected
}
