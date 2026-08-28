//! Finite energy storage, construction, accounting, and exact power integration.

mod accounting;
mod construction_execution;
mod definitions;
mod disassembly_execution;
mod integration;
mod passive_dissipation;
mod state;
mod storage_execution;

pub use accounting::{
    ExplicitEnergyAccounting, ExplicitEnergyAccountingError, calculate_explicit_energy_accounting,
};
pub use construction_execution::{
    EnergyStoreAssemblyCommitError, EnergyStoreAssemblyError, ValidatedEnergyStoreAssembly,
    validate_assemble_energy_store,
};

pub use definitions::{
    EnergyCarrier, EnergyRegistry, EnergyStoreDefinition, EnergyStoreDefinitionId,
};
pub use disassembly_execution::{
    EnergyStoreDisassemblyCommitError, EnergyStoreDisassemblyError, EnergyStoreDisassemblyOutcome,
    ValidatedEnergyStoreDisassembly, validate_disassemble_energy_store,
};
pub use integration::{
    PowerDurationError, PowerIntegration, PowerIntegrationError, PowerRemainder,
    calculate_mass_specific_energy, calculate_power_duration_ceiling, integrate_power,
};
pub use state::{EnergyState, EnergyStoreId, EnergyStoreRecord, EnergyValidationError};
pub use storage_execution::{
    ConsumedEnergyTrace, EnergySinkError, EnergySupplyError, ReleasedEnergyTrace,
    ValidatedEnergySink, ValidatedEnergySupply, validate_energy_sink, validate_energy_supply,
};

#[cfg(feature = "test-gameplay")]
pub(crate) use storage_execution::add_energy_store_with_initial_for_fixture;
#[cfg(test)]
pub(crate) use storage_execution::add_energy_store_with_initial_for_fixture as add_energy_store_with_initial_for_test;
#[cfg(test)]
pub(crate) use storage_execution::{AddEnergyStoreError, add_energy_store};

pub(crate) use passive_dissipation::{
    apply_passive_energy_dissipation, decide_passive_energy_dissipation,
};
pub(crate) use state::validate_loaded_energy;
pub(crate) use storage_execution::{
    EnergyCommitError, EnergyConsumptionReservation, EnergyIngressReservation,
    EnergyIngressReservationError, EnergyReservationError, apply_energy_consumption_reservation,
    apply_released_energy_outcomes, validate_energy_consumption_reservation,
    validate_energy_ingress_reservation,
};
