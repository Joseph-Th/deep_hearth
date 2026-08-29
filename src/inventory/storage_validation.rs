//! Material-reference and containment validation for stockpile admission and relocation boundaries.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Temperature;
use crate::material::{
    CommodityKey, FormId, MaterialComposition, MaterialId, MaterialPhase, MaterialPhaseStateError,
    ParticleSizeDistribution, ParticleSizeStateError, validate_material_particle_size_state,
    validate_material_phase_state,
};
use crate::registry::Registries;

use super::state::{StockpileId, StockpileRecord, StockpileStorageProfile};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CommodityReferenceError {
    UnknownMaterial { material: MaterialId },
    UnknownForm { form: FormId },
    UnsupportedCommodity { commodity: CommodityKey },
}

pub(super) fn validate_commodity_reference(
    registries: &Registries,
    commodity: CommodityKey,
) -> Result<(), CommodityReferenceError> {
    if registries
        .materials()
        .get_material(commodity.material())
        .is_none()
    {
        return Err(CommodityReferenceError::UnknownMaterial {
            material: commodity.material(),
        });
    }
    if registries.materials().get_form(commodity.form()).is_none() {
        return Err(CommodityReferenceError::UnknownForm {
            form: commodity.form(),
        });
    }
    if !registries.materials().has_commodity(commodity) {
        return Err(CommodityReferenceError::UnsupportedCommodity { commodity });
    }
    Ok(())
}

/// Failure because a stockpile's physical containment envelope rejects a material lot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StockpileStorageError {
    UnknownForm {
        form: FormId,
    },
    UnsupportedCommodity {
        commodity: CommodityKey,
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
            Self::UnsupportedCommodity { commodity } => write!(
                formatter,
                "material {} form {} is not an authored runtime commodity",
                commodity.material().value(),
                commodity.form().value()
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
            Self::UnknownForm { form: _form } => None,
            Self::UnsupportedCommodity {
                commodity: _commodity,
            } => None,
            Self::PhaseNotAccepted {
                stockpile: _stockpile,
                phase: _phase,
            } => None,
            Self::TemperatureExceedsMaximum {
                stockpile: _stockpile,
                temperature: _temperature,
                maximum: _maximum,
            } => None,
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
    validate_stockpile_storage_profile(
        registries,
        record.storage_profile(),
        stockpile,
        commodity,
        composition,
        temperature,
        particle_size,
    )
}

pub(super) fn validate_stockpile_storage_profile(
    registries: &Registries,
    profile: StockpileStorageProfile,
    stockpile: StockpileId,
    commodity: CommodityKey,
    composition: &MaterialComposition,
    temperature: Temperature,
    particle_size: Option<&ParticleSizeDistribution>,
) -> Result<(), StockpileStorageError> {
    let form_id = commodity.form();
    let Some(form) = registries.materials().get_form(form_id) else {
        return Err(StockpileStorageError::UnknownForm { form: form_id });
    };
    if registries
        .materials()
        .get_material(commodity.material())
        .is_some()
        && !registries.materials().has_commodity(commodity)
    {
        return Err(StockpileStorageError::UnsupportedCommodity { commodity });
    }
    validate_material_phase_state(registries.materials(), commodity, composition, temperature)
        .map_err(StockpileStorageError::InvalidMaterialPhaseState)?;
    validate_material_particle_size_state(registries.materials(), commodity, particle_size)
        .map_err(StockpileStorageError::InvalidParticleSizeState)?;
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
