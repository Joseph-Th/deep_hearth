//! Nested error-source chaining for process-start admission failures.

use std::error::Error;

use super::StartProcessError;

impl Error for StartProcessError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DestinationStorage(error) => Some(error),
            Self::StructuralLoad(error) => Some(error),
            Self::UnknownProcess { .. }
            | Self::ManualProcessRequiresPlayerWork { .. }
            | Self::ResolutionEquipmentTopologyMismatch { .. }
            | Self::ResolutionEnergyTopologyMismatch { .. }
            | Self::UnknownOutputMaterial { .. }
            | Self::UnknownOutputForm { .. }
            | Self::UnknownOutputCompositionMaterial { .. }
            | Self::UnknownStockpile { .. }
            | Self::OutputDestinationBusyStorageDismantling { .. }
            | Self::OutputRouteCountMismatch { .. }
            | Self::DuplicateOutputRoute { .. }
            | Self::UnknownOutputRoute { .. }
            | Self::MissingOutputRoute { .. }
            | Self::SpatialEndpointMismatch { .. }
            | Self::CapacityExceeded { .. }
            | Self::MassOverflow { .. }
            | Self::CompletionTickOverflow { .. }
            | Self::JobIdExhausted
            | Self::MaterialLotIdExhausted
            | Self::InventoryRevisionExhausted
            | Self::ProductionRevisionExhausted
            | Self::EnergyRevisionExhausted
            | Self::EquipmentRevisionExhausted
            | Self::StructureRevisionExhausted
            | Self::ResolutionSourceMismatch { .. }
            | Self::StaleResolvedInputs { .. }
            | Self::StaleResolvedEnergy { .. }
            | Self::StaleResolvedEquipment { .. }
            | Self::StaleResolvedStructure { .. }
            | Self::ResolvedEnergyStoreMissing
            | Self::ResolvedEnergyInsufficient
            | Self::ResolvedEnergySinkMissing
            | Self::ResolvedEnergySinkCapacity
            | Self::EnergyStoreBusy { .. }
            | Self::EnergyStoreBusyManualPower { .. }
            | Self::ResolvedEquipmentMissing { .. }
            | Self::ResolvedEquipmentDefinitionChanged { .. }
            | Self::ResolvedEquipmentConditionChanged { .. }
            | Self::ResolvedEquipmentSupportChanged { .. }
            | Self::ResolvedEquipmentSupportMissing { .. }
            | Self::ResolvedEquipmentSupportNotActive { .. }
            | Self::EquipmentBusy { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EquipmentBusyManualPower { .. }
            | Self::EquipmentBusyProspecting { .. } => None,
        }
    }
}
