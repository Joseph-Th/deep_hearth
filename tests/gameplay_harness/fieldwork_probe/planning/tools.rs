//! Extraction-tool capability, cost, and market planning for fieldwork.

use super::super::*;
use super::materials::{add_mass, equipment_component_requirements, multiplied_mass};

#[derive(Clone, Copy)]
pub(in super::super) struct FieldworkMiningLimits {
    pub(in super::super) base_quarry_hardness: Pressure,
    pub(in super::super) reinforced_quarry_hardness: Pressure,
    pub(in super::super) reinforced_pick_hardness: Pressure,
    pub(in super::super) base_quarry_batch: Mass,
    pub(in super::super) maximum_candidate_batch: Mass,
}

pub(in super::super) fn fieldwork_mining_limits(registries: &Registries) -> FieldworkMiningLimits {
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
pub(in super::super) struct FieldworkTool {
    pub(in super::super) base: EquipmentDefinitionId,
    pub(in super::super) target: EquipmentDefinitionId,
    pub(in super::super) label: &'static str,
}

// A bounded actor family, not an exhaustive equipment catalog. Equal observable costs prefer
// the light stone pick, then its reinforcement, then the corresponding heavy quarry tools.
pub(in super::super) const FIELDWORK_TOOLS: [FieldworkTool; 4] = [
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
pub(in super::super) struct FieldworkToolEstimate {
    pub(in super::super) tool: FieldworkTool,
    pub(in super::super) preparation_ticks: u64,
    pub(in super::super) order_ticks: u64,
    pub(in super::super) batch: Mass,
    pub(in super::super) raw: BTreeMap<CommodityKey, Mass>,
}

impl FieldworkToolEstimate {
    pub(in super::super) fn total_ticks(&self) -> u64 {
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
pub(in super::super) enum FieldworkToolBlocker {
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

pub(in super::super) fn estimate_fieldwork_tool(
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
pub(in super::super) fn choose_fieldwork_tool(
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
pub(in super::super) struct FieldworkBulkCrossover {
    pub(in super::super) tool_label: &'static str,
    pub(in super::super) batches: u64,
    pub(in super::super) order: Mass,
}

/// Finds the first representative bulk workload where a heavy quarry tool becomes the actor's
/// preferred visible-state investment. This is diagnostic-only: it does not inspect hidden reserve
/// truth and never feeds back into the current order.
pub(in super::super) fn fieldwork_bulk_crossover(
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

pub(in super::super) fn choose_fieldwork_tool_with_market_phase(
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
