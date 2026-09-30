//! Read-only compatible-input aggregation for familiar manual-craft planning.

use std::collections::BTreeMap;

use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::inventory::StockpileId;
use crate::material::{CommodityKey, MaterialComposition};
use crate::production::ProcessId;
use crate::registry::ProcessEquipmentRole;

pub(super) struct CompatibleInputGroup {
    temperature: Temperature,
    mass: Mass,
}

impl CompatibleInputGroup {
    pub(super) const fn temperature(&self) -> Temperature {
        self.temperature
    }

    pub(super) const fn mass(&self) -> Mass {
        self.mass
    }
}

/// One stockpile scan for a durable homogeneous commodity.
///
/// Recipe catalogs can reuse this across every manual transform with the same input commodity,
/// avoiding repeated lot walks while still deriving recipe-specific batch counts afterward.
pub(super) struct CompatibleInputInventory {
    total_eligible_mass: Mass,
    groups: Vec<CompatibleInputGroup>,
}

impl CompatibleInputInventory {
    pub(super) fn availability(
        &self,
        input: CommodityKey,
        batch_mass: Mass,
    ) -> ManualCraftInputAvailability {
        let largest_compatible_mass = self
            .groups
            .iter()
            .map(CompatibleInputGroup::mass)
            .max()
            .unwrap_or(Mass::ZERO);
        let craftable_temperature_groups = self
            .groups
            .iter()
            .filter(|group| group.mass() >= batch_mass)
            .count()
            .try_into()
            .unwrap_or_else(|_| panic!("manual-craft temperature group count exceeds u64"));
        ManualCraftInputAvailability {
            input,
            batch_mass,
            total_eligible_mass: self.total_eligible_mass,
            largest_compatible_mass,
            maximum_batches: largest_compatible_mass.milligrams() / batch_mass.milligrams(),
            craftable_temperature_groups,
        }
    }

    pub(super) fn groups(&self) -> &[CompatibleInputGroup] {
        &self.groups
    }
}

pub(super) fn scan_compatible_input(
    state: &AppState,
    source: StockpileId,
    input: CommodityKey,
) -> CompatibleInputInventory {
    let expected_composition = MaterialComposition::pure(input.material());
    let mut groups_by_temperature = BTreeMap::<Temperature, Mass>::new();
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
        if lot.composition() != &expected_composition {
            continue;
        }
        total_eligible_mass = total_eligible_mass
            .checked_add(lot.mass())
            .unwrap_or_else(|| panic!("validated stockpile eligible manual-craft mass overflowed"));
        let group_mass = groups_by_temperature
            .entry(lot.temperature())
            .or_insert(Mass::ZERO);
        *group_mass = group_mass.checked_add(lot.mass()).unwrap_or_else(|| {
            panic!("validated stockpile compatible manual-craft mass overflowed")
        });
    }
    let groups = groups_by_temperature
        .into_iter()
        .map(|(temperature, mass)| CompatibleInputGroup { temperature, mass })
        .collect();
    CompatibleInputInventory {
        total_eligible_mass,
        groups,
    }
}

/// How one recipe's material input can be presented to an ordinary inventory caller.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManualCraftInputMode {
    /// Durable homogeneous matter can be selected deterministically behind the recipe action.
    Automatic(ManualCraftInputAvailability),
    /// More than one temperature cohort can independently satisfy at least one batch.
    ///
    /// Choosing between them changes output temperature, so presentation must keep that choice
    /// visible instead of allowing persistent lot identity to decide it implicitly.
    TemperatureChoice(ManualCraftInputAvailability),
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
/// `maximum_batches` is based on the largest temperature-compatible pool because manual shaping
/// cannot silently mix different material temperatures. `total_eligible_mass` remains visible so a
/// caller can distinguish a true shortage from matter that exists but is split across incompatible
/// thermal states.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualCraftInputAvailability {
    input: CommodityKey,
    batch_mass: Mass,
    total_eligible_mass: Mass,
    largest_compatible_mass: Mass,
    maximum_batches: u64,
    craftable_temperature_groups: u64,
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
    pub const fn largest_compatible_mass(self) -> Mass {
        self.largest_compatible_mass
    }

    #[must_use]
    pub const fn maximum_batches(self) -> u64 {
        self.maximum_batches
    }

    /// Number of distinct input temperatures that can each supply at least one complete batch.
    #[must_use]
    pub const fn craftable_temperature_groups(self) -> u64 {
        self.craftable_temperature_groups
    }
}
