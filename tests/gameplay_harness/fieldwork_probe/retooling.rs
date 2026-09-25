//! Actor-visible extraction-tool reassessment after new geological evidence.

use deep_hearth::capability::CapabilityValue;
use deep_hearth::content::{
    FORM_NATIVE_METAL, FORM_ORE, MATERIAL_COPPER, MINING_METHOD_HAND_PICK,
    PROCESS_HAND_SORT_NATIVE_COPPER,
};
use deep_hearth::core::quantity::{Mass, Pressure};
use deep_hearth::core::state::AppState;
use deep_hearth::equipment::{
    EquipmentId, project_equipment_capability, validate_disassemble_equipment,
};
use deep_hearth::inventory::StockpileId;
use deep_hearth::material::CommodityKey;
use deep_hearth::mining::{MiningOrderRequest, resolve_mining_order};
use deep_hearth::registry::Registries;

use super::super::manual_ore_recovery::{ManualOreRecoveryPlan, execute_manual_ore_recovery};
use super::planning::{
    FIELDWORK_ORDER_MAX_BATCHES, FIELDWORK_TOOLS, FieldworkToolBlocker,
    choose_fieldwork_tool_quiet, choose_fieldwork_tool_with_market_phase, estimate_fieldwork_tool,
};
use super::preparation::assemble_fieldwork_tool;

#[derive(Clone, Copy, Debug)]
pub(super) struct FieldworkOwnedOreRecovery {
    pub(super) ore_source: StockpileId,
    pub(super) crushed_destination: StockpileId,
    pub(super) residue_destination: StockpileId,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct FieldworkSiteToolRequest<'a> {
    pub(super) raw: StockpileId,
    pub(super) parts: StockpileId,
    pub(super) recovery: FieldworkOwnedOreRecovery,
    pub(super) owned_equipment: &'a [EquipmentId],
    pub(super) observed_hardness_upper: Pressure,
    pub(super) order: Mass,
}

