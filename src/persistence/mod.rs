//! Current-schema persistence envelope and decoded-state validation, independent of storage and encoding.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::inventory::{StockpileId, StockpileStorageProfile};
use crate::player::{PlayerAdmissionState, player_admission_state};
use crate::registry::{Registries, RegistrySchemaVersion};

/// Save schema emitted and accepted by this build.
pub const CURRENT_SAVE_SCHEMA_VERSION: u32 = 91;

/// Borrowed versioned save payload suitable for any Serde encoding adapter.
///
/// Serialization is a privileged persistence boundary, not an actor-safe observation surface. The
/// encoded state necessarily contains authoritative hidden world data such as finite geology; game
/// policy and player-facing planning must use the public observation/knowledge APIs instead.
#[derive(Debug, Serialize)]
pub struct SaveEnvelope<'state> {
    schema_version: u32,
    registry_schema_version: RegistrySchemaVersion,
    #[serde(serialize_with = "crate::core::state::serialize_app_state")]
    state: &'state AppState,
}

impl<'state> SaveEnvelope<'state> {
    /// Wraps current state in the current semantic save schema.
    #[must_use]
    pub const fn new(registries: &Registries, state: &'state AppState) -> Self {
        Self {
            schema_version: CURRENT_SAVE_SCHEMA_VERSION,
            registry_schema_version: registries.schema_version(),
            state,
        }
    }
}

/// Owned decoded save payload; callers must validate it with `into_state` before runtime use.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadedSaveEnvelope {
    schema_version: u32,
    registry_schema_version: RegistrySchemaVersion,
    #[serde(deserialize_with = "crate::core::state::deserialize_unvalidated_app_state")]
    state: AppState,
}

impl LoadedSaveEnvelope {
    /// Requires exact current schemas and validates persistent invariants before returning runtime state.
    pub fn into_state(self, registries: &Registries) -> Result<AppState, LoadError> {
        let Self {
            schema_version,
            registry_schema_version,
            mut state,
        } = self;

        validate_versions(schema_version, registry_schema_version, registries)?;

        state.rebuild_derived_indexes();
        validate_persistable_state(registries, &state)?;
        Ok(state)
    }
}

/// Validates the semantic invariants required for current runtime state to round-trip through
/// trusted persistence.
///
/// This includes the generic cross-owner state audit plus persistence-only ordinary-player
/// admission and carried-custody constraints. It performs no encoding and does not rebuild derived
/// indexes, so callers must pass live runtime state rather than an unpromoted decoded payload.
pub fn validate_persistable_state(
    registries: &Registries,
    state: &AppState,
) -> Result<(), LoadError> {
    validate_loaded_state(registries, state).map_err(LoadError::InvalidState)?;
    validate_persisted_player_admission(registries, state)
}

fn validate_persisted_player_admission(
    registries: &Registries,
    state: &AppState,
) -> Result<(), LoadError> {
    match player_admission_state(state) {
        PlayerAdmissionState::Absent => Ok(()),
        PlayerAdmissionState::Partial {
            survival_initialized,
            logistics_initialized,
        } => Err(LoadError::IncompletePlayerAdmission {
            survival_initialized,
            logistics_initialized,
        }),
        PlayerAdmissionState::Admitted => {
            let player = state
                .logistics()
                .player()
                .unwrap_or_else(|| unreachable!("admitted player has logistics state"));
            let stockpile = state
                .inventory()
                .get_stockpile(player.carried_stockpile())
                .unwrap_or_else(|| {
                    unreachable!("generic validation proved admitted carried custody exists")
                });
            let expected_capacity = registries.survival().physiology().carried_mass_capacity();
            let equipment_mass = state
                .logistics()
                .carried_equipment()
                .try_fold(Mass::ZERO, |total, equipment| {
                    let mass = state
                        .equipment()
                        .get_equipment(equipment)
                        .unwrap_or_else(|| {
                            unreachable!("generic validation proved carried equipment exists")
                        })
                        .embodied_mass();
                    total.checked_add(mass)
                })
                .ok_or(LoadError::PlayerCarriedCapacityOverflow {
                    stockpile: stockpile.id(),
                })?;
            let actual_capacity = stockpile.capacity().checked_add(equipment_mass).ok_or(
                LoadError::PlayerCarriedCapacityOverflow {
                    stockpile: stockpile.id(),
                },
            )?;
            if actual_capacity != expected_capacity {
                return Err(LoadError::PlayerCarriedCapacityMismatch {
                    stockpile: stockpile.id(),
                    expected: expected_capacity,
                    actual: actual_capacity,
                });
            }
            let expected_profile = StockpileStorageProfile::unbounded_solid_only();
            if stockpile.storage_profile() != expected_profile {
                return Err(LoadError::PlayerCarriedStorageProfileMismatch {
                    stockpile: stockpile.id(),
                    expected: expected_profile,
                    actual: stockpile.storage_profile(),
                });
            }
            Ok(())
        }
    }
}

