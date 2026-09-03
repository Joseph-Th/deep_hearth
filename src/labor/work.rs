//! Durable records for work that exclusively occupies the local player's attention.

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Mass, Volume};
use crate::core::time::SimulationTick;
use crate::energy::{EnergyStoreId, ReleasedEnergyTrace};
use crate::equipment::{EquipmentId, EquipmentOperationTrace};
use crate::inventory::{StockpileId, StorageDefinitionId};
use crate::maintenance::Condition;
use crate::material::MaterialId;
use crate::mining::MiningJobId;
use crate::production::ProductionJobId;
use crate::spatial::VoxelBounds;

use super::{ManualPowerMethodId, ProspectingMethodId};

/// Durable direct-labor work order that converts player effort into finite mechanical energy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManualPowerWork {
    method: ManualPowerMethodId,
    equipment: EquipmentOperationTrace,
    condition_after: Condition,
    output: ReleasedEnergyTrace,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl ManualPowerWork {
    pub(crate) const fn new(
        method: ManualPowerMethodId,
        equipment: EquipmentOperationTrace,
        condition_after: Condition,
        output: ReleasedEnergyTrace,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            method,
            equipment,
            condition_after,
            output,
            started_at,
            completes_at,
        }
    }

    #[must_use]
    pub const fn method(self) -> ManualPowerMethodId {
        self.method
    }

    #[must_use]
    pub const fn equipment(self) -> EquipmentId {
        self.equipment.equipment()
    }

    #[must_use]
    pub const fn equipment_trace(self) -> EquipmentOperationTrace {
        self.equipment
    }

    #[must_use]
    pub const fn condition_after(self) -> Condition {
        self.condition_after
    }

    #[must_use]
    pub const fn destination(self) -> EnergyStoreId {
        self.output.destination()
    }

    #[must_use]
    pub const fn output(self) -> ReleasedEnergyTrace {
        self.output
    }

    #[must_use]
    pub const fn started_at(self) -> SimulationTick {
        self.started_at
    }

    #[must_use]
    pub const fn completes_at(self) -> SimulationTick {
        self.completes_at
    }
}

/// Durable direct-labor interval for dismantling one installed storage enclosure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageEnclosureDismantlingWork {
    target: StockpileId,
    recovery_destination: StockpileId,
    definition: StorageDefinitionId,
    enclosure_created_at: SimulationTick,
    recovered_mass: Mass,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl StorageEnclosureDismantlingWork {
    pub(crate) const fn new(
        target: StockpileId,
        recovery_destination: StockpileId,
        definition: StorageDefinitionId,
        enclosure_created_at: SimulationTick,
        recovered_mass: Mass,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            target,
            recovery_destination,
            definition,
            enclosure_created_at,
            recovered_mass,
            started_at,
            completes_at,
        }
    }

    #[must_use]
    pub const fn target(self) -> StockpileId {
        self.target
    }

    #[must_use]
    pub const fn recovery_destination(self) -> StockpileId {
        self.recovery_destination
    }

    #[must_use]
    pub const fn definition(self) -> StorageDefinitionId {
        self.definition
    }

    #[must_use]
    pub const fn enclosure_created_at(self) -> SimulationTick {
        self.enclosure_created_at
    }

    #[must_use]
    pub const fn recovered_mass(self) -> Mass {
        self.recovered_mass
    }

    #[must_use]
    pub const fn started_at(self) -> SimulationTick {
        self.started_at
    }

    #[must_use]
    pub const fn completes_at(self) -> SimulationTick {
        self.completes_at
    }

    #[must_use]
    pub fn occupies_stockpile(self, stockpile: StockpileId) -> bool {
        self.target == stockpile || self.recovery_destination == stockpile
    }
}

