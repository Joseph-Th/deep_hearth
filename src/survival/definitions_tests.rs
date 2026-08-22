//! Tests for the sibling definitions module; isolated so test-only edits do not invalidate production builds.

use super::*;

fn physiology(starvation_loss: u32, dehydration_loss: u32) -> PhysiologyDefinition {
    PhysiologyDefinition::new(
        MetabolismDefinition::new(
            Energy::from_nanojoules(10),
            Energy::from_nanojoules(5),
            Energy::from_nanojoules(1),
        ),
        HydrationDefinition::new(
            Volume::from_microliters(10),
            Volume::from_microliters(5),
            Volume::from_microliters(1),
        ),
        NutritionDefinition::new(1, 1),
        starvation_loss,
        dehydration_loss,
    )
}

#[test]
fn individual_vitality_loss_rates_stay_inside_normalized_range() {
    let maximum = physiology(1_000_000, 1_000_000);
    assert_eq!(maximum.starvation_vitality_loss_ppm_per_tick(), 1_000_000);
    assert_eq!(maximum.dehydration_vitality_loss_ppm_per_tick(), 1_000_000);

    assert!(std::panic::catch_unwind(|| physiology(1_000_001, 1)).is_err());
    assert!(std::panic::catch_unwind(|| physiology(1, 1_000_001)).is_err());
}

#[test]
fn passive_survival_costs_cannot_exceed_full_reserves() {
    assert!(
        std::panic::catch_unwind(|| {
            MetabolismDefinition::new(
                Energy::from_nanojoules(10),
                Energy::from_nanojoules(5),
                Energy::from_nanojoules(11),
            )
        })
        .is_err()
    );
    assert!(
        std::panic::catch_unwind(|| {
            HydrationDefinition::new(
                Volume::from_microliters(10),
                Volume::from_microliters(5),
                Volume::from_microliters(11),
            )
        })
        .is_err()
    );
}

#[test]
fn drink_hydration_multiplier_cannot_create_hydration_volume() {
    let fluid = FluidDefinitionId::new(1);
    let maximum = DrinkDefinition::new(fluid, 1_000_000);
    assert_eq!(maximum.hydration_multiplier_ppm(), 1_000_000);

    assert!(std::panic::catch_unwind(|| DrinkDefinition::new(fluid, 0)).is_err());
    assert!(std::panic::catch_unwind(|| DrinkDefinition::new(fluid, 1_000_001)).is_err());
}
