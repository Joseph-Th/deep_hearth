//! Timed recovery of one simple grounded structural member into player-carried custody.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::inventory::{
    InboundReservationReleaseError, MaterialIngressEntry, MaterialIngressError, MaterialLotId,
    ValidatedInboundReservation, ValidatedInboundReservationRelease, ValidatedMaterialIngress,
    apply_material_ingress, validate_inbound_reservation, validate_inbound_reservation_release,
    validate_material_ingress, validate_reserved_material_ingress,
};
use crate::labor::{
    PlayerWork, PlayerWorkCommitError, PlayerWorkStartError, StructuralConstructionMethodId,
    StructuralDismantlingWork, ValidatedPlayerWorkStart, validate_player_work_start,
};
use crate::logistics::validate_player_stockpile_access;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::{StructuralElementId, StructuralLifecycle, StructuralLoadKind, bounds_are_local};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerStructuralDismantlingError {
    PlayerNotAdmitted,
    UnknownElement {
        element: StructuralElementId,
    },
    ElementNotGrounded {
        element: StructuralElementId,
    },
    ElementNotRecoverable {
        element: StructuralElementId,
    },
    ElementRemote {
        element: StructuralElementId,
    },
    ElementHasDependents {
        element: StructuralElementId,
    },
    ElementHasMemberSupports {
        element: StructuralElementId,
    },
    ElementHasExternalLoad {
        element: StructuralElementId,
    },
    ElementSupportsEquipment {
        element: StructuralElementId,
    },
    ElementSupportsStockpile {
        element: StructuralElementId,
    },
    ElementSupportsFluidStore {
        element: StructuralElementId,
    },
    UnknownMethod {
        method: StructuralConstructionMethodId,
    },
    MemberTooHeavy {
        required: Mass,
        maximum: Mass,
    },
    CompletionTickOverflow,
    RecoveryAccess(crate::logistics::PlayerStockpileAccessError),
    RecoveryIngress,
    RecoveryReservation,
    MaterialLotIdExhausted,
    InventoryRevisionExhausted,
    StructureRevisionExhausted,
    Work(PlayerWorkStartError),
}

impl Display for PlayerStructuralDismantlingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "structural dismantling admission failed: {self:?}"
        )
    }
}

impl Error for PlayerStructuralDismantlingError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerStructuralDismantlingCommitError {
    StaleStructureRevision {
        expected: u64,
        actual: u64,
    },
    StaleLogisticsRevision {
        expected: u64,
        actual: u64,
    },
    PlayerMoved {
        expected: VoxelCoord,
        actual: Option<VoxelCoord>,
    },
    ElementChanged {
        element: StructuralElementId,
    },
    Work(PlayerWorkCommitError),
}

impl Display for PlayerStructuralDismantlingCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "structural dismantling commit failed: {self:?}")
    }
}

impl Error for PlayerStructuralDismantlingCommitError {}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructuralDismantlingStartOutcome {
    element: StructuralElementId,
    recovery_destination: crate::inventory::StockpileId,
    recovered_mass: Mass,
    completes_at: SimulationTick,
}

impl StructuralDismantlingStartOutcome {
    #[must_use]
    pub const fn element(self) -> StructuralElementId {
        self.element
    }
    #[must_use]
    pub const fn recovery_destination(self) -> crate::inventory::StockpileId {
        self.recovery_destination
    }
    #[must_use]
    pub const fn recovered_mass(self) -> Mass {
        self.recovered_mass
    }
    #[must_use]
    pub const fn completes_at(self) -> SimulationTick {
        self.completes_at
    }
}

#[must_use]
pub struct ValidatedPlayerStructuralDismantlingStart {
    expected_structure_revision: u64,
    expected_logistics_revision: u64,
    expected_player_position: VoxelCoord,
    element: StructuralElementId,
    expected_mass: Mass,
    reservation: ValidatedInboundReservation,
    work_start: ValidatedPlayerWorkStart,
    outcome: StructuralDismantlingStartOutcome,
}

impl ValidatedPlayerStructuralDismantlingStart {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<StructuralDismantlingStartOutcome, PlayerStructuralDismantlingCommitError> {
        if state.structures().revision() != self.expected_structure_revision {
            return Err(
                PlayerStructuralDismantlingCommitError::StaleStructureRevision {
                    expected: self.expected_structure_revision,
                    actual: state.structures().revision(),
                },
            );
        }
        if state.logistics().revision() != self.expected_logistics_revision {
            return Err(
                PlayerStructuralDismantlingCommitError::StaleLogisticsRevision {
                    expected: self.expected_logistics_revision,
                    actual: state.logistics().revision(),
                },
            );
        }
        let actual_position = state.logistics().player().map(|player| player.position());
        if actual_position != Some(self.expected_player_position) {
            return Err(PlayerStructuralDismantlingCommitError::PlayerMoved {
                expected: self.expected_player_position,
                actual: actual_position,
            });
        }
        if !state
            .structures()
            .get_element(self.element)
            .is_some_and(|record| record.embodied_mass() == self.expected_mass)
        {
            return Err(PlayerStructuralDismantlingCommitError::ElementChanged {
                element: self.element,
            });
        }
        self.reservation.assert_matches_state(state.inventory());
        self.work_start
            .precheck(state)
            .map_err(PlayerStructuralDismantlingCommitError::Work)?;
        self.reservation.apply(state.inventory_state_mut());
        self.work_start.apply(state);
        Ok(self.outcome)
    }
}

