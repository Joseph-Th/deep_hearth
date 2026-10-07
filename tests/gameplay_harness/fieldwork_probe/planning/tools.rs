//! Extraction-tool capability, cost, and market planning for fieldwork.

use super::super::*;
use super::materials::{
    add_mass, disclosed_raw_inputs, equipment_component_requirements, multiplied_mass,
};
use crate::bulk_fieldwork_workload::BULK_FIELDWORK_ORDER_MAX_BATCHES;

pub(in super::super) const FIELDWORK_ORDER_MAX_BATCHES: u64 = 256;

#[derive(Clone, Copy)]
pub(in super::super) struct FieldworkMiningLimits {
    pub(in super::super) base_quarry_hardness: Pressure,
    pub(in super::super) reinforced_quarry_hardness: Pressure,
    pub(in super::super) reinforced_pick_hardness: Pressure,
    pub(in super::super) base_quarry_batch: Mass,
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
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct FieldworkTool {
    pub(in super::super) target: EquipmentDefinitionId,
}

/// Current portable hand-pick providers that the actor can assemble directly from authored content.
///
/// The gameplay probe must discover this market from the same registry a player-facing catalog
/// would inspect. Adding a valid provider therefore expands the actor's candidate frame instead of
/// making the harness stale until a hard-coded list is updated.
pub(in super::super) fn fieldwork_tools(registries: &Registries) -> Vec<FieldworkTool> {
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("fieldwork hand-pick method disappeared"));
    registries
        .equipment()
        .definitions()
        .filter(|definition| {
            !definition.requires_structural_support()
                && definition.assembly_profile().is_some()
                && matches!(
                    definition
                        .capabilities()
                        .get_capability(method.mass_flow_capability()),
                    Some(CapabilityValue::MassFlow(flow)) if !flow.is_zero()
                )
                && matches!(
                    definition
                        .capabilities()
                        .get_capability(method.max_batch_mass_capability()),
                    Some(CapabilityValue::Mass(batch)) if !batch.is_zero()
                )
                && matches!(
                    definition
                        .capabilities()
                        .get_capability(method.max_hardness_capability()),
                    Some(CapabilityValue::Pressure(hardness)) if !hardness.is_zero()
                )
        })
        .map(|definition| FieldworkTool {
            target: definition.id(),
        })
        .collect()
}

/// Current portable hand-pick upgrades that can be applied to already-owned equipment.
///
/// Upgrade-only definitions belong in reassessment even when they intentionally have no direct
/// assembly route. Fresh-tool planning stays on `fieldwork_tools`, which requires direct assembly.
pub(in super::super) fn fieldwork_upgrade_tools(registries: &Registries) -> Vec<FieldworkTool> {
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("fieldwork hand-pick method disappeared"));
    registries
        .equipment()
        .definitions()
        .filter(|definition| {
            !definition.requires_structural_support()
                && definition.upgrade_profile().is_some()
                && matches!(
                    definition
                        .capabilities()
                        .get_capability(method.mass_flow_capability()),
                    Some(CapabilityValue::MassFlow(flow)) if !flow.is_zero()
                )
                && matches!(
                    definition
                        .capabilities()
                        .get_capability(method.max_batch_mass_capability()),
                    Some(CapabilityValue::Mass(batch)) if !batch.is_zero()
                )
                && matches!(
                    definition
                        .capabilities()
                        .get_capability(method.max_hardness_capability()),
                    Some(CapabilityValue::Pressure(hardness)) if !hardness.is_zero()
                )
        })
        .map(|definition| FieldworkTool {
            target: definition.id(),
        })
        .collect()
}

/// Distinct hand-pick hardness limits reachable from the current portable fresh-build and upgrade
/// market. World generation uses this frontier so adding a legitimate extraction tier also adds a
/// geological pressure band instead of leaving the new capability invisible to organic play.
pub(in super::super) fn fieldwork_hardness_frontier(registries: &Registries) -> Vec<Pressure> {
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("fieldwork hand-pick method disappeared"));
    let mut frontier = fieldwork_tools(registries)
        .into_iter()
        .chain(fieldwork_upgrade_tools(registries))
        .map(|tool| {
            let CapabilityValue::Pressure(hardness) = pristine_equipment_capability(
                registries,
                tool.target,
                method.max_hardness_capability(),
            ) else {
                panic!("fieldwork discovered hardness capability changed physical kind")
            };
            hardness
        })
        .collect::<Vec<_>>();
    frontier.sort_by_key(|hardness| hardness.pascals());
    frontier.dedup();
    assert!(
        !frontier.is_empty(),
        "fieldwork current portable hand-pick market has no hardness frontier"
    );
    frontier
}

