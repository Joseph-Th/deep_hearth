//! Contract tests for survival definitions and intake limits.

use super::*;
use crate::core::quantity::{Energy, Mass, MassSpecificEnergy, Temperature, Volume};
use crate::core::time::TickSpan;
use crate::fluid::FluidDefinitionId;
use crate::material::{CommodityKey, FormId, MaterialId};

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
        DirectConsumptionDefinition::new(
            Mass::from_milligrams(1),
            TickSpan::new(1),
            Volume::from_microliters(1),
            Volume::from_microliters(1),
            TickSpan::new(1),
        ),
        starvation_loss,
        dehydration_loss,
    )
}

#[test]
fn direct_consumption_definition_requires_valid_quantity_and_duration_limits() {
    for definition in [
        || {
            DirectConsumptionDefinition::new(
                Mass::ZERO,
                TickSpan::new(1),
                Volume::from_microliters(1),
                Volume::from_microliters(1),
                TickSpan::new(1),
            )
        },
        || {
            DirectConsumptionDefinition::new(
                Mass::from_milligrams(1),
                TickSpan::ZERO,
                Volume::from_microliters(1),
                Volume::from_microliters(1),
                TickSpan::new(1),
            )
        },
        || {
            DirectConsumptionDefinition::new(
                Mass::from_milligrams(1),
                TickSpan::new(1),
                Volume::ZERO,
                Volume::from_microliters(1),
                TickSpan::new(1),
            )
        },
        || {
            DirectConsumptionDefinition::new(
                Mass::from_milligrams(1),
                TickSpan::new(1),
                Volume::from_microliters(2),
                Volume::from_microliters(1),
                TickSpan::new(1),
            )
        },
        || {
            DirectConsumptionDefinition::new(
                Mass::from_milligrams(1),
                TickSpan::new(1),
                Volume::from_microliters(1),
                Volume::from_microliters(1),
                TickSpan::ZERO,
            )
        },
    ] {
        assert!(std::panic::catch_unwind(definition).is_err());
    }
}

#[test]
fn direct_consumption_duration_scales_with_exact_quantity() {
    let definition = DirectConsumptionDefinition::new(
        Mass::from_milligrams(1_000),
        TickSpan::new(100),
        Volume::from_microliters(100),
        Volume::from_microliters(1_000),
        TickSpan::new(20),
    );
    assert_eq!(
        definition.meal_duration(Mass::from_milligrams(500)),
        Some(TickSpan::new(50))
    );
    assert_eq!(
        definition.meal_duration(Mass::from_milligrams(1)),
        Some(TickSpan::new(1))
    );
    assert_eq!(definition.meal_duration(Mass::ZERO), None);
    assert_eq!(definition.meal_duration(Mass::from_milligrams(1_001)), None);
    assert_eq!(
        definition.drink_duration(Volume::from_microliters(250)),
        Some(TickSpan::new(5))
    );
    assert_eq!(
        definition.drink_duration(Volume::from_microliters(99)),
        None
    );
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
    let temperature = ConsumptionTemperatureRange::new(
        Temperature::from_millikelvin(273_150),
        Temperature::from_millikelvin(333_150),
    );
    let maximum = DrinkDefinition::new(fluid, 1_000_000, temperature);
    assert_eq!(maximum.hydration_multiplier_ppm(), 1_000_000);
    assert_eq!(
        maximum.hydration_offer(Volume::from_microliters(625)),
        Volume::from_microliters(625)
    );
    let diluted = DrinkDefinition::new(fluid, 625_000, temperature);
    assert_eq!(
        diluted.hydration_offer(Volume::from_microliters(1_000)),
        Volume::from_microliters(625)
    );
    assert_eq!(
        diluted.hydration_offer(Volume::from_microliters(1)),
        Volume::ZERO,
        "sub-microliter physiological hydration remains unrepresented rather than rounded up"
    );
    assert_eq!(
        diluted.minimum_volume_for_hydration(Volume::from_microliters(625)),
        Some(Volume::from_microliters(1_000))
    );
    let minimum = diluted
        .minimum_volume_for_hydration(Volume::from_microliters(1))
        .unwrap_or_else(|| {
            panic!("bounded hydration target must have a represented source volume")
        });
    assert_eq!(minimum, Volume::from_microliters(2));
    assert!(diluted.hydration_offer(minimum) >= Volume::from_microliters(1));
    assert!(
        diluted.hydration_offer(Volume::from_microliters(minimum.microliters() - 1))
            < Volume::from_microliters(1)
    );

    assert!(std::panic::catch_unwind(|| DrinkDefinition::new(fluid, 0, temperature)).is_err());
    assert!(
        std::panic::catch_unwind(|| DrinkDefinition::new(fluid, 1_000_001, temperature)).is_err()
    );
}