fn target_record<'a>(
    state: &'a AppState,
    element: StructuralElementId,
) -> Result<&'a super::StructuralElementRecord, PlayerStructuralDismantlingError> {
    let record = state
        .structures()
        .get_element(element)
        .ok_or(PlayerStructuralDismantlingError::UnknownElement { element })?;
    if !record.is_grounded() {
        return Err(PlayerStructuralDismantlingError::ElementNotGrounded { element });
    }
    if record.embodied_mass().is_zero()
        || !matches!(
            record.lifecycle(),
            StructuralLifecycle::Active | StructuralLifecycle::Failed
        )
    {
        return Err(PlayerStructuralDismantlingError::ElementNotRecoverable { element });
    }
    if state
        .structures()
        .supports(element)
        .is_some_and(|mut supports| supports.next().is_some())
    {
        return Err(PlayerStructuralDismantlingError::ElementHasMemberSupports { element });
    }
    if state.structures().elements().any(|candidate| {
        candidate.id() != element
            && state
                .structures()
                .supports(candidate.id())
                .is_some_and(|supports| supports.into_iter().any(|support| support == element))
    }) {
        return Err(PlayerStructuralDismantlingError::ElementHasDependents { element });
    }
    if record
        .loads()
        .any(|(kind, load)| kind != StructuralLoadKind::SelfWeight && !load.is_zero())
    {
        return Err(PlayerStructuralDismantlingError::ElementHasExternalLoad { element });
    }
    if state
        .equipment()
        .supported_equipment(element)
        .next()
        .is_some()
    {
        return Err(PlayerStructuralDismantlingError::ElementSupportsEquipment { element });
    }
    if state
        .inventory()
        .supported_stockpiles(element)
        .next()
        .is_some()
    {
        return Err(PlayerStructuralDismantlingError::ElementSupportsStockpile { element });
    }
    if state.fluid().supported_stores(element).next().is_some() {
        return Err(PlayerStructuralDismantlingError::ElementSupportsFluidStore { element });
    }
    Ok(record)
}

