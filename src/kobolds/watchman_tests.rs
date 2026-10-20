//! A second resident makes real, ration-funded trips rather than appearing at a level threshold.

use crate::content::{FORM_FOOD, MATERIAL_WILD_NUTS};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::validate_loaded_state;
use crate::core::time::SimulationTick;
use crate::energy::EnergyCarrier;
use crate::kobolds::{KoboldRole, available_nearby_kobold_scouts};
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};

use super::{advance_to, scenario};

fn ration(
    registries: &crate::Registries,
    state: &mut crate::core::state::AppState,
    den: VoxelCoord,
) -> crate::surface::SurfaceResourceId {
    ration_mass(registries, state, den, Mass::from_milligrams(100_000))
}

fn ration_mass(
    registries: &crate::Registries,
    state: &mut crate::core::state::AppState,
    den: VoxelCoord,
    amount: Mass,
) -> crate::surface::SurfaceResourceId {
    let commodity = CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD);
    let plan = validate_create_surface_resources(
        registries,
        state,
        [SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            den,
            commodity,
            amount,
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_WILD_NUTS),
        )
        .unwrap()],
    )
    .unwrap();
    let id = plan.first_resource_id();
    plan.commit(state).unwrap();
    id
}

#[test]
fn watchman_is_an_opening_den_resident_but_only_organizes_after_report_and_a_full_meal() {
    let (registries, mut alerted, route) =
        scenario(EnergyCarrier::Electrical, Some(VoxelCoord::new(0, 0, 0)));
    let (_, mut ordinary, _) = scenario(EnergyCarrier::Electrical, None);
    advance_to(&registries, &mut alerted, 195);
    advance_to(&registries, &mut ordinary, 195);
    let food = ration(&registries, &mut alerted, route[0]);
    let ordinary_food = ration(&registries, &mut ordinary, route[0]);
    let conserved = calculate_matter_accounting(&alerted).unwrap().total();
    advance_to(&registries, &mut alerted, 200);
    advance_to(&registries, &mut ordinary, 200);
    assert_eq!(
        alerted
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        route[1]
    );
    assert_eq!(
        ordinary
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        route[0]
    );
    assert_eq!(
        alerted.surface().get(food).unwrap().remaining_mass(),
        Mass::from_milligrams(80_000)
    );
    assert_eq!(
        ordinary
            .surface()
            .get(ordinary_food)
            .unwrap()
            .remaining_mass(),
        Mass::from_milligrams(100_000)
    );
    assert_eq!(
        alerted
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_ready_at,
        SimulationTick::new(300)
    );
    let mut observer = alerted.clone();
    validate_admit_player(&registries, &observer, route[2])
        .unwrap()
        .commit(&mut observer)
        .unwrap();
    assert!(
        available_nearby_kobold_scouts(&registries, &observer)
            .any(|resident| resident.role() == KoboldRole::Watchman
                && resident.position() == route[1])
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &alerted)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let mut replay = decoded.into_state(&registries).unwrap();
    for _ in 0..20 {
        assert_eq!(
            crate::simulation::advance_tick(&registries, &mut alerted).unwrap(),
            crate::simulation::advance_tick(&registries, &mut replay).unwrap(),
        );
    }
    assert_eq!(alerted, replay);
    assert_eq!(
        alerted
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        route[0]
    );
    assert_eq!(
        alerted.surface().get(food).unwrap().remaining_mass(),
        Mass::from_milligrams(80_000),
        "one ration buys one finite watch, not a meal every scout tick"
    );
    validate_admit_player(&registries, &alerted, route[1])
        .unwrap()
        .commit(&mut alerted)
        .unwrap();
    let sightings = available_nearby_kobold_scouts(&registries, &alerted).collect::<Vec<_>>();
    assert!(
        sightings
            .iter()
            .any(|s| s.role() == KoboldRole::Watchman && s.position() == route[0])
    );
    assert_eq!(
        calculate_matter_accounting(&alerted).unwrap().total(),
        conserved
    );
    assert_eq!(
        calculate_matter_accounting(&ordinary).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &alerted), Ok(()));
}

