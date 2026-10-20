//! Mobilization follows witnessed industry, persistent memory, and finite physical work.

use super::*;
use crate::content::{
    FORM_CHIP, FORM_FOOD, MATERIAL_STONE, MATERIAL_WILD_NUTS,
    make_test_registries_with_energy_store,
};
use crate::core::quantity::{Energy, Mass, Power, Temperature};
use crate::core::state::validate_loaded_state;
use crate::energy::{
    EnergyStoreDefinition, EnergyStoreDefinitionId, add_energy_store_with_initial_for_fixture,
};
use crate::kobolds::available_nearby_kobold_scouts;
use crate::logistics::{add_traversable_edge_for_test, place_energy_store_for_fixture};
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};

fn scenario(
    carrier: EnergyCarrier,
    machine_at: Option<VoxelCoord>,
) -> (Registries, AppState, [VoxelCoord; 4]) {
    let machine = EnergyStoreDefinitionId::new(930_607);
    let registries =
        make_test_registries_with_energy_store(EnergyStoreDefinition::new_with_transfer_limits(
            machine,
            "installed machine",
            carrier,
            Energy::from_nanojoules(1_000),
            Power::ZERO,
            Power::from_microwatts(10),
        ));
    let mut state = AppState::new();
    let route = [
        VoxelCoord::new(-1, 0, 0),
        VoxelCoord::new(0, 0, 0),
        VoxelCoord::new(0, 0, 1),
        VoxelCoord::new(1, 0, 0),
    ];
    for (a, b) in [
        (route[0], route[1]),
        (route[1], route[2]),
        (route[1], route[3]),
    ] {
        add_traversable_edge_for_test(&mut state, a, b);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([route[0], route[1], route[2]]);
    if let Some(position) = machine_at {
        let store = add_energy_store_with_initial_for_fixture(
            &registries,
            &mut state,
            machine,
            Energy::ZERO,
        )
        .unwrap();
        place_energy_store_for_fixture(&mut state, store, position);
    }
    (registries, state, route)
}

fn put_resource(
    registries: &Registries,
    state: &mut AppState,
    position: VoxelCoord,
    edible: bool,
) -> crate::surface::SurfaceResourceId {
    let (kind, commodity, mass) = if edible {
        (
            SurfaceResourceKind::Opportunity,
            CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD),
            Mass::from_milligrams(500_000),
        )
    } else {
        (
            SurfaceResourceKind::Drop,
            CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
            Mass::from_milligrams(750_000),
        )
    };
    let source = SurfaceResourceSpec::new(
        kind,
        position,
        commodity,
        mass,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(commodity.material()),
    )
    .unwrap();
    let planned = validate_create_surface_resources(registries, state, [source]).unwrap();
    let id = planned.first_resource_id();
    planned.commit(state).unwrap();
    id
}

fn advance_to(registries: &Registries, state: &mut AppState, tick: u64) {
    while state.tick().value() < tick {
        let _ = advance_tick(registries, state).unwrap();
    }
}

