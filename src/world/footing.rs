//! Local stationary custody claims on a removable fixed collision surface.

use crate::core::state::AppState;
use crate::equipment::EquipmentId;
use crate::inventory::StockpileId;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::structural::StructuralElementId;
use crate::surface::{SurfaceResourceId, SurfaceResourceKind};

/// The ground voxel of an unenclosed, unmounted, stationary stockpile.
/// Carried custody and explicit supports have their own physical owners.
pub(crate) fn loose_stockpile_position(
    state: &AppState,
    stockpile: StockpileId,
) -> Option<VoxelCoord> {
    let pile = state.inventory().get_stockpile(stockpile)?;
    if pile.enclosure().is_some() || pile.supported_by().is_some() {
        return None;
    }
    state.logistics().stationary_stockpile_position(stockpile)
}

/// An empty marker can exist inside rock, but a new inbound reservation
/// makes its location physical.
pub(crate) fn solid_stockpile_position(
    state: &AppState,
    stockpile: StockpileId,
) -> Option<VoxelCoord> {
    let position = state.logistics().stationary_stockpile_position(stockpile)?;
    (state
        .world()
        .block_at(position)
        .is_some_and(|block| block != super::BlockStateId::AIR)
        || state.world().tree_trunk_at(position).is_some())
    .then_some(position)
}

