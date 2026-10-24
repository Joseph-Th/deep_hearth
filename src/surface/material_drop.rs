//! Direct transfer of carried material into exact finite loose surface custody.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    ConsumptionSelectionError, MaterialEgressError, ValidatedMaterialEgress, apply_material_egress,
    validate_consumption_selection, validate_material_egress_from_selection,
};
use crate::material::{CommodityKey, MaterialInputSpec};
use crate::player::{
    PlayerActionGuard, PlayerActionValidationError, StalePlayerSurvival, validate_player_action,
};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::{
    SurfaceResourceCreationCommitError, SurfaceResourceCreationError, SurfaceResourceId,
    SurfaceResourceKind, SurfaceResourceSpec, ValidatedSurfaceResourceCreation,
    validate_create_surface_resources,
};

/// Two seconds at the authoritative 20 Hz control rate, preventing immediate self-pickup.
pub const PLAYER_MATERIAL_DROP_PICKUP_DELAY_CONTROL_TICKS: u64 = 40;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerMaterialDropError {
    PlayerNotAdmitted,
    PlayerDead,
    PlayerBusy,
    NoCarriedMaterial { commodity: CommodityKey },
    InventoryRevisionExhausted,
    SurfaceRevisionExhausted,
    SurfaceResourceIdExhausted,
    MaterialMassOverflow,
    PickupScheduleOverflow,
}

impl Display for PlayerMaterialDropError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => {
                formatter.write_str("material drop requires an admitted player")
            }
            Self::PlayerDead => formatter.write_str("dead player cannot drop carried material"),
            Self::PlayerBusy => {
                formatter.write_str("player cannot drop carried material while work is active")
            }
            Self::NoCarriedMaterial { commodity } => write!(
                formatter,
                "carried inventory contains no material {} form {} to drop",
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted for material drop")
            }
            Self::SurfaceRevisionExhausted => {
                formatter.write_str("surface revision space is exhausted for material drop")
            }
            Self::SurfaceResourceIdExhausted => formatter
                .write_str("surface resource identifier space is exhausted for material drop"),
            Self::MaterialMassOverflow => formatter
                .write_str("selected carried material mass overflows material-drop accounting"),
            Self::PickupScheduleOverflow => {
                formatter.write_str("material-drop pickup delay exceeds authoritative control time")
            }
        }
    }
}

impl Error for PlayerMaterialDropError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerMaterialDropCommitError {
    PlayerBecameBusy,
    StaleSurvival {
        expected: u64,
        actual: u64,
    },
    StaleLogistics {
        expected: u64,
        actual: u64,
    },
    PlayerMoved {
        expected: VoxelCoord,
        actual: Option<VoxelCoord>,
    },
    StaleInventory {
        expected: u64,
        actual: u64,
    },
    TickChanged {
        expected: crate::core::time::SimulationTick,
        actual: crate::core::time::SimulationTick,
    },
    SurfaceRevisionChanged {
        expected: u64,
        actual: u64,
    },
    WorldRevisionChanged {
        expected: u64,
        actual: u64,
    },
    StructureRevisionChanged {
        expected: u64,
        actual: u64,
    },
    SurfaceIdentityChanged {
        expected: u32,
        actual: u32,
    },
}

impl Display for PlayerMaterialDropCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerBecameBusy => {
                formatter.write_str("player work started before material drop commit")
            }
            Self::StaleSurvival { expected, actual } => write!(
                formatter,
                "player survival revision changed from {expected} to {actual} before material drop"
            ),
            Self::StaleLogistics { expected, actual } => write!(
                formatter,
                "logistics revision changed from {expected} to {actual} before material drop"
            ),
            Self::PlayerMoved {
                expected,
                actual: Some(actual),
            } => write!(
                formatter,
                "player moved from ({},{},{}) to ({},{},{}) before material drop",
                expected.x(),
                expected.y(),
                expected.z(),
                actual.x(),
                actual.y(),
                actual.z()
            ),
            Self::PlayerMoved {
                expected,
                actual: None,
            } => write!(
                formatter,
                "player disappeared from ({},{},{}) before material drop",
                expected.x(),
                expected.y(),
                expected.z()
            ),
            Self::StaleInventory { expected, actual } => write!(
                formatter,
                "inventory revision changed from {expected} to {actual} before material drop"
            ),
            Self::TickChanged { expected, actual } => write!(
                formatter,
                "simulation tick changed from {} to {} before material drop",
                expected.value(),
                actual.value()
            ),
            Self::SurfaceRevisionChanged { expected, actual } => write!(
                formatter,
                "surface revision changed from {expected} to {actual} before material drop"
            ),
            Self::WorldRevisionChanged { expected, actual } => write!(
                formatter,
                "world revision changed from {expected} to {actual} before material drop"
            ),
            Self::StructureRevisionChanged { expected, actual } => write!(
                formatter,
                "structural revision changed from {expected} to {actual} before material drop"
            ),
            Self::SurfaceIdentityChanged { expected, actual } => write!(
                formatter,
                "surface resource identity cursor changed from {expected} to {actual} before material drop"
            ),
        }
    }
}

