//! Canonical mining start, work completion, and reserved-output claim transactions.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityId, CapabilityValue, CapabilityValueKind};
use crate::core::quantity::{Mass, MassFlow, Pressure};
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::equipment::{EquipmentId, EquipmentProviderError, resolve_equipment_provider};
use crate::geology::{GeologicalDepositId, GeologicalDepositLifecycle};
use crate::inventory::{
    InboundReservationError, ReservedDepositPlan, ReservedDepositPlanError, ReservedDepositRequest,
    StockpileId, StockpileStorageError, StockpileStoredMassChange, StockpileStructuralLoadError,
    ValidatedInboundReservation, ValidatedStockpileStructuralLoad, apply_reserved_deposits,
    decide_reserved_deposits, validate_inbound_reservation, validate_stockpile_storage,
    validate_stockpile_stored_mass_changes, validate_stockpile_support_for_new_inbound,
};
use crate::labor::{
    PlayerWork, PlayerWorkCommitError, PlayerWorkStartError, ValidatedPlayerWorkStart,
    validate_player_work_start,
};
use crate::maintenance::calculate_condition_after_active_ticks;
use crate::material::{MaterialId, MaterialLotSpec, MaterialLotSpecError};
use crate::ore_processing::{MassFlowDurationError, calculate_mass_flow_duration_ceiling};
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

use super::state::{MiningJobIdentity, MiningJobResources, MiningJobSchedule};
use super::{MiningJobId, MiningJobRecord, MiningMethodId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MiningStartError {
    UnknownMethod {
        method: MiningMethodId,
    },
    UnknownDeposit {
        deposit: GeologicalDepositId,
    },
    DepositDepleted {
        deposit: GeologicalDepositId,
    },
    ZeroMass,
    InsufficientDepositMass {
        available: Mass,
        requested: Mass,
    },
    Equipment(EquipmentProviderError),
    EquipmentMounted {
        equipment: EquipmentId,
    },
    EquipmentBusyProduction {
        equipment: EquipmentId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    MissingCapability {
        capability: CapabilityId,
    },
    CapabilityKindMismatch {
        capability: CapabilityId,
        expected: CapabilityValueKind,
        found: CapabilityValueKind,
    },
    UnknownMaterialDefinition {
        material: MaterialId,
    },
    BatchTooLarge {
        maximum: Mass,
        requested: Mass,
    },
    MaterialTooHard {
        hardness: Pressure,
        maximum: Pressure,
    },
    ZeroThroughput,
    Duration(MassFlowDurationError),
    CompletionTickOverflow,
    InvalidOutput(MaterialLotSpecError),
    UnknownDestination {
        stockpile: StockpileId,
    },
    DestinationStorage(StockpileStorageError),
    DestinationMassOverflow {
        stockpile: StockpileId,
    },
    DestinationCapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed: Mass,
        requested: Mass,
    },
    InventoryRevisionExhausted,
    DestinationSupport(StockpileStructuralLoadError),
    GeologyRevisionExhausted,
    EquipmentRevisionExhausted,
    MiningIdExhausted,
    MiningRevisionExhausted,
    Work(PlayerWorkStartError),
}

impl Display for MiningStartError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "mining start failed: {self:?}")
    }
}

impl Error for MiningStartError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MiningStartCommitError {
    StaleGeology {
        expected: u64,
        actual: u64,
    },
    StaleInventory {
        expected: u64,
        actual: u64,
    },
    StaleEquipment {
        expected: u64,
        actual: u64,
    },
    StaleMining {
        expected: u64,
        actual: u64,
    },
    StaleStructure {
        expected: u64,
        actual: u64,
    },
    EquipmentBusyProduction {
        equipment: EquipmentId,
        job: ProductionJobId,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    Work(PlayerWorkCommitError),
}

impl Display for MiningStartCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "mining commit failed: {self:?}")
    }
}

impl Error for MiningStartCommitError {}

