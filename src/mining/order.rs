//! Bounded read-only projection of caller-selected mining effort, not extraction authorization.

use std::error::Error;
use std::fmt::{Display, Formatter};

use super::MiningMethodDefinition;
use super::physics::{MiningPhysicsError, resolve_mining_physics};
use crate::core::quantity::{Mass, Pressure};
use crate::core::time::{PhysicalTickDuration, TickSpan};
use crate::equipment::{EquipmentDefinition, EquipmentDefinitionId};
use crate::maintenance::Condition;

/// Explicit planning inputs; hardness is the conservative upper bound of acquired evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MiningOrderRequest {
    condition_before: Condition,
    acquired_hardness: Pressure,
    requested_mass: Mass,
    batch_mass: Mass,
    max_batches: u64,
}

impl MiningOrderRequest {
    #[must_use]
    pub const fn new(
        condition_before: Condition,
        acquired_hardness: Pressure,
        requested_mass: Mass,
        batch_mass: Mass,
        max_batches: u64,
    ) -> Self {
        Self {
            condition_before,
            acquired_hardness,
            requested_mass,
            batch_mass,
            max_batches,
        }
    }
}

/// Complete active-effort projection assuming uninterrupted batches and no service or upgrades.
///
/// This is not a reservation or authorization token. It neither observes nor promises deposit
/// supply, destination capacity, player availability, survival, or evidence freshness. Recompute
/// after changing any input; actual batches still require canonical mining admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MiningOrderResolution {
    duration: TickSpan,
    condition_after: Condition,
    batches: u64,
}

impl MiningOrderResolution {
    #[must_use]
    pub const fn duration(self) -> TickSpan {
        self.duration
    }

    #[must_use]
    pub const fn condition_after(self) -> Condition {
        self.condition_after
    }

    #[must_use]
    pub const fn batches(self) -> u64 {
        self.batches
    }
}

/// An invalid effort request, bounded-work rejection, or failure of sequential mining physics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MiningOrderError {
    ZeroRequestedMass,
    ZeroBatchMass,
    EquipmentRequiresStructuralSupport {
        equipment: EquipmentDefinitionId,
    },
    BatchLimitExceeded {
        required: u64,
        maximum: u64,
    },
    DurationOverflow,
    /// `batch` is the one-based batch that could not be resolved.
    Physics {
        batch: u64,
        error: MiningPhysicsError,
    },
}

impl Display for MiningOrderError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroRequestedMass => formatter.write_str("mining order mass must be nonzero"),
            Self::ZeroBatchMass => formatter.write_str("mining order batch mass must be nonzero"),
            Self::EquipmentRequiresStructuralSupport { equipment } => write!(
                formatter,
                "mining equipment definition {} requires structural installation and cannot be used for direct extraction",
                equipment.value()
            ),
            Self::BatchLimitExceeded { required, maximum } => write!(
                formatter,
                "mining order requires {required} batches, exceeding projection bound {maximum}"
            ),
            Self::DurationOverflow => {
                formatter.write_str("mining order duration exceeds tick range")
            }
            Self::Physics { batch, error } => {
                write!(formatter, "mining order batch {batch}: {error}")
            }
        }
    }
}

impl Error for MiningOrderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Physics { error, .. } => Some(error),
            Self::ZeroRequestedMass
            | Self::ZeroBatchMass
            | Self::EquipmentRequiresStructuralSupport { .. }
            | Self::BatchLimitExceeded { .. }
            | Self::DurationOverflow => None,
        }
    }
}

/// Projects exact requested effort as full caller-selected batches followed by a nonzero remainder.
///
/// Rejects zero order/batch mass. Checks the batch count against `max_batches` before resolving any
/// physics (a zero bound therefore rejects every nonzero order). Each batch uses canonical mining
/// physics at the preceding batch's resulting condition, including tick rounding and lifetime
/// limits. The selected batch is not silently reduced to fit equipment capacity. A short order
/// resolves only its requested mass, not an unused full batch. Work is O(required batches), bounded
/// by the caller, with constant memory. No state or hidden geology is read or mutated.
pub fn resolve_mining_order(
    physical_tick_duration: PhysicalTickDuration,
    method: &MiningMethodDefinition,
    equipment: &EquipmentDefinition,
    request: MiningOrderRequest,
) -> Result<MiningOrderResolution, MiningOrderError> {
    if request.requested_mass.is_zero() {
        return Err(MiningOrderError::ZeroRequestedMass);
    }
    if request.batch_mass.is_zero() {
        return Err(MiningOrderError::ZeroBatchMass);
    }
    if equipment.requires_structural_support() {
        return Err(MiningOrderError::EquipmentRequiresStructuralSupport {
            equipment: equipment.id(),
        });
    }
    let batches = request
        .requested_mass
        .milligrams()
        .div_ceil(request.batch_mass.milligrams());
    if batches > request.max_batches {
        return Err(MiningOrderError::BatchLimitExceeded {
            required: batches,
            maximum: request.max_batches,
        });
    }
    let mut remaining = request.requested_mass.milligrams();
    let mut ticks = 0_u64;
    let mut condition = request.condition_before;
    for batch in 1..=batches {
        let mass = remaining.min(request.batch_mass.milligrams());
        let physics = resolve_mining_physics(
            physical_tick_duration,
            method,
            equipment,
            condition,
            request.acquired_hardness,
            Mass::from_milligrams(mass),
        )
        .map_err(|error| MiningOrderError::Physics { batch, error })?;
        ticks = ticks
            .checked_add(physics.duration().value())
            .ok_or(MiningOrderError::DurationOverflow)?;
        condition = physics.condition_after();
        // `mass` is bounded by the remaining effort, so this subtraction cannot underflow.
        remaining -= mass;
    }
    Ok(MiningOrderResolution {
        duration: TickSpan::new(ticks),
        condition_after: condition,
        batches,
    })
}

#[cfg(test)]
#[path = "order_tests.rs"]
mod tests;