impl Error for PlayerMaterialDropCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerMaterialDropOutcome {
    commodity: CommodityKey,
    dropped_mass: Mass,
    position: VoxelCoord,
    resources: Vec<SurfaceResourceId>,
}

impl PlayerMaterialDropOutcome {
    #[must_use]
    pub const fn commodity(&self) -> CommodityKey {
        self.commodity
    }

    #[must_use]
    pub const fn dropped_mass(&self) -> Mass {
        self.dropped_mass
    }

    #[must_use]
    pub const fn position(&self) -> VoxelCoord {
        self.position
    }

    pub fn resources(&self) -> &[SurfaceResourceId] {
        &self.resources
    }
}

#[must_use]
pub struct ValidatedPlayerMaterialDrop {
    player_action: PlayerActionGuard,
    expected_logistics_revision: u64,
    expected_position: VoxelCoord,
    expected_inventory_revision: u64,
    commodity: CommodityKey,
    dropped_mass: Mass,
    egress: ValidatedMaterialEgress,
    surface: ValidatedSurfaceResourceCreation,
}

impl ValidatedPlayerMaterialDrop {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<PlayerMaterialDropOutcome, PlayerMaterialDropCommitError> {
        self.player_action.precheck(state).map_err(
            |StalePlayerSurvival { expected, actual }| {
                PlayerMaterialDropCommitError::StaleSurvival { expected, actual }
            },
        )?;
        if state.logistics().revision() != self.expected_logistics_revision {
            return Err(PlayerMaterialDropCommitError::StaleLogistics {
                expected: self.expected_logistics_revision,
                actual: state.logistics().revision(),
            });
        }
        let actual_position = state.logistics().player().map(|player| player.position());
        if actual_position != Some(self.expected_position) {
            return Err(PlayerMaterialDropCommitError::PlayerMoved {
                expected: self.expected_position,
                actual: actual_position,
            });
        }
        if state.player_work().active().is_some() {
            return Err(PlayerMaterialDropCommitError::PlayerBecameBusy);
        }
        if state.inventory().revision() != self.expected_inventory_revision {
            return Err(PlayerMaterialDropCommitError::StaleInventory {
                expected: self.expected_inventory_revision,
                actual: state.inventory().revision(),
            });
        }
        self.egress.assert_matches_state(state.inventory());
        self.surface.precheck(state).map_err(|error| match error {
            SurfaceResourceCreationCommitError::Tick { expected, actual } => {
                PlayerMaterialDropCommitError::TickChanged { expected, actual }
            }
            SurfaceResourceCreationCommitError::Revision { expected, actual } => {
                PlayerMaterialDropCommitError::SurfaceRevisionChanged { expected, actual }
            }
            SurfaceResourceCreationCommitError::WorldRevision { expected, actual } => {
                PlayerMaterialDropCommitError::WorldRevisionChanged { expected, actual }
            }
            SurfaceResourceCreationCommitError::StructureRevision { expected, actual } => {
                PlayerMaterialDropCommitError::StructureRevisionChanged { expected, actual }
            }
            SurfaceResourceCreationCommitError::NextResourceId { expected, actual } => {
                PlayerMaterialDropCommitError::SurfaceIdentityChanged { expected, actual }
            }
        })?;
        let resources = self.surface.resource_ids().collect::<Vec<_>>();
        apply_material_egress(state.inventory_state_mut(), self.egress);
        self.surface.apply(state);
        Ok(PlayerMaterialDropOutcome {
            commodity: self.commodity,
            dropped_mass: self.dropped_mass,
            position: self.expected_position,
            resources,
        })
    }
}

