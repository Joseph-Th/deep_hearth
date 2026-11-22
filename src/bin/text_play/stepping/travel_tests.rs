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
        water: Vec::new(),
        copper: Vec::new(),
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
