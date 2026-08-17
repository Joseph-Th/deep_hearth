//! Persistent-state validation for geology; this child audits private owner data without exposing mutation.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::material::{
    CompositionError, MaterialId, MaterialPhase, MaterialPhaseStateError, MaterialRegistry,
    ParticleSizeStatePolicy, validate_material_phase_state,
};

use super::{GeologicalDepositId, GeologicalDepositLifecycle, GeologyState};

/// Persistent-state validation failure for geological matter ownership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GeologyValidationError {
    ZeroNextDepositId,
    NextIdNotAfterExisting {
        next: u32,
        highest: GeologicalDepositId,
    },
    ZeroDepositId,
    IdMismatch {
        key: GeologicalDepositId,
        record: GeologicalDepositId,
    },
    ZeroInitialMass {
        deposit: GeologicalDepositId,
    },
    RemainingMassExceedsInitial {
        deposit: GeologicalDepositId,
        initial: Mass,
        remaining: Mass,
    },
    AvailableWithoutMass {
        deposit: GeologicalDepositId,
    },
    DepletedWithRemainingMass {
        deposit: GeologicalDepositId,
        remaining: Mass,
    },
    InvalidComposition {
        deposit: GeologicalDepositId,
        error: CompositionError,
    },
    CompositionMissingHost {
        deposit: GeologicalDepositId,
        host: MaterialId,
    },
    UnknownCommodityMaterial {
        deposit: GeologicalDepositId,
        material: MaterialId,
    },
    UnknownCommodityForm {
        deposit: GeologicalDepositId,
        form: crate::material::FormId,
    },
    UnsupportedCommodityPhase {
        deposit: GeologicalDepositId,
        form: crate::material::FormId,
        phase: MaterialPhase,
    },
    UnsupportedCommodityParticulateForm {
        deposit: GeologicalDepositId,
        form: crate::material::FormId,
    },
    InvalidPhaseState {
        deposit: GeologicalDepositId,
        error: MaterialPhaseStateError,
    },
    UnknownCompositionMaterial {
        deposit: GeologicalDepositId,
        material: MaterialId,
    },
    GeneratedInFuture {
        deposit: GeologicalDepositId,
        generated_at: SimulationTick,
        current: SimulationTick,
    },
}

impl Display for GeologyValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroNextDepositId => {
                formatter.write_str("next geological deposit id must not be zero")
            }
            Self::NextIdNotAfterExisting { next, highest } => write!(
                formatter,
                "next geological deposit id {next} is not after existing id {}",
                highest.value()
            ),
            Self::ZeroDepositId => formatter.write_str("geological deposit id must not be zero"),
            Self::IdMismatch { key, record } => write!(
                formatter,
                "geological deposit map key {} disagrees with record id {}",
                key.value(),
                record.value()
            ),
            Self::ZeroInitialMass { deposit } => write!(
                formatter,
                "geological deposit {} has zero initial mass",
                deposit.value()
            ),
            Self::RemainingMassExceedsInitial {
                deposit,
                initial,
                remaining,
            } => write!(
                formatter,
                "geological deposit {} has {} mg remaining above initial {} mg",
                deposit.value(),
                remaining.milligrams(),
                initial.milligrams()
            ),
            Self::AvailableWithoutMass { deposit } => write!(
                formatter,
                "available geological deposit {} has no remaining mass",
                deposit.value()
            ),
            Self::DepletedWithRemainingMass { deposit, remaining } => write!(
                formatter,
                "depleted geological deposit {} still owns {} mg",
                deposit.value(),
                remaining.milligrams()
            ),
            Self::InvalidComposition { deposit, error } => write!(
                formatter,
                "geological deposit {} has invalid composition: {error}",
                deposit.value()
            ),
            Self::CompositionMissingHost { deposit, host } => write!(
                formatter,
                "geological deposit {} composition omits host material {}",
                deposit.value(),
                host.value()
            ),
            Self::UnknownCommodityMaterial { deposit, material } => write!(
                formatter,
                "geological deposit {} references unknown host material {}",
                deposit.value(),
                material.value()
            ),
            Self::UnknownCommodityForm { deposit, form } => write!(
                formatter,
                "geological deposit {} references unknown form {}",
                deposit.value(),
                form.value()
            ),
            Self::UnsupportedCommodityParticulateForm { deposit, form } => write!(
                formatter,
                "geological deposit {} uses particulate form {}; natural geological ownership does not carry processed particle-size state",
                deposit.value(),
                form.value()
            ),
            Self::UnsupportedCommodityPhase {
                deposit,
                form,
                phase,
            } => write!(
                formatter,
                "geological deposit {} uses {phase:?} form {}; finite geological deposits must be solid",
                deposit.value(),
                form.value()
            ),
            Self::InvalidPhaseState { deposit, error } => write!(
                formatter,
                "geological deposit {} has invalid material phase state: {error}",
                deposit.value()
            ),
            Self::UnknownCompositionMaterial { deposit, material } => write!(
                formatter,
                "geological deposit {} composition references unknown material {}",
                deposit.value(),
                material.value()
            ),
            Self::GeneratedInFuture {
                deposit,
                generated_at,
                current,
            } => write!(
                formatter,
                "geological deposit {} was generated at tick {} after current tick {}",
                deposit.value(),
                generated_at.value(),
                current.value()
            ),
        }
    }
}

