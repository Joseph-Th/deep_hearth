//! Typed failures for trusted-load validation of exclusive player work.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass, Volume};
use crate::inventory::MaterialLotId;
use crate::maintenance::ActiveConditionDurationError;
use crate::material::MaterialId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerWorkValidationError {
    WorkWithoutPlayer,
    ManualProductionJobMissing,
    ManualProductionProcessMismatch,
    MiningJobMissing,
    MiningJobNotWorking,
    MiningMethodMissing,
    ManualProductionMissingWork,
    MultiplePlayerJobs,
    MiningMissingWork,
    ManualPowerMethodMissing,
    ManualPowerEquipmentMissing,
    ManualPowerEquipmentDefinitionMismatch,
    ManualPowerEquipmentConditionMismatch,
    ManualPowerEquipmentMounted,
    ManualPowerDestinationMissing,
    ManualPowerDestinationDefinitionMismatch,
    ManualPowerCarrierMismatch,
    ManualPowerDestinationCannotAcceptEnergy,
    ManualPowerDestinationCapacityExceeded,
    ManualPowerEquipmentCapabilityMissing,
    ManualPowerEquipmentCapabilityKindMismatch,
    ManualPowerZeroPower,
    ManualPowerScheduleInvalid,
    ManualPowerDurationMismatch,
    ManualPowerConditionDuration(ActiveConditionDurationError),
    ManualPowerConditionMismatch,
    ManualPowerResourceDoubleBooked,
    ProspectingMethodMissing,
    ProspectingUnknownMaterial { material: MaterialId },
    ProspectingRegionVolumeOverflow,
    ProspectingRegionTooLarge { actual: u128, maximum: u128 },
    ProspectingScheduleInvalid,
    ProspectingDurationMismatch,
    EatingMassInvalid { mass: Mass },
    EatingScheduleInvalid,
    EatingDurationMismatch,
    DrinkingVolumeInvalid { volume: Volume },
    DrinkingScheduleInvalid,
    DrinkingDurationMismatch,
    EquipmentMaintenanceEquipmentMissing,
    EquipmentMaintenanceDefinitionMismatch,
    EquipmentMaintenanceConditionMismatch,
    EquipmentMaintenanceProfileMissing,
    EquipmentMaintenanceTargetMismatch,
    EquipmentMaintenanceScheduleInvalid,
    EquipmentMaintenanceDurationMismatch,
    EquipmentMaintenanceResourceDoubleBooked,
    StorageDismantlingTargetMissing,
    StorageDismantlingEnclosureMissing,
    StorageDismantlingDefinitionMissing,
    StorageDismantlingDefinitionMismatch,
    StorageDismantlingEnclosureIdentityMismatch,
    StorageDismantlingRecoveredMassMismatch,
    StorageDismantlingTargetMounted,
    StorageDismantlingTargetReservedInbound,
    StorageDismantlingRecoveryMissing,
    StorageDismantlingRecoveryIsTarget,
    StorageDismantlingRecoveryMounted,
    StorageDismantlingStorageProfileMismatch,
    StorageDismantlingTargetContentsIncompatible { lot: MaterialLotId },
    StorageDismantlingStorageHistoryOverflow { lot: MaterialLotId },
    StorageDismantlingScheduleInvalid,
    StorageDismantlingDurationMismatch,
    StorageDismantlingResourceDoubleBooked,
    PendingDirectConsumptionWithoutWork,
    EatingConsumptionMissing,
    EatingConsumptionMismatch,
    DrinkingConsumptionMissing,
    DrinkingConsumptionMismatch,
    PlayerDead,
    MetabolicCostOverflow,
    InsufficientMetabolicEnergy { available: Energy, required: Energy },
    HydrationCostOverflow,
    InsufficientHydration { available: Volume, required: Volume },
}