pub(in super::super) fn fieldwork_tool_label(tool: FieldworkTool) -> String {
    match tool.target {
        EQUIPMENT_STONE_PICK => "stone-pick".to_owned(),
        EQUIPMENT_COPPER_REINFORCED_PICK => "copper-reinforced-hard-pick".to_owned(),
        EQUIPMENT_STONE_QUARRY_PICK => "stone-quarry".to_owned(),
        EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK => "copper-reinforced-quarry".to_owned(),
        other => format!("equipment-{}", other.value()),
    }
}

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

    pub(in super::super) fn actor_cost_key(&self) -> (u64, Mass, Mass) {
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

    fn policy_key(&self) -> (u64, Mass, Mass) {
        self.actor_cost_key()
    }
}

fn select_unique_best_tool(
    candidates: impl IntoIterator<Item = FieldworkToolEstimate>,
    context: &'static str,
) -> Option<FieldworkToolEstimate> {
    let candidates = candidates.into_iter().collect::<Vec<_>>();
    let best_key = candidates
        .iter()
        .map(FieldworkToolEstimate::policy_key)
        .min()?;
    let mut best = candidates
        .into_iter()
        .filter(|candidate| candidate.policy_key() == best_key);
    let selected = best
        .next()
        .unwrap_or_else(|| unreachable!("best fieldwork key came from a candidate"));
    assert!(
        best.next().is_none(),
        "fieldwork {context} has multiple tools tied on every actor-visible policy cost; add an explicit actor preference instead of relying on catalog order"
    );
    Some(selected)
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
    ConstructionRoute {
        commodity: CommodityKey,
    },
    Order(deep_hearth::mining::MiningOrderError),
}

fn estimate_component_preparation(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    parts: StockpileId,
    requirements: impl IntoIterator<Item = (CommodityKey, Mass)>,
) -> Result<(u64, BTreeMap<CommodityKey, Mass>), FieldworkToolBlocker> {
    let parts_record = state
        .inventory()
        .get_stockpile(parts)
        .unwrap_or_else(|| panic!("fieldwork parts stockpile disappeared during tool planning"));
    let mut remaining_parts = BTreeMap::<CommodityKey, Mass>::new();
    let mut raw_required = BTreeMap::new();
    let mut ticks = 0_u64;
    for (commodity, required) in requirements {
        let available = remaining_parts
            .entry(commodity)
            .or_insert_with(|| parts_record.get_mass(commodity));
        let reused = (*available).min(required);
        *available = available
            .checked_sub(reused)
            .unwrap_or_else(|| unreachable!("fieldwork reusable component mass is bounded"));
        let missing = required.checked_sub(reused).unwrap_or_else(|| {
            unreachable!("fieldwork reused component mass cannot exceed demand")
        });
        if missing.is_zero() {
            continue;
        }
        let route = manual_construction_route_from_roots(
            registries,
            commodity,
            missing,
            &disclosed_raw_inputs(),
            "fieldwork pre-action components",
        )
        .ok_or(FieldworkToolBlocker::ConstructionRoute { commodity })?;
        add_mass(
            &mut raw_required,
            route.raw_commodity,
            route.raw_mass,
            "planned cumulative raw input",
        );
        let cumulative = raw_required[&route.raw_commodity];
        let available = state
            .inventory()
            .get_stockpile(raw)
            .unwrap_or_else(|| panic!("fieldwork raw stockpile disappeared during planning"))
            .get_mass(route.raw_commodity);
        if available < cumulative {
            return Err(FieldworkToolBlocker::RawInput {
                commodity: route.raw_commodity,
                required: cumulative,
            });
        }
        ticks = ticks
            .checked_add(route.attention_ticks)
            .unwrap_or_else(|| panic!("fieldwork preparation estimate overflowed"));
    }
    Ok((ticks, raw_required))
}

fn estimate_tool_preparation(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    parts: StockpileId,
    tool: FieldworkTool,
) -> Result<(u64, BTreeMap<CommodityKey, Mass>), FieldworkToolBlocker> {
    let requirements: Vec<_> = equipment_component_requirements(registries, &[tool.target])
        .into_iter()
        .collect();
    estimate_component_preparation(registries, state, raw, parts, requirements)
}

