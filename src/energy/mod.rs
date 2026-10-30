//! Owns finite energy stores, construction, accounting, and exact power integration.

mod accounting;
mod availability;
mod combustion_execution;
mod construction_execution;
mod definitions;
mod disassembly_execution;
#[cfg(any(test, feature = "test-gameplay"))]
mod fixture_execution;
mod illumination;
mod integration;
mod mechanical_transmission;
mod passive_dissipation;
mod player_upgrade;
mod state;
mod storage_execution;
mod upgrade_execution;

pub use crate::core::quantity::PreciseEnergy;
pub use accounting::{
    ExplicitEnergyAccounting, ExplicitEnergyAccountingError, calculate_explicit_energy_accounting,
};
pub use combustion_execution::{
    CombustionCommitError, CombustionError, ValidatedCombustion, validate_combust_fuel,
    validate_combust_selected_fuel,
};
pub use construction_execution::{
    EnergyStoreAssemblyCommitError, EnergyStoreAssemblyError, ValidatedEnergyStoreAssembly,
    validate_assemble_energy_store,
};
pub use illumination::{
    LocalIlluminationAssessment, LocalLightSourceProjection, assess_local_illumination,
    project_local_light_sources,
};

pub use definitions::{
    CombustionProfile, EnergyCarrier, EnergyRegistry, EnergyStoreDefinition,
    EnergyStoreDefinitionId, EnergyStoreUpgradeProfile,
};
pub use disassembly_execution::{
    EnergyStoreDisassemblyCommitError, EnergyStoreDisassemblyError, EnergyStoreDisassemblyOutcome,
    ValidatedEnergyStoreDisassembly, validate_disassemble_energy_store,
};
pub(crate) use integration::integrate_power_or_saturate;
pub use integration::{
    PowerDurationError, PowerIntegration, PowerIntegrationError, PowerRemainder,
    calculate_mass_specific_energy, calculate_mass_specific_energy_capacity,
    calculate_power_duration_ceiling, integrate_power,
};
pub use mechanical_transmission::MechanicalTransmissionReceipt;
pub(crate) use mechanical_transmission::{
    apply_mechanical_transmission, decide_mechanical_transmission,
};
pub use passive_dissipation::{
    PassiveDissipationHorizonError, passive_dissipation_ticks_until_empty,
};
pub use player_upgrade::{
    PlayerEnergyStoreUpgradeCatalogError, PlayerEnergyStoreUpgradeOption,
    player_energy_store_upgrade_options, validate_upgrade_player_energy_store_option,
};
pub use state::{EnergyState, EnergyStoreId, EnergyStoreRecord, EnergyValidationError};
pub use storage_execution::{
    ConsumedEnergyTrace, EnergySinkError, EnergySupplyError, ReleasedEnergyTrace,
    ValidatedEnergySink, ValidatedEnergySupply, validate_energy_sink, validate_energy_supply,
};
pub use upgrade_execution::{
    EnergyStoreUpgradeCommitError, EnergyStoreUpgradeError, ValidatedEnergyStoreUpgrade,
    validate_upgrade_energy_store,
};

#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) use fixture_execution::add_energy_store_with_initial_for_fixture;
#[cfg(test)]
pub(crate) use fixture_execution::{AddEnergyStoreError, add_energy_store};

pub(crate) use availability::{EnergyStoreOccupancy, energy_store_occupancy};
pub(crate) use passive_dissipation::{
    apply_passive_energy_dissipation, decide_passive_energy_dissipation,
};
pub(crate) use state::validate_loaded_energy;
pub(crate) use storage_execution::{
    EnergyConsumptionReservation, EnergyIngressReservation, EnergyIngressReservationError,
    EnergyReservationError, EnergySinkCapacityError, EnergySupplyAccess, ValidatedEnergySinkAccess,
    apply_prechecked_energy_consumption_reservation, apply_released_energy_outcomes,
    assert_released_energy_outcomes_available, assess_energy_supply_access,
    validate_energy_consumption_reservation, validate_energy_ingress_reservation,
    validate_energy_sink_access, validate_energy_sink_capacity_at_release,
    validate_energy_sink_release, validate_energy_supply_request,
};
