//! Exact inventory-to-surface custody transfer for a local kobold cache raid.
//!
//! A surface scavenger may work only on already-accessible ground custody.
//! Its target is selected by the kobold owner; this transaction alone checks
//! the inventory lot, physical cache protection and conserved world placement.

use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::inventory::{
    MaterialLotId, MaterialLotSelection, StockpileId, apply_material_egress,
    validate_explicit_consumption_selection, validate_material_egress_from_selection,
};
use crate::material::MaterialPhase;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::{
    SurfaceResourceId, SurfaceResourceKind, SurfaceResourceSpec,
    validate_create_surface_resources_at,
};

/// Spend one physical scavenger work slot extracting a recoverable lot portion
/// from an unenclosed, stationary cache. The resulting parcel remains at the
/// worksite; the scout must spend a later real step hauling it away.
///
/// The exact egress and surface creation are prevalidated before either owner
/// changes. The caller supplies the scout's physically reached position and
/// decides whether its existing food, threat and scheduling rules permit work.
#[derive(Clone, Copy)]
pub(crate) struct ScavengedGroundStockpileRequest {
    source: StockpileId,
    lot: MaterialLotId,
    position: VoxelCoord,
    at: SimulationTick,
    maximum_mass: Mass,
    maximum_temperature: Temperature,
}

impl ScavengedGroundStockpileRequest {
    pub(crate) const fn new(
        source: StockpileId,
        lot: MaterialLotId,
        position: VoxelCoord,
        at: SimulationTick,
        maximum_mass: Mass,
        maximum_temperature: Temperature,
    ) -> Self {
        Self {
            source,
            lot,
            position,
            at,
            maximum_mass,
            maximum_temperature,
        }
    }
}

pub(crate) fn scavenge_unenclosed_ground_stockpile_material(
    registries: &Registries,
    state: &mut AppState,
    request: ScavengedGroundStockpileRequest,
) -> Option<(SurfaceResourceId, Mass)> {
    let ScavengedGroundStockpileRequest {
        source,
        lot: lot_id,
        position,
        at,
        maximum_mass,
        maximum_temperature,
    } = request;
    if maximum_mass.is_zero()
        || !state.can_spend_inventory_revisions(1)
        || !state.can_spend_surface_revisions(1)
        || state.logistics().stationary_stockpile_position(source) != Some(position)
    {
        return None;
    }
    let stockpile = state.inventory().get_stockpile(source)?;
    if stockpile.enclosure().is_some()
        || stockpile.supported_by().is_some()
        || !stockpile.reserved_inbound().is_zero()
    {
        return None;
    }
    let lot = state.inventory().get_lot(lot_id)?;
    if lot.stockpile() != source
        || lot.temperature() > maximum_temperature
        || !registries
            .materials()
            .get_form(lot.commodity().form())
            .is_some_and(|form| form.phase() == MaterialPhase::Solid)
    {
        return None;
    }
    let mass = lot.mass().min(maximum_mass);
    if mass.is_zero() {
        return None;
    }
    let ambient_age = lot.storage_history().project(
        at,
        stockpile.storage_profile().preservation_multiplier_ppm(),
    )?;
    let spec = SurfaceResourceSpec::from_existing_material(
        SurfaceResourceKind::Drop,
        position,
        mass,
        lot.profile().clone(),
        lot.provenance(),
        ambient_age,
    )
    .ok()?;
    let surface = validate_create_surface_resources_at(registries, state, [spec], at).ok()?;
    let selection = validate_explicit_consumption_selection(
        state.inventory(),
        source,
        &[MaterialLotSelection::new(lot_id, mass)],
    )
    .ok()?;
    let egress = validate_material_egress_from_selection(state.inventory(), selection).ok()?;
    surface.precheck(state).ok()?;
    egress.assert_matches_state(state.inventory());
    let id = surface.first_resource_id();
    apply_material_egress(state.inventory_state_mut(), egress);
    surface.apply(state);
    Some((id, mass))
}
