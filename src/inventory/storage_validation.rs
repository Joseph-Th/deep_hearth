//! Material-containment validation for stockpile ingress and relocation boundaries.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Temperature;
use crate::material::{
    CommodityKey, FormId, MaterialComposition, MaterialPhase, MaterialPhaseStateError,
    ParticleSizeDistribution, ParticleSizeStateError, validate_material_particle_size_state,
    validate_material_phase_state,
};
use crate::registry::Registries;

use super::state::{StockpileId, StockpileRecord};

/// Failure because a stockpile's physical containment envelope rejects a material lot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StockpileStorageError {
    UnknownForm {
        form: FormId,
    },
    InvalidMaterialPhaseState(MaterialPhaseStateError),
    InvalidParticleSizeState(ParticleSizeStateError),
    PhaseNotAccepted {
        stockpile: StockpileId,
        phase: MaterialPhase,
    },
    TemperatureExceedsMaximum {
        stockpile: StockpileId,
        temperature: Temperature,
        maximum: Temperature,
    },
}

impl Display for StockpileStorageError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownForm { form } => write!(
                formatter,
                "storage compatibility references unknown form {}",
                form.value()
            ),
            Self::InvalidParticleSizeState(error) => write!(
                formatter,
                "material particle-size state is invalid for storage: {error}"
            ),
            Self::InvalidMaterialPhaseState(error) => write!(
                formatter,
                "material phase state is invalid for storage: {error}"
            ),
            Self::PhaseNotAccepted { stockpile, phase } => write!(
                formatter,
                "stockpile {} does not accept {phase:?} material",
                stockpile.value()
            ),
            Self::TemperatureExceedsMaximum {
                stockpile,
                temperature,
                maximum,
            } => write!(
                formatter,
                "material temperature {} mK exceeds stockpile {} maximum {} mK",
                temperature.millikelvin(),
                stockpile.value(),
                maximum.millikelvin()
            ),
        }
    }
}

impl Error for StockpileStorageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidMaterialPhaseState(error) => Some(error),
            Self::InvalidParticleSizeState(error) => Some(error),
            Self::UnknownForm { .. }
            | Self::PhaseNotAccepted { .. }
            | Self::TemperatureExceedsMaximum { .. } => None,
        }
    }
}

pub(crate) fn validate_stockpile_storage(
    registries: &Registries,
    record: &StockpileRecord,
    stockpile: StockpileId,
    commodity: CommodityKey,
    composition: &MaterialComposition,
    temperature: Temperature,
    particle_size: Option<&ParticleSizeDistribution>,
) -> Result<(), StockpileStorageError> {
    validate_material_phase_state(registries.materials(), commodity, composition, temperature)
        .map_err(StockpileStorageError::InvalidMaterialPhaseState)?;
    validate_material_particle_size_state(registries.materials(), commodity, particle_size)
        .map_err(StockpileStorageError::InvalidParticleSizeState)?;
    let form_id = commodity.form();
    let Some(form) = registries.materials().get_form(form_id) else {
        return Err(StockpileStorageError::UnknownForm { form: form_id });
    };
    let profile = record.storage_profile();
    if !profile.can_store_phase(form.phase()) {
        return Err(StockpileStorageError::PhaseNotAccepted {
            stockpile,
            phase: form.phase(),
        });
    }
    if temperature > profile.maximum_temperature() {
        return Err(StockpileStorageError::TemperatureExceedsMaximum {
            stockpile,
            temperature,
            maximum: profile.maximum_temperature(),
        });
    }
    Ok(())
}