impl Display for PlayerWorkValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WorkWithoutPlayer => formatter.write_str("player work exists without a player"),
            Self::ManualProductionJobMissing => {
                formatter.write_str("player work references missing manual production job")
            }
            Self::ManualProductionProcessMismatch => formatter.write_str(
                "player work references a production job that is not direct player labor",
            ),
            Self::MiningJobMissing => {
                formatter.write_str("player work references missing mining job")
            }
            Self::MiningJobNotWorking => {
                formatter.write_str("player work references mining that is no longer active")
            }
            Self::MiningMethodMissing => {
                formatter.write_str("player mining work references a missing authored method")
            }
            Self::ManualProductionMissingWork => {
                formatter.write_str("active manual production job does not own player labor")
            }
            Self::MultiplePlayerJobs => {
                formatter.write_str("more than one active job requires exclusive player labor")
            }
            Self::MiningMissingWork => {
                formatter.write_str("working mining job does not own player labor")
            }
            Self::ManualPowerMethodMissing => {
                formatter.write_str("manual power work references missing authored method")
            }
            Self::ManualPowerEquipmentMissing => {
                formatter.write_str("manual power work references missing equipment")
            }
            Self::ManualPowerEquipmentDefinitionMismatch => formatter
                .write_str("manual power equipment definition disagrees with its persisted trace"),
            Self::ManualPowerEquipmentConditionMismatch => formatter
                .write_str("manual power equipment condition disagrees with its persisted trace"),
            Self::ManualPowerEquipmentMounted => {
                formatter.write_str("manual power work requires portable unmounted equipment")
            }
            Self::ManualPowerDestinationMissing => {
                formatter.write_str("manual power work references missing energy destination")
            }
            Self::ManualPowerDestinationDefinitionMismatch => formatter.write_str(
                "manual power destination definition disagrees with its persisted trace",
            ),
            Self::ManualPowerCarrierMismatch => formatter
                .write_str("manual power destination carrier disagrees with its authored method"),
            Self::ManualPowerDestinationCannotAcceptEnergy => formatter
                .write_str("manual power destination has no authored input-power capability"),
            Self::ManualPowerDestinationCapacityExceeded => formatter.write_str(
                "manual power output exceeds destination capacity available by completion",
            ),
            Self::ManualPowerEquipmentCapabilityMissing => {
                formatter.write_str("manual power equipment lacks its authored power capability")
            }
            Self::ManualPowerEquipmentCapabilityKindMismatch => {
                formatter.write_str("manual power equipment capability is not a Power value")
            }
            Self::ManualPowerZeroPower => {
                formatter.write_str("manual power work persists zero usable transfer power")
            }
            Self::ManualPowerScheduleInvalid => {
                formatter.write_str("manual power work has an invalid persisted schedule")
            }
            Self::ManualPowerDurationMismatch => {
                formatter.write_str("manual power duration disagrees with current authored physics")
            }
            Self::ManualPowerConditionDuration(error) => write!(
                formatter,
                "manual power work exceeds equipment condition lifetime: {error}"
            ),
            Self::ManualPowerConditionMismatch => formatter
                .write_str("manual power condition outcome disagrees with current authored wear"),
            Self::ManualPowerResourceDoubleBooked => formatter.write_str(
                "manual power equipment or destination is simultaneously owned elsewhere",
            ),
            Self::ProspectingMethodMissing => {
                formatter.write_str("player prospecting work references a missing authored method")
            }
            Self::ProspectingUnknownMaterial { material } => write!(
                formatter,
                "player prospecting work references unknown material {}",
                material.value()
            ),
            Self::ProspectingRegionVolumeOverflow => {
                formatter.write_str("player prospecting region voxel count overflowed")
            }
            Self::ProspectingRegionTooLarge { actual, maximum } => write!(
                formatter,
                "player prospecting region contains {actual} voxels but method allows at most {maximum}"
            ),
            Self::ProspectingScheduleInvalid => {
                formatter.write_str("player prospecting work has an invalid persisted schedule")
            }
            Self::ProspectingDurationMismatch => formatter
                .write_str("player prospecting duration disagrees with its authored method"),
            Self::EatingMassInvalid { mass } => write!(
                formatter,
                "player eating work records invalid direct-consumption mass {} mg",
                mass.milligrams()
            ),
            Self::EatingScheduleInvalid => {
                formatter.write_str("player eating work has an invalid persisted schedule")
            }
            Self::EatingDurationMismatch => {
                formatter.write_str("player eating duration disagrees with authored intake timing")
            }
            Self::DrinkingVolumeInvalid { volume } => write!(
                formatter,
                "player drinking work records invalid direct-consumption volume {} uL",
                volume.microliters()
            ),
            Self::DrinkingScheduleInvalid => {
                formatter.write_str("player drinking work has an invalid persisted schedule")
            }
            Self::DrinkingDurationMismatch => formatter
                .write_str("player drinking duration disagrees with authored intake timing"),
            Self::EquipmentMaintenanceEquipmentMissing => {
                formatter.write_str("equipment maintenance work references missing equipment")
            }
            Self::EquipmentMaintenanceDefinitionMismatch => formatter.write_str(
                "equipment maintenance definition disagrees with its persisted equipment trace",
            ),
            Self::EquipmentMaintenanceConditionMismatch => formatter.write_str(
                "equipment maintenance condition disagrees with its persisted equipment trace",
            ),
            Self::EquipmentMaintenanceProfileMissing => formatter.write_str(
                "equipment maintenance work references equipment with no service profile",
            ),
            Self::EquipmentMaintenanceTargetMismatch => formatter.write_str(
                "equipment maintenance target condition disagrees with current authored service",
            ),
            Self::EquipmentMaintenanceScheduleInvalid => {
                formatter.write_str("equipment maintenance work has an invalid persisted schedule")
            }
            Self::EquipmentMaintenanceDurationMismatch => formatter.write_str(
                "equipment maintenance duration disagrees with current authored service timing",
            ),
            Self::EquipmentMaintenanceResourceDoubleBooked => formatter.write_str(
                "equipment under maintenance is simultaneously occupied by another operation",
            ),
            Self::StorageDismantlingTargetMissing => formatter
                .write_str("storage dismantling work references a missing target stockpile"),
            Self::StorageDismantlingEnclosureMissing => formatter
                .write_str("storage dismantling target no longer has its persisted enclosure"),
            Self::StorageDismantlingDefinitionMissing => formatter
                .write_str("storage dismantling work references a missing authored definition"),
            Self::StorageDismantlingDefinitionMismatch => formatter
                .write_str("storage dismantling definition disagrees with the target enclosure"),
            Self::StorageDismantlingEnclosureIdentityMismatch => formatter.write_str(
                "storage dismantling enclosure identity disagrees with the installed enclosure",
            ),
            Self::StorageDismantlingRecoveredMassMismatch => formatter.write_str(
                "storage dismantling recovered mass disagrees with the installed enclosure",
            ),
            Self::StorageDismantlingTargetMounted => formatter
                .write_str("storage dismantling target must remain unmounted while work is active"),
            Self::StorageDismantlingTargetReservedInbound => formatter.write_str(
                "storage dismantling target gained an inbound reservation while work is active",
            ),
            Self::StorageDismantlingRecoveryMissing => formatter
                .write_str("storage dismantling work references a missing recovery stockpile"),
            Self::StorageDismantlingRecoveryIsTarget => formatter.write_str(
                "storage dismantling target and recovery stockpile must remain distinct",
            ),
            Self::StorageDismantlingRecoveryMounted => formatter.write_str(
                "storage dismantling recovery stockpile must remain unmounted while work is active",
            ),
            Self::StorageDismantlingStorageProfileMismatch => formatter.write_str(
                "storage dismantling target profile disagrees with its authored enclosure",
            ),
            Self::StorageDismantlingTargetContentsIncompatible { lot } => write!(
                formatter,
                "storage dismantling target lot {} cannot remain in ambient storage at completion",
                lot.value()
            ),
            Self::StorageDismantlingStorageHistoryOverflow { lot } => write!(
                formatter,
                "storage dismantling target lot {} cannot checkpoint preservation history at completion",
                lot.value()
            ),
            Self::StorageDismantlingScheduleInvalid => {
                formatter.write_str("storage dismantling work has an invalid persisted schedule")
            }
            Self::StorageDismantlingDurationMismatch => formatter
                .write_str("storage dismantling duration disagrees with its authored definition"),
            Self::StorageDismantlingResourceDoubleBooked => formatter.write_str(
                "storage dismantling labor is simultaneously owned by another active job",
            ),
            Self::PendingDirectConsumptionWithoutWork => formatter.write_str(
                "pending direct consumption exists without matching eating or drinking work",
            ),
            Self::EatingConsumptionMissing => {
                formatter.write_str("player eating work has no pending consumed matter")
            }
            Self::EatingConsumptionMismatch => {
                formatter.write_str("pending consumed meal disagrees with persisted eating work")
            }
            Self::DrinkingConsumptionMissing => {
                formatter.write_str("player drinking work has no pending consumed fluid")
            }
            Self::DrinkingConsumptionMismatch => {
                formatter.write_str("pending consumed drink disagrees with persisted drinking work")
            }
            Self::PlayerDead => {
                formatter.write_str("player-owned work remains active for a dead player")
            }
            Self::MetabolicCostOverflow => formatter.write_str(
                "remaining player-work metabolic cost exceeds authoritative energy range",
            ),
            Self::InsufficientMetabolicEnergy {
                available,
                required,
            } => write!(
                formatter,
                "player work needs {} nJ to finish but player retains only {} nJ metabolic energy",
                required.nanojoules(),
                available.nanojoules()
            ),
            Self::HydrationCostOverflow => formatter.write_str(
                "remaining player-work hydration cost exceeds authoritative volume range",
            ),
            Self::InsufficientHydration {
                available,
                required,
            } => write!(
                formatter,
                "player work needs {} uL hydration to finish but player retains only {} uL",
                required.microliters(),
                available.microliters()
            ),
        }
    }
}

