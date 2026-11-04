//! Local actor-facing gathering choices and freshness projections.

use crate::core::arithmetic::NORMALIZED_PARTS_PER_MILLION;
use crate::core::quantity::{Energy, Mass, Temperature};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::labor::SurfaceGatheringDefinition;
use crate::material::CommodityKey;
use crate::registry::Registries;
use crate::survival::{
    FoodCategory, FoodFreshness, FoodFreshnessError, SurvivalExertion,
    edible_surface_food_definition, freshness_from_history,
};

use super::super::{SurfaceResourceId, SurfaceResourceKind};
use super::{
    SurfaceGatheringError, hand_gathering_temperature_blocker, validate_gathering_player_observer,
};

/// Actor-facing option for gathering one currently visible loose natural opportunity.
///
/// The authored method identity remains private. Callers can compare the physical commitment and
/// choose a resource/amount without depending on internal method IDs; canonical gathering
/// validation still owns admission, capacity, survival, reservations, and completion.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerSurfaceGatheringOption {
    pub(super) expected_surface_revision: u64,
    pub(super) resource: SurfaceResourceId,
    source_mass: Mass,
    temperature: Temperature,
    pub(super) definition: SurfaceGatheringDefinition,
}

/// Food value of one exact-local, ordinary direct gathering batch, before accepting the work.
///
/// This is not a guarantee of carrying space, future freshness, or eating the food.
/// The present freshness is evaluated from the actual local source history. The separate
/// shelf-life baseline describes a newly gathered fresh batch under ambient storage.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerSurfaceFoodValue {
    batch_mass: Mass,
    dietary_energy: Energy,
    category: FoodCategory,
    hydration_multiplier_ppm: u32,
    fresh_ambient_shelf_life: TickSpan,
    current_freshness: FoodFreshness,
}

impl PlayerSurfaceFoodValue {
    #[must_use]
    pub const fn batch_mass(self) -> Mass {
        self.batch_mass
    }

    #[must_use]
    pub const fn dietary_energy(self) -> Energy {
        self.dietary_energy
    }

    #[must_use]
    pub const fn category(self) -> FoodCategory {
        self.category
    }

    #[must_use]
    pub const fn hydration_multiplier_ppm(self) -> u32 {
        self.hydration_multiplier_ppm
    }

    #[must_use]
    pub const fn fresh_ambient_shelf_life(self) -> TickSpan {
        self.fresh_ambient_shelf_life
    }

    #[must_use]
    pub const fn current_freshness(self) -> FoodFreshness {
        self.current_freshness
    }
}

impl PlayerSurfaceGatheringOption {
    /// Current physical temperature of the observed local source.
    #[must_use]
    pub const fn temperature(self) -> Temperature {
        self.temperature
    }

    /// An immediately knowable blocker for unprotected hand gathering.
    /// The authoritative work admission checks the same rule again.
    #[must_use]
    pub fn hand_handling_blocker(self) -> Option<SurfaceGatheringError> {
        hand_gathering_temperature_blocker(self.resource, self.temperature)
    }

    /// Compares edible value using only the current local source, authored food definition,
    /// and source's physical aging history. A stale option cannot silently describe old
    /// resource custody; inedible gathering options return `Ok(None)`.
    pub fn food_value(
        self,
        registries: &Registries,
        state: &AppState,
    ) -> Result<Option<PlayerSurfaceFoodValue>, SurfaceGatheringError> {
        validate_gathering_player_observer(state)?;
        let actual_surface_revision = state.surface().revision();
        if actual_surface_revision != self.expected_surface_revision {
            return Err(SurfaceGatheringError::StalePlayerOption {
                expected_surface_revision: self.expected_surface_revision,
                actual_surface_revision,
            });
        }
        let resource = state
            .available_surface_resources()
            .find(|resource| resource.id() == self.resource)
            .ok_or(SurfaceGatheringError::ResourceUnavailableAtPlayer {
                resource: self.resource,
            })?;
        let Some(food) = edible_surface_food_definition(registries, resource) else {
            return Ok(None);
        };
        let batch_mass = self.source_mass.min(self.definition.direct_batch_mass());
        let history = resource
            .ambient_storage_history_at(state.tick())
            .unwrap_or_else(|| {
                unreachable!("a currently visible source cannot predate its physical creation")
            });
        let current_freshness = freshness_from_history(
            history,
            NORMALIZED_PARTS_PER_MILLION,
            state.tick(),
            food.shelf_life(),
        )
        .map_err(|error| match error {
            FoodFreshnessError::ShelfLifeOverflow => SurfaceGatheringError::FoodAgeOverflow,
            FoodFreshnessError::UnknownLot { .. }
            | FoodFreshnessError::UnknownStockpile { .. }
            | FoodFreshnessError::NotEdible { .. } => {
                unreachable!("known local food cannot lose its food identity")
            }
        })?;
        Ok(Some(PlayerSurfaceFoodValue {
            batch_mass,
            dietary_energy: food.dietary_energy_for_mass(batch_mass),
            category: food.category(),
            hydration_multiplier_ppm: food.hydration_multiplier_ppm(),
            fresh_ambient_shelf_life: food.shelf_life(),
            current_freshness,
        }))
    }

    #[must_use]
    pub const fn resource(self) -> SurfaceResourceId {
        self.resource
    }

    #[must_use]
    pub const fn commodity(self) -> CommodityKey {
        self.definition.commodity()
    }

    #[must_use]
    pub const fn source_mass(self) -> Mass {
        self.source_mass
    }

    #[must_use]
    pub const fn maximum_batch_mass(self) -> Mass {
        self.definition.maximum_batch_mass()
    }

    #[must_use]
    pub const fn direct_batch_mass(self) -> Mass {
        self.definition.direct_batch_mass()
    }

    #[must_use]
    pub const fn exertion(self) -> SurvivalExertion {
        self.definition.exertion()
    }

    #[must_use]
    pub fn duration_for_mass(self, mass: Mass) -> Option<TickSpan> {
        self.definition.duration_for_mass(mass)
    }
}

/// Projects current exact-local natural gathering choices for the admitted player.
///
/// Player-created drops are excluded because their ordinary path is automatic pickup. A resource
/// with multiple authored gathering methods yields one option per method so callers can surface a
/// real technique choice or reject ambiguity rather than silently selecting an implementation ID.
pub fn player_surface_gathering_options(
    registries: &Registries,
    state: &AppState,
) -> Result<Vec<PlayerSurfaceGatheringOption>, SurfaceGatheringError> {
    validate_gathering_player_observer(state)?;
    let expected_surface_revision = state.surface().revision();
    let mut options = Vec::new();
    for resource in state
        .available_surface_resources()
        .filter(|resource| resource.kind() == SurfaceResourceKind::Opportunity)
    {
        for definition in registries
            .labor()
            .surface_gathering_for_commodity(resource.commodity())
            .copied()
        {
            options.push(PlayerSurfaceGatheringOption {
                expected_surface_revision,
                resource: resource.id(),
                source_mass: resource.remaining_mass(),
                temperature: resource.temperature(),
                definition,
            });
        }
    }
    Ok(options)
}
