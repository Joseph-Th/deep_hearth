//! Actor-selected solid firing and cooling, keeping exact lot membership in inventory.

use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::{MaterialStackProjection, MaterialStackProjectionError};
use crate::logistics::{
    PlayerEnergyStoreAccessError, PlayerEquipmentAccessError, PlayerStockpileAccessError,
    validate_player_energy_store_access, validate_player_equipment_access,
    validate_player_stockpile_access,
};
use crate::player::{PlayerActionValidationError, validate_player_action};
use crate::production::{
    ProcessId, StartPlayerProcessError, ValidatedStartProcess, validate_start_player_process,
};
use crate::registry::Registries;

use super::{
    SensibleCoolingRequest, SensibleCoolingResolutionError, SensibleHeatingRequest,
    SensibleHeatingResolutionError, resolve_sensible_cooling_process,
    resolve_sensible_heating_process,
};

#[derive(Debug)]
pub enum PlayerThermalBatchError {
    PlayerNotAdmitted,
    PlayerDead,
    Stockpile(PlayerStockpileAccessError),
    Equipment(PlayerEquipmentAccessError),
    Energy(PlayerEnergyStoreAccessError),
    Stack(MaterialStackProjectionError),
    UnsupportedProcess(ProcessId),
    Firing(SensibleHeatingResolutionError),
    Cooling(SensibleCoolingResolutionError),
    Start(StartPlayerProcessError),
}

impl Display for PlayerThermalBatchError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => f.write_str("player is not admitted"),
            Self::PlayerDead => f.write_str("dead player cannot start thermal work"),
            Self::Stockpile(e) => write!(f, "{e}"),
            Self::Equipment(e) => write!(f, "{e}"),
            Self::Energy(e) => write!(f, "{e}"),
            Self::Stack(e) => write!(f, "{e}"),
            Self::UnsupportedProcess(id) => write!(
                f,
                "process {} is not authored solid firing or cooling",
                id.value()
            ),
            Self::Firing(e) => write!(f, "{e}"),
            Self::Cooling(e) => write!(f, "{e}"),
            Self::Start(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for PlayerThermalBatchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::PlayerNotAdmitted | Self::PlayerDead => None,
            Self::Stockpile(e) => Some(e),
            Self::Equipment(e) => Some(e),
            Self::Energy(e) => Some(e),
            Self::Stack(e) => Some(e),
            Self::UnsupportedProcess(_) => None,
            Self::Firing(e) => Some(e),
            Self::Cooling(e) => Some(e),
            Self::Start(e) => Some(e),
        }
    }
}

/// Fire at the authored conversion temperature or cool at the authored target.
/// The selected physical stack supplies both input and output custody. The
/// production owner reserves the actual equipment, heat and output capacity.
pub fn validate_start_player_thermal_batch_from_stack(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    stack: &MaterialStackProjection,
    mass: Mass,
    equipment: EquipmentId,
    energy: EnergyStoreId,
) -> Result<ValidatedStartProcess, PlayerThermalBatchError> {
    use PlayerThermalBatchError as E;
    validate_player_action(state).map_err(|e| match e {
        PlayerActionValidationError::PlayerNotAdmitted => E::PlayerNotAdmitted,
        PlayerActionValidationError::PlayerDead => E::PlayerDead,
    })?;
    let source = stack.stockpile();
    validate_player_stockpile_access(state, source).map_err(E::Stockpile)?;
    validate_player_equipment_access(state, equipment).map_err(E::Equipment)?;
    validate_player_energy_store_access(state, energy).map_err(E::Energy)?;
    let selections = stack
        .selections_for_mass(state, source, mass)
        .map_err(E::Stack)?;
    if let Some(conversion) = registries
        .thermal()
        .get_sensible_heating(process)
        .and_then(|definition| definition.firing_conversion())
    {
        let firing = resolve_sensible_heating_process(
            registries,
            state,
            SensibleHeatingRequest::new(
                process,
                source,
                &selections,
                equipment,
                energy,
                conversion.minimum_temperature(),
            ),
        )
        .map_err(E::Firing)?;
        validate_start_player_process(
            registries,
            state,
            firing.process_resolution(),
            source,
            source,
        )
        .map_err(E::Start)
    } else if registries.thermal().get_sensible_cooling(process).is_some() {
        let cooling = resolve_sensible_cooling_process(
            registries,
            state,
            SensibleCoolingRequest::new(process, source, &selections, equipment, energy),
        )
        .map_err(E::Cooling)?;
        validate_start_player_process(
            registries,
            state,
            cooling.process_resolution(),
            source,
            source,
        )
        .map_err(E::Start)
    } else {
        Err(E::UnsupportedProcess(process))
    }
}
