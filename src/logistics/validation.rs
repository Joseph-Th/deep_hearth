//! Trusted-load validation for logistics/inventory location ownership.

mod error;

pub use error::LogisticsValidationError;

use crate::core::state::AppState;
use crate::energy::EnergyState;
use crate::equipment::{EquipmentRegistry, EquipmentState};
use crate::fluid::FluidState;
use crate::inventory::InventoryState;
use crate::registry::Registries;
use crate::structural::StructureState;
use crate::survival::Vitality;

use super::{
    LogisticsState, PlayerActiveSelection, PlayerBodyState, PlayerQuickAccessSlot, TraversalEdge,
    first_player_body_collision, player_body_fits_continuous_world,
};

fn validate_revision(state: &LogisticsState) -> Result<(), LogisticsValidationError> {
    let populated = state.player().is_some()
        || state.traversable_edges().next().is_some()
        || state.carried_equipment().next().is_some()
        || state
            .quick_access_slots()
            .any(|(_, selection)| selection.is_some())
        || state.active_quick_access_slot() != PlayerQuickAccessSlot::FIRST
        || state.pending_secondary_use().is_some()
        || state.pending_primary_break().is_some()
        || state.stockpile_locations().next().is_some()
        || state.equipment_locations().next().is_some()
        || state.energy_store_locations().next().is_some()
        || state.fluid_store_locations().next().is_some();
    match (populated, state.revision()) {
        (false, revision) if revision != 0 => {
            Err(LogisticsValidationError::UninitializedRevisionNonzero { revision })
        }
        (true, 0) => Err(LogisticsValidationError::InitializedRevisionZero),
        _ => Ok(()),
    }
}

fn validate_pending_primary_break(
    app: &AppState,
    state: &LogisticsState,
) -> Result<(), LogisticsValidationError> {
    let Some(pending) = state.pending_primary_break() else {
        return Ok(());
    };
    if state.player().is_none() {
        return Err(LogisticsValidationError::PendingPrimaryBreakWithoutPlayer);
    }
    if app
        .survival()
        .player()
        .is_some_and(|player| player.vitality() == Vitality::ZERO)
    {
        return Err(LogisticsValidationError::PendingPrimaryBreakPlayerDead);
    }
    if pending.requested_at() > app.control_tick() {
        return Err(LogisticsValidationError::PendingPrimaryBreakRequestedInFuture);
    }
    let requested_simulation = pending.requested_at().simulation_tick();
    let interval_is_current = requested_simulation == app.tick();
    let interval_is_due_boundary = app.control_tick().is_simulation_boundary()
        && requested_simulation
            .value()
            .checked_add(1)
            .is_some_and(|due| due == app.tick().value());
    if !interval_is_current && !interval_is_due_boundary {
        return Err(LogisticsValidationError::PendingPrimaryBreakFromPriorSimulationInterval);
    }
    if pending.element().value() == 0 {
        return Err(LogisticsValidationError::PendingPrimaryBreakElementInvalid);
    }
    if pending.direction().is_none() {
        return Err(LogisticsValidationError::PendingPrimaryBreakDirectionInvalid);
    }
    Ok(())
}

fn validate_pending_secondary_use(
    app: &AppState,
    state: &LogisticsState,
) -> Result<(), LogisticsValidationError> {
    let Some(pending) = state.pending_secondary_use() else {
        return Ok(());
    };
    if state.player().is_none() {
        return Err(LogisticsValidationError::PendingSecondaryUseWithoutPlayer);
    }
    if app
        .survival()
        .player()
        .is_some_and(|player| player.vitality() == Vitality::ZERO)
    {
        return Err(LogisticsValidationError::PendingSecondaryUsePlayerDead);
    }
    if pending.requested_at() > app.control_tick() {
        return Err(LogisticsValidationError::PendingSecondaryUseRequestedInFuture);
    }
    let requested_simulation = pending.requested_at().simulation_tick();
    let interval_is_current = requested_simulation == app.tick();
    let interval_is_due_boundary = app.control_tick().is_simulation_boundary()
        && requested_simulation
            .value()
            .checked_add(1)
            .is_some_and(|due| due == app.tick().value());
    if !interval_is_current && !interval_is_due_boundary {
        return Err(LogisticsValidationError::PendingSecondaryUseFromPriorSimulationInterval);
    }
    if pending.direction().is_none() {
        return Err(LogisticsValidationError::PendingSecondaryUseDirectionInvalid);
    }
    if pending
        .surface_request()
        .is_some_and(|(resource, mass)| resource.value() == 0 || mass.is_zero())
    {
        return Err(LogisticsValidationError::PendingSecondaryUseSurfaceRequestInvalid);
    }
    Ok(())
}