/// Durable direct-labor interval for an already-admitted equipment service.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentMaintenanceWork {
    equipment: EquipmentOperationTrace,
    condition_after: Condition,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl EquipmentMaintenanceWork {
    pub(crate) const fn new(
        equipment: EquipmentOperationTrace,
        condition_after: Condition,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            equipment,
            condition_after,
            started_at,
            completes_at,
        }
    }

    #[must_use]
    pub const fn equipment(self) -> EquipmentId {
        self.equipment.equipment()
    }

    #[must_use]
    pub const fn equipment_trace(self) -> EquipmentOperationTrace {
        self.equipment
    }

    #[must_use]
    pub const fn condition_before(self) -> Condition {
        self.equipment.condition()
    }

    #[must_use]
    pub const fn condition_after(self) -> Condition {
        self.condition_after
    }

    #[must_use]
    pub const fn started_at(self) -> SimulationTick {
        self.started_at
    }

    #[must_use]
    pub const fn completes_at(self) -> SimulationTick {
        self.completes_at
    }
}

/// Durable attention interval occupied by one already-admitted direct meal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EatingWork {
    mass: Mass,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl EatingWork {
    pub(crate) const fn new(
        mass: Mass,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            mass,
            started_at,
            completes_at,
        }
    }

    #[must_use]
    pub const fn mass(self) -> Mass {
        self.mass
    }

    #[must_use]
    pub const fn started_at(self) -> SimulationTick {
        self.started_at
    }

    #[must_use]
    pub const fn completes_at(self) -> SimulationTick {
        self.completes_at
    }
}

/// Durable attention interval occupied by one already-admitted direct drink.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrinkingWork {
    volume: Volume,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl DrinkingWork {
    pub(crate) const fn new(
        volume: Volume,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            volume,
            started_at,
            completes_at,
        }
    }

    #[must_use]
    pub const fn volume(self) -> Volume {
        self.volume
    }

    #[must_use]
    pub const fn started_at(self) -> SimulationTick {
        self.started_at
    }

    #[must_use]
    pub const fn completes_at(self) -> SimulationTick {
        self.completes_at
    }
}

/// Durable bounded field-inspection work that will resolve one geological observation at completion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProspectingWork {
    method: ProspectingMethodId,
    region: VoxelBounds,
    material: MaterialId,
    equipment: Option<EquipmentOperationTrace>,
    condition_after: Option<Condition>,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl ProspectingWork {
    pub(crate) const fn new(
        method: ProspectingMethodId,
        region: VoxelBounds,
        material: MaterialId,
        equipment: Option<EquipmentOperationTrace>,
        condition_after: Option<Condition>,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            method,
            region,
            material,
            equipment,
            condition_after,
            started_at,
            completes_at,
        }
    }

    #[must_use]
    pub const fn method(self) -> ProspectingMethodId {
        self.method
    }

    #[must_use]
    pub const fn region(self) -> VoxelBounds {
        self.region
    }

    #[must_use]
    pub const fn material(self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub const fn equipment(self) -> Option<EquipmentId> {
        match self.equipment {
            Some(trace) => Some(trace.equipment()),
            None => None,
        }
    }

    #[must_use]
    pub const fn equipment_trace(self) -> Option<EquipmentOperationTrace> {
        self.equipment
    }

    #[must_use]
    pub const fn condition_after(self) -> Option<Condition> {
        self.condition_after
    }

    #[must_use]
    pub const fn started_at(self) -> SimulationTick {
        self.started_at
    }

    #[must_use]
    pub const fn completes_at(self) -> SimulationTick {
        self.completes_at
    }
}

/// Durable activity currently monopolizing the local player's labor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum PlayerWork {
    ManualProduction {
        job: ProductionJobId,
    },
    Mining {
        job: MiningJobId,
    },
    ManualPower {
        work: ManualPowerWork,
    },
    Prospecting {
        work: ProspectingWork,
    },
    Eating {
        work: EatingWork,
    },
    Drinking {
        work: DrinkingWork,
    },
    EquipmentMaintenance {
        work: EquipmentMaintenanceWork,
    },
    StorageEnclosureDismantling {
        work: StorageEnclosureDismantlingWork,
    },
}
