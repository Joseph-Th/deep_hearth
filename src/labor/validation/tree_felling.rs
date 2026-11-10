//! Trusted-load validation of finite standing-tree felling and equipment wear.

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::labor::TreeFellingWork;
use crate::registry::Registries;

use super::{
    ActivePlayerJobs, PlayerWorkValidationError, project_active_work_schedule,
    validate_remaining_resources,
};

pub(super) fn validate_tree_felling_work(
    registries: &Registries,
    state: &AppState,
    active_jobs: &ActivePlayerJobs,
    work: TreeFellingWork,
    available_energy: Energy,
    available_hydration: Volume,
) -> Result<(), PlayerWorkValidationError> {
    if active_jobs.has_any() {
        return Err(PlayerWorkValidationError::MultiplePlayerJobs);
    }
    let tree = state
        .world()
        .tree_at(work.root())
        .ok_or(PlayerWorkValidationError::TreeFellingTargetMissing)?;
    let player = state
        .logistics()
        .player()
        .ok_or(PlayerWorkValidationError::TreeFellingTargetRemote)?;
    let at = player.position();
    if at.y() != work.root().y()
        || at
            .x()
            .abs_diff(work.root().x())
            .checked_add(at.z().abs_diff(work.root().z()))
            != Some(1)
    {
        return Err(PlayerWorkValidationError::TreeFellingTargetRemote);
    }
    crate::logistics::validate_player_equipment_access(state, work.equipment())
        .map_err(|_| PlayerWorkValidationError::TreeFellingEquipmentInvalid)?;
    let equipment = state
        .equipment()
        .get_equipment(work.equipment())
        .ok_or(PlayerWorkValidationError::TreeFellingEquipmentInvalid)?;
    if equipment.definition() != crate::content::EQUIPMENT_STONE_WOODWORKING_ADZE
        && equipment.definition() != crate::content::EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE
    {
        return Err(PlayerWorkValidationError::TreeFellingEquipmentInvalid);
    }
    if work.equipment_trace().definition() != equipment.definition()
        || work.equipment_trace().condition() != equipment.condition()
    {
        return Err(PlayerWorkValidationError::TreeFellingEquipmentConditionMismatch);
    }
    let schedule =
        project_active_work_schedule(state.tick(), work.started_at(), work.completes_at())
            .ok_or(PlayerWorkValidationError::TreeFellingScheduleInvalid)?;
    let original_mass = tree.wood_at(work.started_at());
    if schedule.duration
        != crate::world::tree_felling_duration(original_mass, equipment.definition())
    {
        return Err(PlayerWorkValidationError::TreeFellingDurationMismatch);
    }
    if crate::world::tree_felling_condition_after(work.equipment_trace(), schedule.duration)
        == crate::maintenance::Condition::FAILED
    {
        return Err(PlayerWorkValidationError::TreeFellingEquipmentTooWorn);
    }
    validate_remaining_resources(
        registries,
        state,
        available_energy,
        available_hydration,
        crate::world::tree_felling_exertion(),
        schedule.remaining,
    )
}
