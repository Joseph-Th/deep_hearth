//! Trusted-load validation for finite loose surface-resource state.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::time::SimulationTick;
use crate::material::{
    CommodityKey, CompositionError, FormId, MaterialId, MaterialPhase, MaterialPhaseStateError,
    MaterialRegistry, ParticleSizeStatePolicy, validate_material_phase_state,
};

use super::{
    SurfaceResourceId, SurfaceResourceLifecycle, SurfaceResourceRecord, SurfaceResourceState,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceResourceValidationError {
    ZeroNextResourceId,
    NextIdNotAfterExisting {
        next: u32,
        highest: SurfaceResourceId,
    },
    ZeroResourceId,
    IdMismatch {
        key: SurfaceResourceId,
        record: SurfaceResourceId,
    },
    ZeroInitialMass {
        resource: SurfaceResourceId,
    },
    RemainingMassExceedsInitial {
        resource: SurfaceResourceId,
    },
    AvailableWithoutMass {
        resource: SurfaceResourceId,
    },
    DepletedWithRemainingMass {
        resource: SurfaceResourceId,
    },
    AvailableHasDepletionTick {
        resource: SurfaceResourceId,
    },
    DepletedMissingDepletionTick {
        resource: SurfaceResourceId,
    },
    GeneratedInFuture {
        resource: SurfaceResourceId,
    },
    DepletionBeforeGeneration {
        resource: SurfaceResourceId,
    },
    DepletedInFuture {
        resource: SurfaceResourceId,
    },
    InvalidComposition {
        resource: SurfaceResourceId,
        error: CompositionError,
    },
    CompositionMissingHost {
        resource: SurfaceResourceId,
        host: MaterialId,
    },
    UnknownMaterial {
        resource: SurfaceResourceId,
        material: MaterialId,
    },
    UnknownForm {
        resource: SurfaceResourceId,
        form: FormId,
    },
    UnsupportedCommodity {
        resource: SurfaceResourceId,
        commodity: CommodityKey,
    },
    UnsupportedPhase {
        resource: SurfaceResourceId,
        form: FormId,
        phase: MaterialPhase,
    },
    UnsupportedParticulateForm {
        resource: SurfaceResourceId,
        form: FormId,
    },
    UnknownCompositionMaterial {
        resource: SurfaceResourceId,
        material: MaterialId,
    },
    InvalidPhaseState {
        resource: SurfaceResourceId,
        error: MaterialPhaseStateError,
    },
}

impl Display for SurfaceResourceValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "surface resource state invariant failed: {self:?}"
        )
    }
}

impl Error for SurfaceResourceValidationError {}

fn validate_material_profile(
    materials: &MaterialRegistry,
    resource: SurfaceResourceId,
    record: &SurfaceResourceRecord,
) -> Result<(), SurfaceResourceValidationError> {
    let commodity = record.commodity;
    if materials.get_material(commodity.material()).is_none() {
        return Err(SurfaceResourceValidationError::UnknownMaterial {
            resource,
            material: commodity.material(),
        });
    }
    let Some(form) = materials.get_form(commodity.form()) else {
        return Err(SurfaceResourceValidationError::UnknownForm {
            resource,
            form: commodity.form(),
        });
    };
    if !materials.has_commodity(commodity) {
        return Err(SurfaceResourceValidationError::UnsupportedCommodity {
            resource,
            commodity,
        });
    }
    if form.phase() != MaterialPhase::Solid {
        return Err(SurfaceResourceValidationError::UnsupportedPhase {
            resource,
            form: commodity.form(),
            phase: form.phase(),
        });
    }
    if form.particle_size_policy() == ParticleSizeStatePolicy::Required {
        return Err(SurfaceResourceValidationError::UnsupportedParticulateForm {
            resource,
            form: commodity.form(),
        });
    }
    for component in record.composition.components() {
        if materials.get_material(component.material()).is_none() {
            return Err(SurfaceResourceValidationError::UnknownCompositionMaterial {
                resource,
                material: component.material(),
            });
        }
    }
    validate_material_phase_state(
        materials,
        commodity,
        &record.composition,
        record.temperature,
    )
    .map_err(|error| SurfaceResourceValidationError::InvalidPhaseState { resource, error })
}

pub(super) fn validate_surface_resource_record(
    materials: &MaterialRegistry,
    key: SurfaceResourceId,
    record: &SurfaceResourceRecord,
    current: SimulationTick,
) -> Result<(), SurfaceResourceValidationError> {
    if key.value() == 0 || record.id.value() == 0 {
        return Err(SurfaceResourceValidationError::ZeroResourceId);
    }
    if key != record.id {
        return Err(SurfaceResourceValidationError::IdMismatch {
            key,
            record: record.id,
        });
    }
    if record.initial_mass.is_zero() {
        return Err(SurfaceResourceValidationError::ZeroInitialMass { resource: key });
    }
    if record.remaining_mass > record.initial_mass {
        return Err(SurfaceResourceValidationError::RemainingMassExceedsInitial { resource: key });
    }
    match record.lifecycle {
        SurfaceResourceLifecycle::Available if record.remaining_mass.is_zero() => {
            return Err(SurfaceResourceValidationError::AvailableWithoutMass { resource: key });
        }
        SurfaceResourceLifecycle::Depleted if !record.remaining_mass.is_zero() => {
            return Err(SurfaceResourceValidationError::DepletedWithRemainingMass {
                resource: key,
            });
        }
        SurfaceResourceLifecycle::Available | SurfaceResourceLifecycle::Depleted => {}
    }
    if record.generated_at > current {
        return Err(SurfaceResourceValidationError::GeneratedInFuture { resource: key });
    }
    match (record.lifecycle, record.depleted_at) {
        (SurfaceResourceLifecycle::Available, Some(_)) => {
            return Err(SurfaceResourceValidationError::AvailableHasDepletionTick {
                resource: key,
            });
        }
        (SurfaceResourceLifecycle::Depleted, None) => {
            return Err(
                SurfaceResourceValidationError::DepletedMissingDepletionTick { resource: key },
            );
        }
        (SurfaceResourceLifecycle::Depleted, Some(at)) if at < record.generated_at => {
            return Err(SurfaceResourceValidationError::DepletionBeforeGeneration {
                resource: key,
            });
        }
        (SurfaceResourceLifecycle::Depleted, Some(at)) if at > current => {
            return Err(SurfaceResourceValidationError::DepletedInFuture { resource: key });
        }
        _ => {}
    }
    record.composition.validate().map_err(|error| {
        SurfaceResourceValidationError::InvalidComposition {
            resource: key,
            error,
        }
    })?;
    if record
        .composition
        .parts_per_million(record.commodity.material())
        == 0
    {
        return Err(SurfaceResourceValidationError::CompositionMissingHost {
            resource: key,
            host: record.commodity.material(),
        });
    }
    validate_material_profile(materials, key, record)
}

pub(crate) fn validate_loaded_surface_resources(
    materials: &MaterialRegistry,
    state: &SurfaceResourceState,
    current: SimulationTick,
) -> Result<(), SurfaceResourceValidationError> {
    if state.next_resource_id == 0 {
        return Err(SurfaceResourceValidationError::ZeroNextResourceId);
    }
    if let Some(highest) = state.resources.keys().next_back().copied()
        && state.next_resource_id <= highest.value()
    {
        return Err(SurfaceResourceValidationError::NextIdNotAfterExisting {
            next: state.next_resource_id,
            highest,
        });
    }
    for (key, record) in &state.resources {
        validate_surface_resource_record(materials, *key, record, current)?;
    }
    Ok(())
}
