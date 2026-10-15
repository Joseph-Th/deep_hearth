//! Player-facing inventory and co-located container projection over canonical custody owners.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::equipment::{EquipmentDefinitionId, EquipmentId};
use crate::inventory::{
    MaterialStackProjection, MaterialStackProjectionError, StockpileId, StorageDefinitionId,
    project_material_stacks,
};
use crate::maintenance::Condition;
use crate::registry::Registries;

use super::{
    PLAYER_QUICK_ACCESS_SLOT_COUNT, PlayerActiveSelection, PlayerCarryingAssessment,
    PlayerQuickAccessSlot, assess_player_carrying,
};

/// Failure while projecting the ordinary player's current inventory/container view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerInventoryProjectionError {
    PlayerNotAdmitted,
    Stack(MaterialStackProjectionError),
}

impl Display for PlayerInventoryProjectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => formatter.write_str("player is not admitted"),
            Self::Stack(error) => write!(formatter, "material stack projection failed: {error}"),
        }
    }
}

impl Error for PlayerInventoryProjectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PlayerNotAdmitted => None,
            Self::Stack(error) => Some(error),
        }
    }
}

/// One carried durable equipment instance rendered alongside material stacks.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerCarriedEquipmentProjection {
    equipment: EquipmentId,
    definition: EquipmentDefinitionId,
    condition: Condition,
    mass: Mass,
}

impl PlayerCarriedEquipmentProjection {
    #[must_use]
    pub const fn equipment(self) -> EquipmentId {
        self.equipment
    }

    #[must_use]
    pub const fn definition(self) -> EquipmentDefinitionId {
        self.definition
    }

    #[must_use]
    pub const fn condition(self) -> Condition {
        self.condition
    }

    #[must_use]
    pub const fn mass(self) -> Mass {
        self.mass
    }
}

/// One carried or co-located stockpile projected as familiar material stacks.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerStockpileProjection {
    stockpile: StockpileId,
    storage_definition: Option<StorageDefinitionId>,
    capacity: Mass,
    stored: Mass,
    reserved_inbound: Mass,
    available: Mass,
    stacks: Vec<MaterialStackProjection>,
}

impl PlayerStockpileProjection {
    #[must_use]
    pub const fn stockpile(&self) -> StockpileId {
        self.stockpile
    }

    #[must_use]
    pub const fn storage_definition(&self) -> Option<StorageDefinitionId> {
        self.storage_definition
    }

    #[must_use]
    pub const fn capacity(&self) -> Mass {
        self.capacity
    }

    #[must_use]
    pub const fn stored(&self) -> Mass {
        self.stored
    }

    #[must_use]
    pub const fn reserved_inbound(&self) -> Mass {
        self.reserved_inbound
    }

    #[must_use]
    pub const fn available(&self) -> Mass {
        self.available
    }

    #[must_use]
    pub fn stacks(&self) -> &[MaterialStackProjection] {
        &self.stacks
    }
}

/// Ordinary read model for carried contents, hotbar bindings, and containers at the player voxel.
///
/// This value owns no custody and grants no authorization. Consequential transfers must pass its
/// revision-bound stack projections back through canonical transfer validators.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerInventoryProjection {
    carrying: PlayerCarryingAssessment,
    quick_access_slots: [Option<PlayerActiveSelection>; PLAYER_QUICK_ACCESS_SLOT_COUNT],
    active_quick_access_slot: PlayerQuickAccessSlot,
    carried: PlayerStockpileProjection,
    equipment: Vec<PlayerCarriedEquipmentProjection>,
    local_containers: Vec<PlayerStockpileProjection>,
}

impl PlayerInventoryProjection {
    #[must_use]
    pub const fn carrying(&self) -> PlayerCarryingAssessment {
        self.carrying
    }

    #[must_use]
    pub const fn quick_access_slots(
        &self,
    ) -> &[Option<PlayerActiveSelection>; PLAYER_QUICK_ACCESS_SLOT_COUNT] {
        &self.quick_access_slots
    }

    #[must_use]
    pub const fn active_quick_access_slot(&self) -> PlayerQuickAccessSlot {
        self.active_quick_access_slot
    }

    #[must_use]
    pub const fn carried(&self) -> &PlayerStockpileProjection {
        &self.carried
    }

    #[must_use]
    pub fn equipment(&self) -> &[PlayerCarriedEquipmentProjection] {
        &self.equipment
    }

    #[must_use]
    pub fn local_containers(&self) -> &[PlayerStockpileProjection] {
        &self.local_containers
    }
}

fn project_stockpile(
    registries: &Registries,
    state: &AppState,
    stockpile: StockpileId,
) -> Result<PlayerStockpileProjection, PlayerInventoryProjectionError> {
    let record = state
        .inventory()
        .get_stockpile(stockpile)
        .unwrap_or_else(|| panic!("runtime invariant broken: player-facing stockpile disappeared"));
    let stacks = project_material_stacks(registries, state, stockpile)
        .map_err(PlayerInventoryProjectionError::Stack)?;
    Ok(PlayerStockpileProjection {
        stockpile,
        storage_definition: record.enclosure().map(|enclosure| enclosure.definition()),
        capacity: record.capacity(),
        stored: record.stored_mass(),
        reserved_inbound: record.reserved_inbound(),
        available: record.available_capacity(),
        stacks,
    })
}

/// Projects the admitted player's carried inventory and only containers at their exact voxel.
pub fn project_player_inventory(
    registries: &Registries,
    state: &AppState,
) -> Result<PlayerInventoryProjection, PlayerInventoryProjectionError> {
    let player = state
        .logistics()
        .player()
        .copied()
        .ok_or(PlayerInventoryProjectionError::PlayerNotAdmitted)?;
    let carrying = assess_player_carrying(state)
        .unwrap_or_else(|| unreachable!("admitted logistics player has carrying assessment"));
    let carried = project_stockpile(registries, state, player.carried_stockpile())?;
    let equipment = state
        .logistics()
        .carried_equipment()
        .map(|equipment| {
            let record = state
                .equipment()
                .get_equipment(equipment)
                .unwrap_or_else(|| {
                    panic!("runtime invariant broken: carried equipment disappeared")
                });
            PlayerCarriedEquipmentProjection {
                equipment,
                definition: record.definition(),
                condition: record.condition(),
                mass: record.embodied_mass(),
            }
        })
        .collect();
    let mut quick_access_slots = [None; PLAYER_QUICK_ACCESS_SLOT_COUNT];
    for (slot, selection) in state.logistics().quick_access_slots() {
        quick_access_slots[slot.index()] = selection;
    }
    let local_containers = state
        .available_local_ground_stockpiles()
        .map(|record| project_stockpile(registries, state, record.id()))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PlayerInventoryProjection {
        carrying,
        quick_access_slots,
        active_quick_access_slot: state.logistics().active_quick_access_slot(),
        carried,
        equipment,
        local_containers,
    })
}

#[cfg(test)]
#[path = "inventory_view_tests.rs"]
mod tests;
