//! Industrial equipment witnessed along a real patrol changes finite kobold behavior.

use crate::capability::{
    CapabilityDefinition, CapabilityId, CapabilityProfile, CapabilityValue, CapabilityValueKind,
};
use crate::content::{FORM_CHIP, MATERIAL_STONE, make_test_registries_with_equipment};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::core::time::SimulationTick;
use crate::equipment::{EquipmentDefinition, EquipmentDefinitionId, add_equipment};
use crate::logistics::{add_traversable_edge_for_test, place_equipment_for_fixture};
use crate::maintenance::{Condition, MaintenanceThresholds};
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::registry::Registries;
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};

use super::scout_sees_engineered_machinery;

const MACHINE: EquipmentDefinitionId = EquipmentDefinitionId::new(981_002);
const WORK: CapabilityId = CapabilityId::new(981_002);

fn fixture(requires_support: bool) -> (Registries, AppState, [VoxelCoord; 3]) {
    let profile =
        CapabilityProfile::new([(WORK, CapabilityValue::Mass(Mass::from_milligrams(1)))]).unwrap();
    let thresholds = MaintenanceThresholds::new(
        Condition::new(600_000).unwrap(),
        Condition::new(250_000).unwrap(),
    )
    .unwrap();
    let definition = EquipmentDefinition::new(
        MACHINE,
        "industrial mill",
        Mass::from_milligrams(5_000_000),
        profile,
        thresholds,
    );
    let definition = if requires_support {
        definition.with_required_structural_support()
    } else {
        definition
    };
    let registries = make_test_registries_with_equipment(
        CapabilityDefinition::new(WORK, "milling capability", CapabilityValueKind::Mass),
        definition,
    );
    let mut state = AppState::new();
    let route = [
        VoxelCoord::new(-1, 0, 0),
        VoxelCoord::new(0, 0, 0),
        VoxelCoord::new(0, 0, 1),
    ];
    for (from, to) in [(route[0], route[1]), (route[1], route[2])] {
        add_traversable_edge_for_test(&mut state, from, to);
    }
    state.kobold_state_mut().apply_opening_colony(route);
    (registries, state, route)
}

fn place_machine(registries: &Registries, state: &mut AppState, at: VoxelCoord) {
    let machine = add_equipment(registries, state, MACHINE, Condition::PRISTINE).unwrap();
    place_equipment_for_fixture(state, machine, at);
}

fn advance_to(registries: &Registries, state: &mut AppState, target: u64) {
    while state.tick().value() < target {
        let _ = advance_tick(registries, state).unwrap();
    }
}

#[test]
fn observed_stationary_industry_changes_real_scout_salvage_after_home_report() {
    let (registries, initial, route) = fixture(true);
    let mut witnessed = initial.clone();
    let mut unseen = initial;
    place_machine(&registries, &mut witnessed, route[1]);
    place_machine(&registries, &mut unseen, VoxelCoord::new(10, 0, 0));
    assert!(scout_sees_engineered_machinery(
        &registries,
        &witnessed,
        route[0]
    ));
    assert!(!scout_sees_engineered_machinery(
        &registries,
        &unseen,
        route[0]
    ));

    advance_to(&registries, &mut witnessed, 190);
    advance_to(&registries, &mut unseen, 190);
    let colony = witnessed.kobold_state().colony.as_ref().unwrap();
    assert_eq!(
        colony.witnessed_machinery_at,
        Some(SimulationTick::new(180))
    );
    assert_eq!(colony.mobilized_at, None);

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &witnessed)).unwrap();
    let mut restored = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..5 {
        assert_eq!(
            advance_tick(&registries, &mut witnessed).unwrap(),
            advance_tick(&registries, &mut restored).unwrap()
        );
    }
    assert_eq!(witnessed, restored);
    advance_to(&registries, &mut unseen, 195);
    assert_eq!(
        witnessed
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .mobilized_at,
        Some(SimulationTick::new(195))
    );
    assert_eq!(
        unseen.kobold_state().colony.as_ref().unwrap().mobilized_at,
        None
    );

    // Both residents encounter the same exposed bulk scrap. Only the scout
    // that physically reported industry takes the larger conserved portion.
    let add_scrap = |state: &mut AppState| {
        let commodity = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
        let spec = SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            route[2],
            commodity,
            Mass::from_milligrams(750_000),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap();
        let validated = validate_create_surface_resources(&registries, state, [spec]).unwrap();
        let id = validated.first_resource_id();
        validated.commit(state).unwrap();
        id
    };
    let informed_scrap = add_scrap(&mut witnessed);
    let ordinary_scrap = add_scrap(&mut unseen);
    let informed_total = calculate_matter_accounting(&witnessed).unwrap().total();
    let ordinary_total = calculate_matter_accounting(&unseen).unwrap().total();
    advance_to(&registries, &mut witnessed, 210);
    advance_to(&registries, &mut unseen, 210);
    assert_eq!(
        witnessed
            .surface()
            .get(informed_scrap)
            .unwrap()
            .remaining_mass(),
        Mass::from_milligrams(250_000)
    );
    assert_eq!(
        unseen
            .surface()
            .get(ordinary_scrap)
            .unwrap()
            .remaining_mass(),
        Mass::from_milligrams(500_000)
    );
    assert_eq!(
        calculate_matter_accounting(&witnessed).unwrap().total(),
        informed_total
    );
    assert_eq!(
        calculate_matter_accounting(&unseen).unwrap().total(),
        ordinary_total
    );
    assert_eq!(validate_loaded_state(&registries, &witnessed), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &unseen), Ok(()));
}

#[test]
fn ordinary_stationary_tools_and_unlocated_machinery_do_not_mobilize_colony() {
    for requires_support in [false, true] {
        let (registries, mut state, route) = fixture(requires_support);
        let unlocated =
            add_equipment(&registries, &mut state, MACHINE, Condition::PRISTINE).unwrap();
        assert_eq!(
            state.logistics().stationary_equipment_position(unlocated),
            None
        );
        assert!(!scout_sees_engineered_machinery(
            &registries,
            &state,
            route[1]
        ));
        if !requires_support {
            place_equipment_for_fixture(&mut state, unlocated, route[1]);
            assert!(!scout_sees_engineered_machinery(
                &registries,
                &state,
                route[1]
            ));
        }
        advance_to(&registries, &mut state, 195);
        let colony = state.kobold_state().colony.as_ref().unwrap();
        assert_eq!(colony.witnessed_machinery_at, None);
        assert_eq!(colony.mobilized_at, None);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }
}
