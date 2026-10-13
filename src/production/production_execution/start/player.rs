//! Player-local authorization for already-resolved production work.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::logistics::{
    PlayerEnergyStoreAccessError, PlayerEquipmentAccessError, PlayerStockpileAccessError,
    validate_player_energy_store_access, validate_player_equipment_access,
    validate_player_stockpile_access,
};
use crate::registry::Registries;

use super::{
    ProcessOutputRoute, StartProcessError, ValidatedStartProcess, validate_start_process_routed,
};
use crate::inventory::StockpileId;
use crate::player::{PlayerAdmissionState, player_admission_state};
use crate::production::ProcessResolution;
use crate::survival::Vitality;

/// Failure while authorizing an already-resolved production process for an admitted player.
#[derive(Debug, PartialEq, Eq)]
pub enum StartPlayerProcessError {
    PlayerNotAdmitted,
    PlayerDead,
    StockpileAccess(PlayerStockpileAccessError),
    EquipmentAccess(PlayerEquipmentAccessError),
    EnergyStoreAccess(PlayerEnergyStoreAccessError),
    Process(StartProcessError),
}

impl Display for StartPlayerProcessError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => formatter.write_str("player is not fully admitted"),
            Self::PlayerDead => formatter.write_str("dead player cannot start production"),
            Self::StockpileAccess(error) => write!(formatter, "stockpile access failed: {error}"),
            Self::EquipmentAccess(error) => write!(formatter, "equipment access failed: {error}"),
            Self::EnergyStoreAccess(error) => {
                write!(formatter, "energy-store access failed: {error}")
            }
            Self::Process(error) => write!(formatter, "process start failed: {error}"),
        }
    }
}

impl Error for StartPlayerProcessError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PlayerNotAdmitted | Self::PlayerDead => None,
            Self::StockpileAccess(error) => Some(error),
            Self::EquipmentAccess(error) => Some(error),
            Self::EnergyStoreAccess(error) => Some(error),
            Self::Process(error) => Some(error),
        }
    }
}

fn validate_player_process_access(
    state: &AppState,
    resolution: &ProcessResolution,
    routes: &[ProcessOutputRoute],
) -> Result<(), StartPlayerProcessError> {
    if player_admission_state(state) != PlayerAdmissionState::Admitted {
        return Err(StartPlayerProcessError::PlayerNotAdmitted);
    }
    let player = state
        .survival()
        .player()
        .unwrap_or_else(|| unreachable!("admitted player has survival state"));
    if player.vitality() == Vitality::ZERO {
        return Err(StartPlayerProcessError::PlayerDead);
    }
    validate_player_stockpile_access(state, resolution.source())
        .map_err(StartPlayerProcessError::StockpileAccess)?;
    for route in routes {
        validate_player_stockpile_access(state, route.destination())
            .map_err(StartPlayerProcessError::StockpileAccess)?;
    }
    if let Some(equipment) = resolution.equipment_input() {
        validate_player_equipment_access(state, equipment.equipment())
            .map_err(StartPlayerProcessError::EquipmentAccess)?;
    }
    if let Some(energy) = resolution.energy_input() {
        validate_player_energy_store_access(state, energy.source())
            .map_err(StartPlayerProcessError::EnergyStoreAccess)?;
    }
    if let Some(sink) = resolution.energy_sink() {
        validate_player_energy_store_access(state, sink.trace().destination())
            .map_err(StartPlayerProcessError::EnergyStoreAccess)?;
    }
    Ok(())
}

/// Authorizes one single-output resolved process for an admitted player's current exact-local
/// custody before entering capability-level production admission.
///
/// Controlled fixtures without a fully admitted player must use capability-level
/// [`super::validate_start_process`] directly. Ordinary player-facing callers should use this
/// surface so living player admission and exact-local access are both enforced.
pub fn validate_start_player_process(
    registries: &Registries,
    state: &AppState,
    resolution: &ProcessResolution,
    source: StockpileId,
    destination: StockpileId,
) -> Result<ValidatedStartProcess, StartPlayerProcessError> {
    let Some(stream) = resolution.single_output_stream() else {
        return Err(StartPlayerProcessError::Process(
            StartProcessError::OutputRouteCountMismatch {
                streams: resolution.output_streams().len(),
                routes: 1,
            },
        ));
    };
    validate_start_player_process_routed(
        registries,
        state,
        resolution,
        source,
        &[ProcessOutputRoute::new(stream.id(), destination)],
    )
}

/// Authorizes routed resolved production for a living admitted player's current exact-local custody
/// before entering capability-level production admission.
pub fn validate_start_player_process_routed(
    registries: &Registries,
    state: &AppState,
    resolution: &ProcessResolution,
    source: StockpileId,
    routes: &[ProcessOutputRoute],
) -> Result<ValidatedStartProcess, StartPlayerProcessError> {
    validate_player_process_access(state, resolution, routes)?;
    let validated = validate_start_process_routed(registries, state, resolution, source, routes)
        .map_err(StartPlayerProcessError::Process)?;
    Ok(validated.bind_player_survival_revision(state.survival().revision()))
}
