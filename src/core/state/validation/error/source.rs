//! Nested error sources for root-state validation failures.

use std::error::Error;

use super::StateValidationError;

impl Error for StateValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Random(error) => Some(error),
            Self::Energy(error) => Some(error),
            Self::Fluid(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::Structure(error) => Some(error),
            Self::StructureAnalysis(error) => Some(error),
            Self::Geology(error) => Some(error),
            Self::GeologicalKnowledge(error) => Some(error),
            Self::Inventory(error) => Some(error),
            Self::StorageEnclosure(error) => Some(error),
            Self::Production(error) => Some(error),
            Self::Mining(error) => Some(error),
            Self::MiningJob(error) => Some(error),
            Self::PlayerWork(error) => Some(error),
            Self::Survival(error) => Some(error),
            Self::ComminutionJob(error) => Some(error),
            Self::ConstituentSeparationJob(error) => Some(error),
            Self::ScreeningJob(error) => Some(error),
            Self::ThermalJob(error) => Some(error),
            Self::ManualCraftJob(error) => Some(error),
            Self::JobOutputStorage { job: _job, error } => Some(error),
            Self::InvalidJobConsumedParticleSizeState { job: _job, error } => Some(error),
            Self::InvalidJobConsumedPhaseState { job: _job, error } => Some(error),
            Self::FluidStructuralLoad(error) => Some(error),
            Self::RandomWorldSeedMismatch {
                world_seed: _world_seed,
                random_seed: _random_seed,
            } => None,
            Self::UnresolvedStructuralDamage { event: _event } => None,
            Self::UnknownStoredCommodity {
                stockpile: _stockpile,
                commodity: _commodity,
            } => None,
            Self::LotCreatedInFuture {
                lot: _lot,
                created_at: _created_at,
                current: _current,
            } => None,
            Self::LotProvenanceInFuture {
                lot: _lot,
                latest_created_at: _latest_created_at,
                current: _current,
            } => None,
            Self::UnknownLotCompositionMaterial {
                lot: _lot,
                material: _material,
            } => None,
            Self::UnknownJobProcess {
                job: _job,
                process: _process,
            } => None,
            Self::MissingJobProcessTopology { .. }
            | Self::UnknownJobSource { .. }
            | Self::JobEnergyTopologyMismatch { .. }
            | Self::JobEquipmentTopologyMismatch { .. }
            | Self::UnknownJobDestination { .. } => None,
            Self::UnknownJobEnergySource {
                job: _job,
                store: _store,
            }
            | Self::UnknownJobEnergySink {
                job: _job,
                store: _store,
            }
            | Self::JobReleasedEnergySinkHasNoInputPower {
                job: _job,
                store: _store,
            }
            | Self::JobReleasedEnergyCapacityOverflow {
                job: _job,
                store: _store,
            } => None,
            Self::JobEnergyDefinitionMismatch {
                job: _job,
                traced: _traced,
                stored: _stored,
            }
            | Self::JobReleasedEnergyDefinitionMismatch {
                job: _job,
                traced: _traced,
                stored: _stored,
            } => None,
            Self::JobEnergyCarrierMismatch {
                job: _job,
                traced: _traced,
                authored: _authored,
            }
            | Self::JobReleasedEnergyCarrierMismatch {
                job: _job,
                traced: _traced,
                authored: _authored,
            } => None,
            Self::JobReleasedEnergyCapacityExceeded {
                job: _job,
                store: _store,
                stored: _stored,
                released: _released,
                capacity: _capacity,
            } => None,
            Self::UnknownJobEquipment {
                job: _job,
                equipment: _equipment,
            } => None,
            Self::JobEquipmentDefinitionMismatch {
                job: _job,
                traced: _traced,
                stored: _stored,
            } => None,
            Self::JobEquipmentConditionMismatch {
                job: _job,
                traced: _traced,
                stored: _stored,
            } => None,
            Self::JobEquipmentSupportRequirementMissing {
                job: _job,
                equipment: _equipment,
                definition: _definition,
            } => None,
            Self::JobEquipmentSupportStateMismatch {
                job: _job,
                equipment: _equipment,
                requires_active_support: _requires_active_support,
                supported_by: _supported_by,
            } => None,
            Self::UnknownEquipmentSupport {
                equipment: _equipment,
                element: _element,
            }
            | Self::EquipmentSupportedByPlannedElement {
                equipment: _equipment,
                element: _element,
            } => None,
            Self::MountedEquipmentMassOverflow { element: _element }
            | Self::MountedEquipmentWeightOverflow { element: _element }
            | Self::StoredMatterMassOverflow { element: _element }
            | Self::StoredMatterWeightOverflow { element: _element } => None,
            Self::EquipmentStructuralLoadMismatch {
                element: _element,
                stored: _stored,
                expected: _expected,
            }
            | Self::StoredMatterStructuralLoadMismatch {
                element: _element,
                stored: _stored,
                expected: _expected,
            } => None,
            Self::UnknownStockpileSupport {
                stockpile: _stockpile,
                element: _element,
            }
            | Self::StockpileSupportedByPlannedElement {
                stockpile: _stockpile,
                element: _element,
            } => None,
            Self::UnknownFluidSupport {
                store: _store,
                element: _element,
            }
            | Self::FluidSupportedByPlannedElement {
                store: _store,
                element: _element,
            } => None,
            Self::NonManualJobSuspendedForPlayerLabor {
                job: _job,
                process: _process,
            } => None,
            Self::ReservedMassOverflow {
                stockpile: _stockpile,
            } => None,
            Self::UnknownJobOutputCommodity {
                job: _job,
                commodity: _commodity,
            }
            | Self::UnknownJobConsumedCommodity {
                job: _job,
                commodity: _commodity,
            } => None,
            Self::JobOutputMassOverflow { job: _job } => None,
            Self::ProductionInventoryRevisionCapacityExhausted { .. }
            | Self::ProductionEquipmentRevisionCapacityExhausted { .. }
            | Self::ProductionEnergyRevisionCapacityExhausted { .. }
            | Self::ProductionStructureRevisionCapacityExhausted { .. }
            | Self::FutureMaterialLotIdCapacityExhausted { .. }
            | Self::FutureMaterialLotIdDemandOverflow
            | Self::FutureInventoryRevisionCapacityExhausted { .. }
            | Self::FutureInventoryRevisionDemandOverflow
            | Self::FutureEnergyRevisionCapacityExhausted { .. }
            | Self::FutureEnergyRevisionDemandOverflow
            | Self::FutureEquipmentRevisionCapacityExhausted { .. }
            | Self::FutureEquipmentRevisionDemandOverflow
            | Self::FutureMiningRevisionCapacityExhausted { .. }
            | Self::FutureMiningRevisionDemandOverflow
            | Self::FutureStructureRevisionCapacityExhausted { .. }
            | Self::FutureStructureRevisionDemandOverflow => None,
            Self::ReservedInboundMismatch {
                stockpile: _stockpile,
                reserved: _reserved,
                expected: _expected,
            } => None,
        }
    }
}
