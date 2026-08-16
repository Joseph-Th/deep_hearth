//! Persistent finite geological deposits; sibling execution code owns every mutation path.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Mass, Temperature};
use crate::core::time::SimulationTick;
use crate::material::{
    CommodityKey, CompositionError, MaterialComposition, MaterialId, MaterialPhase,
    MaterialPhaseStateError, MaterialRegistry, ParticleSizeStatePolicy,
    validate_material_phase_state,
};
use crate::spatial::VoxelBounds;

/// Persistent identifier for one finite geological matter owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeologicalDepositId(u32);

impl GeologicalDepositId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "geological deposit id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Persistent lifecycle derived from whether extractable geological matter remains.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GeologicalDepositLifecycle {
    Available,
    Depleted,
}

/// Opaque world-generation authorization for one homogeneous finite deposit.
///
/// The type is public so a future geological generator can pass an authorized plan into the
/// canonical admission function, but production callers cannot construct one directly. This keeps
/// geological matter creation behind a physical world-generation owner rather than a general spawn
/// API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedDepositSpec {
    bounds: VoxelBounds,
    commodity: CommodityKey,
    mass: Mass,
    temperature: Temperature,
    composition: MaterialComposition,
}

impl GeneratedDepositSpec {
    /// Test-side stand-in for a future regional world-generation resolver.
    ///
    /// Production code deliberately has no constructor until a real geological generator can
    /// establish this source authorization without exposing arbitrary matter creation.
    #[cfg(test)]
    pub(crate) fn new(
        bounds: VoxelBounds,
        commodity: CommodityKey,
        mass: Mass,
        temperature: Temperature,
        composition: MaterialComposition,
    ) -> Result<Self, GeneratedDepositSpecError> {
        if mass.is_zero() {
            return Err(GeneratedDepositSpecError::ZeroMass);
        }
        composition
            .validate()
            .map_err(GeneratedDepositSpecError::InvalidComposition)?;
        if composition.parts_per_million(commodity.material()) == 0 {
            return Err(GeneratedDepositSpecError::MissingHostMaterial {
                host: commodity.material(),
            });
        }
        Ok(Self {
            bounds,
            commodity,
            mass,
            temperature,
            composition,
        })
    }

    #[must_use]
    pub(crate) const fn bounds(&self) -> VoxelBounds {
        self.bounds
    }

    #[must_use]
    pub(crate) const fn commodity(&self) -> CommodityKey {
        self.commodity
    }

    #[must_use]
    pub(crate) const fn mass(&self) -> Mass {
        self.mass
    }

    #[must_use]
    pub(crate) const fn temperature(&self) -> Temperature {
        self.temperature
    }

    #[must_use]
    pub(crate) const fn composition(&self) -> &MaterialComposition {
        &self.composition
    }
}

/// Invalid generated-deposit specification before registry resolution.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GeneratedDepositSpecError {
    ZeroMass,
    InvalidComposition(CompositionError),
    MissingHostMaterial { host: MaterialId },
}

#[cfg(test)]
impl Display for GeneratedDepositSpecError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroMass => {
                formatter.write_str("generated geological deposit mass must be nonzero")
            }
            Self::InvalidComposition(error) => {
                write!(
                    formatter,
                    "generated geological deposit has invalid composition: {error}"
                )
            }
            Self::MissingHostMaterial { host } => write!(
                formatter,
                "generated geological deposit composition omits host material {}",
                host.value()
            ),
        }
    }
}

#[cfg(test)]
impl Error for GeneratedDepositSpecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidComposition(error) => Some(error),
            Self::ZeroMass | Self::MissingHostMaterial { .. } => None,
        }
    }
}

/// One homogeneous finite geological matter owner in persistent world space.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeologicalDepositRecord {
    pub(super) id: GeologicalDepositId,
    pub(super) bounds: VoxelBounds,
    pub(super) commodity: CommodityKey,
    pub(super) initial_mass: Mass,
    pub(super) remaining_mass: Mass,
    pub(super) temperature: Temperature,
    pub(super) composition: MaterialComposition,
    pub(super) lifecycle: GeologicalDepositLifecycle,
    pub(super) generated_at: SimulationTick,
}

impl GeologicalDepositRecord {
    #[must_use]
    pub const fn commodity(&self) -> CommodityKey {
        self.commodity
    }

    #[must_use]
    pub const fn remaining_mass(&self) -> Mass {
        self.remaining_mass
    }

    #[must_use]
    pub const fn temperature(&self) -> Temperature {
        self.temperature
    }

    #[must_use]
    pub const fn composition(&self) -> &MaterialComposition {
        &self.composition
    }

    #[must_use]
    pub const fn lifecycle(&self) -> GeologicalDepositLifecycle {
        self.lifecycle
    }
}

/// Runtime owner for finite geological deposits and their generated identities.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeologyState {
    revision: u64,
    next_deposit_id: u32,
    deposits: BTreeMap<GeologicalDepositId, GeologicalDepositRecord>,
}