impl Error for GeologyValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidComposition {
                deposit: _deposit,
                error,
            } => Some(error),
            Self::InvalidPhaseState {
                deposit: _deposit,
                error,
            } => Some(error),
            Self::NextIdNotAfterExisting {
                next: _next,
                highest: _highest,
            } => None,
            Self::IdMismatch {
                key: _key,
                record: _record,
            } => None,
            Self::ZeroInitialMass { deposit: _deposit }
            | Self::AvailableWithoutMass { deposit: _deposit } => None,
            Self::RemainingMassExceedsInitial {
                deposit: _deposit,
                initial: _initial,
                remaining: _remaining,
            } => None,
            Self::DepletedWithRemainingMass {
                deposit: _deposit,
                remaining: _remaining,
            } => None,
            Self::CompositionMissingHost {
                deposit: _deposit,
                host: _host,
            }
            | Self::UnknownCommodityMaterial {
                deposit: _deposit,
                material: _host,
            }
            | Self::UnknownCompositionMaterial {
                deposit: _deposit,
                material: _host,
            } => None,
            Self::UnknownCommodityForm {
                deposit: _deposit,
                form: _form,
            }
            | Self::UnsupportedCommodityParticulateForm {
                deposit: _deposit,
                form: _form,
            } => None,
            Self::UnsupportedCommodityPhase {
                deposit: _deposit,
                form: _form,
                phase: _phase,
            } => None,
            Self::GeneratedInFuture {
                deposit: _deposit,
                generated_at: _generated_at,
                current: _current,
            } => None,
            Self::ZeroNextDepositId | Self::ZeroDepositId => None,
        }
    }
}

pub(crate) fn validate_loaded_geology(
    materials: &MaterialRegistry,
    state: &GeologyState,
    current: SimulationTick,
) -> Result<(), GeologyValidationError> {
    if state.next_deposit_id == 0 {
        return Err(GeologyValidationError::ZeroNextDepositId);
    }
    if let Some(highest) = state.deposits.keys().next_back().copied()
        && state.next_deposit_id <= highest.value()
    {
        return Err(GeologyValidationError::NextIdNotAfterExisting {
            next: state.next_deposit_id,
            highest,
        });
    }

    for (key, record) in &state.deposits {
        if key.value() == 0 || record.id.value() == 0 {
            return Err(GeologyValidationError::ZeroDepositId);
        }
        if *key != record.id {
            return Err(GeologyValidationError::IdMismatch {
                key: *key,
                record: record.id,
            });
        }
        if record.initial_mass.is_zero() {
            return Err(GeologyValidationError::ZeroInitialMass { deposit: *key });
        }
        if record.remaining_mass > record.initial_mass {
            return Err(GeologyValidationError::RemainingMassExceedsInitial {
                deposit: *key,
                initial: record.initial_mass,
                remaining: record.remaining_mass,
            });
        }
        match record.lifecycle {
            GeologicalDepositLifecycle::Available if record.remaining_mass.is_zero() => {
                return Err(GeologyValidationError::AvailableWithoutMass { deposit: *key });
            }
            GeologicalDepositLifecycle::Depleted if !record.remaining_mass.is_zero() => {
                return Err(GeologyValidationError::DepletedWithRemainingMass {
                    deposit: *key,
                    remaining: record.remaining_mass,
                });
            }
            GeologicalDepositLifecycle::Available | GeologicalDepositLifecycle::Depleted => {}
        }
        record.composition.validate().map_err(|error| {
            GeologyValidationError::InvalidComposition {
                deposit: *key,
                error,
            }
        })?;
        if record
            .composition
            .parts_per_million(record.commodity.material())
            == 0
        {
            return Err(GeologyValidationError::CompositionMissingHost {
                deposit: *key,
                host: record.commodity.material(),
            });
        }
        if materials
            .get_material(record.commodity.material())
            .is_none()
        {
            return Err(GeologyValidationError::UnknownCommodityMaterial {
                deposit: *key,
                material: record.commodity.material(),
            });
        }
        let Some(form) = materials.get_form(record.commodity.form()) else {
            return Err(GeologyValidationError::UnknownCommodityForm {
                deposit: *key,
                form: record.commodity.form(),
            });
        };
        if form.phase() != MaterialPhase::Solid {
            return Err(GeologyValidationError::UnsupportedCommodityPhase {
                deposit: *key,
                form: record.commodity.form(),
                phase: form.phase(),
            });
        }
        if form.particle_size_policy() == ParticleSizeStatePolicy::Required {
            return Err(
                GeologyValidationError::UnsupportedCommodityParticulateForm {
                    deposit: *key,
                    form: record.commodity.form(),
                },
            );
        }
        for component in record.composition.components() {
            if materials.get_material(component.material()).is_none() {
                return Err(GeologyValidationError::UnknownCompositionMaterial {
                    deposit: *key,
                    material: component.material(),
                });
            }
        }
        validate_material_phase_state(
            materials,
            record.commodity,
            &record.composition,
            record.temperature,
        )
        .map_err(|error| GeologyValidationError::InvalidPhaseState {
            deposit: *key,
            error,
        })?;
        if record.generated_at > current {
            return Err(GeologyValidationError::GeneratedInFuture {
                deposit: *key,
                generated_at: record.generated_at,
                current,
            });
        }
    }

    Ok(())
}
