//! Quantization contracts for actor-safe physical-sample excavation hardness.

use crate::content::build_registries;
use crate::core::quantity::{Mass, Pressure, Temperature};
use crate::core::state::AppState;
use crate::geology::{GeneratedDepositSpec, insert_generated_deposit};
use crate::material::{CommodityKey, MaterialComposition};
use crate::spatial::{VoxelBounds, VoxelCoord};

use crate::content::{FORM_ORE, MATERIAL_COPPER};

use super::{excavation_hardness_band_matches_resolution, resolve_region_excavation_hardness};

fn bounds() -> VoxelBounds {
    VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(1, 1, 1))
        .unwrap_or_else(|error| panic!("hardness test bounds failed: {error}"))
}

#[test]
fn representational_ceiling_retains_full_authored_hardness_resolution() {
    let resolution = Pressure::from_pascals(50_000_000);
    let registries = build_registries();
    let mut state = AppState::new();
    let actual = Pressure::from_pascals(u64::MAX - 100);
    insert_generated_deposit(
        &registries,
        &mut state,
        GeneratedDepositSpec::new(
            bounds(),
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Mass::from_milligrams(1_000),
            Temperature::from_millikelvin(293_150),
            actual,
            MaterialComposition::pure(MATERIAL_COPPER),
        )
        .unwrap_or_else(|error| panic!("ceiling hardness deposit spec failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("ceiling hardness deposit insertion failed: {error}"));

    let estimate =
        resolve_region_excavation_hardness(&state, bounds(), MATERIAL_COPPER, resolution)
            .unwrap_or_else(|| panic!("ceiling hardness test produced no estimate"));
    assert_eq!(estimate.upper(), Pressure::from_pascals(u64::MAX));
    assert_eq!(
        estimate.lower(),
        Pressure::from_pascals(u64::MAX - resolution.pascals())
    );
    assert!(estimate.lower() <= actual && actual <= estimate.upper());
    assert!(excavation_hardness_band_matches_resolution(
        estimate, resolution
    ));
}

#[test]
fn hardness_resolution_shape_rejects_narrow_ceiling_band() {
    let resolution = Pressure::from_pascals(50_000_000);
    let aligned_lower = ((u64::MAX - 100) / resolution.pascals()) * resolution.pascals();
    let narrow = crate::geology::ExcavationHardnessEstimate::new(
        Pressure::from_pascals(aligned_lower),
        Pressure::from_pascals(u64::MAX),
    )
    .unwrap_or_else(|error| panic!("narrow hardness estimate fixture failed: {error}"));

    assert!(!excavation_hardness_band_matches_resolution(
        narrow, resolution
    ));
}

fn observed_hardness(hardness_pa: u64) -> crate::geology::ExcavationHardnessEstimate {
    let registries = build_registries();
    let mut state = AppState::new();
    insert_generated_deposit(
        &registries,
        &mut state,
        GeneratedDepositSpec::new(
            bounds(),
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Mass::from_milligrams(1_000),
            Temperature::from_millikelvin(293_150),
            Pressure::from_pascals(hardness_pa),
            MaterialComposition::pure(MATERIAL_COPPER),
        )
        .unwrap_or_else(|error| panic!("hardness test deposit spec failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("hardness test deposit insertion failed: {error}"));
    resolve_region_excavation_hardness(
        &state,
        bounds(),
        MATERIAL_COPPER,
        Pressure::from_pascals(50_000_000),
    )
    .unwrap_or_else(|| panic!("hardness test produced no estimate"))
}

#[test]
fn hardness_buckets_preserve_equipment_limit_boundaries_without_exact_truth() {
    assert_eq!(
        observed_hardness(500_000_000),
        crate::geology::ExcavationHardnessEstimate::new(
            Pressure::from_pascals(450_000_000),
            Pressure::from_pascals(500_000_000),
        )
        .unwrap_or_else(|error| panic!("500 MPa hardness estimate fixture failed: {error}"))
    );
    assert_eq!(
        observed_hardness(500_000_001),
        crate::geology::ExcavationHardnessEstimate::new(
            Pressure::from_pascals(500_000_000),
            Pressure::from_pascals(550_000_000),
        )
        .unwrap_or_else(|error| panic!("above-500 MPa hardness estimate fixture failed: {error}"))
    );
    assert_eq!(
        observed_hardness(600_000_001),
        crate::geology::ExcavationHardnessEstimate::new(
            Pressure::from_pascals(600_000_000),
            Pressure::from_pascals(650_000_000),
        )
        .unwrap_or_else(|error| panic!("above-600 MPa hardness estimate fixture failed: {error}"))
    );
}
