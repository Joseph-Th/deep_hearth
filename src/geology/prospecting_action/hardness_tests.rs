//! Quantization contracts for actor-safe physical-sample excavation hardness.

use crate::content::build_registries;
use crate::core::quantity::{Mass, Pressure, Temperature};
use crate::core::state::AppState;
use crate::core::time::WorldSeed;
use crate::geology::{GeneratedDepositSpec, insert_generated_deposit};
use crate::material::{CommodityKey, MaterialComposition};
use crate::spatial::{VoxelBounds, VoxelCoord};

use crate::content::{FORM_ORE, MATERIAL_COPPER};

use super::resolve_region_excavation_hardness;

fn bounds() -> VoxelBounds {
    VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(1, 1, 1))
        .unwrap_or_else(|error| panic!("hardness test bounds failed: {error}"))
}

fn observed_hardness(hardness_pa: u64) -> crate::geology::ExcavationHardnessEstimate {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(hardness_pa));
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
