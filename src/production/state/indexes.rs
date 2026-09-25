//! Derived production scheduling and exclusive-resource indexes.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::time::SimulationTick;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::StockpileId;

use super::{ProductionJobId, ProductionJobRecord, ProductionSuspensionReason};

mod integrity;
mod mutation;
mod schedule;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProductionJobIndexProjection {
    due_tick: Option<SimulationTick>,
    consumed_energy_store: Option<EnergyStoreId>,
    released_energy_store: Option<EnergyStoreId>,
    equipment: Option<EquipmentId>,
    output_stockpiles: BTreeSet<StockpileId>,
    future_material_lot_id_demand: u64,
    suspended: bool,
    player_labor_suspended: bool,
    requires_active_support: bool,
    requires_energy_revision: bool,
    requires_equipment_revision: bool,
}

impl ProductionJobIndexProjection {
    pub(super) fn from_job(job: &ProductionJobRecord) -> Self {
        Self {
            due_tick: (!job.is_suspended()).then_some(job.completes_at()),
            consumed_energy_store: job.consumed_energy().map(|trace| trace.source()),
            released_energy_store: job.released_energy().map(|trace| trace.destination()),
            equipment: job
                .equipment_provider()
                .map(|provider| provider.equipment()),
            output_stockpiles: job
                .output_streams()
                .iter()
                .map(|stream| stream.destination())
                .collect(),
            future_material_lot_id_demand: job.future_material_lot_id_demand_upper_bound(),
            suspended: job.is_suspended(),
            player_labor_suspended: job.suspension().is_some_and(|suspension| {
                suspension.reason() == ProductionSuspensionReason::PlayerLaborUnavailable
            }),
            requires_active_support: job.has_required_active_support(),
            requires_energy_revision: job.requires_energy_revision_at_completion(),
            requires_equipment_revision: job.requires_equipment_revision_at_completion(),
        }
    }

    fn energy_stores(&self) -> impl Iterator<Item = EnergyStoreId> {
        [self.consumed_energy_store, self.released_energy_store]
            .into_iter()
            .flatten()
    }
}

/// Cross-owner revisions whose changes can alter production physical availability.
///
/// This is a disposable derived cache key, never persisted world truth. A missing snapshot forces a
/// conservative full availability pass, including the first tick after trusted-load index rebuild.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::production) struct ProductionAvailabilityDependencyRevisions {
    inventory_support: u64,
    equipment_support: u64,
    structures: u64,
}

impl ProductionAvailabilityDependencyRevisions {
    pub(in crate::production) const fn new(
        inventory_support: u64,
        equipment_support: u64,
        structures: u64,
    ) -> Self {
        Self {
            inventory_support,
            equipment_support,
            structures,
        }
    }
}

#[cfg(test)]
#[path = "indexes_tests.rs"]
mod tests;

#[derive(Clone, Debug, Default)]
pub(super) struct ProductionIndexes {
    pub(super) due_jobs: BTreeMap<SimulationTick, BTreeSet<ProductionJobId>>,
    energy_revision_jobs_by_due: BTreeMap<SimulationTick, u64>,
    equipment_revision_jobs_by_due: BTreeMap<SimulationTick, u64>,
    energy_occupancy: BTreeMap<EnergyStoreId, ProductionJobId>,
    equipment_occupancy: BTreeMap<EquipmentId, ProductionJobId>,
    output_stockpile_occupancy: BTreeMap<StockpileId, BTreeSet<ProductionJobId>>,
    future_material_lot_id_demand: u64,
    suspended_jobs: BTreeSet<ProductionJobId>,
    player_labor_suspended_jobs: BTreeSet<ProductionJobId>,
    required_active_support_jobs: BTreeSet<ProductionJobId>,
    availability_dependencies: Option<ProductionAvailabilityDependencyRevisions>,
}

impl PartialEq for ProductionIndexes {
    fn eq(&self, other: &Self) -> bool {
        self.due_jobs == other.due_jobs
            && self.energy_revision_jobs_by_due == other.energy_revision_jobs_by_due
            && self.equipment_revision_jobs_by_due == other.equipment_revision_jobs_by_due
            && self.energy_occupancy == other.energy_occupancy
            && self.equipment_occupancy == other.equipment_occupancy
            && self.output_stockpile_occupancy == other.output_stockpile_occupancy
            && self.future_material_lot_id_demand == other.future_material_lot_id_demand
            && self.suspended_jobs == other.suspended_jobs
            && self.player_labor_suspended_jobs == other.player_labor_suspended_jobs
            && self.required_active_support_jobs == other.required_active_support_jobs
    }
}