fn validate_versions(
    schema_version: u32,
    registry_schema_version: RegistrySchemaVersion,
    registries: &Registries,
) -> Result<(), LoadError> {
    if schema_version != CURRENT_SAVE_SCHEMA_VERSION {
        return Err(LoadError::UnsupportedSchemaVersion {
            found: schema_version,
            supported: CURRENT_SAVE_SCHEMA_VERSION,
        });
    }
    if registry_schema_version != registries.schema_version() {
        return Err(LoadError::RegistrySchemaMismatch {
            found: registry_schema_version,
            supported: registries.schema_version(),
        });
    }
    Ok(())
}

/// Semantic persistence validation failure for current-schema runtime state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// The save was produced for a semantic schema this build does not support.
    UnsupportedSchemaVersion { found: u32, supported: u32 },
    /// Stable authored registry identities do not match this build.
    RegistrySchemaMismatch {
        found: RegistrySchemaVersion,
        supported: RegistrySchemaVersion,
    },
    /// Ordinary player state was persisted with only one admission owner present.
    IncompletePlayerAdmission {
        survival_initialized: bool,
        logistics_initialized: bool,
    },
    /// Ordinary player carried custody no longer matches the authored body capacity.
    PlayerCarriedCapacityMismatch {
        stockpile: StockpileId,
        expected: Mass,
        actual: Mass,
    },
    /// Ordinary player carried custody exceeds the represented mass range.
    PlayerCarriedCapacityOverflow { stockpile: StockpileId },
    /// Ordinary player carried custody no longer has its canonical ambient solid-only profile.
    PlayerCarriedStorageProfileMismatch {
        stockpile: StockpileId,
        expected: StockpileStorageProfile,
        actual: StockpileStorageProfile,
    },
    /// The decoded runtime data violates a persistent state invariant.
    InvalidState(StateValidationError),
}

impl Display for LoadError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion { found, supported } => write!(
                formatter,
                "unsupported save schema version {found}; this build supports {supported}"
            ),
            Self::RegistrySchemaMismatch { found, supported } => write!(
                formatter,
                "save registry schema {} does not match this build's schema {}",
                found.value(),
                supported.value()
            ),
            Self::IncompletePlayerAdmission {
                survival_initialized,
                logistics_initialized,
            } => write!(
                formatter,
                "persisted player admission is incomplete: survival initialized={survival_initialized}, logistics initialized={logistics_initialized}"
            ),
            Self::PlayerCarriedCapacityMismatch {
                stockpile,
                expected,
                actual,
            } => write!(
                formatter,
                "persisted player carried custody for stockpile {} represents {} mg capacity; authored body capacity is {} mg",
                stockpile.value(),
                actual.milligrams(),
                expected.milligrams()
            ),
            Self::PlayerCarriedCapacityOverflow { stockpile } => write!(
                formatter,
                "persisted player carried custody for stockpile {} exceeds represented mass range",
                stockpile.value()
            ),
            Self::PlayerCarriedStorageProfileMismatch {
                stockpile,
                expected: _expected,
                actual: _actual,
            } => write!(
                formatter,
                "persisted player carried stockpile {} does not have the canonical ambient solid-only storage profile",
                stockpile.value()
            ),
            Self::InvalidState(error) => write!(formatter, "invalid persisted state: {error}"),
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::UnsupportedSchemaVersion {
                found: _found,
                supported: _supported,
            } => None,
            Self::RegistrySchemaMismatch { .. }
            | Self::IncompletePlayerAdmission { .. }
            | Self::PlayerCarriedCapacityMismatch { .. }
            | Self::PlayerCarriedCapacityOverflow { .. }
            | Self::PlayerCarriedStorageProfileMismatch { .. } => None,
            Self::InvalidState(error) => Some(error),
        }
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