#[test]
fn seeing_industry_mobilizes_larger_real_salvage_and_survives_save_replay() {
    let (registries, mut alerted, route) =
        scenario(EnergyCarrier::Mechanical, Some(VoxelCoord::new(0, 0, 0)));
    let (_, mut normal, _) = scenario(EnergyCarrier::Mechanical, None);
    let a = put_resource(&registries, &mut alerted, route[3], false);
    let b = put_resource(&registries, &mut normal, route[3], false);
    let total = calculate_matter_accounting(&alerted).unwrap().total();
    advance_to(&registries, &mut alerted, 190);
    advance_to(&registries, &mut normal, 190);
    assert_eq!(
        alerted.kobold_state().colony.as_ref().unwrap().mobilized_at,
        Some(SimulationTick::new(180))
    );
    assert_eq!(
        normal.kobold_state().colony.as_ref().unwrap().mobilized_at,
        None
    );
    assert_eq!(
        alerted.surface().get(a).unwrap().remaining_mass(),
        Mass::from_milligrams(250_000)
    );
    assert_eq!(
        normal.surface().get(b).unwrap().remaining_mass(),
        Mass::from_milligrams(500_000)
    );
    let haul = alerted
        .surface()
        .available_resources_at(Some(route[1]))
        .find(|resource| resource.kind() == SurfaceResourceKind::Drop)
        .unwrap()
        .id();
    assert_eq!(
        alerted.surface().get(haul).unwrap().remaining_mass(),
        Mass::from_milligrams(500_000)
    );
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &alerted)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&bytes).unwrap();
    let mut replay = decoded.into_state(&registries).unwrap();
    for _ in 0..5 {
        assert_eq!(
            advance_tick(&registries, &mut alerted).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(alerted, replay);
    assert_eq!(alerted.surface().get(haul).unwrap().position(), route[0]);
    assert_eq!(
        calculate_matter_accounting(&alerted).unwrap().total(),
        total
    );
    validate_admit_player(&registries, &alerted, route[2])
        .unwrap()
        .commit(&mut alerted)
        .unwrap();
    assert!(
        available_nearby_kobold_scouts(&registries, &alerted)
            .next()
            .is_some_and(|sighting| sighting.is_mobilized())
    );
    assert_eq!(validate_loaded_state(&registries, &alerted), Ok(()));
}

#[test]
fn only_nearby_engineered_machinery_triggers_higher_food_provisioning() {
    for (carrier, position) in [
        (EnergyCarrier::Thermal, VoxelCoord::new(0, 0, 0)),
        (EnergyCarrier::Electrical, VoxelCoord::new(100, 0, 0)),
    ] {
        let (registries, mut state, _) = scenario(carrier, Some(position));
        advance_to(&registries, &mut state, 195);
        assert!(
            state
                .kobold_state()
                .colony
                .as_ref()
                .unwrap()
                .mobilized_at
                .is_none()
        );
    }
    let (registries, mut alerted, route) =
        scenario(EnergyCarrier::Electrical, Some(VoxelCoord::new(0, 0, 0)));
    let (_, mut normal, _) = scenario(EnergyCarrier::Electrical, None);
    let fresh = put_resource(&registries, &mut alerted, route[1], true);
    let baseline = put_resource(&registries, &mut normal, route[1], true);
    let total = calculate_matter_accounting(&alerted).unwrap().total();
    advance_to(&registries, &mut alerted, 195);
    advance_to(&registries, &mut normal, 195);
    assert_eq!(
        alerted
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .next_forage_at,
        Some(SimulationTick::new(245))
    );
    assert_eq!(
        normal
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .next_forage_at,
        Some(SimulationTick::new(295))
    );
    assert_eq!(
        alerted.surface().get(fresh).unwrap().remaining_mass(),
        Mass::from_milligrams(300_000)
    );
    assert_eq!(
        normal.surface().get(baseline).unwrap().remaining_mass(),
        Mass::from_milligrams(400_000)
    );
    assert_eq!(
        calculate_matter_accounting(&alerted).unwrap().total(),
        total
    );
    assert_eq!(validate_loaded_state(&registries, &alerted), Ok(()));
    let saved = serde_json::to_value(SaveEnvelope::new(&registries, &alerted)).unwrap();
    for fake in [179_u64, 181, 400] {
        let mut altered = saved.clone();
        altered["state"]["systems"]["kobolds"]["colony"]["mobilized_at"] = serde_json::json!(fake);
        let decoded: LoadedSaveEnvelope = serde_json::from_value(altered).unwrap();
        assert!(decoded.into_state(&registries).is_err());
    }
    let mut missing = saved;
    missing["state"]["systems"]["kobolds"]["colony"]
        .as_object_mut()
        .unwrap()
        .remove("mobilized_at");
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(missing).is_err());
}