pub fn validate_start_player_structural_dismantling(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
    method: StructuralConstructionMethodId,
) -> Result<ValidatedPlayerStructuralDismantlingStart, PlayerStructuralDismantlingError> {
    let player = state
        .logistics()
        .player()
        .copied()
        .ok_or(PlayerStructuralDismantlingError::PlayerNotAdmitted)?;
    let record = target_record(state, element)?;
    if !bounds_are_local(record.bounds(), player.position()) {
        return Err(PlayerStructuralDismantlingError::ElementRemote { element });
    }
    let definition = registries
        .labor()
        .get_structural_construction(method)
        .copied()
        .ok_or(PlayerStructuralDismantlingError::UnknownMethod { method })?;
    let recovered_mass = record.embodied_mass();
    let duration = definition.duration_for_mass(recovered_mass).ok_or(
        PlayerStructuralDismantlingError::MemberTooHeavy {
            required: recovered_mass,
            maximum: definition.maximum_member_mass(),
        },
    )?;
    let completes_at = state
        .tick()
        .checked_add_span(duration)
        .ok_or(PlayerStructuralDismantlingError::CompletionTickOverflow)?;
    let recovery_destination = player.carried_stockpile();
    validate_player_stockpile_access(state, recovery_destination)
        .map_err(PlayerStructuralDismantlingError::RecoveryAccess)?;
    if !state.can_spend_inventory_revisions(2) {
        return Err(PlayerStructuralDismantlingError::InventoryRevisionExhausted);
    }
    if !state.can_spend_structure_revisions(1) {
        return Err(PlayerStructuralDismantlingError::StructureRevisionExhausted);
    }
    let recovery_parcels = u64::try_from(record.embodied_material().len())
        .unwrap_or_else(|_| unreachable!("resident structural trace count fits u64"));
    if !state.has_material_lot_id_headroom_from(state.inventory().next_lot_id(), recovery_parcels) {
        return Err(PlayerStructuralDismantlingError::MaterialLotIdExhausted);
    }
    let _ = validate_material_ingress(
        registries,
        state.inventory(),
        recovery_destination,
        record
            .embodied_material()
            .iter()
            .map(MaterialIngressEntry::from_consumed_trace),
        state.tick(),
    )
    .map_err(|_| PlayerStructuralDismantlingError::RecoveryIngress)?;
    let reservation =
        validate_inbound_reservation(state.inventory(), recovery_destination, recovered_mass)
            .map_err(|_| PlayerStructuralDismantlingError::RecoveryReservation)?;
    let work = StructuralDismantlingWork::new(
        element,
        recovery_destination,
        method,
        recovered_mass,
        recovery_parcels,
        state.tick(),
        completes_at,
    );
    let work_start = validate_player_work_start(
        registries,
        state,
        PlayerWork::StructuralDismantling { work },
        duration,
        definition.exertion(),
    )
    .map_err(PlayerStructuralDismantlingError::Work)?;
    Ok(ValidatedPlayerStructuralDismantlingStart {
        expected_structure_revision: state.structures().revision(),
        expected_logistics_revision: state.logistics().revision(),
        expected_player_position: player.position(),
        element,
        expected_mass: recovered_mass,
        reservation,
        work_start,
        outcome: StructuralDismantlingStartOutcome {
            element,
            recovery_destination,
            recovered_mass,
            completes_at,
        },
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructuralDismantlingTickError {
    MaterialLotIds,
    InventoryRevision,
    StructureRevision,
}

#[must_use]
pub(crate) struct StructuralDismantlingTickPlan {
    work: StructuralDismantlingWork,
    ingress: ValidatedMaterialIngress,
    expected_structure_revision: u64,
    next_structure_revision: u64,
}

#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuralDismantlingOutcome {
    element: StructuralElementId,
    recovered_lots: Vec<MaterialLotId>,
}

impl StructuralDismantlingOutcome {
    #[must_use]
    pub const fn element(&self) -> StructuralElementId {
        self.element
    }
    #[must_use]
    pub fn recovered_lots(&self) -> &[MaterialLotId] {
        &self.recovered_lots
    }
}

pub(crate) fn decide_structural_dismantling_tick(
    registries: &Registries,
    state: &AppState,
    projected_inventory: &crate::inventory::InventoryState,
    projected_structures: &super::StructureState,
    next_tick: SimulationTick,
) -> Result<Option<StructuralDismantlingTickPlan>, StructuralDismantlingTickError> {
    let Some(work) = state.player_work().structural_dismantling_due_at(next_tick) else {
        return Ok(None);
    };
    let record = projected_structures
        .get_element(work.element())
        .unwrap_or_else(|| panic!("active structural dismantling target disappeared"));
    assert_eq!(record.embodied_mass(), work.recovered_mass());
    assert!(
        projected_structures
            .elements()
            .all(|candidate| candidate.id() == work.element()
                || projected_structures
                    .supports(candidate.id())
                    .is_none_or(|supports| supports
                        .into_iter()
                        .all(|support| support != work.element()))),
        "admitted simple structural dismantling acquired a dependent before completion"
    );
    let ingress = validate_reserved_material_ingress(
        registries,
        projected_inventory,
        work.recovery_destination(),
        record
            .embodied_material()
            .iter()
            .map(MaterialIngressEntry::from_consumed_trace),
        next_tick,
        work.recovered_mass(),
    )
    .map_err(|error| match error {
        MaterialIngressError::LotIdExhausted => StructuralDismantlingTickError::MaterialLotIds,
        MaterialIngressError::RevisionExhausted => {
            StructuralDismantlingTickError::InventoryRevision
        }
        other => panic!("admitted structural recovery became invalid: {other}"),
    })?;
    let expected_structure_revision = projected_structures.revision();
    let next_structure_revision = expected_structure_revision
        .checked_add(1)
        .ok_or(StructuralDismantlingTickError::StructureRevision)?;
    Ok(Some(StructuralDismantlingTickPlan {
        work,
        ingress,
        expected_structure_revision,
        next_structure_revision,
    }))
}

pub(crate) fn apply_structural_dismantling_tick(
    state: &mut AppState,
    plan: Option<StructuralDismantlingTickPlan>,
) -> Option<StructuralDismantlingOutcome> {
    let plan = plan?;
    plan.ingress.assert_matches_state(state.inventory());
    assert_eq!(
        state.structures().revision(),
        plan.expected_structure_revision
    );
    let recovered_lots = apply_material_ingress(state.inventory_state_mut(), plan.ingress);
    state
        .structure_state_mut()
        .remove_element(plan.work.element());
    state
        .structure_state_mut()
        .apply_revision(plan.next_structure_revision);
    Some(StructuralDismantlingOutcome {
        element: plan.work.element(),
        recovered_lots,
    })
}

pub(crate) struct StructuralDismantlingCancellationPlan {
    release: ValidatedInboundReservationRelease,
}

pub(crate) fn decide_structural_dismantling_cancellation(
    projected_inventory: &crate::inventory::InventoryState,
    work: StructuralDismantlingWork,
) -> Result<StructuralDismantlingCancellationPlan, StructuralDismantlingTickError> {
    let release = validate_inbound_reservation_release(
        projected_inventory,
        work.recovery_destination(),
        work.recovered_mass(),
    )
    .map_err(|error| match error {
        InboundReservationReleaseError::RevisionExhausted => {
            StructuralDismantlingTickError::InventoryRevision
        }
    })?;
    Ok(StructuralDismantlingCancellationPlan { release })
}

pub(crate) fn apply_structural_dismantling_cancellation(
    state: &mut AppState,
    plan: StructuralDismantlingCancellationPlan,
) {
    plan.release.apply(state.inventory_state_mut());
}

#[cfg(test)]
#[path = "dismantling_tests.rs"]
mod tests;
