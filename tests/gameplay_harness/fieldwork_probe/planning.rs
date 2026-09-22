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

pub(super) fn project_sampling_hammer_upgrade_ticks(
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

#[derive(Clone, Copy)]
pub(super) struct FieldworkMiningLimits {
    pub(super) base_quarry_hardness: Pressure,
    pub(super) reinforced_quarry_hardness: Pressure,
    pub(super) reinforced_pick_hardness: Pressure,
    pub(super) base_quarry_batch: Mass,
    pub(super) maximum_candidate_batch: Mass,
}

pub(super) fn fieldwork_mining_limits(registries: &Registries) -> FieldworkMiningLimits {
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("fieldwork hand-pick mining method disappeared"));
    let CapabilityValue::Pressure(base_hardness) = pristine_equipment_capability(
        registries,
        EQUIPMENT_STONE_QUARRY_PICK,
        method.max_hardness_capability(),
    ) else {
        panic!("fieldwork stone quarry hardness capability changed physical kind")
    };
    let CapabilityValue::Pressure(reinforced_hardness) = pristine_equipment_capability(
        registries,
        EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
        method.max_hardness_capability(),
    ) else {
        panic!("fieldwork reinforced quarry hardness capability changed physical kind")
    };
    let CapabilityValue::Mass(base_batch) = pristine_equipment_capability(
        registries,
        EQUIPMENT_STONE_QUARRY_PICK,
        method.max_batch_mass_capability(),
    ) else {
        panic!("fieldwork stone quarry batch capability changed physical kind")
    };
    let maximum_candidate_batch = FIELDWORK_TOOLS
        .iter()
        .map(|tool| {
            let CapabilityValue::Mass(batch) = pristine_equipment_capability(
                registries,
                tool.target,
                method.max_batch_mass_capability(),
            ) else {
                panic!("fieldwork candidate batch capability changed physical kind")
            };
            batch
        })
        .max()
        .unwrap_or_else(|| unreachable!("fieldwork candidate family is nonempty"));
    let CapabilityValue::Pressure(hard_pick_hardness) = pristine_equipment_capability(
        registries,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        method.max_hardness_capability(),
    ) else {
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
        maximum_candidate_batch,
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
    let CapabilityValue::Pressure(maximum) =
        pristine_equipment_capability(registries, tool.target, method.max_hardness_capability())
    else {
        panic!("fieldwork hardness kind changed")
    };
    if observed_upper > maximum {
        return Err(FieldworkToolBlocker::AcquiredHardness {
            upper: observed_upper,
            maximum,
        });
    }
    let CapabilityValue::Mass(batch) =
        pristine_equipment_capability(registries, tool.target, method.max_batch_mass_capability())
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

#[cfg(test)]
pub(super) fn choose_fieldwork_tool(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    observed_upper: Pressure,
    order: Mass,
) -> Option<FieldworkToolEstimate> {
    choose_fieldwork_tool_with_market_phase(
        registries,
        state,
        raw,
        observed_upper,
        order,
        "unspecified",
    )
}

#[derive(Clone, Copy, Debug)]
pub(super) struct FieldworkBulkCrossover {
    pub(super) tool_label: &'static str,
    pub(super) batches: u64,
    pub(super) order: Mass,
}

/// Finds the first representative bulk workload where a heavy quarry tool becomes the actor's
/// preferred visible-state investment. This is diagnostic-only: it does not inspect hidden reserve
/// truth and never feeds back into the current order.
pub(super) fn fieldwork_bulk_crossover(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    observed_upper: Pressure,
    base_batch: Mass,
) -> Option<FieldworkBulkCrossover> {
    const REPRESENTATIVE_BATCHES: [u64; 12] = [1, 2, 4, 8, 16, 24, 32, 40, 48, 64, 80, 96];
    for batches in REPRESENTATIVE_BATCHES {
        let order = multiplied_mass(base_batch, batches, "bulk crossover diagnostic");
        let selected = FIELDWORK_TOOLS
            .iter()
            .filter_map(|&tool| {
                estimate_fieldwork_tool(registries, state, raw, tool, observed_upper, order).ok()
            })
            .min_by_key(FieldworkToolEstimate::policy_key);
        let Some(selected) = selected else {
            continue;
        };
        if matches!(
            selected.tool.target,
            EQUIPMENT_STONE_QUARRY_PICK | EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK
        ) {
            return Some(FieldworkBulkCrossover {
                tool_label: selected.tool.label,
                batches,
                order,
            });
        }
    }
    None
}

pub(super) fn choose_fieldwork_tool_with_market_phase(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    observed_upper: Pressure,
    order: Mass,
    market_phase: &'static str,
) -> Option<FieldworkToolEstimate> {
    let mut viable = Vec::new();
    for tool in FIELDWORK_TOOLS {
        let estimate = estimate_fieldwork_tool(registries, state, raw, tool, observed_upper, order);
        reviewln!(
            "FIELDWORK CANDIDATE tick={} tool={} observed-upper={}Pa order={}mg estimate={estimate:?} scope=four-raw-build-tools authorization=not-yet assumptions=no-service,caller-supplied-visible-workload",
            state.tick().value(),
            tool.label,
            observed_upper.pascals(),
            order.milligrams()
        );
        if let Ok(estimate) = estimate {
            viable.push(estimate);
        }
    }
    let selected = viable
        .iter()
        .min_by_key(|estimate| estimate.policy_key())
        .cloned();
    if let Some(selected) = &selected {
        let heavy = viable
            .iter()
            .filter(|estimate| {
                matches!(
                    estimate.tool.target,
                    EQUIPMENT_STONE_QUARRY_PICK | EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK
                )
            })
            .min_by_key(|estimate| estimate.policy_key());
        if let Some(heavy) = heavy {
            let preparation_extra =
                i128::from(heavy.preparation_ticks) - i128::from(selected.preparation_ticks);
            let order_saving = i128::from(selected.order_ticks) - i128::from(heavy.order_ticks);
            let total_delta = i128::from(heavy.total_ticks()) - i128::from(selected.total_ticks());
            reviewln!(
                "FIELDWORK TOOL MARKET phase={market_phase} selected={} selected-total={}t heavy-best={} heavy-total={}t heavy-preparation-extra={preparation_extra:+}t heavy-order-saving={order_saving:+}t heavy-total-delta={total_delta:+}t heavy-investment={}",
                selected.tool.label,
                selected.total_ticks(),
                heavy.tool.label,
                heavy.total_ticks(),
                if heavy.tool.target == selected.tool.target {
                    "selected"
                } else {
                    "deferred"
                },
            );
        } else {
            reviewln!(
                "FIELDWORK TOOL MARKET phase={market_phase} selected={} selected-total={}t heavy-best=unavailable heavy-investment=unavailable",
                selected.tool.label,
                selected.total_ticks(),
            );
        }
    }
    selected
}
