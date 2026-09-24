//! Runtime diagnostics for shared powered ore equipment, supply, and timing.

use crate::capability::CapabilityEvaluationError;
use crate::core::quantity::Mass;
use crate::core::throughput::MassFlowDurationError;
use crate::energy::{EnergyCarrier, EnergySupplyError, PowerDurationError};
use crate::equipment::EquipmentProviderError;
use crate::maintenance::ActiveConditionDurationError;

/// Failure while resolving condition-adjusted equipment limits for one powered ore batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ore_processing) enum PoweredOreEquipmentError {
    MissingMassFlowCapability,
    MissingMaximumBatchMassCapability,
    BatchMassExceeded { selected: Mass, maximum: Mass },
}

/// Shared provider-admission failure before process-specific output physics are resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ore_processing) enum PoweredOreProviderError {
    Provider(EquipmentProviderError),
    Capability(CapabilityEvaluationError),
    Equipment(PoweredOreEquipmentError),
}

/// Shared finite-energy and active-time failure after process-specific output physics succeed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ore_processing) enum PoweredOreSupplyError {
    Supply(EnergySupplyError),
    WrongEnergyCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    Timing(PoweredOreTimingError),
}

/// Failure while resolving common active-time and wear physics after equipment admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ore_processing) enum PoweredOreTimingError {
    Throughput(MassFlowDurationError),
    Energy(PowerDurationError),
    Condition(ActiveConditionDurationError),
}
