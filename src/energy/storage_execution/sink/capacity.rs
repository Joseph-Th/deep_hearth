//! Pure deferred-capacity projection shared by energy admission and trusted-load replay.

use crate::core::quantity::Energy;
use crate::core::time::TickSpan;
use crate::energy::definitions::EnergyStoreDefinitionId;
use crate::energy::passive_dissipation::project_stored_energy_after_passive_dissipation;
use crate::registry::Registries;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EnergySinkCapacityError {
    Overflow,
    Insufficient {
        stored: Energy,
        requested: Energy,
        capacity: Energy,
    },
}

/// Projects sink contents immediately before a deferred completion releases energy. Completion is
/// applied before passive loss on its due tick, so only the preceding `release_after - 1` ticks can
/// be credited as guaranteed recovery. This is conservative across suspension because extra wall
/// time can only create additional passive capacity.
pub(crate) fn project_energy_sink_stored_at_release(
    registries: &Registries,
    definition: EnergyStoreDefinitionId,
    stored: Energy,
    release_after: TickSpan,
) -> Energy {
    let definition = registries
        .energy()
        .get_store(definition)
        .unwrap_or_else(|| {
            panic!(
                "validated deferred energy sink references missing immutable definition {}",
                definition.value()
            )
        });
    let passive_ticks = TickSpan::new(release_after.value().saturating_sub(1));
    project_stored_energy_after_passive_dissipation(registries, definition, stored, passive_ticks)
}

/// Returns exact free capacity immediately before a deferred completion releases energy.
///
/// This shares the completion-before-passive-loss timing used by exact sink admission. The caller
/// must already have validated that `stored` belongs to `definition` and does not exceed capacity.
pub(crate) fn available_energy_sink_capacity_at_release(
    registries: &Registries,
    definition: EnergyStoreDefinitionId,
    stored: Energy,
    release_after: TickSpan,
) -> Energy {
    let capacity = registries
        .energy()
        .get_store(definition)
        .unwrap_or_else(|| {
            panic!(
                "validated deferred energy sink references missing immutable definition {}",
                definition.value()
            )
        })
        .capacity();
    let projected_stored =
        project_energy_sink_stored_at_release(registries, definition, stored, release_after);
    capacity.checked_sub(projected_stored).unwrap_or_else(|| {
        panic!("validated energy sink projected stored energy exceeds authored capacity")
    })
}

/// Validates exact energy against the capacity guaranteed to exist at its future release point.
/// Returns the projected pre-release stored energy for caller diagnostics.
pub(crate) fn validate_energy_sink_capacity_at_release(
    registries: &Registries,
    definition: EnergyStoreDefinitionId,
    stored: Energy,
    requested: Energy,
    release_after: TickSpan,
) -> Result<Energy, EnergySinkCapacityError> {
    let capacity = registries
        .energy()
        .get_store(definition)
        .unwrap_or_else(|| {
            panic!(
                "validated deferred energy sink references missing immutable definition {}",
                definition.value()
            )
        })
        .capacity();
    let projected_stored =
        project_energy_sink_stored_at_release(registries, definition, stored, release_after);
    let after = projected_stored
        .checked_add(requested)
        .ok_or(EnergySinkCapacityError::Overflow)?;
    if after > capacity {
        return Err(EnergySinkCapacityError::Insufficient {
            stored: projected_stored,
            requested,
            capacity,
        });
    }
    Ok(projected_stored)
}