fn validate_traversal_edges(state: &LogisticsState) -> Result<(), LogisticsValidationError> {
    let mut previous: Option<TraversalEdge> = None;
    for edge in state.traversable_edges() {
        if !edge.is_canonical() {
            let (first, second) = edge.endpoints();
            return Err(LogisticsValidationError::TraversalEdgeNotCanonical { first, second });
        }
        if let Some(previous_edge) = previous
            && previous_edge >= edge
        {
            let (previous_first, previous_second) = previous_edge.endpoints();
            let (current_first, current_second) = edge.endpoints();
            return Err(LogisticsValidationError::TraversalEdgesNotStrictlyOrdered {
                previous_first,
                previous_second,
                current_first,
                current_second,
            });
        }
        previous = Some(edge);
    }
    if !state.has_valid_traversal_index() {
        return Err(LogisticsValidationError::TraversalAdjacencyIndexInconsistent);
    }
    Ok(())
}

fn validate_carried_equipment(
    state: &LogisticsState,
    equipment: &EquipmentState,
    registry: &EquipmentRegistry,
) -> Result<(), LogisticsValidationError> {
    let mut previous = None;
    for equipment_id in state.carried_equipment() {
        if state.player().is_none() {
            return Err(LogisticsValidationError::CarriedEquipmentWithoutPlayer {
                equipment: equipment_id,
            });
        }
        if let Some(previous_id) = previous
            && previous_id >= equipment_id
        {
            return Err(
                LogisticsValidationError::CarriedEquipmentNotStrictlyOrdered {
                    previous: previous_id,
                    current: equipment_id,
                },
            );
        }
        previous = Some(equipment_id);
        let record = equipment.get_equipment(equipment_id).ok_or(
            LogisticsValidationError::UnknownCarriedEquipment {
                equipment: equipment_id,
            },
        )?;
        if state.stationary_equipment_position(equipment_id).is_some() {
            return Err(LogisticsValidationError::CarriedEquipmentAlsoLocated {
                equipment: equipment_id,
            });
        }
        if let Some(element) = record.supported_by() {
            return Err(LogisticsValidationError::CarriedEquipmentMounted {
                equipment: equipment_id,
                element,
            });
        }
        let definition = registry
            .get_equipment(record.definition())
            .unwrap_or_else(|| {
                unreachable!(
                    "equipment owner validation proves carried equipment definition exists"
                )
            });
        if definition.requires_structural_support() {
            return Err(LogisticsValidationError::CarriedEquipmentRequiresSupport {
                equipment: equipment_id,
            });
        }
    }
    Ok(())
}

fn validate_quick_access(
    registries: &Registries,
    state: &LogisticsState,
) -> Result<(), LogisticsValidationError> {
    let active = state.active_quick_access_slot();
    if !active.is_valid() {
        return Err(LogisticsValidationError::QuickAccessSlotOutOfRange {
            slot: active.value(),
        });
    }

    let slots = state.quick_access_slots().collect::<Vec<_>>();
    for (index, (slot, selection)) in slots.iter().copied().enumerate() {
        let Some(selection) = selection else {
            continue;
        };
        if state.player().is_none() {
            return Err(LogisticsValidationError::QuickAccessSelectionWithoutPlayer { slot });
        }
        match selection {
            PlayerActiveSelection::Equipment(equipment) => {
                if !state.is_equipment_carried(equipment) {
                    return Err(LogisticsValidationError::QuickAccessEquipmentNotCarried {
                        slot,
                        equipment,
                    });
                }
            }
            PlayerActiveSelection::Material(commodity) => {
                if !registries.materials().has_commodity(commodity) {
                    return Err(LogisticsValidationError::QuickAccessMaterialUnsupported {
                        slot,
                        commodity,
                    });
                }
            }
        }
        if let Some((duplicate_slot, _)) = slots[index + 1..]
            .iter()
            .copied()
            .find(|(_, other)| *other == Some(selection))
        {
            return Err(LogisticsValidationError::DuplicateQuickAccessSelection {
                first: slot,
                second: duplicate_slot,
            });
        }
    }
    Ok(())
}

fn validate_player_custody(
    state: &LogisticsState,
    inventory: &InventoryState,
) -> Result<(), LogisticsValidationError> {
    let Some(player) = state.player().copied() else {
        return Ok(());
    };
    let stockpile = inventory.get_stockpile(player.carried_stockpile()).ok_or(
        LogisticsValidationError::UnknownCarriedStockpile {
            stockpile: player.carried_stockpile(),
        },
    )?;
    if let Some(element) = stockpile.supported_by() {
        return Err(LogisticsValidationError::CarriedStockpileMounted {
            stockpile: player.carried_stockpile(),
            element,
        });
    }
    if let Some(enclosure) = stockpile.enclosure() {
        return Err(LogisticsValidationError::CarriedStockpileEnclosed {
            stockpile: player.carried_stockpile(),
            definition: enclosure.definition(),
        });
    }
    Ok(())
}