impl GeologyState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            next_deposit_id: 1,
            deposits: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub(super) const fn next_deposit_id(&self) -> u32 {
        self.next_deposit_id
    }

    #[must_use]
    pub fn get_deposit(&self, id: GeologicalDepositId) -> Option<&GeologicalDepositRecord> {
        self.deposits.get(&id)
    }

    /// Iterates deposits deterministically by persistent identity.
    pub fn deposits(&self) -> impl Iterator<Item = &GeologicalDepositRecord> {
        self.deposits.values()
    }

    pub(super) fn insert_deposit(
        &mut self,
        record: GeologicalDepositRecord,
        next_deposit_id: u32,
        next_revision: u64,
    ) {
        let replaced = self.deposits.insert(record.id, record);
        assert!(
            replaced.is_none(),
            "geological deposit ID allocation must be unique"
        );
        self.next_deposit_id = next_deposit_id;
        self.revision = next_revision;
    }

    pub(super) fn apply_extraction(
        &mut self,
        deposit: GeologicalDepositId,
        remaining_after: Mass,
        next_revision: u64,
    ) {
        let record = self.deposits.get_mut(&deposit).unwrap_or_else(|| {
            panic!("validated geological deposit disappeared without revision change")
        });
        record.remaining_mass = remaining_after;
        if remaining_after.is_zero() {
            record.lifecycle = GeologicalDepositLifecycle::Depleted;
        }
        self.revision = next_revision;
    }

    pub(crate) const fn has_valid_id_cursor(&self) -> bool {
        self.next_deposit_id != 0
    }
}

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
            Self::InvalidComposition { error, .. } => Some(error),
            Self::InvalidPhaseState { error, .. } => Some(error),
            Self::ZeroNextDepositId
            | Self::NextIdNotAfterExisting { .. }
            | Self::ZeroDepositId
            | Self::IdMismatch { .. }
            | Self::ZeroInitialMass { .. }
            | Self::RemainingMassExceedsInitial { .. }
            | Self::AvailableWithoutMass { .. }
            | Self::DepletedWithRemainingMass { .. }
            | Self::CompositionMissingHost { .. }
            | Self::UnknownCommodityMaterial { .. }
            | Self::UnknownCommodityForm { .. }
            | Self::UnsupportedCommodityPhase { .. }
            | Self::UnsupportedCommodityParticulateForm { .. }
            | Self::UnknownCompositionMaterial { .. }
            | Self::GeneratedInFuture { .. } => None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{FORM_CRUSHED, FORM_MOLTEN, FORM_ORE, MATERIAL_COPPER, build_registries};
    use crate::spatial::VoxelCoord;

    fn bounds() -> VoxelBounds {
        match VoxelBounds::new(VoxelCoord::new(0, -8, 0), VoxelCoord::new(4, -4, 4)) {
            Ok(bounds) => bounds,
            Err(error) => panic!("geology state bounds fixture failed: {error}"),
        }
    }

    #[test]
    fn loaded_validation_rejects_lifecycle_mass_disagreement() {
        let registries = build_registries();
        let deposit = GeologicalDepositId::new(1);
        let mut state = GeologyState::new();
        state.next_deposit_id = 2;
        state.deposits.insert(
            deposit,
            GeologicalDepositRecord {
                id: deposit,
                bounds: bounds(),
                commodity: CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
                initial_mass: Mass::from_milligrams(100),
                remaining_mass: Mass::from_milligrams(25),
                temperature: Temperature::from_millikelvin(300_000),
                composition: MaterialComposition::pure(MATERIAL_COPPER),
                lifecycle: GeologicalDepositLifecycle::Depleted,
                generated_at: SimulationTick::ZERO,
            },
        );

        assert_eq!(
            validate_loaded_geology(registries.materials(), &state, SimulationTick::ZERO),
            Err(GeologyValidationError::DepletedWithRemainingMass {
                deposit,
                remaining: Mass::from_milligrams(25),
            })
        );
    }

    #[test]
    fn loaded_validation_rejects_liquid_geological_deposit() {
        let registries = build_registries();
        let deposit = GeologicalDepositId::new(1);
        let mut state = GeologyState::new();
        state.next_deposit_id = 2;
        state.deposits.insert(
            deposit,
            GeologicalDepositRecord {
                id: deposit,
                bounds: bounds(),
                commodity: CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
                initial_mass: Mass::from_milligrams(100),
                remaining_mass: Mass::from_milligrams(100),
                temperature: Temperature::from_millikelvin(1_357_770),
                composition: MaterialComposition::pure(MATERIAL_COPPER),
                lifecycle: GeologicalDepositLifecycle::Available,
                generated_at: SimulationTick::ZERO,
            },
        );

        assert_eq!(
            validate_loaded_geology(registries.materials(), &state, SimulationTick::ZERO),
            Err(GeologyValidationError::UnsupportedCommodityPhase {
                deposit,
                form: FORM_MOLTEN,
                phase: MaterialPhase::Liquid,
            })
        );
    }

    #[test]
    fn loaded_validation_rejects_processed_particulate_geological_deposit() {
        let registries = build_registries();
        let deposit = GeologicalDepositId::new(1);
        let mut state = GeologyState::new();
        state.next_deposit_id = 2;
        state.deposits.insert(
            deposit,
            GeologicalDepositRecord {
                id: deposit,
                bounds: bounds(),
                commodity: CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
                initial_mass: Mass::from_milligrams(100),
                remaining_mass: Mass::from_milligrams(100),
                temperature: Temperature::from_millikelvin(300_000),
                composition: MaterialComposition::pure(MATERIAL_COPPER),
                lifecycle: GeologicalDepositLifecycle::Available,
                generated_at: SimulationTick::ZERO,
            },
        );

        assert_eq!(
            validate_loaded_geology(registries.materials(), &state, SimulationTick::ZERO),
            Err(
                GeologyValidationError::UnsupportedCommodityParticulateForm {
                    deposit,
                    form: FORM_CRUSHED,
                }
            )
        );
    }
}
