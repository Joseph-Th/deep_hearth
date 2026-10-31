//! Read-only compatible-input aggregation for familiar manual-craft planning.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{MaterialLotRecord, StockpileId};
use crate::material::{CommodityKey, MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE, MaterialComposition};
use crate::production::ProcessId;
use crate::registry::ProcessEquipmentRole;

/// One stockpile scan for a durable homogeneous commodity.
///
/// Recipe catalogs can reuse this across every manual transform with the same input commodity,
/// avoiding repeated lot walks while still deriving recipe-specific batch counts afterward.
pub(super) struct CompatibleInputInventory {
    total_eligible_mass: Mass,
}

impl CompatibleInputInventory {
    pub(super) fn availability(
        &self,
        input: CommodityKey,
        batch_mass: Mass,
    ) -> ManualCraftInputAvailability {
        ManualCraftInputAvailability {
            input,
            batch_mass,
            total_eligible_mass: self.total_eligible_mass,
            maximum_batches: self.total_eligible_mass.milligrams() / batch_mass.milligrams(),
        }
    }
}

/// One eligibility rule for both the recipe catalog and exact automatic lot allocation.
/// The precomputed expected composition avoids reconstructing a component vector per lot.
pub(super) fn is_eligible_manual_input(
    lot: &MaterialLotRecord,
    expected_composition: &MaterialComposition,
) -> bool {
    lot.composition() == expected_composition
        && lot.temperature() <= MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE
}

pub(super) fn scan_compatible_input(
    state: &AppState,
    source: StockpileId,
    input: CommodityKey,
) -> CompatibleInputInventory {
    let expected_composition = MaterialComposition::pure(input.material());
    let mut total_eligible_mass = Mass::ZERO;
    for lot_id in state.inventory().lot_ids_for_commodity(source, input) {
        let lot = state.inventory().get_lot(lot_id).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: stockpile {} indexes missing lot {}",
                source.value(),
                lot_id.value()
            )
        });
        debug_assert_eq!(
            lot.commodity(),
            input,
            "commodity index must only return lots for the requested recipe input"
        );
        if !is_eligible_manual_input(lot, &expected_composition) {
            continue;
        }
        total_eligible_mass = total_eligible_mass
            .checked_add(lot.mass())
            .unwrap_or_else(|| panic!("validated stockpile eligible manual-craft mass overflowed"));
    }
    CompatibleInputInventory {
        total_eligible_mass,
    }
}

/// How one recipe's material input can be presented to an ordinary inventory caller.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManualCraftInputMode {
    /// Durable homogeneous matter can be selected deterministically behind the recipe action.
    Automatic(ManualCraftInputAvailability),
    /// Stack-local state such as food age is gameplay-relevant and must remain player-selected.
    ExplicitStackChoice {
        input: CommodityKey,
        batch_mass: Mass,
    },
}

/// One stable recipe-book row for a stockpile, without claiming runtime tool availability.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualCraftStockpileOption {
    pub(super) process: ProcessId,
    pub(super) input_mode: ManualCraftInputMode,
    pub(super) equipment_role: ProcessEquipmentRole,
}

impl ManualCraftStockpileOption {
    #[must_use]
    pub const fn process(self) -> ProcessId {
        self.process
    }

    pub const fn input_mode(self) -> ManualCraftInputMode {
        self.input_mode
    }

    #[must_use]
    pub const fn equipment_role(self) -> ProcessEquipmentRole {
        self.equipment_role
    }
}

/// Read-only stockpile availability for one authored manual recipe.
///
/// Safe-to-handle durable temperature cohorts compose because manual shaping retains their exact
/// thermal state in separate output parcels. Hot stock is unavailable to hand crafting.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualCraftInputAvailability {
    input: CommodityKey,
    batch_mass: Mass,
    total_eligible_mass: Mass,
    maximum_batches: u64,
}

impl ManualCraftInputAvailability {
    #[must_use]
    pub const fn input(self) -> CommodityKey {
        self.input
    }

    #[must_use]
    pub const fn batch_mass(self) -> Mass {
        self.batch_mass
    }

    #[must_use]
    pub const fn total_eligible_mass(self) -> Mass {
        self.total_eligible_mass
    }

    #[must_use]
    pub const fn maximum_batches(self) -> u64 {
        self.maximum_batches
    }
}