impl<'a> FieldworkSiteToolRequest<'a> {
    pub(super) const fn new(
        raw: StockpileId,
        parts: StockpileId,
        recovery: FieldworkOwnedOreRecovery,
        owned_equipment: &'a [EquipmentId],
        observed_hardness_upper: Pressure,
        order: Mass,
    ) -> Self {
        Self {
            raw,
            parts,
            recovery,
            owned_equipment,
            observed_hardness_upper,
            order,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ExistingToolProjection {
    batch: Mass,
    order_ticks: u64,
    label: &'static str,
}

#[derive(Clone, Copy, Debug)]
struct SalvageProjection {
    equipment: EquipmentId,
    total_attention_ticks: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FieldworkOreRecoveryReason {
    RequiredAccess,
    Payback,
}

impl FieldworkOreRecoveryReason {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::RequiredAccess => "required-access",
            Self::Payback => "payback",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct FieldworkSiteToolChoice {
    pub(super) equipment: EquipmentId,
    pub(super) batch: Mass,
    pub(super) preparation_ticks: u64,
    pub(super) projected_order_ticks: u64,
    pub(super) label: &'static str,
    pub(super) reused_existing: bool,
    pub(super) ore_recovery_ticks: u64,
    pub(super) ore_feed_mass: Mass,
    pub(super) recovered_native: Mass,
    pub(super) ore_recovery_reason: Option<FieldworkOreRecoveryReason>,
    pub(super) salvaged_equipment: Option<EquipmentId>,
}

fn existing_tool_projection(
    registries: &Registries,
    state: &AppState,
    equipment: EquipmentId,
    observed_hardness_upper: Pressure,
    order: Mass,
) -> Option<ExistingToolProjection> {
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("fieldwork recovery hand-pick method disappeared"));
    let record = state.equipment().get_equipment(equipment)?;
    let definition = registries
        .equipment()
        .get_equipment(record.definition())
        .unwrap_or_else(|| panic!("fieldwork recovery equipment definition disappeared"));
    let CapabilityValue::Mass(batch) = project_equipment_capability(
        definition,
        record.condition(),
        method.max_batch_mass_capability(),
    )?
    else {
        panic!("fieldwork recovery batch capability changed physical kind")
    };
    let resolution = resolve_mining_order(
        registries.core().physical_tick_duration(),
        method,
        definition,
        MiningOrderRequest::new(
            record.condition(),
            observed_hardness_upper,
            order,
            batch,
            FIELDWORK_ORDER_MAX_BATCHES,
        ),
    )
    .ok()?;
    let label = FIELDWORK_TOOLS
        .iter()
        .find(|candidate| candidate.target == record.definition())
        .map(|candidate| candidate.label)
        .unwrap_or("existing-tool");
    Some(ExistingToolProjection {
        batch,
        order_ticks: resolution.duration().value(),
        label,
    })
}

fn prepare_from_current_materials(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    parts: StockpileId,
    owned_equipment: &[EquipmentId],
    observed_hardness_upper: Pressure,
    order: Mass,
) -> Option<FieldworkSiteToolChoice> {
    let existing = owned_equipment
        .iter()
        .filter_map(|&equipment| {
            existing_tool_projection(registries, state, equipment, observed_hardness_upper, order)
                .map(|projection| (equipment, projection))
        })
        .min_by_key(|(equipment, projection)| (projection.order_ticks, equipment.value()));
    let fresh = choose_fieldwork_tool_with_market_phase(
        registries,
        state,
        raw,
        parts,
        observed_hardness_upper,
        order,
        "new-site-evidence",
    );
    if let Some((equipment, existing)) = existing
        && fresh
            .as_ref()
            .is_none_or(|candidate| existing.order_ticks <= candidate.total_ticks())
    {
        return Some(FieldworkSiteToolChoice {
            equipment,
            batch: existing.batch,
            preparation_ticks: 0,
            projected_order_ticks: existing.order_ticks,
            label: existing.label,
            reused_existing: true,
            ore_recovery_ticks: 0,
            ore_feed_mass: Mass::ZERO,
            recovered_native: Mass::ZERO,
            ore_recovery_reason: None,
            salvaged_equipment: None,
        });
    }

    let fresh = fresh?;
    let (equipment, preparation_ticks) =
        assemble_fieldwork_tool(registries, state, raw, parts, fresh.tool);
    assert_eq!(
        preparation_ticks, fresh.preparation_ticks,
        "fieldwork new-site tool preparation must match its pre-action projection"
    );
    Some(FieldworkSiteToolChoice {
        equipment,
        batch: fresh.batch,
        preparation_ticks,
        projected_order_ticks: fresh.order_ticks,
        label: fresh.tool.label,
        reused_existing: false,
        ore_recovery_ticks: 0,
        ore_feed_mass: Mass::ZERO,
        recovered_native: Mass::ZERO,
        ore_recovery_reason: None,
        salvaged_equipment: None,
    })
}

pub(super) fn projected_current_material_attention(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    parts: StockpileId,
    owned_equipment: &[EquipmentId],
    observed_hardness_upper: Pressure,
    order: Mass,
) -> Option<u64> {
    let existing = owned_equipment
        .iter()
        .filter_map(|&equipment| {
            existing_tool_projection(registries, state, equipment, observed_hardness_upper, order)
                .map(|projection| projection.order_ticks)
        })
        .min();
    let fresh = choose_fieldwork_tool_quiet(
        registries,
        state,
        raw,
        parts,
        observed_hardness_upper,
        order,
    )
    .map(|estimate| estimate.total_ticks());
    match (existing, fresh) {
        (Some(existing), Some(fresh)) => Some(existing.min(fresh)),
        (Some(existing), None) => Some(existing),
        (None, Some(fresh)) => Some(fresh),
        (None, None) => None,
    }
}

fn projected_salvage(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    parts: StockpileId,
    owned_equipment: &[EquipmentId],
    observed_hardness_upper: Pressure,
    order: Mass,
) -> Option<SalvageProjection> {
    owned_equipment
        .iter()
        .filter_map(|&equipment| {
            if existing_tool_projection(
                registries,
                state,
                equipment,
                observed_hardness_upper,
                order,
            )
            .is_some()
            {
                return None;
            }
            let salvaged_definition = state.equipment().get_equipment(equipment)?.definition();
            let mut projected = state.clone();
            let _ = validate_disassemble_equipment(registries, &projected, equipment, parts)
                .ok()?
                .commit(&mut projected)
                .ok()?;
            let replacement = choose_fieldwork_tool_quiet(
                registries,
                &projected,
                raw,
                parts,
                observed_hardness_upper,
                order,
            )?;
            if replacement.tool.target == salvaged_definition {
                return None;
            }
            Some(SalvageProjection {
                equipment,
                total_attention_ticks: replacement.total_ticks(),
            })
        })
        .min_by_key(|projection| {
            (
                projection.total_attention_ticks,
                projection.equipment.value(),
            )
        })
}

fn native_copper_shortage_for_feasible_tool(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    parts: StockpileId,
    observed_hardness_upper: Pressure,
    order: Mass,
) -> Option<Mass> {
    let native = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
    FIELDWORK_TOOLS
        .iter()
        .filter_map(|tool| {
            match estimate_fieldwork_tool(
                registries,
                state,
                raw,
                parts,
                *tool,
                observed_hardness_upper,
                order,
            ) {
                Err(FieldworkToolBlocker::RawInput {
                    commodity,
                    required,
                }) if commodity == native => Some(required),
                _ => None,
            }
        })
        .min_by_key(|mass| mass.milligrams())
}

fn homogeneous_owned_ore(state: &AppState, source: StockpileId) -> Option<(u32, Mass)> {
    let ore = CommodityKey::new(MATERIAL_COPPER, FORM_ORE);
    let mut assay = None;
    let mut composition = None;
    let mut temperature = None;
    let mut total = Mass::ZERO;
    let mut found = false;
    for lot_id in state.inventory().lot_ids(source) {
        let lot = state.inventory().get_lot(lot_id)?;
        if lot.commodity() != ore {
            return None;
        }
        let lot_assay = lot.composition().parts_per_million(MATERIAL_COPPER);
        if lot_assay == 0
            || assay.is_some_and(|value| value != lot_assay)
            || composition
                .as_ref()
                .is_some_and(|value| value != lot.composition())
            || temperature.is_some_and(|value| value != lot.temperature())
        {
            return None;
        }
        assay.get_or_insert(lot_assay);
        composition.get_or_insert_with(|| lot.composition().clone());
        temperature.get_or_insert(lot.temperature());
        total = total.checked_add(lot.mass())?;
        found = true;
    }
    found.then_some((assay?, total))
}

fn recover_native_copper_from_owned_ore(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    parts: StockpileId,
    recovery: FieldworkOwnedOreRecovery,
    observed_hardness_upper: Pressure,
    order: Mass,
) -> Option<(u64, Mass, Mass)> {
    let required_native = native_copper_shortage_for_feasible_tool(
        registries,
        state,
        raw,
        parts,
        observed_hardness_upper,
        order,
    )?;
    let (copper_ppm, available_ore) = homogeneous_owned_ore(state, recovery.ore_source)?;
    let sorting = registries
        .ore_processing()
        .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("fieldwork recovery lost hand-sorting definition"));
    let feed_mass =
        sorting.minimum_homogeneous_feed_mass_for_target_recovery(required_native, copper_ppm)?;
    if feed_mass > available_ore
        || state.inventory().get_stockpile(raw)?.available_capacity() < required_native
        || state
            .inventory()
            .get_stockpile(recovery.crushed_destination)?
            .available_capacity()
            < feed_mass
        || state
            .inventory()
            .get_stockpile(recovery.residue_destination)?
            .available_capacity()
            < feed_mass
    {
        return None;
    }
    let execution = execute_manual_ore_recovery(
        registries,
        state,
        ManualOreRecoveryPlan {
            ore_source: recovery.ore_source,
            crushed_destination: recovery.crushed_destination,
            native_destination: raw,
            residue_destination: recovery.residue_destination,
            feed_mass,
        },
    );
    (execution.recovered_native >= required_native).then_some((
        execution.attention_ticks,
        feed_mass,
        execution.recovered_native,
    ))
}

/// Reassesses carried extraction tools against one newly observed local opportunity.
///
/// The actor compares owned tools, immediately buildable tools, salvage-and-rebuild, and an
/// ore-funded specialization using only acquired hardness, visible workload, owned inventory, and
/// canonical process physics. Salvage is projected through canonical equipment disassembly on a
/// cloned state, so embodied copper and handles can become real adaptation capital without free
/// matter. If native copper is the sole missing input for a physically feasible tool, a cloned
/// visible state prices ordinary hand-breaking and hand-sorting before commitment. Alternatives
/// execute only when their full visible attention cost beats the current-material plan; ties keep
/// existing equipment and resources intact.
pub(super) fn prepare_fieldwork_tool_for_site(
    registries: &Registries,
    state: &mut AppState,
    request: FieldworkSiteToolRequest<'_>,
) -> Option<FieldworkSiteToolChoice> {
    let FieldworkSiteToolRequest {
        raw,
        parts,
        recovery,
        owned_equipment,
        observed_hardness_upper,
        order,
    } = request;
    let current_attention = projected_current_material_attention(
        registries,
        state,
        raw,
        parts,
        owned_equipment,
        observed_hardness_upper,
        order,
    );
    let salvage_projection = projected_salvage(
        registries,
        state,
        raw,
        parts,
        owned_equipment,
        observed_hardness_upper,
        order,
    );
    let mut ore_projection_state = state.clone();
    let ore_projection = recover_native_copper_from_owned_ore(
        registries,
        &mut ore_projection_state,
        raw,
        parts,
        recovery,
        observed_hardness_upper,
        order,
    )
    .and_then(|(recovery_ticks, feed_mass, recovered_native)| {
        projected_current_material_attention(
            registries,
            &ore_projection_state,
            raw,
            parts,
            owned_equipment,
            observed_hardness_upper,
            order,
        )
        .and_then(|post_recovery_attention| {
            recovery_ticks
                .checked_add(post_recovery_attention)
                .map(|total| (total, recovery_ticks, feed_mass, recovered_native))
        })
    });
    let current_limit = current_attention.unwrap_or(u64::MAX);
    let salvage_attention = salvage_projection.map(|projection| projection.total_attention_ticks);
    let ore_attention = ore_projection.map(|projection| projection.0);
    let choose_salvage = salvage_attention.is_some_and(|attention| {
        attention < current_limit && ore_attention.is_none_or(|ore| attention < ore)
    });
    if choose_salvage {
        let salvage = salvage_projection
            .unwrap_or_else(|| unreachable!("selected salvage branch was projected"));
        let _ = validate_disassemble_equipment(registries, state, salvage.equipment, parts)
            .unwrap_or_else(|error| panic!("fieldwork salvage validation diverged: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("fieldwork salvage commit diverged: {error}"));
        let mut choice = prepare_from_current_materials(
            registries,
            state,
            raw,
            parts,
            owned_equipment,
            observed_hardness_upper,
            order,
        )?;
        let actual_total = choice
            .preparation_ticks
            .checked_add(choice.projected_order_ticks)
            .unwrap_or_else(|| panic!("fieldwork salvage attention overflowed"));
        assert_eq!(
            actual_total, salvage.total_attention_ticks,
            "fieldwork salvage execution must match its pre-action projection"
        );
        choice.salvaged_equipment = Some(salvage.equipment);
        return Some(choice);
    }

    let ore_recovery_reason = match (current_attention, ore_projection) {
        (None, Some(_)) => Some(FieldworkOreRecoveryReason::RequiredAccess),
        (Some(current), Some((ore, ..))) if ore < current => {
            Some(FieldworkOreRecoveryReason::Payback)
        }
        _ => None,
    };
    if ore_recovery_reason.is_none() {
        return prepare_from_current_materials(
            registries,
            state,
            raw,
            parts,
            owned_equipment,
            observed_hardness_upper,
            order,
        );
    }

    let (_, projected_recovery_ticks, projected_feed_mass, projected_recovered_native) =
        ore_projection.unwrap_or_else(|| unreachable!("selected ore-funded branch was projected"));
    let (ore_recovery_ticks, ore_feed_mass, recovered_native) =
        recover_native_copper_from_owned_ore(
            registries,
            state,
            raw,
            parts,
            recovery,
            observed_hardness_upper,
            order,
        )?;
    assert_eq!(ore_recovery_ticks, projected_recovery_ticks);
    assert_eq!(ore_feed_mass, projected_feed_mass);
    assert_eq!(recovered_native, projected_recovered_native);
    let mut choice = prepare_from_current_materials(
        registries,
        state,
        raw,
        parts,
        owned_equipment,
        observed_hardness_upper,
        order,
    )?;
    choice.preparation_ticks = choice
        .preparation_ticks
        .checked_add(ore_recovery_ticks)
        .unwrap_or_else(|| panic!("fieldwork owned-ore adaptation attention overflowed"));
    choice.ore_recovery_ticks = ore_recovery_ticks;
    choice.ore_feed_mass = ore_feed_mass;
    choice.recovered_native = recovered_native;
    choice.ore_recovery_reason = ore_recovery_reason;
    Some(choice)
}