#[must_use]
pub struct ValidatedMiningStart {
    revisions: MiningStartRevisions,
    remaining_after: Mass,
    next_mining_job_id: u64,
    reservation: ValidatedInboundReservation,
    work: ValidatedPlayerWorkStart,
    record: MiningJobRecord,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RevisionTransition {
    expected: u64,
    next: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MiningStartRevisions {
    geology: RevisionTransition,
    equipment: RevisionTransition,
    mining: RevisionTransition,
    structure: Option<u64>,
}

impl ValidatedMiningStart {
    pub fn commit(self, state: &mut AppState) -> Result<MiningJobId, MiningStartCommitError> {
        self.work
            .precheck(state)
            .map_err(MiningStartCommitError::Work)?;
        if state.geology().revision() != self.revisions.geology.expected {
            return Err(MiningStartCommitError::StaleGeology {
                expected: self.revisions.geology.expected,
                actual: state.geology().revision(),
            });
        }
        if state.inventory().revision() != self.reservation.expected_revision() {
            return Err(MiningStartCommitError::StaleInventory {
                expected: self.reservation.expected_revision(),
                actual: state.inventory().revision(),
            });
        }
        if state.equipment().revision() != self.revisions.equipment.expected {
            return Err(MiningStartCommitError::StaleEquipment {
                expected: self.revisions.equipment.expected,
                actual: state.equipment().revision(),
            });
        }
        if state.mining().revision() != self.revisions.mining.expected {
            return Err(MiningStartCommitError::StaleMining {
                expected: self.revisions.mining.expected,
                actual: state.mining().revision(),
            });
        }
        if let Some(expected) = self.revisions.structure
            && state.structures().revision() != expected
        {
            return Err(MiningStartCommitError::StaleStructure {
                expected,
                actual: state.structures().revision(),
            });
        }
        if let Some(job) = state
            .production()
            .get_equipment_occupant(self.record.equipment())
        {
            return Err(MiningStartCommitError::EquipmentBusyProduction {
                equipment: self.record.equipment(),
                job: job.id(),
            });
        }
        if let Some(job) = state
            .mining()
            .get_equipment_occupant(self.record.equipment())
        {
            return Err(MiningStartCommitError::EquipmentBusyMining {
                equipment: self.record.equipment(),
                job,
            });
        }
        let id = self.record.id();
        self.reservation.apply(state.inventory_state_mut());
        state.geology_state_mut().apply_extraction(
            self.record.deposit(),
            self.remaining_after,
            self.revisions.geology.next,
        );
        state.equipment_state_mut().apply_condition_change(
            self.record.equipment(),
            self.record.equipment_condition_before(),
            self.record.equipment_condition_after(),
            self.revisions.equipment.next,
        );
        state.mining_state_mut().insert_job(
            self.record,
            self.next_mining_job_id,
            self.revisions.mining.next,
        );
        self.work.apply(state);
        Ok(id)
    }
}

/// Resolves one finite geological slice against a real hand tool and reserves its eventual output.
pub fn validate_start_mining(
    registries: &Registries,
    state: &AppState,
    method: MiningMethodId,
    deposit: GeologicalDepositId,
    destination: StockpileId,
    equipment: EquipmentId,
    mass: Mass,
) -> Result<ValidatedMiningStart, MiningStartError> {
    if mass.is_zero() {
        return Err(MiningStartError::ZeroMass);
    }
    let method_def = registries
        .mining()
        .get_method(method)
        .ok_or(MiningStartError::UnknownMethod { method })?;
    let deposit_record = state
        .geology()
        .get_deposit(deposit)
        .ok_or(MiningStartError::UnknownDeposit { deposit })?;
    if deposit_record.lifecycle() == GeologicalDepositLifecycle::Depleted {
        return Err(MiningStartError::DepositDepleted { deposit });
    }
    if mass > deposit_record.remaining_mass() {
        return Err(MiningStartError::InsufficientDepositMass {
            available: deposit_record.remaining_mass(),
            requested: mass,
        });
    }

    let provider = resolve_equipment_provider(registries, state, equipment)
        .map_err(MiningStartError::Equipment)?;
    if state
        .equipment()
        .get_equipment(equipment)
        .is_some_and(|record| record.supported_by().is_some())
    {
        return Err(MiningStartError::EquipmentMounted { equipment });
    }
    if let Some(job) = state.production().get_equipment_occupant(equipment) {
        return Err(MiningStartError::EquipmentBusyProduction {
            equipment,
            job: job.id(),
            release: job.occupancy_release(),
        });
    }
    if let Some(job) = state.mining().get_equipment_occupant(equipment) {
        return Err(MiningStartError::EquipmentBusyMining { equipment, job });
    }
    let flow_capability = method_def.mass_flow_capability();
    let flow_value =
        provider
            .get_capability(flow_capability)
            .ok_or(MiningStartError::MissingCapability {
                capability: flow_capability,
            })?;
    let CapabilityValue::MassFlow(flow) = flow_value else {
        return Err(MiningStartError::CapabilityKindMismatch {
            capability: flow_capability,
            expected: CapabilityValueKind::MassFlow,
            found: flow_value.kind(),
        });
    };
    let batch_capability = method_def.max_batch_mass_capability();
    let batch_value =
        provider
            .get_capability(batch_capability)
            .ok_or(MiningStartError::MissingCapability {
                capability: batch_capability,
            })?;
    let CapabilityValue::Mass(max_batch) = batch_value else {
        return Err(MiningStartError::CapabilityKindMismatch {
            capability: batch_capability,
            expected: CapabilityValueKind::Mass,
            found: batch_value.kind(),
        });
    };
    let hardness_capability = method_def.max_hardness_capability();
    let hardness_value = provider.get_capability(hardness_capability).ok_or(
        MiningStartError::MissingCapability {
            capability: hardness_capability,
        },
    )?;
    let CapabilityValue::Pressure(max_hardness) = hardness_value else {
        return Err(MiningStartError::CapabilityKindMismatch {
            capability: hardness_capability,
            expected: CapabilityValueKind::Pressure,
            found: hardness_value.kind(),
        });
    };
    if flow == MassFlow::ZERO {
        return Err(MiningStartError::ZeroThroughput);
    }
    if mass > max_batch {
        return Err(MiningStartError::BatchTooLarge {
            maximum: max_batch,
            requested: mass,
        });
    }
    let material_id = deposit_record.commodity().material();
    let material = registries.materials().get_material(material_id).ok_or(
        MiningStartError::UnknownMaterialDefinition {
            material: material_id,
        },
    )?;
    let hardness = Pressure::from_pascals(
        u64::from(material.properties().mechanical().hardness_mpa()) * 1_000_000,
    );
    if hardness > max_hardness {
        return Err(MiningStartError::MaterialTooHard {
            hardness,
            maximum: max_hardness,
        });
    }
    let duration =
        calculate_mass_flow_duration_ceiling(flow, mass, registries.core().ticks_per_second())
            .map_err(MiningStartError::Duration)?;
    let completes_at = state
        .tick()
        .checked_add_span(duration)
        .ok_or(MiningStartError::CompletionTickOverflow)?;
    let condition_before = provider.condition();
    let condition_after = calculate_condition_after_active_ticks(
        method_def.condition_wear_ppm_per_active_tick(),
        condition_before,
        duration,
    );
    let output = MaterialLotSpec::with_composition(
        deposit_record.commodity(),
        mass,
        deposit_record.temperature(),
        deposit_record.composition().clone(),
    )
    .map_err(MiningStartError::InvalidOutput)?;
    let destination_record = state.inventory().get_stockpile(destination).ok_or(
        MiningStartError::UnknownDestination {
            stockpile: destination,
        },
    )?;
    validate_stockpile_storage(
        registries,
        destination_record,
        destination,
        output.commodity(),
        output.composition(),
        output.temperature(),
        output.particle_size_distribution(),
    )
    .map_err(MiningStartError::DestinationStorage)?;
    let expected_structure_revision =
        validate_stockpile_support_for_new_inbound(state, destination)
            .map_err(MiningStartError::DestinationSupport)?;
    let reservation =
        validate_inbound_reservation(state.inventory(), destination, mass).map_err(|error| {
            match error {
                InboundReservationError::UnknownStockpile { stockpile } => {
                    MiningStartError::UnknownDestination { stockpile }
                }
                InboundReservationError::MassOverflow { stockpile } => {
                    MiningStartError::DestinationMassOverflow { stockpile }
                }
                InboundReservationError::CapacityExceeded {
                    stockpile,
                    capacity,
                    committed,
                    requested,
                } => MiningStartError::DestinationCapacityExceeded {
                    stockpile,
                    capacity,
                    committed,
                    requested,
                },
                InboundReservationError::RevisionExhausted => {
                    MiningStartError::InventoryRevisionExhausted
                }
            }
        })?;

    let expected_geology_revision = state.geology().revision();
    let next_geology_revision = expected_geology_revision
        .checked_add(1)
        .ok_or(MiningStartError::GeologyRevisionExhausted)?;
    let remaining_after = deposit_record.remaining_mass().checked_sub(mass).ok_or(
        MiningStartError::InsufficientDepositMass {
            available: deposit_record.remaining_mass(),
            requested: mass,
        },
    )?;
    let expected_equipment_revision = state.equipment().revision();
    let next_equipment_revision = expected_equipment_revision
        .checked_add(1)
        .ok_or(MiningStartError::EquipmentRevisionExhausted)?;
    let expected_mining_revision = state.mining().revision();
    let next_mining_revision = expected_mining_revision
        .checked_add(1)
        .ok_or(MiningStartError::MiningRevisionExhausted)?;
    let job_value = state.mining().next_job_id();
    let next_mining_job_id = job_value
        .checked_add(1)
        .ok_or(MiningStartError::MiningIdExhausted)?;
    let job = MiningJobId::new(job_value);
    let work = validate_player_work_start(state, PlayerWork::Mining { job })
        .map_err(MiningStartError::Work)?;
    Ok(ValidatedMiningStart {
        revisions: MiningStartRevisions {
            geology: RevisionTransition {
                expected: expected_geology_revision,
                next: next_geology_revision,
            },
            equipment: RevisionTransition {
                expected: expected_equipment_revision,
                next: next_equipment_revision,
            },
            mining: RevisionTransition {
                expected: expected_mining_revision,
                next: next_mining_revision,
            },
            structure: expected_structure_revision,
        },
        remaining_after,
        next_mining_job_id,
        reservation,
        work,
        record: MiningJobRecord::new(
            MiningJobIdentity {
                id: job,
                method,
                deposit,
            },
            MiningJobResources {
                destination,
                equipment,
                output,
                equipment_condition_before: condition_before,
                equipment_condition_after: condition_after,
            },
            MiningJobSchedule {
                started_at: state.tick(),
                completes_at,
                ready_at: None,
            },
        ),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MiningTickError {
    RevisionExhausted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MiningTickPlan {
    expected_revision: u64,
    next_revision: u64,
    ready_at: SimulationTick,
}

pub(crate) fn decide_mining_tick(
    state: &AppState,
    next_tick: SimulationTick,
) -> Result<Option<MiningTickPlan>, MiningTickError> {
    if !state.mining().has_jobs_due_at(next_tick) {
        return Ok(None);
    }
    let expected_revision = state.mining().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(MiningTickError::RevisionExhausted)?;
    Ok(Some(MiningTickPlan {
        expected_revision,
        next_revision,
        ready_at: next_tick,
    }))
}

pub(crate) fn apply_mining_tick(
    state: &mut AppState,
    plan: Option<MiningTickPlan>,
) -> Vec<MiningJobId> {
    let Some(plan) = plan else {
        return Vec::new();
    };
    state.mining_state_mut().mark_due_jobs_ready(
        plan.expected_revision,
        plan.next_revision,
        plan.ready_at,
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MiningClaimError {
    UnknownJob { job: MiningJobId },
    NotReady { job: MiningJobId },
    LotIdExhausted,
    InventoryRevisionExhausted,
    MiningRevisionExhausted,
    DestinationMassOverflow { stockpile: StockpileId },
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for MiningClaimError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "mining claim failed: {self:?}")
    }
}
impl Error for MiningClaimError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MiningClaimCommitError {
    StaleInventory { expected: u64, actual: u64 },
    StaleMining { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}
impl Display for MiningClaimCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "mining claim commit failed: {self:?}")
    }
}
impl Error for MiningClaimCommitError {}

#[must_use]
pub struct ValidatedMiningClaim {
    job: MiningJobId,
    expected_mining_revision: u64,
    next_mining_revision: u64,
    inventory: ReservedDepositPlan,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedMiningClaim {
    pub fn commit(self, state: &mut AppState) -> Result<(), MiningClaimCommitError> {
        if state.inventory().revision() != self.inventory.expected_revision() {
            return Err(MiningClaimCommitError::StaleInventory {
                expected: self.inventory.expected_revision(),
                actual: state.inventory().revision(),
            });
        }
        if state.mining().revision() != self.expected_mining_revision {
            return Err(MiningClaimCommitError::StaleMining {
                expected: self.expected_mining_revision,
                actual: state.mining().revision(),
            });
        }
        if let Some(load) = self.structural_load {
            load.commit(state)
                .map_err(MiningClaimCommitError::Structure)?;
        }
        apply_reserved_deposits(state.inventory_state_mut(), self.inventory);
        state.mining_state_mut().remove_ready_job(
            self.job,
            self.expected_mining_revision,
            self.next_mining_revision,
        );
        Ok(())
    }
}

pub fn validate_claim_mining_output(
    registries: &Registries,
    state: &AppState,
    job: MiningJobId,
) -> Result<ValidatedMiningClaim, MiningClaimError> {
    let record = state
        .mining()
        .get_job(job)
        .ok_or(MiningClaimError::UnknownJob { job })?;
    let ready_at = record
        .ready_at()
        .ok_or(MiningClaimError::NotReady { job })?;
    let mass = record.output().mass();
    let inventory = decide_reserved_deposits(
        state.inventory(),
        ready_at,
        vec![ReservedDepositRequest::new(
            record.destination(),
            vec![record.output().clone()],
            mass,
        )],
    )
    .map_err(|error| match error {
        ReservedDepositPlanError::LotIdExhausted => MiningClaimError::LotIdExhausted,
        ReservedDepositPlanError::RevisionExhausted => MiningClaimError::InventoryRevisionExhausted,
    })?;
    let destination = state
        .inventory()
        .get_stockpile(record.destination())
        .ok_or(MiningClaimError::DestinationMassOverflow {
            stockpile: record.destination(),
        })?;
    let stored_after = destination.stored_mass().checked_add(mass).ok_or(
        MiningClaimError::DestinationMassOverflow {
            stockpile: record.destination(),
        },
    )?;
    let structural_load = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new_committed_inbound(
            record.destination(),
            stored_after,
        )],
    )
    .map_err(MiningClaimError::StructuralLoad)?;
    let expected_mining_revision = state.mining().revision();
    let next_mining_revision = expected_mining_revision
        .checked_add(1)
        .ok_or(MiningClaimError::MiningRevisionExhausted)?;
    Ok(ValidatedMiningClaim {
        job,
        expected_mining_revision,
        next_mining_revision,
        inventory,
        structural_load,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{
        EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_JAW_CRUSHER, EQUIPMENT_STONE_PICK, FORM_HANDLE,
        FORM_INGOT, FORM_LOG, FORM_LUMP, FORM_ORE, FORM_TOOL, MATERIAL_COPPER, MATERIAL_STONE,
        MATERIAL_WOOD, MINING_METHOD_HAND_PICK, PROCESS_KNAP_STONE_TOOL, PROCESS_SHAPE_WOOD_HANDLE,
        build_registries,
    };
    use crate::core::quantity::Temperature;
    use crate::core::state::validate_loaded_state;
    use crate::core::time::WorldSeed;
    use crate::crafting::{
        ManualCraftStartRequest, StartManualCraftError, validate_start_manual_craft,
    };
    use crate::energy::calculate_explicit_energy_accounting;
    use crate::equipment::{add_equipment, validate_assemble_equipment};
    use crate::geology::{GeneratedDepositSpec, insert_generated_deposit};
    use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
    use crate::labor::{PlayerWork, PlayerWorkStartError};
    use crate::maintenance::Condition;
    use crate::material::{CommodityKey, MaterialComposition};
    use crate::matter::calculate_matter_accounting;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    use crate::simulation::advance_tick;
    use crate::spatial::{VoxelBounds, VoxelCoord};
    use crate::survival::{assess_survival, initialize_player_survival};

    fn deposit_spec() -> GeneratedDepositSpec {
        let bounds = VoxelBounds::new(VoxelCoord::new(0, -8, 0), VoxelCoord::new(4, -4, 4))
            .unwrap_or_else(|error| panic!("mining test bounds failed: {error}"));
        GeneratedDepositSpec::new(
            bounds,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Mass::from_milligrams(1_000),
            Temperature::from_millikelvin(300_000),
            MaterialComposition::pure(MATERIAL_COPPER),
        )
        .unwrap_or_else(|error| panic!("mining test deposit failed: {error}"))
    }

    fn assemble_pick_for_test(registries: &Registries, state: &mut AppState) -> EquipmentId {
        let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_000))
            .unwrap_or_else(|error| panic!("pick assembly source failed: {error}"));
        for (commodity, mass) in [
            (
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(800),
            ),
            (
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200),
            ),
        ] {
            deposit_lot_for_test(
                registries,
                state,
                source,
                commodity,
                mass,
                Temperature::from_millikelvin(293_150),
            )
            .unwrap_or_else(|error| panic!("pick assembly material failed: {error}"));
        }
        validate_assemble_equipment(registries, state, EQUIPMENT_STONE_PICK, source)
            .unwrap_or_else(|error| panic!("pick assembly validation failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("pick assembly commit failed: {error}"))
    }

    fn assemble_reinforced_pick_for_test(
        registries: &Registries,
        state: &mut AppState,
    ) -> EquipmentId {
        let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_020))
            .unwrap_or_else(|error| panic!("reinforced pick assembly source failed: {error}"));
        for (commodity, mass) in [
            (
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(800),
            ),
            (
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200),
            ),
            (
                CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
                Mass::from_milligrams(20),
            ),
        ] {
            deposit_lot_for_test(
                registries,
                state,
                source,
                commodity,
                mass,
                Temperature::from_millikelvin(293_150),
            )
            .unwrap_or_else(|error| panic!("reinforced pick assembly material failed: {error}"));
        }
        validate_assemble_equipment(registries, state, EQUIPMENT_COPPER_REINFORCED_PICK, source)
            .unwrap_or_else(|error| panic!("reinforced pick assembly validation failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("reinforced pick assembly commit failed: {error}"))
    }

    #[test]
    fn stone_pick_refuses_material_above_authored_hardness() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0xA11E_0002));
        initialize_player_survival(&registries, &mut state)
            .unwrap_or_else(|error| panic!("hardness survival initialization failed: {error}"));
        let pick = assemble_pick_for_test(&registries, &mut state);
        let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
            .unwrap_or_else(|error| panic!("hardness destination failed: {error}"));
        let bounds = VoxelBounds::new(VoxelCoord::new(8, -8, 0), VoxelCoord::new(9, -7, 1))
            .unwrap_or_else(|error| panic!("hardness bounds failed: {error}"));
        let deposit = insert_generated_deposit(
            &registries,
            &mut state,
            GeneratedDepositSpec::new(
                bounds,
                CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
                Mass::from_milligrams(100),
                Temperature::from_millikelvin(300_000),
                MaterialComposition::pure(MATERIAL_STONE),
            )
            .unwrap_or_else(|error| panic!("hardness deposit fixture failed: {error}")),
        )
        .unwrap_or_else(|error| panic!("hardness deposit insertion failed: {error}"));

