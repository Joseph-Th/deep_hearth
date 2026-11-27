//! Player-facing manual ore processing rejection messages.

use super::*;
use deep_hearth::content::{FORM_CRUSHED, FORM_ORE, MATERIAL_COPPER, build_registries};
use deep_hearth::core::quantity::Temperature;
use deep_hearth::material::CommodityKey;
use deep_hearth::ore_processing::{
    ManualComminutionResolutionError, ManualConstituentSeparationResolutionError,
};

#[test]
fn hot_ore_blockers_identify_material_and_exact_temperature_limit() {
    let registries = build_registries();
    let hot_input = |form| ManualOrePhysicsError::InputTooHot {
        commodity: CommodityKey::new(MATERIAL_COPPER, form),
        temperature: Temperature::from_millikelvin(353_151),
        maximum: Temperature::from_millikelvin(353_150),
    };
    assert_eq!(
        resolution_blocker(
            &registries,
            ManualComminutionResolutionError::Physics(hot_input(FORM_ORE)),
        ),
        "copper ore is too hot for hand ore processing (80.001 C; maximum 80 C)"
    );
    assert_eq!(
        resolution_blocker(
            &registries,
            ManualConstituentSeparationResolutionError::Physics(hot_input(FORM_CRUSHED)),
        ),
        "crushed copper ore is too hot for hand ore processing (80.001 C; maximum 80 C)"
    );
    let other = ManualComminutionResolutionError::Physics(ManualOrePhysicsError::ZeroBatchMass);
    let expected = other.to_string();
    assert_eq!(resolution_blocker(&registries, other), expected);
}