#[test]
fn watchman_requires_unblocked_safe_route_and_whole_food_ration() {
    let (registries, mut state, route) =
        scenario(EnergyCarrier::Electrical, Some(VoxelCoord::new(0, 0, 0)));
    advance_to(&registries, &mut state, 195);
    let food = ration(&registries, &mut state, route[0]);
    let mut nearby = state.clone();
    validate_admit_player(&registries, &nearby, route[2])
        .unwrap()
        .commit(&mut nearby)
        .unwrap();
    advance_to(&registries, &mut nearby, 200);
    assert!(
        nearby
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_deployed_at
            .is_none(),
        "a watchman cannot ignore a living player adjacent to its planned post"
    );
    let mut blocked = state.clone();
    validate_admit_player(&registries, &blocked, route[1])
        .unwrap()
        .commit(&mut blocked)
        .unwrap();
    advance_to(&registries, &mut blocked, 200);
    assert!(
        blocked
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_deployed_at
            .is_none()
    );
    assert_eq!(
        blocked
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        route[0]
    );
    advance_to(&registries, &mut state, 200);
    assert!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_deployed_at
            .is_some()
    );
    assert_eq!(
        state.surface().get(food).unwrap().remaining_mass(),
        Mass::from_milligrams(80_000)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &blocked), Ok(()));
}

#[test]
fn short_rations_cannot_deploy_watchman_and_a_player_can_trap_a_returning_guard() {
    let (registries, mut hungry, route) =
        scenario(EnergyCarrier::Mechanical, Some(VoxelCoord::new(0, 0, 0)));
    advance_to(&registries, &mut hungry, 195);
    let small = ration_mass(
        &registries,
        &mut hungry,
        route[0],
        Mass::from_milligrams(15_000),
    );
    advance_to(&registries, &mut hungry, 200);
    assert!(
        hungry
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_deployed_at
            .is_none()
    );
    assert_eq!(
        hungry.surface().get(small).unwrap().remaining_mass(),
        Mass::from_milligrams(15_000)
    );
    let mut provisioned = hungry.clone();
    ration(&registries, &mut provisioned, route[0]);
    advance_to(&registries, &mut provisioned, 205);
    assert_eq!(
        provisioned
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        route[1]
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &provisioned)).unwrap();
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let mut replay = loaded.into_state(&registries).unwrap();
    validate_admit_player(&registries, &provisioned, route[0])
        .unwrap()
        .commit(&mut provisioned)
        .unwrap();
    validate_admit_player(&registries, &replay, route[0])
        .unwrap()
        .commit(&mut replay)
        .unwrap();
    advance_to(&registries, &mut provisioned, 230);
    advance_to(&registries, &mut replay, 230);
    assert_eq!(provisioned, replay);
    assert_eq!(
        provisioned
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        route[1],
        "the guard cannot vanish through a player-occupied den entrance"
    );
    assert_eq!(validate_loaded_state(&registries, &provisioned), Ok(()));
}

#[test]
fn trusted_load_rejects_unreported_or_out_of_schedule_watch_deployments() {
    let (registries, mut state, route) =
        scenario(EnergyCarrier::Mechanical, Some(VoxelCoord::new(0, 0, 0)));
    advance_to(&registries, &mut state, 195);
    ration(&registries, &mut state, route[0]);
    advance_to(&registries, &mut state, 200);
    let valid = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    for (deployed, ready) in [(199_u64, 299_u64), (201, 301), (205, 305), (200, 250)] {
        let mut invalid = valid.clone();
        invalid["state"]["systems"]["kobolds"]["colony"]["watch_deployed_at"] =
            serde_json::json!(deployed);
        invalid["state"]["systems"]["kobolds"]["colony"]["watch_ready_at"] =
            serde_json::json!(ready);
        let loaded: LoadedSaveEnvelope = serde_json::from_value(invalid).unwrap();
        assert!(loaded.into_state(&registries).is_err());
    }
    let mut missing = valid;
    missing["state"]["systems"]["kobolds"]["colony"]
        .as_object_mut()
        .unwrap()
        .remove("watch_deployed_at");
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(missing).is_err());
}