        let error = validate_start_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            pick,
            Mass::from_milligrams(100),
        )
        .err()
        .unwrap_or_else(|| panic!("stone pick unexpectedly mined material above its hardness"));
        assert_eq!(
            error,
            MiningStartError::MaterialTooHard {
                hardness: Pressure::from_pascals(50_000_000_000),
                maximum: Pressure::from_pascals(500_000_000),
            }
        );
        assert_eq!(state.player_work().active(), None);
        assert_eq!(
            state
                .geology()
                .get_deposit(deposit)
                .unwrap_or_else(|| panic!("hardness deposit disappeared"))
                .remaining_mass(),
            Mass::from_milligrams(100)
        );
    }

    #[test]
    fn copper_reinforcement_turns_processed_metal_into_more_capable_extraction() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0xA11E_0004));
        initialize_player_survival(&registries, &mut state)
            .unwrap_or_else(|error| panic!("reinforced mining survival setup failed: {error}"));
        let stone_pick = assemble_pick_for_test(&registries, &mut state);
        let reinforced_pick = assemble_reinforced_pick_for_test(&registries, &mut state);
        let reinforced_record = state
            .equipment()
            .get_equipment(reinforced_pick)
            .unwrap_or_else(|| panic!("reinforced pick disappeared after assembly"));
        assert_eq!(
            reinforced_record.embodied_mass(),
            Mass::from_milligrams(1_020)
        );
        assert!(reinforced_record.embodied_material().iter().any(|trace| {
            trace.profile().commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_INGOT)
                && trace.mass() == Mass::from_milligrams(20)
        }));

        let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(300))
            .unwrap_or_else(|error| panic!("reinforced mining destination failed: {error}"));
        let deposit = insert_generated_deposit(&registries, &mut state, deposit_spec())
            .unwrap_or_else(|error| panic!("reinforced mining deposit failed: {error}"));
        let requested = Mass::from_milligrams(250);

        assert_eq!(
            validate_start_mining(
                &registries,
                &state,
                MINING_METHOD_HAND_PICK,
                deposit,
                destination,
                stone_pick,
                requested,
            )
            .err(),
            Some(MiningStartError::BatchTooLarge {
                maximum: Mass::from_milligrams(200),
                requested,
            })
        );

        let job = validate_start_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            reinforced_pick,
            requested,
        )
        .unwrap_or_else(|error| panic!("reinforced pick mining validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("reinforced pick mining commit failed: {error}"));
        let job_record = state
            .mining()
            .get_job(job)
            .unwrap_or_else(|| panic!("reinforced mining job disappeared"));
        assert_eq!(
            job_record.completes_at().value() - job_record.started_at().value(),
            167
        );
        assert_eq!(
            state
                .geology()
                .get_deposit(deposit)
                .unwrap_or_else(|| panic!("reinforced mining deposit disappeared"))
                .remaining_mass(),
            Mass::from_milligrams(750)
        );
        validate_loaded_state(&registries, &state)
            .unwrap_or_else(|error| panic!("reinforced mining state audit failed: {error}"));
    }

    #[test]
    fn missing_mining_capability_reports_the_exact_authored_requirement() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0xA11E_0003));
        initialize_player_survival(&registries, &mut state)
            .unwrap_or_else(|error| panic!("missing-capability survival setup failed: {error}"));
        let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
            .unwrap_or_else(|error| panic!("missing-capability destination failed: {error}"));
        let deposit = insert_generated_deposit(&registries, &mut state, deposit_spec())
            .unwrap_or_else(|error| panic!("missing-capability deposit failed: {error}"));
        let crusher = add_equipment(
            &registries,
            &mut state,
            EQUIPMENT_JAW_CRUSHER,
            Condition::PRISTINE,
        )
        .unwrap_or_else(|error| panic!("missing-capability equipment failed: {error}"));
        let expected_capability = registries
            .mining()
            .get_method(MINING_METHOD_HAND_PICK)
            .unwrap_or_else(|| panic!("hand-pick mining method disappeared"))
            .mass_flow_capability();

        let error = validate_start_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            crusher,
            Mass::from_milligrams(1),
        )
        .err()
        .unwrap_or_else(|| panic!("crusher unexpectedly satisfied hand-mining capabilities"));

        assert_eq!(
            error,
            MiningStartError::MissingCapability {
                capability: expected_capability,
            }
        );
        assert_eq!(state.player_work().active(), None);
        assert_eq!(
            state
                .geology()
                .get_deposit(deposit)
                .unwrap_or_else(|| panic!("missing-capability deposit disappeared"))
                .remaining_mass(),
            Mass::from_milligrams(1_000)
        );
    }

    #[test]
    fn knap_assemble_mine_claim_loop_is_conserved_exclusive_and_persistent() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0xA11E_0001));
        initialize_player_survival(&registries, &mut state)
            .unwrap_or_else(|error| panic!("mining survival initialization failed: {error}"));

        let stone_source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(3_000))
            .unwrap_or_else(|error| panic!("mining primitive-material source failed: {error}"));
        let shaped = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000))
            .unwrap_or_else(|error| panic!("mining shaped stockpile failed: {error}"));
        let ore_destination =
            add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000))
                .unwrap_or_else(|error| panic!("mining ore destination failed: {error}"));
        deposit_lot_for_test(
            &registries,
            &mut state,
            stone_source,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(2_000),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("mining stone ingress failed: {error}"));
        deposit_lot_for_test(
            &registries,
            &mut state,
            stone_source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(1_000),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("mining handle wood ingress failed: {error}"));

        validate_start_manual_craft(
            &registries,
            &state,
            ManualCraftStartRequest::single(PROCESS_KNAP_STONE_TOOL, stone_source, shaped),
        )
        .unwrap_or_else(|error| panic!("mining knapping start failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("mining knapping commit failed: {error}"));
        for _ in 0..40 {
            advance_tick(&registries, &mut state)
                .unwrap_or_else(|error| panic!("mining knapping tick failed: {error}"));
        }
        validate_start_manual_craft(
            &registries,
            &state,
            ManualCraftStartRequest::single(PROCESS_SHAPE_WOOD_HANDLE, stone_source, shaped),
        )
        .unwrap_or_else(|error| panic!("mining handle shaping start failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("mining handle shaping commit failed: {error}"));
        for _ in 0..40 {
            advance_tick(&registries, &mut state)
                .unwrap_or_else(|error| panic!("mining handle shaping tick failed: {error}"));
        }

        let energy_before_assembly = calculate_explicit_energy_accounting(&registries, &state)
            .unwrap_or_else(|error| panic!("pre-assembly energy accounting failed: {error}"))
            .total()
            .unwrap_or_else(|| panic!("pre-assembly energy total overflowed"));
        let pick = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, shaped)
            .unwrap_or_else(|error| panic!("stone pick assembly validation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("stone pick assembly commit failed: {error}"));
        let energy_after_assembly = calculate_explicit_energy_accounting(&registries, &state)
            .unwrap_or_else(|error| panic!("post-assembly energy accounting failed: {error}"))
            .total()
            .unwrap_or_else(|| panic!("post-assembly energy total overflowed"));
        assert_eq!(energy_after_assembly, energy_before_assembly);
        let pick_record = state
            .equipment()
            .get_equipment(pick)
            .unwrap_or_else(|| panic!("assembled stone pick disappeared"));
        assert_eq!(pick_record.embodied_mass(), Mass::from_milligrams(1_000));
        assert_eq!(pick_record.embodied_material().len(), 2);
        assert!(pick_record.embodied_material().iter().any(|trace| {
            trace.profile().commodity() == CommodityKey::new(MATERIAL_STONE, FORM_TOOL)
                && trace.mass() == Mass::from_milligrams(800)
        }));
        assert!(pick_record.embodied_material().iter().any(|trace| {
            trace.profile().commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE)
                && trace.mass() == Mass::from_milligrams(200)
        }));

        let deposit = insert_generated_deposit(&registries, &mut state, deposit_spec())
            .unwrap_or_else(|error| panic!("mining copper deposit insertion failed: {error}"));
        let matter_before = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("mining initial matter accounting failed: {error}"))
            .total();
        let energy_before_mining = calculate_explicit_energy_accounting(&registries, &state)
            .unwrap_or_else(|error| panic!("mining initial energy accounting failed: {error}"))
            .total()
            .unwrap_or_else(|| panic!("mining initial energy total overflowed"));
        let survival_before_mining = assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("mining survival state disappeared before work"));

        let mining = validate_start_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            deposit,
            ore_destination,
            pick,
            Mass::from_milligrams(100),
        )
        .unwrap_or_else(|error| panic!("mining start validation failed: {error}"));
        let job = mining
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("mining start commit failed: {error}"));
        assert_eq!(
            state
                .geology()
                .get_deposit(deposit)
                .unwrap_or_else(|| panic!("mining deposit disappeared"))
                .remaining_mass(),
            Mass::from_milligrams(900)
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(ore_destination)
                .unwrap_or_else(|| panic!("mining destination disappeared"))
                .reserved_inbound(),
            Mass::from_milligrams(100)
        );
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!("mining WIP accounting failed: {error}"))
                .total(),
            matter_before
        );
        assert_eq!(
            calculate_explicit_energy_accounting(&registries, &state)
                .unwrap_or_else(|error| panic!("mining WIP energy accounting failed: {error}"))
                .total(),
            Some(energy_before_mining)
        );
        assert_eq!(
            state.player_work().active(),
            Some(PlayerWork::Mining { job })
        );

        let craft_error = validate_start_manual_craft(
            &registries,
            &state,
            ManualCraftStartRequest::single(PROCESS_KNAP_STONE_TOOL, stone_source, shaped),
        )
        .err()
        .unwrap_or_else(|| panic!("manual crafting unexpectedly started during mining"));
        assert_eq!(
            craft_error,
            StartManualCraftError::Work(PlayerWorkStartError::Busy {
                active: PlayerWork::Mining { job },
            })
        );

        let mut final_tick = None;
        for _ in 0..100 {
            final_tick = Some(
                advance_tick(&registries, &mut state)
                    .unwrap_or_else(|error| panic!("mining work tick failed: {error}")),
            );
        }
        assert_eq!(
            final_tick
                .as_ref()
                .unwrap_or_else(|| panic!("mining work produced no tick outcome"))
                .ready_mining_jobs(),
            &[job]
        );
        assert_eq!(state.player_work().active(), None);
        let survival_after_mining = assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("mining survival state disappeared after work"));
        let physiology = registries.survival().physiology();
        let exertion = registries
            .mining()
            .get_method(MINING_METHOD_HAND_PICK)
            .unwrap_or_else(|| panic!("hand mining method disappeared"))
            .exertion();
        assert_eq!(
            survival_before_mining.metabolic_energy().nanojoules()
                - survival_after_mining.metabolic_energy().nanojoules(),
            (physiology.basal_energy_cost_per_tick().nanojoules()
                + exertion.energy_cost_per_tick().nanojoules())
                * 100
        );
        assert_eq!(
            survival_before_mining.hydration().microliters()
                - survival_after_mining.hydration().microliters(),
            (physiology.hydration_loss_per_tick().microliters()
                + exertion.hydration_loss_per_tick().microliters())
                * 100
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(ore_destination)
                .unwrap_or_else(|| panic!("mining destination disappeared before claim"))
                .stored_mass(),
            Mass::ZERO
        );

        validate_claim_mining_output(&registries, &state, job)
            .unwrap_or_else(|error| panic!("mining claim validation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("mining claim commit failed: {error}"));
        let destination = state
            .inventory()
            .get_stockpile(ore_destination)
            .unwrap_or_else(|| panic!("mining destination disappeared after claim"));
        assert_eq!(
            destination.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_ORE)),
            Mass::from_milligrams(100)
        );
        assert_eq!(destination.reserved_inbound(), Mass::ZERO);
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!("mining final matter accounting failed: {error}"))
                .total(),
            matter_before
        );
        assert_eq!(
            calculate_explicit_energy_accounting(&registries, &state)
                .unwrap_or_else(|error| panic!("mining final energy accounting failed: {error}"))
                .total(),
            Some(energy_before_mining)
        );
        validate_loaded_state(&registries, &state)
            .unwrap_or_else(|error| panic!("mining final state audit failed: {error}"));

        let encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
            .unwrap_or_else(|error| panic!("mining save serialization failed: {error}"));
        let loaded: LoadedSaveEnvelope = serde_json::from_value(encoded)
            .unwrap_or_else(|error| panic!("mining save decode failed: {error}"));
        let restored = loaded
            .into_state(&registries)
            .unwrap_or_else(|error| panic!("mining save validation failed: {error}"));
        assert_eq!(restored, state);
    }

    fn run_mining_soak(seed: WorldSeed) -> AppState {
        let registries = build_registries();
        let mut state = AppState::new(seed);
        initialize_player_survival(&registries, &mut state)
            .unwrap_or_else(|error| panic!("mining soak survival initialization failed: {error}"));
        let pick = assemble_pick_for_test(&registries, &mut state);
        let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000))
            .unwrap_or_else(|error| panic!("mining soak destination failed: {error}"));
        let deposit = insert_generated_deposit(&registries, &mut state, deposit_spec())
            .unwrap_or_else(|error| panic!("mining soak deposit failed: {error}"));
        let initial_matter = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("mining soak matter accounting failed: {error}"))
            .total();
        let initial_energy = calculate_explicit_energy_accounting(&registries, &state)
            .unwrap_or_else(|error| panic!("mining soak energy accounting failed: {error}"))
            .total()
            .unwrap_or_else(|| panic!("mining soak energy total overflowed"));

        for step in 0_u64..1_000 {
            let job = validate_start_mining(
                &registries,
                &state,
                MINING_METHOD_HAND_PICK,
                deposit,
                destination,
                pick,
                Mass::from_milligrams(1),
            )
            .unwrap_or_else(|error| panic!("mining soak start failed at step {step}: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("mining soak start commit failed at step {step}: {error}")
            });

            if step == 500 {
                let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
                    .unwrap_or_else(|error| panic!("mining soak save failed: {error}"));
                let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
                    .unwrap_or_else(|error| panic!("mining soak decode failed: {error}"));
                state = decoded
                    .into_state(&registries)
                    .unwrap_or_else(|error| panic!("mining soak active-job load failed: {error}"));
            }

            let job_record = state
                .mining()
                .get_job(job)
                .unwrap_or_else(|| panic!("mining soak job disappeared at step {step}"));
            let duration = job_record
                .completes_at()
                .value()
                .checked_sub(job_record.started_at().value())
                .unwrap_or_else(|| panic!("mining soak duration underflowed at step {step}"));
            assert!(duration > 0);
            for _ in 0..duration {
                advance_tick(&registries, &mut state).unwrap_or_else(|error| {
                    panic!("mining soak tick failed at step {step}: {error}")
                });
            }
            validate_claim_mining_output(&registries, &state, job)
                .unwrap_or_else(|error| panic!("mining soak claim failed at step {step}: {error}"))
                .commit(&mut state)
                .unwrap_or_else(|error| {
                    panic!("mining soak claim commit failed at step {step}: {error}")
                });

            if step.is_multiple_of(97) {
                validate_loaded_state(&registries, &state).unwrap_or_else(|error| {
                    panic!("mining soak exhaustive audit failed at step {step}: {error}")
                });
                assert_eq!(
                    calculate_matter_accounting(&state)
                        .unwrap_or_else(|error| panic!("mining soak matter audit failed: {error}"))
                        .total(),
                    initial_matter
                );
                assert_eq!(
                    calculate_explicit_energy_accounting(&registries, &state)
                        .unwrap_or_else(|error| panic!("mining soak energy audit failed: {error}"))
                        .total(),
                    Some(initial_energy)
                );
            }
        }

        assert_eq!(
            state
                .geology()
                .get_deposit(deposit)
                .unwrap_or_else(|| panic!("mining soak deposit disappeared"))
                .lifecycle(),
            GeologicalDepositLifecycle::Depleted
        );
        let destination = state
            .inventory()
            .get_stockpile(destination)
            .unwrap_or_else(|| panic!("mining soak destination disappeared"));
        assert_eq!(destination.stored_mass(), Mass::from_milligrams(1_000));
        assert_eq!(destination.lot_ids().count(), 1);
        assert_eq!(state.mining().jobs().count(), 0);
        assert_eq!(state.player_work().active(), None);
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!("mining soak final matter audit failed: {error}"))
                .total(),
            initial_matter
        );
        assert_eq!(
            calculate_explicit_energy_accounting(&registries, &state)
                .unwrap_or_else(|error| panic!("mining soak final energy audit failed: {error}"))
                .total(),
            Some(initial_energy)
        );
        state
    }

    #[test]
    #[ignore = "long-horizon soak"]
    fn mining_soak_preserves_depletion_conservation_persistence_and_replay() {
        let seed = WorldSeed::new(0xA11E_5000);
        let first = run_mining_soak(seed);
        let second = run_mining_soak(seed);

        assert_eq!(first, second);
    }
}