/// Abstract locations have no terrain authority. In generated space, fixed
/// custody needs an exact, persistent floor rather than an actor's footprint.
pub(crate) fn generated_position_lacks_fixed_footing(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> bool {
    if !state.world().has_generated_terrain_at(position) {
        return false;
    }
    !position.y().checked_sub(1).is_some_and(|y| {
        let floor = VoxelCoord::new(position.x(), y, position.z());
        (super::WORLD_MIN_Y..super::WORLD_MAX_Y_EXCLUSIVE).contains(&floor.y())
            && !fixed_floor_reserved_for_removal(state, floor)
            && (state
                .world()
                .full_cell_collision_at(registries.world(), floor)
                == Some(true)
                || state.structures().blocks_full_cell(floor))
    })
}

pub(crate) fn unsupported_loose_stockpile_position(
    registries: &Registries,
    state: &AppState,
    stockpile: StockpileId,
) -> Option<VoxelCoord> {
    let position = loose_stockpile_position(state, stockpile)?;
    generated_position_lacks_fixed_footing(registries, state, position).then_some(position)
}

/// Physical custody without independent falling motion. Terrain excavation,
/// structural recovery and tree removal share this ordered local assessment.
pub(crate) enum FixedFootingOwner {
    StorageEnclosure(StockpileId),
    OccupiedGroundStockpile(StockpileId),
    LooseSurfaceDrop(SurfaceResourceId),
    RootedForage,
    EnergyStore(crate::energy::EnergyStoreId),
    StationaryEquipment(EquipmentId),
    NaturalSpring(crate::fluid::FluidStoreId),
    OpeningResident,
    GroundedStructure(StructuralElementId),
}

impl FixedFootingOwner {
    pub(crate) fn at(state: &AppState, floor: VoxelCoord) -> Option<Self> {
        storage_enclosure_resting_on(state, floor)
            .map(Self::StorageEnclosure)
            .or_else(|| {
                occupied_ground_stockpile_resting_on(state, floor)
                    .map(Self::OccupiedGroundStockpile)
            })
            .or_else(|| loose_surface_drop_resting_on(state, floor).map(Self::LooseSurfaceDrop))
            .or_else(|| rooted_forage_resting_on(state, floor).then_some(Self::RootedForage))
            .or_else(|| stationary_energy_store_resting_on(state, floor).map(Self::EnergyStore))
            .or_else(|| {
                stationary_equipment_resting_on(state, floor).map(Self::StationaryEquipment)
            })
            .or_else(|| natural_spring_resting_on(state, floor).map(Self::NaturalSpring))
            .or_else(|| {
                crate::kobolds::opening_kobold_stands_on(state, floor)
                    .then_some(Self::OpeningResident)
            })
            .or_else(|| grounded_structure_resting_on(state, floor).map(Self::GroundedStructure))
    }
}

fn upper_trunk(state: &AppState, root: VoxelCoord) -> Option<VoxelCoord> {
    let tree = state.world().tree_at(root)?;
    let upper = VoxelCoord::new(root.x(), root.y().checked_add(1)?, root.z());
    tree.occupies_trunk(upper).then_some(upper)
}

/// The lower trunk is covered by the upper trunk; only the upper surface can
/// carry stationary custody outside the tree's own occupied cells.
pub(crate) fn tree_has_fixed_footing_claim(state: &AppState, root: VoxelCoord) -> bool {
    upper_trunk(state, root).is_some_and(|floor| FixedFootingOwner::at(state, floor).is_some())
}

pub(crate) fn tree_felling_floor_reserved_at(state: &AppState, floor: VoxelCoord) -> bool {
    state
        .player_work()
        .active()
        .and_then(|work| work.tree_felling())
        .is_some_and(|work| upper_trunk(state, work.root()) == Some(floor))
}

/// A pending complete removal must not acquire new dependent custody.
pub(crate) fn fixed_floor_reserved_for_removal(state: &AppState, floor: VoxelCoord) -> bool {
    super::full_block_excavation_reserved_at(state, floor)
        || crate::structural::dismantling_floor_reserved_at(state, floor)
        || tree_felling_floor_reserved_at(state, floor)
}

/// Natural water is a stationary hollow, not a vessel that can fall after the
/// last supporting block disappears. Ordinary placed fluid stores do not claim
/// terrain footing, because their support is owned by their installation path.
fn natural_spring_resting_on(
    state: &AppState,
    floor: VoxelCoord,
) -> Option<crate::fluid::FluidStoreId> {
    let above = VoxelCoord::new(floor.x(), floor.y().checked_add(1)?, floor.z());
    state
        .logistics()
        .fluid_stores_at(above)
        .find(|&store| state.fluid().is_natural_spring(store))
}

/// A built enclosure still needs terrain when its contents are empty.
fn storage_enclosure_resting_on(state: &AppState, floor: VoxelCoord) -> Option<StockpileId> {
    let above = VoxelCoord::new(floor.x(), floor.y().checked_add(1)?, floor.z());
    state.logistics().stockpiles_at(above).find(|&id| {
        state
            .inventory()
            .get_stockpile(id)
            .is_some_and(|stockpile| stockpile.enclosure().is_some())
    })
}

/// Unenclosed storage contains physical matter or an inbound material promise.
/// Empty site markers do not obstruct removal; structurally supported caches
/// depend on their member rather than the underlying terrain cell.
fn occupied_ground_stockpile_resting_on(
    state: &AppState,
    floor: VoxelCoord,
) -> Option<StockpileId> {
    let above = VoxelCoord::new(floor.x(), floor.y().checked_add(1)?, floor.z());
    state.logistics().stockpiles_at(above).find(|&id| {
        state
            .inventory()
            .get_stockpile(id)
            .is_some_and(|stockpile| {
                stockpile.supported_by().is_none()
                    && (!stockpile.stored_mass().is_zero()
                        || !stockpile.reserved_inbound().is_zero())
            })
    })
}

/// Available loose parcels have no falling simulation after the block under
/// their actual position disappears. Depleted historical matter does not block
/// mining; the live surface position index is the sole lookup.
fn loose_surface_drop_resting_on(state: &AppState, floor: VoxelCoord) -> Option<SurfaceResourceId> {
    let above = VoxelCoord::new(floor.x(), floor.y().checked_add(1)?, floor.z());
    state
        .surface()
        .available_resources_at(Some(above))
        .find(|resource| resource.kind() == SurfaceResourceKind::Drop)
        .map(|resource| resource.id())
}

/// A harvested perennial may retain its entire physical root reserve even
/// with no gatherable crop; unlike a depleted non-perennial drop, it still
/// claims the terrain beneath its indexed site.
fn rooted_forage_resting_on(state: &AppState, floor: VoxelCoord) -> bool {
    floor.y().checked_add(1).is_some_and(|y| {
        state
            .surface()
            .rooted_biomass_occupies_voxel(VoxelCoord::new(floor.x(), y, floor.z()))
    })
}

/// A material-built energy store cannot stay suspended when its original
/// terrain footing is fully removed. Unembodied capability fixtures do not
/// claim this physical support. Use the indexed local position, not all stores.
fn stationary_energy_store_resting_on(
    state: &AppState,
    floor: VoxelCoord,
) -> Option<crate::energy::EnergyStoreId> {
    let above = VoxelCoord::new(floor.x(), floor.y().checked_add(1)?, floor.z());
    state.logistics().energy_stores_at(above).find(|&id| {
        state
            .energy()
            .get_store(id)
            .is_some_and(|store| !store.embodied_material().is_empty())
    })
}

/// Unmounted, material-built equipment has no simulated falling motion after
/// it is assembled or dropped. Once mounted, its structural member owns the
/// support instead of the ground directly beneath its current voxel.
fn stationary_equipment_resting_on(state: &AppState, floor: VoxelCoord) -> Option<EquipmentId> {
    let above = VoxelCoord::new(floor.x(), floor.y().checked_add(1)?, floor.z());
    state.logistics().equipment_at(above).find(|&id| {
        state
            .equipment()
            .get_equipment(id)
            .is_some_and(|equipment| {
                equipment.supported_by().is_none() && !equipment.embodied_material().is_empty()
            })
    })
}

/// Ground anchors remain attached to their actual supporting terrain until
/// dismantled. An empty blueprint does not own physical matter and cannot
/// reserve a permanent veto over earthworks.
fn grounded_structure_resting_on(
    state: &AppState,
    floor: VoxelCoord,
) -> Option<StructuralElementId> {
    let above = VoxelCoord::new(floor.x(), floor.y().checked_add(1)?, floor.z());
    state
        .structures()
        .elements_covering(above)
        .filter(|member| {
            member.is_grounded()
                && !member.embodied_material().is_empty()
                && member.bounds().min().y() == above.y()
        })
        .map(|member| member.id())
        .min()
}
