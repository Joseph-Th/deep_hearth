//! Human-readable diagnostics for persisted mining-job validation failures.

use std::fmt::{Display, Formatter};

use super::MiningJobValidationError;

impl Display for MiningJobValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMethod { job } => write!(
                formatter,
                "mining job {} references an unknown method",
                job.value()
            ),
            Self::WorkingPlayerOutsideDeposit {
                job,
                player_position,
                bounds,
            } => {
                let min = bounds.min();
                let max = bounds.max_exclusive();
                write!(
                    formatter,
                    "active mining job {} player at voxel ({},{},{}) is outside deposit bounds [({},{},{}),({},{},{}))",
                    job.value(),
                    player_position.x(),
                    player_position.y(),
                    player_position.z(),
                    min.x(),
                    min.y(),
                    min.z(),
                    max.x(),
                    max.y(),
                    max.z()
                )
            }
            Self::WorkingEquipmentAccess { job, error } => write!(
                formatter,
                "active mining job {} equipment access is invalid: {error}",
                job.value()
            ),
            Self::WorkingDestinationAccess { job, error } => write!(
                formatter,
                "active mining job {} destination access is invalid: {error}",
                job.value()
            ),
            Self::UnknownDeposit { job } => write!(
                formatter,
                "mining job {} references an unknown deposit",
                job.value()
            ),
            Self::UnknownDestination { job } => write!(
                formatter,
                "mining job {} references an unknown destination stockpile",
                job.value()
            ),
            Self::WorkingEquipmentMissing { job } => write!(
                formatter,
                "active mining job {} equipment is missing",
                job.value()
            ),
            Self::UnknownEquipmentDefinition { job, definition } => write!(
                formatter,
                "mining job {} equipment references unknown definition {}",
                job.value(),
                definition.value()
            ),
            Self::WorkingEquipmentDefinitionMismatch {
                job,
                expected,
                actual,
            } => write!(
                formatter,
                "mining job {} equipment definition {} does not match traced definition {}",
                job.value(),
                actual.value(),
                expected.value()
            ),
            Self::WorkingEquipmentRequiresStructuralSupport { job } => write!(
                formatter,
                "active mining job {} uses equipment that requires structural installation and cannot be a portable extraction tool",
                job.value()
            ),
            Self::WorkingEquipmentMounted { job } => write!(
                formatter,
                "active mining job {} uses equipment that is mounted to a structure",
                job.value()
            ),
            Self::EquipmentConditionMismatch { job } => write!(
                formatter,
                "mining job {} equipment condition differs from its start trace",
                job.value()
            ),
            Self::OutputProfileMismatch { job } => write!(
                formatter,
                "mining job {} output no longer matches its geological deposit",
                job.value()
            ),
            Self::ZeroRequestedMass { job } => write!(
                formatter,
                "mining job {} has zero requested extraction mass",
                job.value()
            ),
            Self::OutputMassMismatch {
                job,
                requested,
                available,
                output,
            } => write!(
                formatter,
                "mining job {} output {} mg does not match the recoverable slice for requested {} mg against {} mg available",
                job.value(),
                output.milligrams(),
                requested.milligrams(),
                available.milligrams()
            ),
            Self::OutputExceedsDepositTrace {
                job,
                traced,
                output,
            } => write!(
                formatter,
                "mining job {} output {} mg exceeds traced pre-extraction deposit mass {} mg",
                job.value(),
                output.milligrams(),
                traced.milligrams()
            ),
            Self::WorkingDepositMassMismatch {
                job,
                expected,
                actual,
            } => write!(
                formatter,
                "working mining job {} expects geological source mass {} mg but found {} mg",
                job.value(),
                expected.milligrams(),
                actual.milligrams()
            ),
            Self::ReadyDepositMassAbovePostExtraction {
                job,
                maximum,
                actual,
            } => write!(
                formatter,
                "ready mining job {} requires its extraction to have reduced geological source mass to at most {} mg but found {} mg",
                job.value(),
                maximum.milligrams(),
                actual.milligrams()
            ),
            Self::OverlappingRetainedWork {
                earlier,
                later,
                earlier_completes,
                later_starts,
            } => write!(
                formatter,
                "retained mining job {} completes at tick {} after mining job {} starts at tick {}",
                earlier.value(),
                earlier_completes.value(),
                later.value(),
                later_starts.value()
            ),
            Self::DepositHistoryMassIncrease {
                earlier,
                later,
                maximum_later_mass,
                later_mass,
            } => write!(
                formatter,
                "mining job {} leaves at most {} mg before later retained job {} but that job traces {} mg available",
                earlier.value(),
                maximum_later_mass.milligrams(),
                later.value(),
                later_mass.milligrams()
            ),
            Self::OutputStorageInvalid { job } => write!(
                formatter,
                "mining job {} output is incompatible with its destination storage",
                job.value()
            ),
            Self::EquipmentAlsoUsedByProduction { job } => write!(
                formatter,
                "mining job {} equipment is also occupied by production",
                job.value()
            ),
            Self::EquipmentAlsoUsedByManualPower { job } => write!(
                formatter,
                "mining job {} equipment is also occupied by manual power generation",
                job.value()
            ),
            Self::MissingCapability { job, capability } => write!(
                formatter,
                "mining job {} equipment lacks required capability {}",
                job.value(),
                capability.value()
            ),
            Self::CapabilityKindMismatch {
                job,
                capability,
                expected,
                found,
            } => write!(
                formatter,
                "mining job {} capability {} has {found:?} value kind instead of {expected:?}",
                job.value(),
                capability.value()
            ),
            Self::BatchTooLarge {
                job,
                maximum,
                requested,
            } => write!(
                formatter,
                "mining job {} batch {} mg exceeds equipment maximum {} mg",
                job.value(),
                requested.milligrams(),
                maximum.milligrams()
            ),
            Self::DepositTooHard {
                job,
                hardness,
                maximum,
            } => write!(
                formatter,
                "mining job {} deposit hardness {} Pa exceeds equipment maximum {} Pa",
                job.value(),
                hardness.pascals(),
                maximum.pascals()
            ),
            Self::ZeroThroughput { job } => write!(
                formatter,
                "mining job {} resolves zero throughput",
                job.value()
            ),
            Self::Duration { job, error } => write!(
                formatter,
                "mining job {} duration is invalid: {error}",
                job.value()
            ),
            Self::ConditionDuration { job, error } => write!(
                formatter,
                "mining job {} exceeds equipment condition lifetime: {error}",
                job.value()
            ),
            Self::InvalidSchedule { job } => write!(
                formatter,
                "mining job {} has an invalid work schedule",
                job.value()
            ),
            Self::DurationMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "mining job {} stores {} active ticks but current physics requires {}",
                job.value(),
                stored.value(),
                required.value()
            ),
            Self::ConditionOutcomeMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "mining job {} stores post-work condition {} ppm but current physics requires {} ppm",
                job.value(),
                stored.parts_per_million(),
                required.parts_per_million()
            ),
            Self::WorkingMiningRevisionExhausted { job } => write!(
                formatter,
                "working mining job {} cannot reserve its automatic ready-state revision",
                job.value()
            ),
            Self::WorkingGeologyRevisionExhausted { job } => write!(
                formatter,
                "working mining job {} cannot reserve its extraction revision",
                job.value()
            ),
            Self::WorkingEquipmentRevisionExhausted { job } => write!(
                formatter,
                "working mining job {} cannot reserve its completion equipment revision",
                job.value()
            ),
        }
    }
}