fn validate_stockpile_locations(
    state: &LogisticsState,
    inventory: &InventoryState,
    structures: &StructureState,
) -> Result<(), LogisticsValidationError> {
    if !state.has_valid_stockpile_position_index() {
        return Err(LogisticsValidationError::StockpilePositionIndexInconsistent);
    }
    for (stockpile, position) in state.stockpile_locations() {
        if state
            .player()
            .is_some_and(|player| player.carried_stockpile() == stockpile)
        {
            return Err(LogisticsValidationError::LocatedStockpileAlsoCarried { stockpile });
        }
        let record = inventory
            .get_stockpile(stockpile)
            .ok_or(LogisticsValidationError::UnknownLocatedStockpile { stockpile })?;
        if let Some(element) = record.supported_by()
            && let Some(support) = structures.get_element(element)
            && !support.bounds().has_voxel(position)
        {
            return Err(LogisticsValidationError::StockpileOutsideSupport {
                stockpile,
                position,
                element,
            });
        }
    }
    Ok(())
}

fn validate_equipment_locations(
    state: &LogisticsState,
    equipment: &EquipmentState,
    structures: &StructureState,
) -> Result<(), LogisticsValidationError> {
    if !state.has_valid_equipment_position_index() {
        return Err(LogisticsValidationError::EquipmentPositionIndexInconsistent);
    }
    for (equipment_id, position) in state.equipment_locations() {
        let record = equipment.get_equipment(equipment_id).ok_or(
            LogisticsValidationError::UnknownLocatedEquipment {
                equipment: equipment_id,
            },
        )?;
        if let Some(element) = record.supported_by()
            && let Some(support) = structures.get_element(element)
            && !support.bounds().has_voxel(position)
        {
            return Err(LogisticsValidationError::EquipmentOutsideSupport {
                equipment: equipment_id,
                position,
                element,
            });
        }
    }
    Ok(())
}

fn validate_energy_store_locations(
    state: &LogisticsState,
    energy: &EnergyState,
) -> Result<(), LogisticsValidationError> {
    if !state.has_valid_energy_position_index() {
        return Err(LogisticsValidationError::EnergyStorePositionIndexInconsistent);
    }
    for (store, _) in state.energy_store_locations() {
        if energy.get_store(store).is_none() {
            return Err(LogisticsValidationError::UnknownLocatedEnergyStore { store });
        }
    }
    Ok(())
}

fn validate_fluid_store_locations(
    state: &LogisticsState,
    fluid: &FluidState,
    structures: &StructureState,
) -> Result<(), LogisticsValidationError> {
    if !state.has_valid_fluid_position_index() {
        return Err(LogisticsValidationError::FluidStorePositionIndexInconsistent);
    }
    for (store, position) in state.fluid_store_locations() {
        let record = fluid
            .get_store(store)
            .ok_or(LogisticsValidationError::UnknownLocatedFluidStore { store })?;
        if let Some(element) = record.supported_by()
            && let Some(support) = structures.get_element(element)
            && !support.bounds().has_voxel(position)
        {
            return Err(LogisticsValidationError::FluidStoreOutsideSupport {
                store,
                position,
                element,
            });
        }
    }
    Ok(())
}

fn validate_player_body_spatial_clearance(
    registries: &Registries,
    app: &AppState,
    body: PlayerBodyState,
) -> Result<(), LogisticsValidationError> {
    let locomotion = registries.survival().physiology().locomotion();
    if let Some(voxel) =
        first_player_body_collision(registries.world(), app.world(), body, locomotion)
    {
        return Err(LogisticsValidationError::PlayerBodyIntersectsSolid { voxel });
    }
    if let Some(voxel) =
        crate::logistics::first_player_temple_caste_collision(registries, app, body)
    {
        return Err(LogisticsValidationError::PlayerBodyIntersectsTempleCaste { voxel });
    }
    if let Some(voxel) = crate::logistics::first_player_structure_collision(registries, app, body) {
        return Err(LogisticsValidationError::PlayerBodyIntersectsStructure { voxel });
    }
    Ok(())
}

