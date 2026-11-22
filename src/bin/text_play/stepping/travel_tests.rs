//! Travel interruption regressions over real observations and physical movement.

use super::*;
use deep_hearth::world::HabitatKind;

#[test]
fn approaching_already_visible_stone_does_not_interrupt_trek_as_a_discovery() {
    let mut session =
        Session::new_natural_in_habitat(1_837_596_006, Some(HabitatKind::MixedWoodland)).unwrap();
    let initial = TravelDiscoveries::capture(&session);
    let origin = session.player_position().unwrap();
    assert!(initial.resources.iter().any(|(position, _)| {
        position.x().abs_diff(origin.x()) == 17
            && position.x() < origin.x()
            && position.z() == origin.z()
    }));

    let (report, _) = session.command("trek w 5").unwrap();
    assert!(report.contains("Trekked 5 of 5 cells"), "{report}");
    assert!(
        !report.contains("A new loose resource is visible"),
        "{report}"
    );
}

#[test]
fn an_unseen_nearby_landmark_or_new_scent_interrupts_but_known_cues_do_not() {
    let origin = VoxelCoord::new(0, 0, 0);
    let material = CommodityKey::new(
        deep_hearth::content::MATERIAL_WOOD,
        deep_hearth::content::FORM_LOG,
    );
    let far_resource = (VoxelCoord::new(17, 0, 0), material);
    let near_resource = (VoxelCoord::new(3, 0, 0), material);
    let snapshot = |observer, resources, scents| TravelDiscoveries {
        observer: Some(observer),
        resources,
        water: BTreeSet::new(),
        copper: BTreeSet::new(),
        scents,
    };
    let previous = snapshot(origin, BTreeSet::from([far_resource]), BTreeSet::new());
    let now_close = snapshot(
        VoxelCoord::new(5, 0, 0),
        BTreeSet::from([far_resource]),
        BTreeSet::new(),
    );
    assert_eq!(previous.attention(&now_close), None);
    let newly_visible = snapshot(
        VoxelCoord::new(5, 0, 0),
        BTreeSet::from([far_resource, near_resource]),
        BTreeSet::new(),
    );
    assert_eq!(
        previous.attention(&newly_visible),
        Some("A new loose resource is visible.")
    );
    let distant_new = snapshot(
        origin,
        BTreeSet::from([far_resource, (VoxelCoord::new(19, 0, 0), material)]),
        BTreeSet::new(),
    );
    assert_eq!(previous.attention(&distant_new), None);
    let new_scent = snapshot(
        origin,
        BTreeSet::from([far_resource]),
        BTreeSet::from([ForageScentKind::SweetFruit]),
    );
    assert_eq!(
        previous.attention(&new_scent),
        Some("A new forage scent is noticeable.")
    );
}

#[test]
fn discovery_attention_uses_minimum_cardinal_detour_not_square_sight_range() {
    let origin = VoxelCoord::new(0, 0, 0);
    let commodity = CommodityKey::new(
        deep_hearth::content::MATERIAL_WOOD,
        deep_hearth::content::FORM_LOG,
    );
    let previous = TravelDiscoveries {
        observer: Some(origin),
        resources: BTreeSet::new(),
        water: BTreeSet::new(),
        copper: BTreeSet::new(),
        scents: BTreeSet::new(),
    };
    let sighting = |position| TravelDiscoveries {
        observer: Some(origin),
        resources: BTreeSet::from([(position, commodity)]),
        water: BTreeSet::new(),
        copper: BTreeSet::new(),
        scents: BTreeSet::new(),
    };

    assert_eq!(
        previous.attention(&sighting(VoxelCoord::new(12, 0, 12))),
        None,
        "a diagonal site requiring 24 cardinal steps cannot interrupt a 12-step discovery radius"
    );
    assert_eq!(
        previous.attention(&sighting(VoxelCoord::new(8, 0, 5))),
        None
    );
    assert_eq!(
        previous.attention(&sighting(VoxelCoord::new(8, 0, 4))),
        Some("A new loose resource is visible."),
        "the radius includes its exact 12-step boundary"
    );
    assert_eq!(
        previous.attention(&sighting(VoxelCoord::new(12, 0, 0))),
        Some("A new loose resource is visible.")
    );
}

#[test]
fn previously_sighted_landmarks_do_not_become_false_discoveries_after_occlusion() {
    let material = CommodityKey::new(
        deep_hearth::content::MATERIAL_WOOD,
        deep_hearth::content::FORM_LOG,
    );
    let resource = (VoxelCoord::new(4, 0, 0), material);
    let copper = VoxelCoord::new(5, 0, 0);
    let mut history = TravelDiscoveries {
        observer: Some(VoxelCoord::new(0, 0, 0)),
        resources: BTreeSet::from([resource]),
        water: BTreeSet::new(),
        copper: BTreeSet::from([copper]),
        scents: BTreeSet::from([ForageScentKind::SweetFruit]),
    };
    let occluded = TravelDiscoveries {
        observer: Some(VoxelCoord::new(1, 0, 0)),
        resources: BTreeSet::new(),
        water: BTreeSet::new(),
        copper: BTreeSet::new(),
        scents: BTreeSet::new(),
    };
    assert_eq!(history.observe(occluded), None);
    let reappeared = TravelDiscoveries {
        observer: Some(VoxelCoord::new(2, 0, 0)),
        resources: BTreeSet::from([resource]),
        water: BTreeSet::new(),
        copper: BTreeSet::from([copper]),
        scents: BTreeSet::new(),
    };
    assert_eq!(history.observe(reappeared), None);
    assert!(history.resources.contains(&resource));
    assert!(history.copper.contains(&copper));

    // Windborne cues are transient: losing and later reacquiring a scent is
    // a legitimate new observation, unlike rediscovering a known landmark.
    assert_eq!(
        history.observe(TravelDiscoveries {
            observer: Some(VoxelCoord::new(3, 0, 0)),
            resources: BTreeSet::from([resource]),
            water: BTreeSet::new(),
            copper: BTreeSet::from([copper]),
            scents: BTreeSet::from([ForageScentKind::SweetFruit]),
        }),
        Some("A new forage scent is noticeable.")
    );
}

#[test]
fn once_visible_water_is_not_rediscovered_when_the_same_source_returns_to_sight() {
    let mut session = Session::new(24318).unwrap();
    session.command("e 20").unwrap();
    session.command("wait 10").unwrap();
    let mut history = TravelDiscoveries::capture(&session);
    let visible_water = history.water.clone();
    assert!(
        visible_water
            .iter()
            .any(|(position, _)| history.close_to(*position)),
        "the opening branch must expose at least one actionable water sighting"
    );
    let observer = history.observer;
    let resources = history.resources.clone();
    let copper = history.copper.clone();
    let scents = history.scents.clone();
    assert_eq!(
        history.observe(TravelDiscoveries {
            observer,
            resources: resources.clone(),
            water: BTreeSet::new(),
            copper: copper.clone(),
            scents: scents.clone(),
        }),
        None
    );
    assert_eq!(history.water, visible_water);
    assert_eq!(
        history.observe(TravelDiscoveries {
            observer,
            resources,
            water: visible_water.clone(),
            copper,
            scents,
        }),
        None
    );
    assert_eq!(
        history.water, visible_water,
        "repeated sightings must not create duplicate landmark records"
    );
}
