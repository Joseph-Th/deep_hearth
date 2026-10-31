//! Regression contracts for physical-source and gathering-method contextual selection.

use super::*;
use crate::content::{FORM_FOOD, FORM_LUMP, MATERIAL_BERRIES, MATERIAL_STONE, build_registries};
use crate::core::quantity::Temperature;
use crate::material::MaterialComposition;
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};

#[test]
fn contextual_gathering_counts_physical_sources_before_method_choices() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let specs = [
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
    ]
    .map(|commodity| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Opportunity,
            position,
            commodity,
            Mass::from_milligrams(500_000),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(commodity.material()),
        )
        .unwrap()
    });
    validate_create_surface_resources(&registries, &state, specs)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let options = player_surface_gathering_options(&registries, &state).unwrap();
    assert_eq!(options.len(), 2);
    assert_ne!(options[0].resource(), options[1].resource());

    assert!(matches!(
        classify_direct_surface_gathering_options(&[]),
        DirectSurfaceGatheringResolution::None
    ));
    assert!(matches!(
        classify_direct_surface_gathering_options(&options[..1]),
        DirectSurfaceGatheringResolution::One(_)
    ));
    // An additional method for the same observed source must not be
    // counted as another physical resource or hide the second source.
    assert!(matches!(
        classify_direct_surface_gathering_options(&[options[0], options[0]]),
        DirectSurfaceGatheringResolution::AmbiguousMethods { count: 2 }
    ));
    assert!(matches!(
        classify_direct_surface_gathering_options(&[options[0], options[0], options[1]]),
        DirectSurfaceGatheringResolution::AmbiguousResources { count: 2 }
    ));
}