pub(crate) fn validate_loaded_logistics(
    registries: &Registries,
    app: &AppState,
) -> Result<(), LogisticsValidationError> {
    let state = app.logistics();
    validate_revision(state)?;
    validate_traversal_edges(state)?;
    validate_player_custody(state, app.inventory())?;
    validate_carried_equipment(state, app.equipment(), registries.equipment())?;
    validate_quick_access(registries, state)?;
    if state.pending_secondary_use().is_some() && state.pending_primary_break().is_some() {
        return Err(LogisticsValidationError::MultiplePendingDirectActions);
    }
    validate_pending_secondary_use(app, state)?;
    validate_pending_primary_break(app, state)?;
    validate_stockpile_locations(state, app.inventory(), app.structures())?;
    if let Some(voxel) = crate::world::first_buried_stationary_enclosure_in_world(registries, app) {
        return Err(LogisticsValidationError::StationaryEnclosureIntersectsSolidTerrain { voxel });
    }
    validate_equipment_locations(state, app.equipment(), app.structures())?;
    validate_energy_store_locations(state, app.energy())?;
    validate_fluid_store_locations(state, app.fluid(), app.structures())?;
    if let Some((_, voxel)) = app.logistics().fluid_store_locations().find(|&(_, voxel)| {
        app.world()
            .block_at(voxel)
            .and_then(|block| registries.world().block_for_state(block))
            .is_some_and(|block| block.collision() == crate::world::BlockCollisionShape::FullCube)
    }) {
        return Err(LogisticsValidationError::LocatedFluidStoreIntersectsSolidTerrain { voxel });
    }
    if let Some(player) = state.player() {
        let locomotion = registries.survival().physiology().locomotion();
        if !player_body_fits_continuous_world(player.body(), locomotion) {
            return Err(LogisticsValidationError::PlayerBodyOutsideContinuousWorld);
        }
        let respawn_body = PlayerBodyState::centered_on_voxel_floor(player.respawn_position())
            .filter(|body| player_body_fits_continuous_world(*body, locomotion));
        if respawn_body.is_none() {
            return Err(
                LogisticsValidationError::PlayerRespawnPositionOutsideContinuousWorld {
                    position: player.respawn_position(),
                },
            );
        }
        let horizontal = player.body().horizontal_velocity();
        let x = u128::from(horizontal.x_nanometers_per_second().unsigned_abs());
        let z = u128::from(horizontal.z_nanometers_per_second().unsigned_abs());
        let horizontal_squared = x * x + z * z;
        let maximum_horizontal = locomotion.sprinting_speed().nanometers_per_second();
        if horizontal_squared > u128::from(maximum_horizontal) * u128::from(maximum_horizontal) {
            return Err(
                LogisticsValidationError::PlayerHorizontalVelocityExceedsMaximumSpeed {
                    x_nanometers_per_second: horizontal.x_nanometers_per_second(),
                    z_nanometers_per_second: horizontal.z_nanometers_per_second(),
                    maximum_nanometers_per_second: maximum_horizontal,
                },
            );
        }
        let vertical = player.body().vertical_velocity().nanometers_per_second();
        let maximum_vertical = if vertical >= 0 {
            locomotion
                .jump_speed_for_load(
                    crate::core::quantity::Mass::ZERO,
                    registries.survival().physiology().carried_mass_capacity(),
                )
                .nanometers_per_second()
        } else {
            locomotion.maximum_fall_speed().nanometers_per_second()
        };
        if vertical.unsigned_abs() > maximum_vertical {
            return Err(
                LogisticsValidationError::PlayerVerticalVelocityExceedsMaximum {
                    nanometers_per_second: vertical,
                    maximum_nanometers_per_second: maximum_vertical,
                },
            );
        }
        let locomotion_exertion = player.locomotion_exertion();
        if !locomotion_exertion.has_valid_remainders() {
            return Err(LogisticsValidationError::PlayerLocomotionExertionRemainderInvalid);
        }
        let pending = locomotion_exertion.pending();
        let maximum = locomotion.maximum_pending_exertion();
        if pending.energy_cost_per_tick() > maximum.energy_cost_per_tick()
            || pending.hydration_loss_per_tick() > maximum.hydration_loss_per_tick()
        {
            return Err(LogisticsValidationError::PlayerPendingLocomotionExertionExceedsEnvelope);
        }
        if player.pending_fall_vitality_loss_ppm()
            > crate::core::arithmetic::NORMALIZED_PARTS_PER_MILLION
        {
            return Err(
                LogisticsValidationError::PlayerPendingFallVitalityLossInvalid {
                    loss_ppm: player.pending_fall_vitality_loss_ppm(),
                },
            );
        }
        validate_player_body_spatial_clearance(registries, app, player.body())?;
    }
    Ok(())
}