pub fn validate_drop_player_material(
    registries: &Registries,
    state: &AppState,
    commodity: CommodityKey,
) -> Result<ValidatedPlayerMaterialDrop, PlayerMaterialDropError> {
    let player_action = validate_player_action(state).map_err(|error| match error {
        PlayerActionValidationError::PlayerNotAdmitted => {
            PlayerMaterialDropError::PlayerNotAdmitted
        }
        PlayerActionValidationError::PlayerDead => PlayerMaterialDropError::PlayerDead,
    })?;
    if state.player_work().active().is_some() {
        return Err(PlayerMaterialDropError::PlayerBusy);
    }
    let player = state
        .logistics()
        .player()
        .copied()
        .ok_or(PlayerMaterialDropError::PlayerNotAdmitted)?;
    let source = player.carried_stockpile();
    let source_record = state
        .inventory()
        .get_stockpile(source)
        .unwrap_or_else(|| unreachable!("admitted player carried stockpile exists"));
    let dropped_mass = source_record.get_mass(commodity);
    if dropped_mass.is_zero() {
        return Err(PlayerMaterialDropError::NoCarriedMaterial { commodity });
    }
    if !state.can_spend_inventory_revisions(1) {
        return Err(PlayerMaterialDropError::InventoryRevisionExhausted);
    }
    if !state.can_spend_surface_revisions(1) {
        return Err(PlayerMaterialDropError::SurfaceRevisionExhausted);
    }
    let selection = validate_consumption_selection(
        state.inventory(),
        source,
        &[MaterialInputSpec::new(commodity, dropped_mass)],
    )
    .map_err(|error| match error {
        ConsumptionSelectionError::MassOverflow { .. } => {
            PlayerMaterialDropError::MaterialMassOverflow
        }
        ConsumptionSelectionError::UnknownStockpile { .. }
        | ConsumptionSelectionError::InsufficientMass { .. } => {
            unreachable!("carried commodity total was read from the same inventory snapshot")
        }
    })?;
    let lot_selections = selection.lot_selections().collect::<Vec<_>>();
    let resource_count = u32::try_from(lot_selections.len())
        .map_err(|_| PlayerMaterialDropError::SurfaceResourceIdExhausted)?;
    if !state.has_surface_resource_id_headroom(resource_count) {
        return Err(PlayerMaterialDropError::SurfaceResourceIdExhausted);
    }
    let source_preservation = source_record
        .storage_profile()
        .preservation_multiplier_ppm();
    let pickup_delay_started_at = state.control_tick();
    pickup_delay_started_at
        .checked_add_steps(PLAYER_MATERIAL_DROP_PICKUP_DELAY_CONTROL_TICKS)
        .ok_or(PlayerMaterialDropError::PickupScheduleOverflow)?;
    let specs = lot_selections
        .iter()
        .map(|selection| {
            let lot = state
                .inventory()
                .get_lot(selection.lot())
                .unwrap_or_else(|| unreachable!("validated material selection lot exists"));
            let ambient_age_parts = lot
                .storage_history()
                .project(state.tick(), source_preservation)
                .unwrap_or_else(|| {
                    unreachable!("trusted inventory storage history projects at current tick")
                });
            SurfaceResourceSpec::from_existing_material(
                SurfaceResourceKind::Drop,
                player.position(),
                selection.mass(),
                lot.profile().clone(),
                lot.provenance(),
                ambient_age_parts,
            )
            .map(|spec| spec.with_automatic_pickup_delay_started_at(pickup_delay_started_at))
        })
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| {
            unreachable!("trusted inventory lot cannot form an invalid surface parcel: {error}")
        });
    let surface =
        validate_create_surface_resources(registries, state, specs).map_err(
            |error| match error {
                SurfaceResourceCreationError::IdExhausted => {
                    PlayerMaterialDropError::SurfaceResourceIdExhausted
                }
                SurfaceResourceCreationError::RevisionExhausted => {
                    PlayerMaterialDropError::SurfaceRevisionExhausted
                }
                SurfaceResourceCreationError::Empty
                | SurfaceResourceCreationError::InvalidResource(_) => {
                    unreachable!("trusted nonempty carried material produces valid surface custody")
                }
            },
        )?;
    let egress =
        validate_material_egress_from_selection(state.inventory(), selection).map_err(|error| {
            match error {
                MaterialEgressError::RevisionExhausted => {
                    PlayerMaterialDropError::InventoryRevisionExhausted
                }
                MaterialEgressError::StaleSelection { .. } => {
                    unreachable!("material drop selection is immediate")
                }
            }
        })?;
    Ok(ValidatedPlayerMaterialDrop {
        player_action,
        expected_logistics_revision: state.logistics().revision(),
        expected_position: player.position(),
        expected_inventory_revision: state.inventory().revision(),
        commodity,
        dropped_mass,
        egress,
        surface,
    })
}