pub(in super::super) fn estimate_fieldwork_upgrade_preparation(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    parts: StockpileId,
    target: EquipmentDefinitionId,
) -> Result<(u64, BTreeMap<CommodityKey, Mass>), FieldworkToolBlocker> {
    let upgrade = registries
        .equipment()
        .get_equipment(target)
        .and_then(|definition| definition.upgrade_profile())
        .unwrap_or_else(|| panic!("fieldwork owned-tool upgrade target lost its authored upgrade"));
    estimate_component_preparation(
        registries,
        state,
        raw,
        parts,
        upgrade
            .additions()
            .inputs()
            .iter()
            .map(|input| (input.commodity(), input.mass())),
    )
}

pub(in super::super) fn estimate_fieldwork_tool(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    parts: StockpileId,
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
            FIELDWORK_ORDER_MAX_BATCHES,
        ),
    )
    .map_err(FieldworkToolBlocker::Order)?;
    let (preparation_ticks, raw) = estimate_tool_preparation(registries, state, raw, parts, tool)?;
    Ok(FieldworkToolEstimate {
        tool,
        preparation_ticks,
        order_ticks: projection.duration().value(),
        batch,
        raw,
    })
}

#[derive(Clone, Debug)]
pub(in super::super) struct FieldworkBulkCrossover {
    pub(in super::super) tool_label: String,
    pub(in super::super) batches: u64,
    pub(in super::super) order: Mass,
}

/// Finds the first bounded bulk workload where a heavy quarry tool becomes the actor's preferred
/// visible-state investment. This never inspects hidden reserve truth. Callers may use it either as
/// report evidence or to author a controlled witness around the current live investment boundary.
pub(in super::super) fn fieldwork_bulk_crossover(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    parts: StockpileId,
    observed_upper: Pressure,
    base_batch: Mass,
) -> Option<FieldworkBulkCrossover> {
    let tools = fieldwork_tools(registries);
    for batches in 1..=BULK_FIELDWORK_ORDER_MAX_BATCHES {
        let order = multiplied_mass(base_batch, batches, "bulk crossover diagnostic");
        let selected = select_unique_best_tool(
            tools.iter().copied().filter_map(|tool| {
                estimate_fieldwork_tool(registries, state, raw, parts, tool, observed_upper, order)
                    .ok()
            }),
            "bulk-crossover diagnostic",
        );
        let Some(selected) = selected else {
            continue;
        };
        if fieldwork_tool_is_heavy(registries, selected.tool, base_batch) {
            return Some(FieldworkBulkCrossover {
                tool_label: fieldwork_tool_label(selected.tool),
                batches,
                order,
            });
        }
    }
    None
}

/// Explains why the bounded visible-state bulk search found no heavy-tool crossover.
///
/// This uses only the same acquired hardness, owned materials, and authored order physics as the
/// actor planner. It is report evidence, never a policy input.
pub(in super::super) fn fieldwork_bulk_crossover_blocker(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    parts: StockpileId,
    observed_upper: Pressure,
    base_batch: Mass,
) -> &'static str {
    let order = multiplied_mass(
        base_batch,
        BULK_FIELDWORK_ORDER_MAX_BATCHES,
        "bulk crossover blocker diagnostic",
    );
    let heavy_results = fieldwork_tools(registries)
        .into_iter()
        .filter(|&tool| fieldwork_tool_is_heavy(registries, tool, base_batch))
        .map(|tool| {
            estimate_fieldwork_tool(registries, state, raw, parts, tool, observed_upper, order)
        })
        .collect::<Vec<_>>();
    if heavy_results.iter().any(Result::is_ok) {
        return "no-payback";
    }
    if heavy_results
        .iter()
        .any(|result| matches!(result, Err(FieldworkToolBlocker::RawInput { .. })))
    {
        return "raw-input";
    }
    if heavy_results
        .iter()
        .all(|result| matches!(result, Err(FieldworkToolBlocker::AcquiredHardness { .. })))
    {
        return "hardness";
    }
    if heavy_results
        .iter()
        .any(|result| matches!(result, Err(FieldworkToolBlocker::ConstructionRoute { .. })))
    {
        return "construction-route";
    }
    if heavy_results
        .iter()
        .any(|result| matches!(result, Err(FieldworkToolBlocker::Order(_))))
    {
        return "order-limit";
    }
    "mixed"
}

