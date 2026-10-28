//! Nested error-source routing for trusted player-work validation failures.

use std::error::Error;

use super::PlayerWorkValidationError;

impl Error for PlayerWorkValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ManualPowerConditionDuration(error) => Some(error),
            Self::ManualPowerEquipmentAccess(error) => Some(error),
            Self::ManualPowerDestinationAccess(error) => Some(error),
            Self::ProspectingEquipmentConditionDuration(error) => Some(error),
            Self::ProspectingEquipmentAccess(error) => Some(error),
            Self::EquipmentMaintenanceAccess(error) => Some(error),
            Self::ManualProductionAccess(error) => Some(error),
            Self::StorageDismantlingAccess(error) => Some(error),
            Self::StorageDismantlingRecoveryStorage(error) => Some(error),
            Self::SurfaceGatheringDestinationAccess(error) => Some(error),
            Self::SurfaceGatheringDestinationStorage(error) => Some(error),
            Self::BlockExcavationEquipmentAccess(error) => Some(error),
            Self::BlockExcavationDestinationAccess(error) => Some(error),
            Self::BlockExcavationDestinationStorage(error) => Some(error),
            Self::WorkWithoutPlayer
            | Self::TreeFellingTargetMissing
            | Self::TreeFellingTargetRemote
            | Self::TreeFellingEquipmentInvalid
            | Self::TreeFellingScheduleInvalid
            | Self::TreeFellingDurationMismatch
            | Self::PlayerAirborne
            | Self::PlayerMoving
            | Self::RevisionExhausted
            | Self::SurvivalRevisionExhausted
            | Self::SurfaceGatheringMethodMissing
            | Self::SurfaceGatheringResourceMissing
            | Self::SurfaceGatheringResourceDepleted
            | Self::SurfaceGatheringSourceMassMismatch
            | Self::SurfaceGatheringMassInvalid
            | Self::SurfaceGatheringPlayerNotLocated
            | Self::SurfaceGatheringPlayerRemote
            | Self::SurfaceGatheringCommodityMismatch { .. }
            | Self::SurfaceGatheringDestinationMissing
            | Self::SurfaceGatheringDestinationMounted
            | Self::SurfaceGatheringReservationMissing
            | Self::SurfaceGatheringScheduleInvalid
            | Self::SurfaceGatheringDurationMismatch
            | Self::BlockExcavationMethodMissing
            | Self::BlockExcavationTargetMissing
            | Self::BlockExcavationTargetStateMismatch
            | Self::BlockExcavationDefinitionMissing
            | Self::BlockExcavationSourceMassMismatch
            | Self::BlockExcavationMassInvalid
            | Self::BlockExcavationEquipmentMissing
            | Self::BlockExcavationEquipmentDefinitionMismatch
            | Self::BlockExcavationEquipmentConditionMismatch
            | Self::BlockExcavationEquipmentMounted
            | Self::BlockExcavationEquipmentResourceDoubleBooked
            | Self::BlockExcavationPlayerNotLocated
            | Self::BlockExcavationDestinationMissing
            | Self::BlockExcavationDestinationNotCarried
            | Self::BlockExcavationDestinationMounted
            | Self::BlockExcavationReservationMissing
            | Self::BlockExcavationSurfacePositionMismatch { .. }
            | Self::BlockExcavationPhysicsInvalid
            | Self::BlockExcavationScheduleInvalid
            | Self::BlockExcavationDurationMismatch
            | Self::BlockExcavationConditionOutcomeMismatch
            | Self::ManualProductionJobMissing
            | Self::ManualProductionProcessMismatch
            | Self::ManualProductionScheduleInvalid
            | Self::MiningJobMissing
            | Self::MiningJobNotWorking
            | Self::MiningMethodMissing
            | Self::MiningScheduleInvalid
            | Self::ManualProductionMissingWork
            | Self::MultiplePlayerJobs
            | Self::MiningMissingWork
            | Self::ManualPowerMethodMissing
            | Self::ManualPowerEquipmentMissing
            | Self::ManualPowerEquipmentDefinitionMismatch
            | Self::ManualPowerEquipmentConditionMismatch
            | Self::ManualPowerEquipmentRequiresStructuralSupport
            | Self::ManualPowerEquipmentMounted
            | Self::ManualPowerDestinationMissing
            | Self::ManualPowerDestinationDefinitionMismatch
            | Self::ManualPowerCarrierMismatch
            | Self::ManualPowerDestinationCannotAcceptEnergy
            | Self::ManualPowerDestinationCapacityExceeded
            | Self::ManualPowerEquipmentCapabilityMissing
            | Self::ManualPowerEquipmentCapabilityKindMismatch
            | Self::ManualPowerZeroPower
            | Self::ManualPowerScheduleInvalid
            | Self::ManualPowerDurationMismatch
            | Self::ManualPowerExertionMismatch
            | Self::ManualPowerConditionMismatch
            | Self::ManualPowerResourceDoubleBooked
            | Self::ManualPowerEquipmentRevisionExhausted
            | Self::ManualPowerEnergyRevisionExhausted
            | Self::ProspectingMethodMissing
            | Self::ProspectingUnknownMaterial { .. }
            | Self::ProspectingRegionVolumeOverflow
            | Self::ProspectingRegionTooLarge { .. }
            | Self::ProspectingPlayerOutsideRegion { .. }
            | Self::ProspectingEquipmentMissing
            | Self::ProspectingUnexpectedEquipment { .. }
            | Self::ProspectingEquipmentDefinitionMismatch
            | Self::ProspectingEquipmentDefinitionNotAccepted { .. }
            | Self::ProspectingEquipmentConditionMismatch
            | Self::ProspectingEquipmentMounted { .. }
            | Self::ProspectingEquipmentConditionOutcomeMismatch { .. }
            | Self::ProspectingEquipmentResourceDoubleBooked { .. }
            | Self::ProspectingObservationIdExhausted
            | Self::ProspectingKnowledgeRevisionExhausted
            | Self::ProspectingEquipmentRevisionExhausted
            | Self::ProspectingScheduleInvalid
            | Self::ProspectingDurationMismatch
            | Self::EatingMassInvalid { .. }
            | Self::EatingScheduleInvalid
            | Self::EatingDurationMismatch
            | Self::DrinkingVolumeInvalid { .. }
            | Self::DrinkingScheduleInvalid
            | Self::DrinkingDurationMismatch
            | Self::EquipmentMaintenanceEquipmentMissing
            | Self::EquipmentMaintenanceDefinitionMismatch
            | Self::EquipmentMaintenanceConditionMismatch
            | Self::EquipmentMaintenanceProfileMissing
            | Self::EquipmentMaintenanceAdmissionMissing
            | Self::EquipmentMaintenanceAdmissionMismatch
            | Self::EquipmentMaintenanceTargetMismatch
            | Self::EquipmentMaintenanceScheduleInvalid
            | Self::EquipmentMaintenanceDurationMismatch
            | Self::EquipmentMaintenanceResourceDoubleBooked
            | Self::EquipmentMaintenanceEquipmentRevisionExhausted
            | Self::StorageDismantlingTargetMissing
            | Self::StorageDismantlingEnclosureMissing
            | Self::StorageDismantlingDefinitionMissing
            | Self::StorageDismantlingDefinitionMismatch
            | Self::StorageDismantlingEnclosureIdentityMismatch
            | Self::StorageDismantlingRecoveredMassMismatch
            | Self::StorageDismantlingTargetMounted
            | Self::StorageDismantlingTargetReservedInbound
            | Self::StorageDismantlingRecoveryMissing
            | Self::StorageDismantlingRecoveryIsTarget
            | Self::StorageDismantlingRecoveryMounted
            | Self::StorageDismantlingStorageProfileMismatch
            | Self::StorageDismantlingTargetContentsIncompatible { .. }
            | Self::StorageDismantlingRecoveryLotIdExhausted
            | Self::StorageDismantlingScheduleInvalid
            | Self::StorageDismantlingDurationMismatch
            | Self::StorageDismantlingResourceDoubleBooked
            | Self::StorageDismantlingInventoryRevisionExhausted
            | Self::StructuralConstructionMethodMissing
            | Self::StructuralConstructionElementMissing
            | Self::StructuralConstructionElementNotPlanned
            | Self::StructuralConstructionElementUnsupported
            | Self::StructuralConstructionElementNotMaterialized
            | Self::StructuralConstructionPlayerNotLocated
            | Self::StructuralConstructionPlayerRemote
            | Self::StructuralConstructionScheduleInvalid
            | Self::StructuralConstructionDurationMismatch
            | Self::StructuralConstructionRevisionExhausted
            | Self::StructuralDismantlingMethodMissing
            | Self::StructuralDismantlingElementMissing
            | Self::StructuralDismantlingElementNotRecoverable
            | Self::StructuralDismantlingElementNotLocal
            | Self::StructuralDismantlingElementHasDependents
            | Self::StructuralDismantlingElementHasExternalLoad
            | Self::StructuralDismantlingElementSupportsResource
            | Self::StructuralDismantlingRecoveryMismatch
            | Self::StructuralDismantlingRecoveryReservationMissing
            | Self::StructuralDismantlingScheduleInvalid
            | Self::StructuralDismantlingDurationMismatch
            | Self::StructuralDismantlingLotDemandMismatch
            | Self::StructuralDismantlingInventoryRevisionExhausted
            | Self::StructuralDismantlingStructureRevisionExhausted
            | Self::PendingDirectConsumptionWithoutWork
            | Self::EatingConsumptionMissing
            | Self::EatingConsumptionMismatch
            | Self::DrinkingConsumptionMissing
            | Self::DrinkingConsumptionMismatch
            | Self::PlayerDead
            | Self::MetabolicCostOverflow
            | Self::InsufficientMetabolicEnergy { .. }
            | Self::HydrationCostOverflow
            | Self::InsufficientHydration { .. } => None,
        }
    }
}