#[test]
fn food_hydration_supports_fractional_water_equivalent_without_exceeding_consumed_mass() {
    let commodity = CommodityKey::new(MaterialId::new(1), FormId::new(1));
    let temperature = ConsumptionTemperatureRange::new(
        Temperature::from_millikelvin(273_150),
        Temperature::from_millikelvin(333_150),
    );
    let maximally_hydrating_food = FoodDefinition::new(
        commodity,
        FoodCategory::Fruit,
        MassSpecificEnergy::from_nanojoules_per_milligram(1),
        1_000_000,
        TickSpan::new(10),
        temperature,
    );
    assert_eq!(
        maximally_hydrating_food.hydration_multiplier_ppm(),
        1_000_000
    );
    let fractional = FoodDefinition::new(
        commodity,
        FoodCategory::Fruit,
        MassSpecificEnergy::from_nanojoules_per_milligram(1),
        625_000,
        TickSpan::new(10),
        temperature,
    );
    assert_eq!(
        calculate_food_hydration_offer([(fractional, Mass::from_milligrams(8))]),
        Some(Volume::from_microliters(5))
    );
    assert!(
        std::panic::catch_unwind(|| FoodDefinition::new(
            commodity,
            FoodCategory::Fruit,
            MassSpecificEnergy::from_nanojoules_per_milligram(1),
            1_000_001,
            TickSpan::new(10),
            temperature,
        ))
        .is_err()
    );
}

#[test]
fn food_hydration_rounding_is_independent_of_inventory_lot_fragmentation() {
    let food = FoodDefinition::new(
        CommodityKey::new(MaterialId::new(1), FormId::new(1)),
        FoodCategory::Fruit,
        MassSpecificEnergy::from_nanojoules_per_milligram(1),
        625_000,
        TickSpan::new(10),
        ConsumptionTemperatureRange::new(
            Temperature::from_millikelvin(273_150),
            Temperature::from_millikelvin(333_150),
        ),
    );
    assert_eq!(
        calculate_food_hydration_offer([(food, Mass::from_milligrams(8))]),
        calculate_food_hydration_offer([
            (food, Mass::from_milligrams(3)),
            (food, Mass::from_milligrams(5)),
        ])
    );
}

#[test]
fn food_dietary_energy_cannot_exceed_conservative_fat_energy_density() {
    let commodity = CommodityKey::new(MaterialId::new(1), FormId::new(1));
    let temperature = ConsumptionTemperatureRange::new(
        Temperature::from_millikelvin(273_150),
        Temperature::from_millikelvin(333_150),
    );
    let maximum = FoodDefinition::new(
        commodity,
        FoodCategory::Protein,
        MassSpecificEnergy::from_nanojoules_per_milligram(40_000_000_000),
        0,
        TickSpan::new(10),
        temperature,
    );
    assert_eq!(
        maximum.dietary_energy(),
        MassSpecificEnergy::from_nanojoules_per_milligram(40_000_000_000)
    );
    assert!(
        std::panic::catch_unwind(|| FoodDefinition::new(
            commodity,
            FoodCategory::Protein,
            MassSpecificEnergy::from_nanojoules_per_milligram(40_000_000_001),
            0,
            TickSpan::new(10),
            temperature,
        ))
        .is_err()
    );
}

#[test]
fn food_definition_projects_minimum_mass_for_dietary_energy() {
    let food = FoodDefinition::new(
        CommodityKey::new(MaterialId::new(1), FormId::new(1)),
        FoodCategory::Grain,
        MassSpecificEnergy::from_nanojoules_per_milligram(40),
        0,
        TickSpan::new(10),
        ConsumptionTemperatureRange::new(
            Temperature::from_millikelvin(273_150),
            Temperature::from_millikelvin(333_150),
        ),
    );
    assert_eq!(
        food.minimum_mass_for_dietary_energy(Energy::from_nanojoules(81)),
        Some(Mass::from_milligrams(3))
    );
    assert_eq!(
        food.minimum_mass_for_dietary_energy(Energy::ZERO),
        Some(Mass::ZERO)
    );
    assert_eq!(
        food.dietary_energy_for_mass(Mass::from_milligrams(3)),
        Energy::from_nanojoules(120)
    );
}

#[test]
fn direct_consumption_temperature_range_is_ordered_and_inclusive() {
    let minimum = Temperature::from_millikelvin(273_150);
    let maximum = Temperature::from_millikelvin(333_150);
    let range = ConsumptionTemperatureRange::new(minimum, maximum);

    assert!(range.contains(minimum));
    assert!(range.contains(maximum));
    assert!(!range.contains(Temperature::from_millikelvin(273_149)));
    assert!(!range.contains(Temperature::from_millikelvin(333_151)));
    assert!(
        std::panic::catch_unwind(|| ConsumptionTemperatureRange::new(maximum, minimum)).is_err()
    );
}
