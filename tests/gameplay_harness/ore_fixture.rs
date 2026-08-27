//! Composition fixtures shared only by gameplay probes that bootstrap copper ore.

use deep_hearth::content::{MATERIAL_CLAY, MATERIAL_COPPER, MATERIAL_STONE};
use deep_hearth::material::{
    COMPOSITION_PARTS_PER_MILLION, CompositionComponent, MaterialComposition,
};

pub(super) fn copper_ore_composition(copper_ppm: u32, clay_share_ppm: u32) -> MaterialComposition {
    assert!(copper_ppm < COMPOSITION_PARTS_PER_MILLION);
    assert!(clay_share_ppm <= COMPOSITION_PARTS_PER_MILLION);
    let gangue_ppm = COMPOSITION_PARTS_PER_MILLION - copper_ppm;
    let clay_ppm = u32::try_from(
        u64::from(gangue_ppm) * u64::from(clay_share_ppm)
            / u64::from(COMPOSITION_PARTS_PER_MILLION),
    )
    .unwrap_or_else(|_| unreachable!("normalized gangue fraction fits u32"));
    MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, copper_ppm),
        CompositionComponent::new(MATERIAL_STONE, gangue_ppm - clay_ppm),
        CompositionComponent::new(MATERIAL_CLAY, clay_ppm),
    ])
    .unwrap_or_else(|error| panic!("gameplay harness ore composition failed: {error}"))
}