impl Eq for ProductionIndexes {}

impl ProductionIndexes {
    pub(super) const fn new() -> Self {
        Self {
            due_jobs: BTreeMap::new(),
            energy_revision_jobs_by_due: BTreeMap::new(),
            equipment_revision_jobs_by_due: BTreeMap::new(),
            energy_occupancy: BTreeMap::new(),
            equipment_occupancy: BTreeMap::new(),
            output_stockpile_occupancy: BTreeMap::new(),
            future_material_lot_id_demand: 0,
            suspended_jobs: BTreeSet::new(),
            player_labor_suspended_jobs: BTreeSet::new(),
            required_active_support_jobs: BTreeSet::new(),
            availability_dependencies: None,
        }
    }

    pub(super) fn rebuild<'a>(
        &mut self,
        jobs: impl Iterator<Item = (&'a ProductionJobId, &'a ProductionJobRecord)>,
    ) {
        *self = Self::new();
        for (id, job) in jobs {
            self.insert_rebuilt(*id, &ProductionJobIndexProjection::from_job(job));
        }
    }

    fn insert_rebuilt(&mut self, id: ProductionJobId, projection: &ProductionJobIndexProjection) {
        self.future_material_lot_id_demand = self
            .future_material_lot_id_demand
            .checked_add(projection.future_material_lot_id_demand)
            .unwrap_or_else(|| unreachable!("resident production output parcels fit u64"));
        if let Some(due_tick) = projection.due_tick {
            self.insert_due_job_with_requirements(
                id,
                due_tick,
                projection.requires_energy_revision,
                projection.requires_equipment_revision,
            );
        }
        for store in projection.energy_stores() {
            self.energy_occupancy.entry(store).or_insert(id);
        }
        if let Some(equipment) = projection.equipment {
            self.equipment_occupancy.entry(equipment).or_insert(id);
        }
        if projection.suspended {
            self.suspended_jobs.insert(id);
        }
        if projection.player_labor_suspended {
            self.player_labor_suspended_jobs.insert(id);
        }
        if projection.requires_active_support {
            self.required_active_support_jobs.insert(id);
        }
        for stockpile in &projection.output_stockpiles {
            self.output_stockpile_occupancy
                .entry(*stockpile)
                .or_default()
                .insert(id);
        }
    }

    pub(super) const fn future_material_lot_id_demand(&self) -> u64 {
        self.future_material_lot_id_demand
    }

    pub(super) fn player_labor_suspended_jobs(&self) -> impl Iterator<Item = ProductionJobId> + '_ {
        self.player_labor_suspended_jobs.iter().copied()
    }

    pub(super) fn physical_availability_candidate_jobs(
        &self,
        supported_stockpiles: impl IntoIterator<Item = StockpileId>,
    ) -> BTreeSet<ProductionJobId> {
        let mut candidates = self.suspended_jobs.clone();
        candidates.extend(self.required_active_support_jobs.iter().copied());
        for stockpile in supported_stockpiles {
            if let Some(jobs) = self.output_stockpile_occupancy.get(&stockpile) {
                candidates.extend(jobs.iter().copied());
            }
        }
        candidates
    }

    pub(super) fn physical_availability_dependencies_changed(
        &self,
        current: ProductionAvailabilityDependencyRevisions,
    ) -> bool {
        self.availability_dependencies != Some(current)
    }

    pub(super) fn record_physical_availability_dependencies(
        &mut self,
        current: ProductionAvailabilityDependencyRevisions,
    ) {
        self.availability_dependencies = Some(current);
    }

    pub(super) fn energy_occupant(&self, store: EnergyStoreId) -> Option<ProductionJobId> {
        self.energy_occupancy.get(&store).copied()
    }

    pub(super) fn equipment_occupant(&self, equipment: EquipmentId) -> Option<&ProductionJobId> {
        self.equipment_occupancy.get(&equipment)
    }

    pub(super) fn output_stockpile_occupants(
        &self,
        stockpile: StockpileId,
    ) -> Option<&BTreeSet<ProductionJobId>> {
        self.output_stockpile_occupancy.get(&stockpile)
    }
}
