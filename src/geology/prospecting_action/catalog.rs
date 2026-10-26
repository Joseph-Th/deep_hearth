//! Actor-safe prospecting technique and local-instrument projection.

use crate::core::quantity::{Mass, Pressure};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::equipment::{EquipmentDefinitionId, EquipmentId, equipment_occupancy};
use crate::labor::{
    ProspectingMethodId, ProspectingSpatialResolution, ProspectingWorkProjection,
    ProspectingWorkProjectionError, project_prospecting_work,
};
use crate::maintenance::{Condition, calculate_usable_condition_after_active_ticks};
use crate::registry::Registries;
use crate::spatial::VoxelBounds;

use super::super::GeologicalEvidenceKind;

/// One currently usable local instrument for a projected prospecting technique.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerProspectingInstrumentOption {
    equipment: EquipmentId,
    definition: EquipmentDefinitionId,
    condition: Condition,
    condition_after: Condition,
}

impl PlayerProspectingInstrumentOption {
    #[must_use]
    pub const fn equipment(self) -> EquipmentId {
        self.equipment
    }

    #[must_use]
    pub const fn definition(self) -> EquipmentDefinitionId {
        self.definition
    }

    #[must_use]
    pub const fn condition(self) -> Condition {
        self.condition
    }

    #[must_use]
    pub const fn condition_after(self) -> Condition {
        self.condition_after
    }
}

/// Player-facing read model for one authored prospecting technique over a chosen region.
///
/// This projection exposes only authored technique properties, physiological work cost, and
/// instruments already visible at the admitted player's exact location. It is not authorization;
/// work start still revalidates locality, survival, attention, occupancy, revisions, and identity
/// headroom through the canonical prospecting admission path.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerProspectingOption {
    method: ProspectingMethodId,
    evidence: GeologicalEvidenceKind,
    spatial_resolution: ProspectingSpatialResolution,
    maximum_region_voxels: u128,
    abundance_uncertainty_ppm: u32,
    excavation_hardness_resolution: Option<Pressure>,
    resource_mass_resolution: Option<Mass>,
    work: ProspectingWorkProjection,
    accepted_instrument_definitions: Vec<EquipmentDefinitionId>,
    instruments: Vec<PlayerProspectingInstrumentOption>,
}

impl PlayerProspectingOption {
    #[must_use]
    pub const fn method(&self) -> ProspectingMethodId {
        self.method
    }

    #[must_use]
    pub const fn evidence(&self) -> GeologicalEvidenceKind {
        self.evidence
    }

    #[must_use]
    pub const fn spatial_resolution(&self) -> ProspectingSpatialResolution {
        self.spatial_resolution
    }

    #[must_use]
    pub const fn maximum_region_voxels(&self) -> u128 {
        self.maximum_region_voxels
    }

    #[must_use]
    pub const fn abundance_uncertainty_ppm(&self) -> u32 {
        self.abundance_uncertainty_ppm
    }

    #[must_use]
    pub const fn excavation_hardness_resolution(&self) -> Option<Pressure> {
        self.excavation_hardness_resolution
    }

    #[must_use]
    pub const fn resource_mass_resolution(&self) -> Option<Mass> {
        self.resource_mass_resolution
    }

    pub const fn work(&self) -> ProspectingWorkProjection {
        self.work
    }

    #[must_use]
    pub const fn duration(&self) -> TickSpan {
        self.work.duration()
    }

    #[must_use]
    pub const fn requires_equipment(&self) -> bool {
        self.work.requires_equipment()
    }

    pub fn accepted_instrument_definitions(&self) -> &[EquipmentDefinitionId] {
        &self.accepted_instrument_definitions
    }

    pub fn instruments(&self) -> &[PlayerProspectingInstrumentOption] {
        &self.instruments
    }
}

/// Projects every authored prospecting technique whose footprint can cover `region`.
///
/// Instrument lists contain only exact-local, unmounted, idle equipment that matches the authored
/// method and has enough current condition to survive the complete survey. A required-instrument
/// technique remains visible with an empty instrument list, allowing the player to see a useful
/// technique before owning its capital rather than hiding the option entirely.
pub fn player_prospecting_options(
    registries: &Registries,
    state: &AppState,
    region: VoxelBounds,
) -> Result<Vec<PlayerProspectingOption>, ProspectingWorkProjectionError> {
    let mut options = Vec::new();
    for definition in registries.labor().prospecting_definitions().copied() {
        let work = match project_prospecting_work(registries, definition.id(), region) {
            Ok(work) => work,
            Err(ProspectingWorkProjectionError::RegionTooLarge { .. }) => continue,
            Err(error) => return Err(error),
        };
        let accepted_instrument_definitions =
            definition.equipment().map_or_else(Vec::new, |profile| {
                let mut definitions = vec![profile.primary()];
                if let Some(alternative) = profile.alternative() {
                    definitions.push(alternative);
                }
                definitions
            });
        let instruments = definition.equipment().map_or_else(Vec::new, |profile| {
            state
                .available_local_equipment()
                .filter(|record| record.supported_by().is_none())
                .filter(|record| equipment_occupancy(state, record.id()).is_none())
                .filter_map(|record| {
                    let wear = profile.condition_wear_ppm_per_active_tick(record.definition())?;
                    let condition_after = calculate_usable_condition_after_active_ticks(
                        wear,
                        record.condition(),
                        definition.duration(),
                    )
                    .ok()?;
                    Some(PlayerProspectingInstrumentOption {
                        equipment: record.id(),
                        definition: record.definition(),
                        condition: record.condition(),
                        condition_after,
                    })
                })
                .collect()
        });
        options.push(PlayerProspectingOption {
            method: definition.id(),
            evidence: definition.evidence(),
            spatial_resolution: definition.spatial_resolution(),
            maximum_region_voxels: definition.maximum_region_voxels(),
            abundance_uncertainty_ppm: definition.abundance_uncertainty_ppm(),
            excavation_hardness_resolution: definition.excavation_hardness_resolution(),
            resource_mass_resolution: definition.resource_mass_resolution(),
            work,
            accepted_instrument_definitions,
            instruments,
        });
    }
    Ok(options)
}
