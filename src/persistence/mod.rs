//! Current-schema persistence envelope and decoded-state validation, independent of storage and encoding.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::registry::{Registries, RegistrySchemaVersion};

/// Save schema emitted and accepted by this build.
pub const CURRENT_SAVE_SCHEMA_VERSION: u32 = 55;

/// Borrowed versioned save payload suitable for any Serde encoding adapter.
#[derive(Debug, Serialize)]
pub struct SaveEnvelope<'state> {
    schema_version: u32,
    registry_schema_version: RegistrySchemaVersion,
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
        validate_loaded_state(registries, &state).map_err(LoadError::InvalidState)?;
        Ok(state)
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

/// Semantic persistence failure after bytes have already been decoded by an adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// The save was produced for a semantic schema this build does not support.
    UnsupportedSchemaVersion { found: u32, supported: u32 },
    /// Stable authored registry identities do not match this build.
    RegistrySchemaMismatch {
        found: RegistrySchemaVersion,
        supported: RegistrySchemaVersion,
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
            Self::RegistrySchemaMismatch {
                found: _found,
                supported: _supported,
            } => None,
            Self::InvalidState(error) => Some(error),
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