fn viable_fieldwork_tools(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    parts: StockpileId,
    observed_upper: Pressure,
    order: Mass,
    report_candidates: bool,
) -> Vec<FieldworkToolEstimate> {
    let mut viable = Vec::new();
    for tool in fieldwork_tools(registries) {
        let estimate =
            estimate_fieldwork_tool(registries, state, raw, parts, tool, observed_upper, order);
        if report_candidates {
            println!(
                "FIELDWORK CANDIDATE tick={} tool={} equipment={} observed-upper={}Pa order={}mg estimate={estimate:?} scope=current-portable-direct-assembly-hand-pick-market authorization=not-yet assumptions=no-service,caller-supplied-visible-workload",
                state.tick().value(),
                fieldwork_tool_label(tool),
                tool.target.value(),
                observed_upper.pascals(),
                order.milligrams()
            );
        }
        if let Ok(estimate) = estimate {
            viable.push(estimate);
        }
    }
    viable
}

pub(in super::super) fn choose_fieldwork_tool_quiet(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    parts: StockpileId,
    observed_upper: Pressure,
    order: Mass,
) -> Option<FieldworkToolEstimate> {
    select_unique_best_tool(
        viable_fieldwork_tools(registries, state, raw, parts, observed_upper, order, false),
        "quiet planning",
    )
}

pub(in super::super) fn choose_fieldwork_tool_with_market_phase(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    parts: StockpileId,
    observed_upper: Pressure,
    order: Mass,
    market_phase: &'static str,
) -> Option<FieldworkToolEstimate> {
    let viable = viable_fieldwork_tools(registries, state, raw, parts, observed_upper, order, true);
    let selected = select_unique_best_tool(viable.iter().cloned(), "market planning");
    if let Some(selected) = &selected {
        let base_batch = fieldwork_mining_limits(registries).base_quarry_batch;
        let heavy = select_unique_best_tool(
            viable
                .iter()
                .filter(|estimate| fieldwork_tool_is_heavy(registries, estimate.tool, base_batch))
                .cloned(),
            "heavy-tool market comparison",
        );
        let light = select_unique_best_tool(
            viable
                .iter()
                .filter(|estimate| !fieldwork_tool_is_heavy(registries, estimate.tool, base_batch))
                .cloned(),
            "light-tool market comparison",
        );
        if let Some(heavy) = &heavy {
            if let Some(light) = &light {
                let preparation_extra =
                    i128::from(heavy.preparation_ticks) - i128::from(light.preparation_ticks);
                let order_saving = i128::from(light.order_ticks) - i128::from(heavy.order_ticks);
                let total_delta = i128::from(heavy.total_ticks()) - i128::from(light.total_ticks());
                reviewln!(
                    "FIELDWORK TOOL MARKET phase={market_phase} selected={} selected-total={}t light-best={} light-total={}t heavy-best={} heavy-total={}t heavy-preparation-extra={preparation_extra:+}t heavy-order-saving={order_saving:+}t heavy-total-delta={total_delta:+}t heavy-investment={}",
                    fieldwork_tool_label(selected.tool),
                    selected.total_ticks(),
                    fieldwork_tool_label(light.tool),
                    light.total_ticks(),
                    fieldwork_tool_label(heavy.tool),
                    heavy.total_ticks(),
                    if heavy.tool.target == selected.tool.target {
                        "selected"
                    } else {
                        "deferred"
                    },
                );
            } else {
                reviewln!(
                    "FIELDWORK TOOL MARKET phase={market_phase} selected={} selected-total={}t light-best=unavailable heavy-best={} heavy-total={}t heavy-investment=selected",
                    fieldwork_tool_label(selected.tool),
                    selected.total_ticks(),
                    fieldwork_tool_label(heavy.tool),
                    heavy.total_ticks(),
                );
            }
        } else {
            reviewln!(
                "FIELDWORK TOOL MARKET phase={market_phase} selected={} selected-total={}t heavy-best=unavailable heavy-investment=unavailable",
                fieldwork_tool_label(selected.tool),
                selected.total_ticks(),
            );
        }
    }
    selected
}

fn fieldwork_tool_is_heavy(registries: &Registries, tool: FieldworkTool, base_batch: Mass) -> bool {
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("fieldwork hand-pick method disappeared"));
    matches!(
        pristine_equipment_capability(
            registries,
            tool.target,
            method.max_batch_mass_capability(),
        ),
        CapabilityValue::Mass(batch) if batch >= base_batch
    )
}