impl Error for PlayerWorkValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ManualPowerConditionDuration(error) => Some(error),
            Self::WorkWithoutPlayer
            | Self::ManualProductionJobMissing
            | Self::ManualProductionProcessMismatch
            | Self::MiningJobMissing
            | Self::MiningJobNotWorking
            | Self::MiningMethodMissing
            | Self::ManualProductionMissingWork
            | Self::MultiplePlayerJobs
            | Self::MiningMissingWork
            | Self::ManualPowerMethodMissing
            | Self::ManualPowerEquipmentMissing
            | Self::ManualPowerEquipmentDefinitionMismatch
            | Self::ManualPowerEquipmentConditionMismatch
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
            | Self::ManualPowerConditionMismatch
            | Self::ManualPowerResourceDoubleBooked
            | Self::ProspectingMethodMissing
            | Self::ProspectingUnknownMaterial { .. }
            | Self::ProspectingRegionVolumeOverflow
            | Self::ProspectingRegionTooLarge { .. }
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
            | Self::EquipmentMaintenanceTargetMismatch
            | Self::EquipmentMaintenanceScheduleInvalid
            | Self::EquipmentMaintenanceDurationMismatch
            | Self::EquipmentMaintenanceResourceDoubleBooked
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
            | Self::StorageDismantlingStorageHistoryOverflow { .. }
            | Self::StorageDismantlingScheduleInvalid
            | Self::StorageDismantlingDurationMismatch
            | Self::StorageDismantlingResourceDoubleBooked
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
